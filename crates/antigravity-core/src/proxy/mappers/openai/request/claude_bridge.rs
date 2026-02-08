//! Bridge converter: OpenAI request format → Claude request format.
//!
//! Used when Claude models (claude-opus-4-6, etc.) are requested through
//! the OpenAI `/v1/chat/completions` endpoint. Converts the OpenAI message
//! structure into Claude's native format so `transform_claude_request_in`
//! can produce the correct Gemini body with toolConfig, thinkingConfig, etc.

use crate::proxy::mappers::claude::claude_models::{
    ClaudeRequest, Message, MessageContent, SystemBlock, SystemPrompt, ThinkingConfig,
};
use crate::proxy::mappers::claude::claude_response::Tool;
use crate::proxy::mappers::claude::content_block::ContentBlock;

use super::super::models::{OpenAIContent, OpenAIContentBlock, OpenAIMessage, OpenAIRequest};

/// Convert an OpenAI chat completion request into a Claude Messages API request.
///
/// This enables Claude models requested via `/v1/chat/completions` to use
/// the Claude pipeline for body transformation (correct toolConfig, thinkingConfig, etc.)
/// while keeping the OpenAI response format unchanged.
pub fn openai_to_claude_request(req: &OpenAIRequest) -> ClaudeRequest {
    let mut system_texts: Vec<String> = Vec::new();
    let mut messages: Vec<Message> = Vec::new();

    let mut pending_tool_results: Vec<ContentBlock> = Vec::new();

    for msg in &req.messages {
        match msg.role.as_str() {
            "system" | "developer" => {
                flush_tool_results(&mut pending_tool_results, &mut messages);
                if let Some(text) = extract_text_content(&msg.content) {
                    system_texts.push(text);
                }
            },
            "assistant" => {
                flush_tool_results(&mut pending_tool_results, &mut messages);
                messages.push(convert_assistant_message(msg));
            },
            "tool" => {
                pending_tool_results.push(convert_tool_result_block(msg));
            },
            _ => {
                flush_tool_results(&mut pending_tool_results, &mut messages);
                messages.push(convert_user_message(msg));
            },
        }
    }
    flush_tool_results(&mut pending_tool_results, &mut messages);

    if let Some(inst) = &req.instructions {
        if !inst.is_empty() {
            system_texts.insert(0, inst.clone());
        }
    }

    let system = if system_texts.is_empty() {
        None
    } else if system_texts.len() == 1 {
        Some(SystemPrompt::String(system_texts.remove(0)))
    } else {
        Some(SystemPrompt::Array(
            system_texts
                .into_iter()
                .map(|text| SystemBlock { block_type: "text".to_string(), text })
                .collect(),
        ))
    };

    let tools = req.tools.as_ref().map(|tool_list| convert_tools(tool_list));

    let is_thinking = req.model.ends_with("-thinking");
    let thinking = if is_thinking {
        Some(ThinkingConfig { type_: "enabled".to_string(), budget_tokens: Some(10000) })
    } else {
        None
    };

    ClaudeRequest {
        model: req.model.clone(),
        messages,
        system,
        tools,
        stream: req.stream,
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
        top_k: None,
        thinking,
        metadata: None,
        output_config: None,
    }
}

