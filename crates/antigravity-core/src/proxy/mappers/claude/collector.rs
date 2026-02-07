// Stream collector - converts SSE stream to complete JSON response
// For non-stream request auto-conversion

use super::models::*;
use bytes::Bytes;
use futures::StreamExt;
use serde_json::{json, Value};
use std::io;

/// SSE event type
#[derive(Debug, Clone)]
struct SseEvent {
    event_type: String,
    data: Value,
}

/// Parse SSE line
fn parse_sse_line(line: &str) -> Option<(String, String)> {
    if let Some(colon_pos) = line.find(':') {
        let key = &line[..colon_pos];
        let value = line[colon_pos + 1..].trim_start();
        Some((key.to_string(), value.to_string()))
    } else {
        None
    }
}

/// Collect SSE Stream as complete Claude Response
///
/// This function receives an SSE byte stream, parses all events, and reconstructs a complete ClaudeResponse object.
/// This allows non-stream clients to transparently enjoy stream mode quota advantages.
pub async fn collect_stream_to_json<S>(mut stream: S) -> Result<ClaudeResponse, String>
where
    S: futures::Stream<Item = Result<Bytes, io::Error>> + Unpin,
{
    let mut events = Vec::new();
    let mut current_event_type = String::new();
    let mut current_data = String::new();

    // 1. Collect all SSE events
    let mut line_buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        let text = String::from_utf8_lossy(&chunk);

        line_buffer.push_str(&text);

        while let Some(newline_pos) = line_buffer.find('\n') {
            let line = line_buffer[..newline_pos].trim_end_matches('\r').to_string();
            line_buffer = line_buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                if !current_data.is_empty() {
                    if let Ok(data) = serde_json::from_str::<Value>(&current_data) {
                        events.push(SseEvent { event_type: current_event_type.clone(), data });
                    }
                    current_event_type.clear();
                    current_data.clear();
                }
            } else if let Some((key, value)) = parse_sse_line(&line) {
                match key.as_str() {
                    "event" => current_event_type = value,
                    "data" => current_data = value,
                    // Intentionally ignored: only "event" and "data" SSE fields are used
                    _ => {},
                }
            }
        }
    }

    // 2. Reconstruct ClaudeResponse
    let mut response = ClaudeResponse {
        id: "msg_unknown".to_string(),
        type_: "message".to_string(),
        role: "assistant".to_string(),
        model: String::new(),
        content: Vec::new(),
        stop_reason: "end_turn".to_string(),
        stop_sequence: None,
        usage: Usage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            server_tool_use: None,
        },
    };

    // For accumulating content blocks
    let mut current_text = String::new();
    let mut current_thinking = String::new();
    let mut current_signature: Option<String> = None;
    let mut current_tool_use: Option<Value> = None;
    let mut current_tool_input = String::new();

    for event in events {
        match event.event_type.as_str() {
            "message_start" => {
                // Extract basic info
                if let Some(message) = event.data.get("message") {
                    if let Some(id) = message.get("id").and_then(|v| v.as_str()) {
                        response.id = id.to_string();
                    }
                    if let Some(model) = message.get("model").and_then(|v| v.as_str()) {
                        response.model = model.to_string();
                    }
                    if let Some(usage) = message.get("usage") {
                        if let Ok(u) = serde_json::from_value::<Usage>(usage.clone()) {
                            response.usage = u;
                        }
                    }
                }
            },

            "content_block_start" => {
                if let Some(content_block) = event.data.get("content_block") {
                    if let Some(block_type) = content_block.get("type").and_then(|v| v.as_str()) {
                        match block_type {
                            "text" => current_text.clear(),
                            "thinking" => {
                                current_thinking.clear();
                                // Extract signature from content_block
                                current_signature = content_block
                                    .get("signature")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                            },
                            "tool_use" => {
                                current_tool_use = Some(content_block.clone());
                                current_tool_input.clear();
                            },
                            // Intentionally ignored: only text/thinking/tool_use block types need accumulation
                            _ => {},
                        }
                    }
                }
            },

            "content_block_delta" => {
                if let Some(delta) = event.data.get("delta") {
                    if let Some(delta_type) = delta.get("type").and_then(|v| v.as_str()) {
                        match delta_type {
                            "text_delta" => {
                                if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                    current_text.push_str(text);
                                }
                            },
                            "thinking_delta" => {
                                if let Some(thinking) =
                                    delta.get("thinking").and_then(|v| v.as_str())
                                {
                                    current_thinking.push_str(thinking);
                                }
                                // In case signature comes in delta (less likely but possible update)
                                if let Some(sig) = delta.get("signature").and_then(|v| v.as_str()) {
                                    current_signature = Some(sig.to_string());
                                }
                            },
                            "input_json_delta" => {
                                if let Some(partial_json) =
                                    delta.get("partial_json").and_then(|v| v.as_str())
                                {
                                    current_tool_input.push_str(partial_json);
                                }
                            },
                            // Intentionally ignored: only text_delta/thinking_delta/input_json_delta carry content
                            _ => {},
                        }
                    }
                }
            },

            "content_block_stop" => {
                // Complete current block
                if !current_text.is_empty() {
                    response.content.push(ContentBlock::Text { text: current_text.clone() });
                    current_text.clear();
                } else if !current_thinking.is_empty() {
                    response.content.push(ContentBlock::Thinking {
                        thinking: current_thinking.clone(),
                        signature: current_signature.take(),
                        cache_control: None,
                    });
                    current_thinking.clear();
                } else if let Some(tool_use) = current_tool_use.take() {
                    // build tool_use block
                    let id = tool_use
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let name = tool_use
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let input = if !current_tool_input.is_empty() {
                        serde_json::from_str(&current_tool_input).unwrap_or(json!({}))
                    } else {
                        json!({})
                    };

                    response.content.push(ContentBlock::ToolUse {
                        id,
                        name,
                        input,
                        signature: None,
                        cache_control: None,
                    });
                    current_tool_input.clear();
                }
            },

            "message_delta" => {
                if let Some(delta) = event.data.get("delta") {
                    if let Some(stop_reason) = delta.get("stop_reason").and_then(|v| v.as_str()) {
                        response.stop_reason = stop_reason.to_string();
                    }
                }
                if let Some(usage) = event.data.get("usage") {
                    if let Ok(u) = serde_json::from_value::<Usage>(usage.clone()) {
                        response.usage = u;
                    }
                }
            },

            "message_stop" => {
                // Stream end
                break;
            },

            "error" => {
                // Error event
                return Err(format!("Stream error: {:?}", event.data));
            },

            _ => {
                // Ignore unknown event types
            },
        }
    }

    Ok(response)
}
