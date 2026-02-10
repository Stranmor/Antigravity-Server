#![allow(
    deprecated,
    reason = "signature_store API uses deprecated snake_case naming for upstream compatibility"
)]

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::{Stream, StreamExt};
use rand::Rng;
use serde_json::{json, Value};
use std::pin::Pin;

use super::usage::extract_usage_metadata;
use crate::proxy::mappers::openai::models::OpenAIUsage;
use crate::proxy::mappers::signature_store::store_thought_signature;

pub fn create_legacy_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    model: String,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    let mut buffer = BytesMut::new();

    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    let random_str: String = (0..28)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    let stream_id = format!("cmpl-{}", random_str);
    let created_ts = Utc::now().timestamp();

    let stream = async_stream::stream! {
        let mut final_usage: Option<OpenAIUsage> = None;
        while let Some(item) = gemini_stream.next().await {
            match item {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_raw = buffer.split_to(pos + 1);
                        if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                            let line = line_str.trim();
                            if line.is_empty() { continue; }

                            if line.starts_with("data: ") {
                                let json_part = line.trim_start_matches("data: ").trim();
                                if json_part == "[DONE]" { continue; }

                                if let Ok(mut json) = serde_json::from_str::<Value>(json_part) {
                                    let actual_data = if let Some(inner) = json.get_mut("response").map(|v| v.take()) { inner } else { json };

                                    if let Some(u) = actual_data.get("usageMetadata") {
                                        final_usage = extract_usage_metadata(u);
                                    }

                                    let mut content_out = String::new();
                                    if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                        if let Some(parts) = candidates.first().and_then(|c| c.get("content")).and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                            for part in parts {
                                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                    content_out.push_str(text);
                                                }
                                                if let Some(sig) = part.get("thoughtSignature").or(part.get("thought_signature")).and_then(|s| s.as_str()) {
                                                    store_thought_signature(sig);
                                                }
                                            }
                                        }
                                    }

                                    let finish_reason = actual_data.get("candidates")
                                        .and_then(|c| c.as_array())
                                        .and_then(|c| c.first())
                                        .and_then(|c| c.get("finishReason"))
                                        .and_then(|f| f.as_str())
                                        .map(|f| match f {
                                            "STOP" => "stop",
                                            "MAX_TOKENS" => "length",
                                            "SAFETY" => "content_filter",
                                            _ => f,
                                        });

                                    let legacy_chunk = json!({
                                        "id": &stream_id,
                                        "object": "text_completion",
                                        "created": created_ts,
                                        "model": &model,
                                        "choices": [
                                            {
                                                "text": content_out,
                                                "index": 0,
                                                "logprobs": null,
                                                "finish_reason": finish_reason
                                            }
                                        ]
                                    });

                                    let json_str = serde_json::to_string(&legacy_chunk).unwrap_or_default();
                                    tracing::debug!("Legacy Stream Chunk: {}", json_str);
                                    let sse_out = format!("data: {}\n\n", json_str);
                                    yield Ok::<Bytes, String>(Bytes::from(sse_out));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Legacy stream error (graceful finish): {}", e);
                    crate::proxy::prometheus::record_stream_graceful_finish("openai_legacy");
                    // Emit graceful completion instead of error event to prevent
                    // AI agents from endlessly retrying truncated responses
                    let finish_chunk = json!({
                        "id": &stream_id,
                        "object": "text_completion",
                        "created": created_ts,
                        "model": &model,
                        "choices": [{
                            "text": "",
                            "index": 0,
                            "logprobs": null,
                            "finish_reason": "length"
                        }]
                    });
                    let sse_out = format!("data: {}\n\n", serde_json::to_string(&finish_chunk).unwrap_or_default());
                    yield Ok(Bytes::from(sse_out));
                    break;
                }
            }
        }

        if let Some(usage) = final_usage {
            let usage_chunk = json!({
                "id": &stream_id,
                "object": "text_completion",
                "created": created_ts,
                "model": &model,
                "choices": [],
                "usage": usage
            });
            let sse_out = format!("data: {}\n\n", serde_json::to_string(&usage_chunk).unwrap_or_default());
            yield Ok::<Bytes, String>(Bytes::from(sse_out));
        }

        tracing::debug!("Stream finished. Yielding [DONE]");
        yield Ok::<Bytes, String>(Bytes::from("data: [DONE]\n\n"));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    Box::pin(stream)
}
