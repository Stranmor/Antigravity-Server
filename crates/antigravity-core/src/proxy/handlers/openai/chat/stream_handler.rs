//! OpenAI chat streaming handler.

use crate::proxy::mappers::openai::OpenAIResponse;
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::StreamExt;
use tracing::info;

use crate::proxy::mappers::openai::streaming::create_openai_sse_stream;
use crate::proxy::retry::{peek_first_data_chunk, PeekConfig, PeekResult};

pub enum OpenAIStreamResult {
    StreamingResponse(Response),
    JsonResponse(StatusCode, String, String, String, OpenAIResponse),
    Retry(String),
    EmptyStream,
}

pub async fn handle_stream_response<S>(
    gemini_stream: S,
    model: String,
    email: String,
    mapped_model: String,
    reason: String,
    client_wants_stream: bool,
    trace_id: &str,
    session_id: String,
) -> OpenAIStreamResult
where
    S: futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    let openai_stream = create_openai_sse_stream(
        Box::pin(gemini_stream),
        model,
        None,
        Some(session_id),
        trace_id.to_string(),
    );

    let peek_config = PeekConfig::openai();
    let (first_data_chunk, openai_stream) =
        match peek_first_data_chunk(openai_stream, &peek_config, trace_id).await {
            PeekResult::Data(bytes, stream) => (Some(bytes), stream),
            PeekResult::Retry(err) => return OpenAIStreamResult::Retry(err),
        };

    match first_data_chunk {
        Some(bytes) => {
            let combined_stream = build_combined_stream(bytes, openai_stream, trace_id.to_string());

            if client_wants_stream {
                OpenAIStreamResult::StreamingResponse(build_sse_response(
                    combined_stream,
                    email,
                    mapped_model,
                    reason,
                ))
            } else {
                collect_to_json(combined_stream, email, mapped_model, reason).await
            }
        },
        None => OpenAIStreamResult::EmptyStream,
    }
}

fn build_combined_stream(
    first_bytes: Bytes,
    rest: impl futures::Stream<Item = Result<Bytes, String>> + Send + 'static,
    trace_id: String,
) -> impl futures::Stream<Item = Result<Bytes, String>> + Send + 'static {
    let mut bytes_from_rest: usize = 0;
    futures::stream::once(async move { Ok(first_bytes) }).chain(rest.map(
        move |result| -> Result<Bytes, String> {
            match result {
                Ok(b) => {
                    bytes_from_rest += b.len();
                    Ok(b)
                },
                Err(e) => {
                    tracing::warn!("[{}] Stream error during transmission (graceful finish, {} bytes received): {}", trace_id, bytes_from_rest, e);
                    crate::proxy::prometheus::record_stream_graceful_finish("openai_handler");

                    if bytes_from_rest == 0 {
                        Ok(Bytes::from(
                            "data: {\"id\":\"chatcmpl-timeout\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"unknown\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"[This request timed out — the model was still processing when the upstream server closed the connection (~55s limit). This typically happens with very large contexts (100K+ tokens). Try reducing conversation history or splitting the task.]\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chatcmpl-timeout\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"unknown\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
                        ))
                    } else {
                        Ok(Bytes::from(
                            "data: {\"id\":\"chatcmpl-error\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"unknown\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n"
                        ))
                    }
                }
            }
        },
    ))
}

fn build_sse_response(
    stream: impl futures::Stream<Item = Result<Bytes, String>> + Send + 'static,
    email: String,
    mapped_model: String,
    reason: String,
) -> Response {
    let mapped_stream = stream.map(|r| r.map_err(std::io::Error::other));
    let body = Body::from_stream(mapped_stream);
    Response::builder()
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("X-Account-Email", &email)
        .header("X-Mapped-Model", &mapped_model)
        .header("X-Mapping-Reason", &reason)
        .body(body)
        .unwrap_or_else(|e| {
            tracing::error!("Failed to build SSE response: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal streaming setup error").into_response()
        })
}

async fn collect_to_json(
    stream: impl futures::Stream<Item = Result<Bytes, String>> + Send + 'static,
    email: String,
    mapped_model: String,
    reason: String,
) -> OpenAIStreamResult {
    use crate::proxy::mappers::openai::collect_openai_stream_to_json;
    use std::pin::pin;

    let mapped_stream = stream.map(|r| r.map_err(std::io::Error::other));
    let mut pinned = pin!(mapped_stream);

    match collect_openai_stream_to_json(&mut pinned).await {
        Ok(full_response) => {
            info!("[OpenAI] ✓ Stream collected and converted to JSON");
            OpenAIStreamResult::JsonResponse(
                StatusCode::OK,
                email,
                mapped_model,
                reason,
                full_response,
            )
        },
        Err(e) => OpenAIStreamResult::Retry(format!("Stream collection error: {}", e)),
    }
}
