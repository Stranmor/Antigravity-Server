//! Streaming response handling for Claude messages

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures::StreamExt;
use std::pin::Pin;

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
            let combined_stream = build_combined_stream(bytes, stream_rest);

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
) -> Pin<Box<dyn futures::Stream<Item = Result<Bytes, std::io::Error>> + Send>>
where
    S: futures::Stream<Item = Result<Bytes, String>> + Send + 'static,
{
    Box::pin(
        futures::stream::once(async move { Ok(first_chunk) }).chain(stream_rest.map(
            |result| -> Result<Bytes, std::io::Error> {
                match result {
                    Ok(b) => Ok(b),
                    Err(e) => {
                        let err_str = e.to_string();
                        tracing::warn!(
                            "Stream error during transmission (graceful finish): {}",
                            err_str
                        );
                        crate::proxy::prometheus::record_stream_graceful_finish("claude");
                        Ok(Bytes::from(
                            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\",\"stop_sequence\":null},\"usage\":{\"input_tokens\":0,\"output_tokens\":0}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
                        ))
                    }
                }
            },
        )),
    )
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
        .header("X-Account-Email", &ctx.email)
        .header("X-Mapped-Model", &ctx.mapped_model)
        .header("X-Mapping-Reason", &ctx.reason)
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
                .header("X-Account-Email", &ctx.email)
                .header("X-Mapped-Model", &ctx.mapped_model)
                .header("X-Mapping-Reason", &ctx.reason)
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
