//! Streaming response handling for Claude messages

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures::StreamExt;
use std::pin::Pin;

use crate::proxy::common::header_constants::{X_ACCOUNT_EMAIL, X_MAPPED_MODEL, X_MAPPING_REASON};
use crate::proxy::mappers::claude::create_claude_sse_stream;
use crate::proxy::retry::{peek_first_data_chunk, PeekConfig, PeekResult};

pub struct StreamingContext {
    pub trace_id: String,
    pub email: String,
    pub session_id: String,
    pub mapped_model: String,
    pub reason: String,
    pub scaling_enabled: bool,
    pub context_limit: u32,
    pub estimated_tokens: Option<u32>,
    pub client_wants_stream: bool,
}

pub enum ClaudeStreamResult {
    Success(Response),
    Retry(String),
}

pub async fn handle_streaming_response(
    response: reqwest::Response,
    ctx: &StreamingContext,
) -> ClaudeStreamResult {
    let stream = response.bytes_stream();
    let gemini_stream = Box::pin(stream);

    let claude_stream = create_claude_sse_stream(
        gemini_stream,
        ctx.trace_id.clone(),
        ctx.email.clone(),
        Some(ctx.session_id.clone()),
        ctx.scaling_enabled,
        ctx.context_limit,
        ctx.estimated_tokens,
    );

    let peek_config = PeekConfig::default();
    let (first_data_chunk, claude_stream) =
        match peek_first_data_chunk(claude_stream, &peek_config, &ctx.trace_id).await {
            PeekResult::Data(bytes, stream) => (Some(bytes), stream),
            PeekResult::Retry(err) => {
                return ClaudeStreamResult::Retry(err);
            },
        };

    match first_data_chunk {
        Some(bytes) => {
            let stream_rest = claude_stream;
            let combined_stream = build_combined_stream(
                bytes,
                stream_rest,
                ctx.trace_id.clone(),
                ctx.client_wants_stream,
            );

            if ctx.client_wants_stream {
                ClaudeStreamResult::Success(build_sse_response(ctx, combined_stream))
            } else {
                ClaudeStreamResult::Success(collect_to_json_response(ctx, combined_stream).await)
            }
        },
        None => {
            tracing::warn!(
                "[{}] No data after peek loop (should not happen), retrying...",
                ctx.trace_id
            );
            ClaudeStreamResult::Retry("Empty response after peek".to_string())
        },
    }
}

fn build_combined_stream<S>(
    first_chunk: Bytes,
    stream_rest: S,
    trace_id: String,
    client_wants_stream: bool,
) -> Pin<Box<dyn futures::Stream<Item = Result<Bytes, std::io::Error>> + Send>>
where
    S: futures::Stream<Item = Result<Bytes, String>> + Send + 'static,
{
    Box::pin(async_stream::stream! {
        let mut next_block_index: usize = 0;

        update_block_index(&first_chunk, &mut next_block_index);
        yield Ok(first_chunk);

        let mut stream_rest = std::pin::pin!(stream_rest);
        while let Some(result) = stream_rest.next().await {
            match result {
                Ok(b) => {
                    update_block_index(&b, &mut next_block_index);
                    yield Ok(b);
                },
                Err(e) => {
                    tracing::warn!(
                        "[{}] Stream error — aborting connection (v4): {}",
                        trace_id,
                        e
                    );
                    crate::proxy::prometheus::record_stream_abort("claude");

                    // Only inject SSE error block for streaming clients.
                    // Non-streaming (JSON) path discards SSE — collect_stream_to_json
                    // handles the subsequent Err and returns HTTP 500.
                    if client_wants_stream {
                        let idx = next_block_index;
                        yield Ok(Bytes::from(format!(
                            "\n\nevent: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":{idx},\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\nevent: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":{idx},\"delta\":{{\"type\":\"text_delta\",\"text\":\"[Response truncated — upstream connection closed unexpectedly. The model may still have been processing. Try reducing context size or splitting the task.]\"}}}}\n\nevent: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{idx}}}\n\n"
                        )));
                    }
                    // v4: abort after text — no message_delta, no message_stop
                    yield Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted,
                        "stream abort",
                    ));
                    break;
                }
            }
        }
    })
}

/// Extract `"index":N` from a `content_block_start` SSE event line.
fn extract_index_from_sse(data: &str) -> Option<usize> {
    if !data.contains("content_block_start") {
        return None;
    }
    let idx_marker = "\"index\":";
    let start = data.find(idx_marker)?;
    let after = &data[start.checked_add(idx_marker.len())?..];
    let trimmed = after.trim_start();
    let end = trimmed.find(|c: char| !c.is_ascii_digit()).unwrap_or(trimmed.len());
    if end == 0 {
        return None;
    }
    trimmed[..end].parse::<usize>().ok()
}

