#![allow(deprecated)]

use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt};
use rand::Rng;
use serde_json::{json, Value};
use std::pin::Pin;

use super::function_call_handler::process_function_call;
use super::ssop_detector::detect_and_emit_ssop_events;
use super::usage::extract_usage_metadata;
#[allow(deprecated)]
use crate::proxy::mappers::signature_store::store_thought_signature;

pub fn create_codex_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    _model: String,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    let mut buffer = BytesMut::new();

    // Generate alphanumeric ID
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    let random_str: String = (0..24)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    let response_id = format!("resp-{}", random_str);

    let stream = async_stream::stream! {
        // 1. Emit response.created
        let created_ev = json!({
            "type": "response.created",
            "response": {
                "id": &response_id,
                "object": "response"
            }
        });
        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&created_ev).unwrap_or_default())));

        let mut full_content = String::new();
        let mut emitted_tool_calls = std::collections::HashSet::new();
        let mut last_finish_reason = "stop".to_string();

        while let Some(item) = gemini_stream.next().await {
            match item {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_raw = buffer.split_to(pos + 1);
                        if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                            let line = line_str.trim();
                            if line.is_empty() || !line.starts_with("data: ") { continue; }

                            let json_part = line.trim_start_matches("data: ").trim();
                            if json_part == "[DONE]" { continue; }

                            if let Ok(mut json) = serde_json::from_str::<Value>(json_part) {
                                let actual_data = if let Some(inner) = json.get_mut("response").map(|v| v.take()) { inner } else { json };

                                // Capture usageMetadata if present (for future use)
                                if let Some(u) = actual_data.get("usageMetadata") {
                                    let _ = extract_usage_metadata(u);
                                }

                                // Capture finish reason
                                if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                    if let Some(candidate) = candidates.first() {
                                        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
                                            last_finish_reason = match reason {
                                                "STOP" => "stop".to_string(),
                                                "MAX_TOKENS" => "length".to_string(),
                                                _ => "stop".to_string(),
                                            };
                                        }
                                    }
                                }

                                // text delta
                                let mut delta_text = String::new();
                                if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                    if let Some(candidate) = candidates.first() {
                                        if let Some(parts) = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                            for part in parts {
                                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                    // Sanitize smart quotes to standard quotes for JSON compatibility
                                                    let clean_text = text.replace(['“', '”'], "\"");
                                                    delta_text.push_str(&clean_text);
                                                }
                                                /* Disable thought chain output to main text
                                                if let Some(thought_text) = part.get("thought").and_then(|t| t.as_str()) {
                                                    let clean_thought = thought_text.replace('"', "\"").replace('"', "\"");
                                                    // delta_text.push_str(&clean_thought);
                                                }
                                                */
                                                // Capture thoughtSignature (required for Gemini 3 tool calls)
                                                // Store to global state, no longer embed in user-visible text
                                                if let Some(sig) = part.get("thoughtSignature").or(part.get("thought_signature")).and_then(|s| s.as_str()) {
                                                    tracing::debug!("[Codex-SSE] capture thoughtSignature (length: {})", sig.len());
                                                    store_thought_signature(sig);
                                                }
                                                // Handle function call in chunk with deduplication
                                                if let Some(func_call) = part.get("functionCall") {
                                                    let call_key = serde_json::to_string(func_call).unwrap_or_default();
                                                    if emitted_tool_calls.insert(call_key) {
                                                        if let Some((added_bytes, done_bytes)) = process_function_call(func_call) {
                                                            yield Ok::<Bytes, String>(added_bytes);
                                                            yield Ok::<Bytes, String>(done_bytes);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if !delta_text.is_empty() {
                                    full_content.push_str(&delta_text);
                                    // 2. Emit response.output_text.delta
                                    let delta_ev = json!({
                                        "type": "response.output_text.delta",
                                        "delta": delta_text
                                    });
                                    yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&delta_ev).unwrap_or_default())));
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
                        "Codex stream error occurred"
                    );

                    // Send friendly error event (containing i18n_key for frontend translation)
                    let error_ev = json!({
                        "type": "error",
                        "error": {
                            "type": error_type,
                            "message": user_message,
                            "code": "stream_error",
                            "i18n_key": i18n_key
                        }
                    });
                    yield Ok(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&error_ev).unwrap_or_default())));
                    break;
                }
            }
        }

        // 3. Emit response.output_item.done
        let item_done_ev = json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": full_content
                    }
                ]
            }
        });
        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&item_done_ev).unwrap_or_default())));

        // SSOP: Check full_content for embedded JSON command signatures if no tools were emitted natively
        if emitted_tool_calls.is_empty() {
            let ssop_result = detect_and_emit_ssop_events(&full_content);
            for event_bytes in ssop_result.events {
                yield Ok::<Bytes, String>(event_bytes);
            }
        }

        // 4. Emit response.completed
        let completed_ev = json!({
            "type": "response.completed",
            "response": {
                "id": &response_id,
                "object": "response",
                "status": "completed",
                "finish_reason": last_finish_reason,
                "usage": {
                    "input_tokens": 0,
                    "input_tokens_details": { "cached_tokens": 0 },
                    "output_tokens": 0,
                    "output_tokens_details": { "reasoning_tokens": 0 },
                    "total_tokens": 0
                }
            }
        });
        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&completed_ev).unwrap_or_default())));
    };

    Box::pin(stream)
}
