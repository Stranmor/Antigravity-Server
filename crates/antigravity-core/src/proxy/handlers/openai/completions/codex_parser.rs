// Codex-style input parsing for OpenAI completions handler
// Handles: instructions, input array with function_call, local_shell_call, web_search_call

use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::debug;

/// Build a mapping from call_id to function name for tool result matching
pub fn build_call_id_map(input_items: Option<&Vec<Value>>) -> HashMap<String, String> {
    let mut call_id_to_name = HashMap::new();

    if let Some(items) = input_items {
        for item in items {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match item_type {
                "function_call" | "local_shell_call" | "web_search_call" => {
                    let call_id = item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .or_else(|| item.get("id").and_then(|v| v.as_str()))
                        .unwrap_or("unknown");

                    let name = if item_type == "local_shell_call" {
                        "shell"
                    } else if item_type == "web_search_call" {
                        "google_search"
                    } else {
                        item.get("name").and_then(|v| v.as_str()).unwrap_or("unknown")
                    };

                    call_id_to_name.insert(call_id.to_string(), name.to_string());
                    tracing::debug!("Mapped call_id {} to name {}", call_id, name);
                },
                // Intentionally ignored: only function_call/local_shell_call/web_search_call have call IDs
                _ => {},
            }
        }
    }

    call_id_to_name
}

/// Parse a message item from Codex input array
fn parse_message_item(item: &Value) -> Value {
    let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
    let content = item.get("content").and_then(|v| v.as_array());
    let mut text_parts = Vec::new();
    let mut image_parts: Vec<Value> = Vec::new();

    if let Some(parts) = content {
        for part in parts {
            // Handle text blocks
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                text_parts.push(text.to_string());
            }
            // Handle image blocks (Codex input_image format)
            else if part.get("type").and_then(|v| v.as_str()) == Some("input_image") {
                if let Some(image_url) = part.get("image_url").and_then(|v| v.as_str()) {
                    image_parts.push(json!({
                        "type": "image_url",
                        "image_url": { "url": image_url }
                    }));
                    debug!("[Codex] Found input_image: {}", image_url);
                }
            }
            // Handle standard OpenAI image_url format
            else if part.get("type").and_then(|v| v.as_str()) == Some("image_url") {
                if let Some(url_obj) = part.get("image_url") {
                    image_parts.push(json!({
                        "type": "image_url",
                        "image_url": url_obj.clone()
                    }));
                }
            }
        }
    }

    // Build message content: use array format if images present
    if image_parts.is_empty() {
        json!({
            "role": role,
            "content": text_parts.join("\n")
        })
    } else {
        let mut content_blocks: Vec<Value> = Vec::new();
        if !text_parts.is_empty() {
            content_blocks.push(json!({
                "type": "text",
                "text": text_parts.join("\n")
            }));
        }
        content_blocks.extend(image_parts);
        json!({
            "role": role,
            "content": content_blocks
        })
    }
}

/// Parse a function/shell/web_search call item
fn parse_function_call_item(item: &Value, item_type: &str) -> Value {
    let mut name = item.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
    let mut args_str = item.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}").to_string();
    let call_id = item
        .get("call_id")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("id").and_then(|v| v.as_str()))
        .unwrap_or("unknown");

    // Handle native shell calls
    if item_type == "local_shell_call" {
        name = "shell";
        if let Some(action) = item.get("action") {
            if let Some(exec) = action.get("exec") {
                let mut args_obj = serde_json::Map::new();
                if let Some(cmd) = exec.get("command") {
                    // CRITICAL: 'shell' tool schema defines 'command' as ARRAY of strings
                    let cmd_val = if cmd.is_string() {
                        json!([cmd]) // Wrap in array
                    } else {
                        cmd.clone() // Assume already array
                    };
                    args_obj.insert("command".to_string(), cmd_val);
                }
                if let Some(wd) = exec.get("working_directory").or(exec.get("workdir")) {
                    args_obj.insert("workdir".to_string(), wd.clone());
                }
                args_str = serde_json::to_string(&args_obj).unwrap_or("{}".to_string());
            }
        }
    } else if item_type == "web_search_call" {
        name = "google_search";
        if let Some(action) = item.get("action") {
            let mut args_obj = serde_json::Map::new();
            if let Some(q) = action.get("query") {
                args_obj.insert("query".to_string(), q.clone());
            }
            args_str = serde_json::to_string(&args_obj).unwrap_or("{}".to_string());
        }
    }

    json!({
        "role": "assistant",
        "tool_calls": [
            {
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": args_str
                }
            }
        ]
    })
}

/// Parse a function call output item
fn parse_function_output_item(item: &Value, call_id_to_name: &HashMap<String, String>) -> Value {
    let call_id = item.get("call_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let output = item.get("output");
    let output_str = if let Some(o) = output {
        if o.is_string() {
            o.as_str().unwrap_or("").to_string()
        } else if let Some(content) = o.get("content").and_then(|v| v.as_str()) {
            content.to_string()
        } else {
            o.to_string()
        }
    } else {
        String::new()
    };

    let name = call_id_to_name.get(call_id).cloned().unwrap_or_else(|| {
        tracing::warn!("Unknown tool name for call_id {}, defaulting to 'shell'", call_id);
        "shell".to_string()
    });

    json!({
        "role": "tool",
        "tool_call_id": call_id,
        "name": name,
        "content": output_str
    })
}

/// Convert Codex-style input array to OpenAI messages format
pub fn parse_codex_input_to_messages(
    instructions: &str,
    input_items: Option<&Vec<Value>>,
) -> Vec<Value> {
    let mut messages = Vec::new();

    // System instructions
    if !instructions.is_empty() {
        messages.push(json!({ "role": "system", "content": instructions }));
    }

    // Build call_id to name map first
    let call_id_to_name = build_call_id_map(input_items);

    // Map input items to messages
    if let Some(items) = input_items {
        for item in items {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match item_type {
                "message" => {
                    messages.push(parse_message_item(item));
                },
                "function_call" | "local_shell_call" | "web_search_call" => {
                    messages.push(parse_function_call_item(item, item_type));
                },
                "function_call_output" | "custom_tool_call_output" => {
                    messages.push(parse_function_output_item(item, &call_id_to_name));
                },
                // Intentionally ignored: unrecognized Codex item types are skipped
                _ => {
                    tracing::trace!("Skipping unrecognized Codex input item type: {}", item_type);
                },
            }
        }
    }

    messages
}
