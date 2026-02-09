// OpenAI SSE stream transformation: byte buffer operations and index tracking.
// Buffer sizes are bounded by stream chunk sizes. Index operations are validated.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "SSE streaming: bounded buffer operations, validated indices"
)]

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::{Stream, StreamExt};
use serde_json::Value;
use std::pin::Pin;
use tracing::debug;
use uuid::Uuid;

use super::candidate_processor::{process_candidate, CandidateContext};
use super::stream_formatters::{error_chunk, sse_line};
use super::usage::extract_usage_metadata;
use crate::proxy::mappers::openai::models::OpenAIUsage;

pub fn create_openai_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    model: String,
    _estimated_tokens: Option<u32>,
    session_id: Option<String>,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    let mut buffer = BytesMut::new();
    const MAX_BUFFER_SIZE: usize = 50 * 1024 * 1024; // 50MB — supports 2K+ image generation (~12MB base64)
    let stream_id = format!("chatcmpl-{}", Uuid::new_v4());
    let created_ts = Utc::now().timestamp();

    let stream = async_stream::stream! {
        let mut emitted_tool_calls = std::collections::HashSet::new();
        let mut final_usage: Option<OpenAIUsage> = None;
        let mut accumulated_thinking = String::new();
        while let Some(item) = gemini_stream.next().await {
            match item {
                Ok(bytes) => {
                    debug!("[OpenAI-SSE] Received chunk: {} bytes", bytes.len());
                    buffer.extend_from_slice(&bytes);

                    if buffer.len() > MAX_BUFFER_SIZE {
                        tracing::error!("[OpenAI-SSE] Buffer exceeded {}MB limit, aborting stream", MAX_BUFFER_SIZE / 1024 / 1024);
                        let err = error_chunk(&stream_id, created_ts, &model, "buffer_overflow", "Response too large", "error.buffer_overflow");
                        yield Ok(Bytes::from(sse_line(&err)));
                        yield Ok(Bytes::from("data: [DONE]\n\n"));
                        break;
                    }

                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_raw = buffer.split_to(pos + 1);
                        if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                            let line = line_str.trim();
                            if line.is_empty() { continue; }

                            if line.starts_with("data: ") {
                                let json_part = line.trim_start_matches("data: ").trim();
                                if json_part == "[DONE]" { continue; }

                                if let Ok(mut json) = serde_json::from_str::<Value>(json_part) {
                                    tracing::debug!("Gemini SSE Chunk: {}", json_part);

                                    let actual_data = if let Some(inner) = json.get_mut("response").map(|v| v.take()) {
                                        inner
                                    } else {
                                        json
                                    };

                                    if let Some(u) = actual_data.get("usageMetadata") {
                                        final_usage = extract_usage_metadata(u);
                                    }

                                    if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                        for (idx, candidate) in candidates.iter().enumerate() {
                                            let mut ctx = CandidateContext {
                                                stream_id: &stream_id,
                                                created_ts,
                                                model: &model,
                                                session_id: &session_id,
                                                accumulated_thinking: &mut accumulated_thinking,
                                                emitted_tool_calls: &mut emitted_tool_calls,
                                            };
                                            let chunks = process_candidate(candidate, idx, &mut ctx);
                                            for chunk in chunks {
                                                yield Ok::<Bytes, String>(chunk);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    use crate::proxy::mappers::error_classifier::classify_stream_error;
                    let (error_type, user_message, i18n_key) = classify_stream_error(&e);

                    tracing::error!(
                        error_type = %error_type,
                        user_message = %user_message,
                        i18n_key = %i18n_key,
                        raw_error = %e,
                        "OpenAI stream error occurred"
                    );

                    let err = error_chunk(&stream_id, created_ts, &model, error_type, user_message, i18n_key);
                    yield Ok(Bytes::from(sse_line(&err)));
                    yield Ok(Bytes::from("data: [DONE]\n\n"));
                    break;
                }
            }
        }

        if let Some(usage) = final_usage {
            let u = super::stream_formatters::usage_chunk(&stream_id, created_ts, &model, &usage);
            yield Ok::<Bytes, String>(Bytes::from(sse_line(&u)));
        }

        yield Ok::<Bytes, String>(Bytes::from("data: [DONE]\n\n"));
    };

    Box::pin(stream)
}
