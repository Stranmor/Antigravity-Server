use super::super::models::*;
use super::signature_validator::{
    ensure_thinking_block_first, resolve_tool_signature, should_use_tool_signature,
    validate_thinking_signature, SignatureAction,
};
use super::tool_result_handler::{build_tool_result_part, inject_missing_tool_results};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn build_contents(
    content: &MessageContent,
    is_assistant: bool,
    _claude_req: &ClaudeRequest,
    is_thinking_enabled: bool,
    session_id: &str,
    allow_dummy_thought: bool,
    is_retry: bool,
    tool_id_to_name: &mut HashMap<String, String>,
    mapped_model: &str,
    last_thought_signature: &mut Option<String>,
    pending_tool_use_ids: &mut Vec<String>,
    last_user_task_text_normalized: &mut Option<String>,
    previous_was_tool_result: &mut bool,
    _existing_tool_result_ids: &std::collections::HashSet<String>,
    tool_name_to_schema: &HashMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let mut parts = Vec::new();
    let mut current_turn_tool_result_ids = std::collections::HashSet::new();
    let mut saw_non_thinking = false;

    match content {
        MessageContent::String(text) => {
            if text != "(no content)" && !text.trim().is_empty() {
                parts.push(json!({"text": text.trim()}));
            }
        },
        MessageContent::Array(blocks) => {
            for item in blocks {
                match item {
                    ContentBlock::Text { text } => {
                        if text != "(no content)" {
                            // [NEW] taskdeduplogic: ifcurrentis User message，andimmediately followat ToolResult after，
                            // checkthetextwhetherandprevious roundtaskdescriptioncompletelysame。
                            if !is_assistant && *previous_was_tool_result {
                                if let Some(last_task) = last_user_task_text_normalized {
                                    let current_normalized =
                                        text.replace(|c: char| c.is_whitespace(), "");
                                    if !current_normalized.is_empty()
                                        && current_normalized == *last_task
                                    {
                                        tracing::info!(
                                            "[Claude-Request] Dropping duplicated task text echo (len: {})",
                                            text.len()
                                        );
                                        continue;
                                    }
                                }
                            }

                            parts.push(json!({"text": text}));
                            saw_non_thinking = true;

                            if !is_assistant {
                                *last_user_task_text_normalized =
                                    Some(text.replace(|c: char| c.is_whitespace(), ""));
                            }
                            *previous_was_tool_result = false;
                        }
                    },
                    ContentBlock::Thinking { thinking, signature, .. } => {
                        tracing::debug!(
                            "[DEBUG-TRANSFORM] Processing thinking block. Sig: {:?}",
                            signature
                        );

                        // [HOTFIX] Gemini Protocol Enforcement: Thinking block MUST be the first block.
                        // If we already have content (like Text), we must downgrade this thinking block to Text.
                        if saw_non_thinking || !parts.is_empty() {
                            tracing::warn!(
                                "[Claude-Request] Thinking block found at non-zero index (prev parts: {}). Downgrading to Text.",
                                parts.len()
                            );
                            if !thinking.is_empty() {
                                parts.push(json!({
                                    "text": thinking
                                }));
                                saw_non_thinking = true;
                            }
                            continue;
                        }

                        // [FIX] If thinking is disabled (smart downgrade), convert ALL thinking blocks to text
                        // to avoid "thinking is disabled but message contains thinking" error
                        if !is_thinking_enabled {
                            tracing::warn!(
                                "[Claude-Request] Thinking disabled. Downgrading thinking block to text."
                            );
                            if !thinking.is_empty() {
                                parts.push(json!({
                                    "text": thinking
                                }));
                            }
                            continue;
                        }

                        // [FIX] Empty thinking blocks cause "Field required" errors.
                        // We downgrade them to Text to avoid structural errors and signature mismatch.
                        if thinking.is_empty() {
                            tracing::warn!(
                                "[Claude-Request] Empty thinking block detected. Downgrading to Text."
                            );
                            parts.push(json!({
                                "text": "..."
                            }));
                            continue;
                        }

                        // [FIX #752] Strict signature validation
                        match validate_thinking_signature(
                            thinking,
                            signature.as_ref(),
                            is_retry,
                            mapped_model,
                            last_thought_signature,
                        ) {
                            SignatureAction::UseWithSignature { part } => {
                                parts.push(part);
                            },
                            SignatureAction::DowngradeToText { text } => {
                                parts.push(json!({"text": text}));
                                saw_non_thinking = true;
                            },
                        }
                    },
                    ContentBlock::RedactedThinking { data } => {
                        // [FIX] will RedactedThinking asnormaltexthandle，preservecontext
                        tracing::debug!("[Claude-Request] Degrade RedactedThinking to text");
                        parts.push(json!({
                            "text": format!("[Redacted Thinking: {}]", data)
                        }));
                        saw_non_thinking = true;
                        continue;
                    },
                    ContentBlock::Image { source, .. } => {
                        if source.source_type == "base64" {
                            parts.push(json!({
                                "inlineData": {
                                    "mimeType": source.media_type,
                                    "data": source.data
                                }
                            }));
                            saw_non_thinking = true;
                        }
                    },
                    ContentBlock::Document { source, .. } => {
                        if source.source_type == "base64" {
                            parts.push(json!({
                                "inlineData": {
                                    "mimeType": source.media_type,
                                    "data": source.data
                                }
                            }));
                            saw_non_thinking = true;
                        }
                    },
                    ContentBlock::ToolUse { id, name, input, signature, .. } => {
                        let mut final_input = input.clone();

                        // [CRITICAL FIX] Shell tool command must be an array of strings
                        if name == "local_shell_call" {
                            if let Some(command) = final_input.get_mut("command") {
                                if let Value::String(s) = command {
                                    tracing::info!(
                                        "[Claude-Request] Converting shell command string to array: {}",
                                        s
                                    );
                                    *command = json!([s]);
                                }
                            }
                        }

                        // Fix tool call argument types using original schema
                        if let Some(original_schema) = tool_name_to_schema.get(name) {
                            crate::proxy::common::json_schema::fix_tool_call_args(
                                &mut final_input,
                                original_schema,
                            );
                        }

                        let mut part = json!({
                            "functionCall": {
                                "name": name,
                                "args": final_input,
                                "id": id
                            }
                        });
                        saw_non_thinking = true;

                        // Track pending tool use
                        if is_assistant {
                            pending_tool_use_ids.push(id.clone());
                        }

                        crate::proxy::common::json_schema::clean_json_schema(&mut part);
                        tool_id_to_name.insert(id.clone(), name.clone());

                        let final_sig = resolve_tool_signature(
                            id,
                            signature.as_ref(),
                            last_thought_signature,
                            session_id,
                        );

                        if let Some(sig) = final_sig {
                            if is_retry && signature.is_none() {
                                tracing::warn!(
                                    "[Tool-Signature] Skipping signature backfill for tool_use: {} during retry.",
                                    id
                                );
                            } else if should_use_tool_signature(
                                &sig,
                                id,
                                mapped_model,
                                is_thinking_enabled,
                            ) {
                                part["thoughtSignature"] = json!(sig);
                            }
                        } else {
                            let is_google_cloud = mapped_model.starts_with("projects/");
                            if is_thinking_enabled && !is_google_cloud {
                                tracing::debug!(
                                    "[Tool-Signature] Adding GEMINI_SKIP_SIGNATURE for tool_use: {}",
                                    id
                                );
                                part["thoughtSignature"] =
                                    json!("skip_thought_signature_validator");
                            }
                        }
                        parts.push(part);
                    },
                    ContentBlock::ToolResult { tool_use_id, content, is_error, .. } => {
                        current_turn_tool_result_ids.insert(tool_use_id.clone());
                        let func_name = tool_id_to_name
                            .get(tool_use_id)
                            .cloned()
                            .unwrap_or_else(|| tool_use_id.clone());

                        let part = build_tool_result_part(
                            tool_use_id,
                            content,
                            *is_error,
                            func_name,
                            last_thought_signature.as_ref(),
                        );
                        parts.push(part);
                        *previous_was_tool_result = true;
                    },
                    // ContentBlock::RedactedThinking handled above at line 583
                    ContentBlock::ServerToolUse { .. }
                    | ContentBlock::WebSearchToolResult { .. } => {
                        // Search result blocks should not be sent back to upstream by client (already replaced by tool_result)
                        continue;
                    },
                }
            }
        },
    }

    if !is_assistant && !pending_tool_use_ids.is_empty() {
        inject_missing_tool_results(
            &mut parts,
            pending_tool_use_ids,
            &current_turn_tool_result_ids,
            tool_id_to_name,
        );
    }

    if allow_dummy_thought && is_assistant && is_thinking_enabled {
        ensure_thinking_block_first(&mut parts);
    }

    Ok(parts)
}
