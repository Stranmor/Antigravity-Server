use crate::proxy::mappers::tool_result_compressor::{self, MAX_TOOL_RESULT_CHARS};
use serde_json::{json, Value};

pub fn build_tool_result_part(
    tool_use_id: &str,
    content: &Value,
    is_error: Option<bool>,
    func_name: String,
    last_thought_signature: Option<&String>,
) -> Vec<Value> {
    let mut compacted_content = content.clone();
    if let Some(blocks) = compacted_content.as_array_mut() {
        tool_result_compressor::sanitize_tool_result_blocks(blocks);
    }

    let (text_content, image_parts) = extract_content_and_images(&compacted_content, content);
    let mut merged_content = truncate_if_needed(text_content);
    merged_content = ensure_non_empty(merged_content, is_error);

    let mut part = json!({
        "functionResponse": {
            "name": func_name,
            "response": {"result": merged_content},
            "id": tool_use_id
        }
    });

    if let Some(sig) = last_thought_signature {
        part["thoughtSignature"] = json!(sig);
    }

    let mut result = vec![part];
    result.extend(image_parts);
    result
}

fn extract_content_and_images(compacted: &Value, original: &Value) -> (String, Vec<Value>) {
    let mut images = Vec::new();
    let text = match compacted {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let text_parts: Vec<_> = arr
                .iter()
                .filter_map(|block| {
                    let block_type = block.get("type").and_then(|v| v.as_str());
                    if block_type == Some("text") {
                        block.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
                    } else if block_type == Some("image") {
                        if let Some(source) = block.get("source") {
                            let data = source.get("data").and_then(|v| v.as_str());
                            let media = source
                                .get("media_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("image/png");
                            if let Some(b64) = data {
                                images.push(json!({
                                    "inlineData": {
                                        "mimeType": media,
                                        "data": b64
                                    }
                                }));
                                return Some("[image attached below]".to_string());
                            }
                        }
                        None
                    } else {
                        None
                    }
                })
                .collect();

            if text_parts.is_empty() && !arr.is_empty() {
                if images.is_empty() {
                    compacted.to_string()
                } else {
                    "[image content attached]".to_string()
                }
            } else {
                text_parts.join("\n")
            }
        },
        _ => original.to_string(),
    };
    (text, images)
}

fn truncate_if_needed(content: String) -> String {
    let char_count = content.chars().count();
    if char_count > MAX_TOOL_RESULT_CHARS {
        tracing::warn!(
            "Truncating tool result from {} chars to {}",
            char_count,
            MAX_TOOL_RESULT_CHARS
        );
        let suffix = "\n...[truncated output]";
        let suffix_len = suffix.chars().count();
        let mut truncated = content
            .chars()
            .take(MAX_TOOL_RESULT_CHARS.saturating_sub(suffix_len))
            .collect::<String>();
        truncated.push_str(suffix);
        truncated
    } else {
        content
    }
}

fn ensure_non_empty(content: String, is_error: Option<bool>) -> String {
    if content.trim().is_empty() {
        if is_error.unwrap_or(false) {
            "Tool execution failed with no output.".to_string()
        } else {
            "Command executed successfully.".to_string()
        }
    } else {
        content
    }
}

use std::collections::{HashMap, HashSet};

pub fn inject_missing_tool_results(
    parts: &mut Vec<Value>,
    pending_tool_use_ids: &mut Vec<String>,
    current_turn_tool_result_ids: &HashSet<String>,
    tool_id_to_name: &HashMap<String, String>,
) {
    let missing_ids: Vec<_> = pending_tool_use_ids
        .iter()
        .filter(|id| !current_turn_tool_result_ids.contains(*id))
        .cloned()
        .collect();

    if !missing_ids.is_empty() {
        tracing::warn!(
            "[Elastic-Recovery] Injecting {} missing tool results into User message (IDs: {:?})",
            missing_ids.len(),
            missing_ids
        );
        for id in missing_ids.iter().rev() {
            let name = tool_id_to_name.get(id).cloned().unwrap_or(id.clone());
            let synthetic_part = json!({
                "functionResponse": {
                    "name": name,
                    "response": {
                        "result": "Tool execution interrupted. No result provided."
                    },
                    "id": id
                }
            });
            parts.insert(0, synthetic_part);
        }
    }
    pending_tool_use_ids.clear();
}
