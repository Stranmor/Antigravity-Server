// SSE stream transformation: Gemini → Claude format
// Handles streaming, heartbeat, and SSE line processing

use super::models::{GeminiPart, UsageMetadata};
use super::streaming::{PartProcessor, StreamingState};
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

/// Create Gemini SSE stream to Claude SSE stream converter
pub fn create_claude_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    trace_id: String,
    email: String,
    session_id: Option<String>,
    scaling_enabled: bool,
    context_limit: u32,
    estimated_tokens: Option<u32>,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    use async_stream::stream;
    use bytes::BytesMut;
    use futures::StreamExt;

    Box::pin(stream! {
        let mut state = StreamingState::new();
        state.session_id = session_id;
        state.scaling_enabled = scaling_enabled;
        state.context_limit = context_limit;
        state.estimated_tokens = estimated_tokens;
        let mut buffer = BytesMut::new();

        loop {
            // 15 second heartbeat keepalive: if no data for long time, send ping packet
            let next_chunk = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                gemini_stream.next()
            ).await;

            match next_chunk {
                Ok(Some(chunk_result)) => {
                    match chunk_result {
                        Ok(chunk) => {
                            buffer.extend_from_slice(&chunk);

                            // Process complete lines
                            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                                let line_raw = buffer.split_to(pos + 1);
                                if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                                    let line = line_str.trim();
                                    if line.is_empty() { continue; }

                                    if let Some(sse_chunks) = process_sse_line(line, &mut state, &trace_id, &email) {
                                        for sse_chunk in sse_chunks {
                                            yield Ok(sse_chunk);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            yield Err(format!("Stream error: {}", e));
                            break;
                        }
                    }
                }
                Ok(None) => break, // Stream ended normally
                Err(_) => {
                    // Timeout, send heartbeat packet (SSE Comment format)
                    yield Ok(Bytes::from(": ping\n\n"));
                }
            }
        }

        // Ensure termination events are sent
        for chunk in emit_force_stop(&mut state) {
            yield Ok(chunk);
        }
    })
}

