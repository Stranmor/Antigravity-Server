use crate::proxy::mappers::tool_result_compressor;
use serde_json::{json, Value};

const MAX_TOOL_RESULT_CHARS: usize = 200_000;

pub fn build_tool_result_part(
    tool_use_id: &str,
    content: &Value,
    is_error: Option<bool>,
    func_name: String,
    last_thought_signature: Option<&String>,
) -> Value {
    let mut compacted_content = content.clone();
    if let Some(blocks) = compacted_content.as_array_mut() {
        tool_result_compressor::sanitize_tool_result_blocks(blocks);
    }

    let mut merged_content = extract_text_content(&compacted_content, content);
    merged_content = truncate_if_needed(merged_content);
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

    part
}

fn extract_text_content(compacted: &Value, original: &Value) -> String {
    match compacted {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|block| {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    Some(text.to_string())
                } else if block.get("source").is_some() {
                    if block.get("type").and_then(|v| v.as_str()) == Some("image") {
                        Some("[image omitted to save context]".to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => original.to_string(),
    }
}

fn truncate_if_needed(content: String) -> String {
    if content.len() > MAX_TOOL_RESULT_CHARS {
        tracing::warn!(
            "Truncating tool result from {} chars to {}",
            content.len(),
            MAX_TOOL_RESULT_CHARS
        );
        let mut truncated = content
            .chars()
            .take(MAX_TOOL_RESULT_CHARS)
            .collect::<String>();
        truncated.push_str("\n...[truncated output]");
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
