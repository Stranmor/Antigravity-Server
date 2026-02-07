use super::super::models::*;
use super::content_parts::transform_content_block;
use crate::proxy::mappers::tool_result_compressor;
use crate::proxy::SignatureCache;
use serde_json::{json, Value};
use std::collections::HashMap;

#[allow(
    dead_code,
    reason = "Fields used for struct construction, may be needed for future validation"
)]
pub struct MessageTransformContext<'a> {
    pub global_thought_sig: &'a Option<String>,
    pub actual_include_thinking: bool,
    pub is_thinking_model: bool,
    pub is_claude_thinking: bool,
    pub mapped_model: &'a str,
    pub tool_id_to_name: &'a HashMap<String, String>,
    pub tool_name_to_schema: &'a HashMap<String, Value>,
}

pub fn transform_message(msg: &OpenAIMessage, ctx: &MessageTransformContext<'_>) -> Value {
    let role = match msg.role.as_str() {
        "assistant" => "model",
        "tool" | "function" => "user",
        _ => &msg.role,
    };

    let mut parts = Vec::new();

    transform_reasoning_content(msg, role, ctx, &mut parts);
    transform_content(msg, &mut parts);
    transform_tool_calls(msg, ctx, &mut parts);
    transform_tool_response(msg, ctx, &mut parts);

    json!({ "role": role, "parts": parts })
}

fn transform_reasoning_content(
    msg: &OpenAIMessage,
    role: &str,
    ctx: &MessageTransformContext<'_>,
    parts: &mut Vec<Value>,
) {
    if let Some(reasoning) = &msg.reasoning_content {
        if !reasoning.is_empty() {
            // Try to recover signature from content cache
            if let Some((sig, family)) = SignatureCache::global().get_content_signature(reasoning) {
                tracing::info!(
                    "[OpenAI-Thinking] Recovered signature from CONTENT cache (len={}, family={})",
                    sig.len(),
                    family
                );
                let thought_part = json!({
                    "text": reasoning,
                    "thought": true,
                    "thoughtSignature": sig
                });
                parts.push(thought_part);
            } else if let Some(ref sig) = ctx.global_thought_sig {
                let thought_part = json!({
                    "text": reasoning,
                    "thought": true,
                    "thoughtSignature": sig
                });
                parts.push(thought_part);
            } else {
                tracing::warn!(
                    "[OpenAI-Thinking] No signature available, downgrading reasoning_content to text"
                );
                parts.push(json!({"text": reasoning}));
            }
        }
    } else if ctx.actual_include_thinking && role == "model" {
        // [FIX] We cannot inject placeholder thinking blocks without a valid signature.
        // The API will reject with "signature: Field required".
        if ctx.is_claude_thinking && ctx.global_thought_sig.is_none() {
            tracing::debug!(
                "[OpenAI-Thinking] Skipping placeholder thinking block for Claude - no signature available"
            );
            // Don't inject thinking block for Claude without signature
        } else if let Some(ref sig) = ctx.global_thought_sig {
            tracing::debug!(
                "[OpenAI-Thinking] Injecting placeholder thinking block with signature for assistant message"
            );
            let thought_part = json!({
                "text": "Applying tool decisions and generating response...",
                "thought": true,
                "thoughtSignature": sig,
            });
            parts.push(thought_part);
        } else {
            tracing::debug!(
                "[OpenAI-Thinking] No signature available, skipping placeholder thinking block"
            );
        }
    }
}

fn transform_content(msg: &OpenAIMessage, parts: &mut Vec<Value>) {
    let is_tool_role = msg.role == "tool" || msg.role == "function";
    if let (Some(content), false) = (&msg.content, is_tool_role) {
        match content {
            OpenAIContent::String(s) => {
                if !s.is_empty() {
                    parts.push(json!({"text": s}));
                }
            },
            OpenAIContent::Array(blocks) => {
                for block in blocks {
                    if let Some(part) = transform_content_block(block) {
                        parts.push(part);
                    }
                }
            },
        }
    }
}

fn transform_tool_calls(
    msg: &OpenAIMessage,
    ctx: &MessageTransformContext<'_>,
    parts: &mut Vec<Value>,
) {
    if let Some(tool_calls) = &msg.tool_calls {
        for tc in tool_calls.iter() {
            let mut args =
                serde_json::from_str::<Value>(&tc.function.arguments).unwrap_or(json!({}));

            if let Some(original_schema) = ctx.tool_name_to_schema.get(&tc.function.name) {
                crate::proxy::common::json_schema::fix_tool_call_args(&mut args, original_schema);
            }

            if tc.function.name == "local_shell_call" {
                if let Some(command) = args.get_mut("command") {
                    if let Value::String(s) = command {
                        tracing::info!(
                            "[OpenAI-Request] Converting shell command string to array: {}",
                            s
                        );
                        *command = json!([s.clone()]);
                    }
                }
            }

            let mut func_call_part = json!({
                "functionCall": {
                    "name": if tc.function.name == "local_shell_call" { "shell" } else { &tc.function.name },
                    "args": args,
                    "id": &tc.id,
                }
            });

            if let Some(ref sig) = ctx.global_thought_sig {
                func_call_part["thoughtSignature"] = json!(sig);
            } else {
                tracing::debug!(
                    "[OpenAI-Signature] No signature available for tool_use: {}. Omitting thoughtSignature.",
                    tc.id
                );
            }

            parts.push(func_call_part);
        }
    }
}

fn transform_tool_response(
    msg: &OpenAIMessage,
    ctx: &MessageTransformContext<'_>,
    parts: &mut Vec<Value>,
) {
    if msg.role == "tool" || msg.role == "function" {
        let name = msg.name.as_deref().unwrap_or("unknown");
        let final_name = if name == "local_shell_call" {
            "shell"
        } else if let Some(id) = &msg.tool_call_id {
            ctx.tool_id_to_name.get(id).map(|s| s.as_str()).unwrap_or(name)
        } else {
            name
        };

        let raw_content = match &msg.content {
            Some(OpenAIContent::String(s)) => s.clone(),
            Some(OpenAIContent::Array(blocks)) => blocks
                .iter()
                .filter_map(|b| {
                    if let OpenAIContentBlock::Text { text } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
            None => String::new(),
        };

        const MAX_TOOL_RESULT_CHARS: usize = 200_000;
        let content_val =
            tool_result_compressor::compact_tool_result_text(&raw_content, MAX_TOOL_RESULT_CHARS);

        if content_val.len() < raw_content.len() {
            tracing::debug!(
                "[OpenAI-Request] Compressed tool result from {} to {} chars",
                raw_content.len(),
                content_val.len()
            );
        }

        parts.push(json!({
            "functionResponse": {
               "name": final_name,
               "response": { "result": content_val },
               "id": msg.tool_call_id.clone().unwrap_or_default()
            }
        }));
    }
}

pub fn merge_consecutive_roles(contents: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for msg in contents {
        if let Some(last) = merged.last_mut() {
            if last["role"] == msg["role"] {
                if let (Some(last_parts), Some(msg_parts)) =
                    (last["parts"].as_array_mut(), msg["parts"].as_array())
                {
                    last_parts.extend(msg_parts.iter().cloned());
                    continue;
                }
            }
        }
        merged.push(msg);
    }
    merged
}