fn extract_text_content(content: &Option<OpenAIContent>) -> Option<String> {
    let text = content.as_ref().map(|c| match c {
        OpenAIContent::String(s) => s.clone(),
        OpenAIContent::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                OpenAIContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    })?;
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn convert_user_message(msg: &OpenAIMessage) -> Message {
    let content = match &msg.content {
        Some(OpenAIContent::String(s)) => MessageContent::String(s.clone()),
        Some(OpenAIContent::Array(blocks)) => {
            let claude_blocks: Vec<ContentBlock> =
                blocks.iter().filter_map(convert_content_block).collect();
            if claude_blocks.len() == 1 {
                if let ContentBlock::Text { ref text } = claude_blocks[0] {
                    return Message {
                        role: "user".to_string(),
                        content: MessageContent::String(text.clone()),
                    };
                }
            }
            MessageContent::Array(claude_blocks)
        },
        None => MessageContent::String(String::new()),
    };
    Message { role: "user".to_string(), content }
}

fn convert_assistant_message(msg: &OpenAIMessage) -> Message {
    let mut blocks: Vec<ContentBlock> = Vec::new();

    if let Some(reasoning) = &msg.reasoning_content {
        if !reasoning.is_empty() {
            blocks.push(ContentBlock::Thinking {
                thinking: reasoning.clone(),
                signature: None,
                cache_control: None,
            });
        }
    }

    match &msg.content {
        Some(OpenAIContent::String(s)) => {
            if !s.is_empty() {
                blocks.push(ContentBlock::Text { text: s.clone() });
            }
        },
        Some(OpenAIContent::Array(arr)) => {
            for b in arr {
                if let Some(cb) = convert_content_block(b) {
                    blocks.push(cb);
                }
            }
        },
        None => {},
    }

    if let Some(tool_calls) = &msg.tool_calls {
        for tc in tool_calls {
            let input: serde_json::Value = match serde_json::from_str(&tc.function.arguments) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        "[Claude-Bridge] Malformed tool call arguments for '{}': {}",
                        tc.function.name,
                        e
                    );
                    serde_json::json!({})
                },
            };
            blocks.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                input,
                signature: None,
                cache_control: None,
            });
        }
    }

    if blocks.is_empty() {
        blocks.push(ContentBlock::Text { text: String::new() });
    }

    Message { role: "assistant".to_string(), content: MessageContent::Array(blocks) }
}

fn flush_tool_results(pending: &mut Vec<ContentBlock>, messages: &mut Vec<Message>) {
    if pending.is_empty() {
        return;
    }
    let blocks = std::mem::take(pending);
    messages.push(Message { role: "user".to_string(), content: MessageContent::Array(blocks) });
}

fn convert_tool_result_block(msg: &OpenAIMessage) -> ContentBlock {
    let tool_use_id = msg.tool_call_id.clone().unwrap_or_default();
    let text = match &msg.content {
        Some(OpenAIContent::String(s)) => s.clone(),
        Some(OpenAIContent::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| match b {
                OpenAIContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => String::new(),
    };

    let content_value = serde_json::json!([{"type": "text", "text": text}]);

    ContentBlock::ToolResult { tool_use_id, content: content_value, is_error: None }
}

fn convert_content_block(block: &OpenAIContentBlock) -> Option<ContentBlock> {
    match block {
        OpenAIContentBlock::Text { text } => Some(ContentBlock::Text { text: text.clone() }),
        OpenAIContentBlock::ImageUrl { image_url } => {
            if let Some(rest) = image_url.url.strip_prefix("data:") {
                if let Some((media_and_enc, data)) = rest.split_once(',') {
                    // Only accept base64-encoded data URIs (RFC 2397)
                    if let Some(media_with_params) = media_and_enc.strip_suffix(";base64") {
                        // Strip MIME parameters (e.g., "image/png;charset=utf-8" → "image/png")
                        let media_type = media_with_params
                            .split_once(';')
                            .map_or(media_with_params, |(mime, _)| mime);
                        if media_type.starts_with("image/") {
                            return Some(ContentBlock::Image {
                                source: crate::proxy::mappers::claude::content_block::ImageSource {
                                    source_type: "base64".to_string(),
                                    media_type: media_type.to_string(),
                                    data: data.to_string(),
                                },
                                cache_control: None,
                            });
                        }
                    }
                }
            }
            Some(ContentBlock::Text { text: format!("[Image: {}]", image_url.url) })
        },
        // Audio and video are not supported by Claude — skip
        OpenAIContentBlock::InputAudio { .. } | OpenAIContentBlock::VideoUrl { .. } => None,
    }
}

fn convert_tools(tools: &[serde_json::Value]) -> Vec<Tool> {
    tools
        .iter()
        .filter_map(|tool_val| {
            let func = tool_val.get("function").or(Some(tool_val));
            let name = func?.get("name")?.as_str()?.to_string();
            let description = func?.get("description").and_then(|d| d.as_str()).map(String::from);
            let input_schema = func?.get("parameters").cloned();

            Some(Tool { type_: None, name: Some(name), description, input_schema })
        })
        .collect()
}