/// Handle SSE data line
fn process_sse_line(
    line: &str,
    state: &mut StreamingState,
    trace_id: &str,
    email: &str,
) -> Option<Vec<Bytes>> {
    if !line.starts_with("data: ") {
        return None;
    }

    let data_str = line[6..].trim();
    if data_str.is_empty() {
        return None;
    }

    if data_str == "[DONE]" {
        let chunks = emit_force_stop(state);
        if chunks.is_empty() {
            return None;
        }
        return Some(chunks);
    }

    // parse JSON
    let json_value: serde_json::Value = match serde_json::from_str(data_str) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let mut chunks = Vec::new();

    // unwrap response field (ifexist)
    let raw_json = json_value.get("response").unwrap_or(&json_value);

    // send message_start
    if !state.message_start_sent {
        chunks.push(state.emit_message_start(raw_json));
    }

    // capture groundingMetadata (Web Search)
    if let Some(candidate) = raw_json.get("candidates").and_then(|c| c.get(0)) {
        if let Some(grounding) = candidate.get("groundingMetadata") {
            // Extract search query
            if let Some(query) = grounding
                .get("webSearchQueries")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
            {
                state.web_search_query = Some(query.to_string());
            }

            // Extract result blocks
            if let Some(chunks_arr) = grounding.get("groundingChunks").and_then(|v| v.as_array()) {
                state.grounding_chunks = Some(chunks_arr.clone());
            } else if let Some(chunks_arr) = grounding
                .get("grounding_metadata")
                .and_then(|m| m.get("groundingChunks"))
                .and_then(|v| v.as_array())
            {
                state.grounding_chunks = Some(chunks_arr.clone());
            }
        }
    }

    // Handle all parts
    if let Some(parts) = raw_json
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|cand| cand.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|p| p.as_array())
    {
        for part_value in parts {
            if let Ok(part) = serde_json::from_value::<GeminiPart>(part_value.clone()) {
                let mut processor = PartProcessor::new(state);
                chunks.extend(processor.process(&part));
            }
        }
    }

    // Check if ended
    if let Some(finish_reason) = raw_json
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|cand| cand.get("finishReason"))
        .and_then(|f| f.as_str())
    {
        let usage = raw_json
            .get("usageMetadata")
            .and_then(|u| serde_json::from_value::<UsageMetadata>(u.clone()).ok());

        if let Some(ref u) = usage {
            let cached_tokens = u.cached_content_token_count.unwrap_or(0);
            let cache_info = if cached_tokens > 0 {
                format!(", Cached: {}", cached_tokens)
            } else {
                String::new()
            };

            let thinking_info = if state.has_thinking_received() { " | Thinking: ✓" } else { "" };

            tracing::info!(
                "[{}] ✓ Stream completed | Account: {} | In: {} tokens | Out: {} tokens{}{} | Reason: {}",
                trace_id,
                email,
                u.prompt_token_count
                    .unwrap_or(0)
                    .saturating_sub(cached_tokens),
                u.candidates_token_count.unwrap_or(0),
                cache_info,
                thinking_info,
                finish_reason
            );

            // [2026-02-07] Response-side thinking validation
            // If the model is a thinking model but no thinking was received,
            // this indicates upstream silently ignored our thinkingConfig.
            if let Some(ref model) = state.model_name {
                let is_thinking_model = model.contains("thinking")
                    || model.contains("pro-2.5")
                    || model.contains("flash-2.5");
                if is_thinking_model && !state.has_thinking_received() {
                    let out_tokens = u.candidates_token_count.unwrap_or(0);
                    tracing::debug!(
                        "[{}] Thinking model responded without thinking | Model: {} | \
                         Output tokens: {} (model decided thinking was not needed).",
                        trace_id,
                        model,
                        out_tokens
                    );
                }
                if is_thinking_model && state.has_thinking_received() {
                    // Check if we have a valid signature cached
                    if let Some(ref sid) = state.session_id {
                        let has_sig = crate::proxy::SignatureCache::global()
                            .get_session_signature(sid)
                            .is_some();
                        if has_sig {
                            tracing::debug!(
                                "[{}] ✓ Thinking response validated: signature cached for session {}",
                                trace_id,
                                sid
                            );
                        } else {
                            tracing::warn!(
                                "[{}] ⚠ Thinking response received but no signature cached for session {}. \
                                 Next request may fail signature validation.",
                                trace_id,
                                sid
                            );
                        }
                    }
                }
            }
        }

        chunks.extend(state.emit_finish(Some(finish_reason), usage.as_ref()));
    }

    if chunks.is_empty() {
        None
    } else {
        Some(chunks)
    }
}

/// Send force end event
pub fn emit_force_stop(state: &mut StreamingState) -> Vec<Bytes> {
    if !state.message_stop_sent {
        let mut chunks = state.emit_finish(None, None);
        if chunks.is_empty() {
            chunks.push(Bytes::from("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));
            state.message_stop_sent = true;
        }
        return chunks;
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_sse_line_done() {
        let mut state = StreamingState::new();
        let result = process_sse_line("data: [DONE]", &mut state, "test_id", "test@example.com");
        assert!(result.is_some());
        let chunks = result.unwrap();
        assert!(!chunks.is_empty());

        let all_text: String =
            chunks.iter().map(|b| String::from_utf8(b.to_vec()).unwrap_or_default()).collect();
        assert!(all_text.contains("message_stop"));
    }

    #[test]
    fn test_process_sse_line_with_text() {
        let mut state = StreamingState::new();

        let test_data = r#"data: {"candidates":[{"content":{"parts":[{"text":"Hello"}]}}],"usageMetadata":{},"modelVersion":"test","responseId":"123"}"#;

        let result = process_sse_line(test_data, &mut state, "test_id", "test@example.com");
        assert!(result.is_some());

        let chunks = result.unwrap();
        assert!(!chunks.is_empty());

        // Should contain message_start and text delta
        let all_text: String =
            chunks.iter().map(|b| String::from_utf8(b.to_vec()).unwrap_or_default()).collect();

        assert!(all_text.contains("message_start"));
        assert!(all_text.contains("content_block_start"));
        assert!(all_text.contains("Hello"));
    }
}
