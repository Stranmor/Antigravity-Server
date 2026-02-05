// Streaming response builder for completions handler
#![allow(clippy::expect_used, reason = "Response::builder() with valid headers cannot fail")]

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::StreamExt;
use std::pin::Pin;

pub fn build_streaming_response(
    first_chunk: Bytes,
    sse_stream: Pin<Box<dyn futures::Stream<Item = Result<Bytes, String>> + Send>>,
    email: &str,
    mapped_model: &str,
) -> Response {
    let combined_stream = Box::pin(
        futures::stream::once(async move { Ok(first_chunk) }).chain(sse_stream.map(
            |result: Result<Bytes, String>| -> Result<Bytes, std::io::Error> {
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
        )),
    );

    let body = Body::from_stream(combined_stream);
    Response::builder()
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("X-Account-Email", email)
        .header("X-Mapped-Model", mapped_model)
        .body(body)
        .expect("valid streaming response")
        .into_response()
}
