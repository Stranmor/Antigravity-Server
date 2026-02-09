// SSE stream transformation: Gemini → Claude format
// Handles streaming, heartbeat, and SSE line processing

use super::models::{GeminiPart, UsageMetadata};
use super::streaming::{PartProcessor, StreamingState};
use super::thinking_validation::validate_thinking_response;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

/// Guard that aborts a spawned task when dropped (client disconnect cleanup)
struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

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
    use tokio::time::MissedTickBehavior;

    Box::pin(stream! {
        let mut state = StreamingState::new();
        state.session_id = session_id;
        state.scaling_enabled = scaling_enabled;
        state.context_limit = context_limit;
        state.estimated_tokens = estimated_tokens;
        let mut buffer = BytesMut::new();
        const MAX_BUFFER_SIZE: usize = 10 * 1024 * 1024;

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Bytes, reqwest::Error>>(2);

        let pump = tokio::spawn(async move {
            while let Some(item) = gemini_stream.next().await {
                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        let _pump_guard = AbortOnDrop(pump);

        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(15));
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        heartbeat.tick().await;

        loop {
            tokio::select! {
                maybe_chunk = rx.recv() => {
                    match maybe_chunk {
                        Some(Ok(chunk)) => {
                            buffer.extend_from_slice(&chunk);

                            if buffer.len() > MAX_BUFFER_SIZE {
                                tracing::error!("[{}] SSE buffer exceeded {}MB limit, aborting stream", trace_id, MAX_BUFFER_SIZE / 1024 / 1024);
                                yield Err("SSE buffer overflow: response too large".to_string());
                                break;
                            }

                            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                                let line_raw = buffer.split_to(pos + 1);
                                let line_str = match std::str::from_utf8(&line_raw) {
                                    Ok(s) => s,
                                    Err(e) => {
                                        tracing::warn!("[{}] SSE line UTF-8 decode error: {} | {} bytes", trace_id, e, line_raw.len());
                                        continue;
                                    }
                                };
                                let line = line_str.trim();
                                    if line.is_empty() { continue; }

                                    if let Some(sse_chunks) = process_sse_line(line, &mut state, &trace_id, &email) {
                                        for sse_chunk in sse_chunks {
                                            yield Ok(sse_chunk);
                                        }
                                    }
                            }
                        }
                        Some(Err(e)) => {
                            state.stream_errored = true;
                            yield Err(format!("Stream error: {}", e));
                            break;
                        }
                        None => break,
                    }
                }
                _ = heartbeat.tick() => {
                    yield Ok(Bytes::from(": ping\n\n"));
                }
            }
        }

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
        Err(e) => {
            tracing::warn!(
                "[{}] SSE JSON parse error: {} | data: {}",
                trace_id,
                e,
                &data_str[..data_str.len().min(200)]
            );
            return None;
        },
    };

    let mut chunks = Vec::new();

    // unwrap response field (ifexist)
    let raw_json = json_value.get("response").unwrap_or(&json_value);

    if let Some(error) = raw_json.get("error") {
        tracing::error!("[{}] Upstream error in SSE stream: {}", trace_id, error);
        let error_event = format!(
            "event: error\ndata: {{\"type\":\"overloaded_error\",\"error\":{{\"type\":\"overloaded_error\",\"message\":\"Upstream error: {}\"}}}}\n\n",
            error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown upstream error")
        );
        return Some(vec![Bytes::from(error_event)]);
    }

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
            match serde_json::from_value::<GeminiPart>(part_value.clone()) {
                Ok(part) => {
                    let mut processor = PartProcessor::new(state);
                    chunks.extend(processor.process(&part));
                },
                Err(e) => {
                    tracing::warn!(
                        "[{}] Failed to deserialize GeminiPart: {} | part: {}",
                        trace_id,
                        e,
                        part_value
                    );
                },
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
            let input_tokens = u.prompt_token_count.unwrap_or(0);
            let output_tokens = u.candidates_token_count.unwrap_or(0);

            tracing::info!(
                "[{}] ✓ Stream completed | Account: {} | Reason: {}",
                trace_id,
                email,
                finish_reason
            );

            validate_thinking_response(
                state,
                trace_id,
                finish_reason,
                input_tokens,
                output_tokens,
                cached_tokens,
            );
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
    if state.stream_errored {
        return vec![];
    }
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
    use crate::proxy::mappers::claude::streaming::BlockType;
    use serde_json::json;

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

    #[test]
    fn test_emit_force_stop_truncation_emits_error_event() {
        let mut state = StreamingState::new();

        state.start_block(BlockType::Text, json!({ "type": "text", "text": "" }));

        let chunks = emit_force_stop(&mut state);
        let all_text: String =
            chunks.iter().map(|b| String::from_utf8(b.to_vec()).unwrap_or_default()).collect();

        assert!(all_text.contains("event: error"));
        assert!(all_text.contains("stream_truncated"));
        assert!(all_text.contains("message_stop"));
        assert!(!all_text.contains("max_tokens"));
    }

    #[test]
    fn test_emit_force_stop_skips_when_stream_errored() {
        let mut state = StreamingState::new();
        state.start_block(BlockType::Text, json!({ "type": "text", "text": "" }));
        state.stream_errored = true;

        let chunks = emit_force_stop(&mut state);
        assert!(chunks.is_empty());
    }
}
