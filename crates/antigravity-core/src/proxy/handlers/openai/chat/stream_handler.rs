use crate::proxy::mappers::openai::OpenAIResponse;
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::StreamExt;
use tracing::info;

use crate::proxy::handlers::retry_strategy::{peek_first_data_chunk, PeekConfig, PeekResult};
use crate::proxy::mappers::openai::streaming::create_openai_sse_stream;

pub enum StreamResult {
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
) -> StreamResult
where
    S: futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    let openai_stream = create_openai_sse_stream(Box::pin(gemini_stream), model, None);

    let peek_config = PeekConfig::openai();
    let (first_data_chunk, openai_stream) =
        match peek_first_data_chunk(openai_stream, &peek_config, trace_id).await {
            PeekResult::Data(bytes, stream) => (Some(bytes), stream),
            PeekResult::Retry(err) => return StreamResult::Retry(err),
        };

    match first_data_chunk {
        Some(bytes) => {
            let combined_stream = build_combined_stream(bytes, openai_stream);

            if client_wants_stream {
                StreamResult::StreamingResponse(build_sse_response(
                    combined_stream,
                    email,
                    mapped_model,
                    reason,
                ))
            } else {
                collect_to_json(combined_stream, email, mapped_model, reason).await
            }
        },
        None => StreamResult::EmptyStream,
    }
}

fn build_combined_stream(
    first_bytes: Bytes,
    rest: impl futures::Stream<Item = Result<Bytes, String>> + Send + 'static,
) -> impl futures::Stream<Item = Result<Bytes, String>> + Send + 'static {
    futures::stream::once(async move { Ok(first_bytes) }).chain(rest.map(
        |result| -> Result<Bytes, String> {
            match result {
                Ok(b) => Ok(b),
                Err(e) => {
                    let user_message = if e.contains("decoding") || e.contains("hyper") {
                        "Upstream server closed connection (overload). Please retry your request."
                    } else {
                        "Stream interrupted by upstream. Please retry your request."
                    };
                    tracing::warn!(
                        "Stream error during transmission: {} (user msg: {})",
                        e,
                        user_message
                    );
                    Ok(Bytes::from(format!(
                        "data: {{\"error\":{{\"message\":\"{}\",\"type\":\"server_error\",\"code\":\"overloaded\",\"param\":null}}}}\n\ndata: [DONE]\n\n",
                        user_message
                    )))
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
        .expect("valid streaming response")
        .into_response()
}

async fn collect_to_json(
    stream: impl futures::Stream<Item = Result<Bytes, String>> + Send + 'static,
    email: String,
    mapped_model: String,
    reason: String,
) -> StreamResult {
    use crate::proxy::mappers::openai::collect_openai_stream_to_json;
    use std::pin::pin;

    let mapped_stream = stream.map(|r| r.map_err(std::io::Error::other));
    let mut pinned = pin!(mapped_stream);

    match collect_openai_stream_to_json(&mut pinned).await {
        Ok(full_response) => {
            info!("[OpenAI] ✓ Stream collected and converted to JSON");
            StreamResult::JsonResponse(StatusCode::OK, email, mapped_model, reason, full_response)
        },
        Err(e) => StreamResult::Retry(format!("Stream collection error: {}", e)),
    }
}