/// Scan a chunk for `content_block_start` events and advance `next_block_index`
/// past the highest seen index.
fn update_block_index(chunk: &[u8], next_block_index: &mut usize) {
    let text = String::from_utf8_lossy(chunk);
    for line in text.lines() {
        if let Some(idx) = extract_index_from_sse(line) {
            if let Some(next) = idx.checked_add(1) {
                if next > *next_block_index {
                    *next_block_index = next;
                }
            }
        }
    }
}

fn build_sse_response<S>(ctx: &StreamingContext, stream: S) -> Response
where
    S: futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .header(X_ACCOUNT_EMAIL, &ctx.email)
        .header(X_MAPPED_MODEL, &ctx.mapped_model)
        .header(X_MAPPING_REASON, &ctx.reason)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|e| {
            tracing::error!("Failed to build SSE response: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal streaming setup error").into_response()
        })
}

async fn collect_to_json_response<S>(ctx: &StreamingContext, stream: S) -> Response
where
    S: futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    use crate::proxy::mappers::claude::collect_stream_to_json;

    match collect_stream_to_json(Box::pin(stream)).await {
        Ok(full_response) => {
            tracing::info!("[{}] ✓ Stream collected and converted to JSON", ctx.trace_id);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .header(X_ACCOUNT_EMAIL, &ctx.email)
                .header(X_MAPPED_MODEL, &ctx.mapped_model)
                .header(X_MAPPING_REASON, &ctx.reason)
                .body(Body::from(match serde_json::to_string(&full_response) {
                    Ok(json) => json,
                    Err(e) => {
                        tracing::error!("Failed to serialize ClaudeResponse: {}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Serialization error: {}", e),
                        )
                            .into_response();
                    },
                }))
                .unwrap_or_else(|e| {
                    tracing::error!("Failed to build JSON response: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, "Internal response setup error")
                        .into_response()
                })
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Stream collection error: {}", e))
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_index_from_content_block_start() {
        let line = r#"data: {"type":"content_block_start","index":3,"content_block":{"type":"text","text":""}}"#;
        assert_eq!(extract_index_from_sse(line), Some(3));
    }

    #[test]
    fn extract_index_zero() {
        let line = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        assert_eq!(extract_index_from_sse(line), Some(0));
    }

    #[test]
    fn extract_index_ignores_non_block_start() {
        let line = r#"data: {"type":"content_block_delta","index":5,"delta":{"type":"text_delta","text":"hi"}}"#;
        assert_eq!(extract_index_from_sse(line), None);
    }

    #[test]
    fn extract_index_ignores_event_line() {
        assert_eq!(extract_index_from_sse("event: content_block_delta"), None);
    }

    #[test]
    fn extract_index_large_number() {
        let line = r#"data: {"type":"content_block_start","index":42,"content_block":{"type":"text","text":""}}"#;
        assert_eq!(extract_index_from_sse(line), Some(42));
    }

    #[test]
    fn extract_index_with_whitespace_after_colon() {
        let line = r#"data: {"type": "content_block_start", "index": 7, "content_block": {"type": "text"}}"#;
        assert_eq!(extract_index_from_sse(line), Some(7));
    }

    #[test]
    fn update_block_index_single_event() {
        let chunk = b"data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n";
        let mut idx = 0usize;
        update_block_index(chunk, &mut idx);
        assert_eq!(idx, 3);
    }

    #[test]
    fn update_block_index_multiple_events() {
        let chunk = b"data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n";
        let mut idx = 0usize;
        update_block_index(chunk, &mut idx);
        assert_eq!(idx, 2);
    }

    #[test]
    fn update_block_index_no_block_start() {
        let chunk = b"data: {\"type\":\"content_block_delta\",\"index\":5,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n";
        let mut idx = 0usize;
        update_block_index(chunk, &mut idx);
        assert_eq!(idx, 0);
    }

    #[test]
    fn update_block_index_keeps_maximum() {
        let mut idx = 10usize;
        let chunk = b"data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n";
        update_block_index(chunk, &mut idx);
        assert_eq!(idx, 10);
    }

    #[test]
    fn update_block_index_invalid_utf8() {
        let chunk: &[u8] = &[0xFF, 0xFE, 0xFD];
        let mut idx = 0usize;
        update_block_index(chunk, &mut idx);
        assert_eq!(idx, 0);
    }
}
