//! Function call event generation for Codex streaming
//!
//! Handles conversion of Gemini functionCall to Codex-compatible events
//! (local_shell_call, web_search_call, function_call).

use bytes::Bytes;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Generate a stable call_id from function call content
pub fn generate_call_id(func_call: &Value) -> String {
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(func_call)
        .unwrap_or_default()
        .hash(&mut hasher);
    format!("call_{:x}", hasher.finish())
}

/// Parse shell command from args object, handling various formats
pub fn parse_shell_command(args_obj: &Value) -> Vec<String> {
    if args_obj.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        tracing::debug!("shell command args empty, using silent success command");
        return vec![
            "powershell.exe".to_string(),
            "-Command".to_string(),
            "exit 0".to_string(),
        ];
    }

    if let Some(arr) = args_obj.get("command").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
    }

    if let Some(cmd_str) = args_obj.get("command").and_then(|v| v.as_str()) {
        if cmd_str.contains(' ') {
            return vec![
                "powershell.exe".to_string(),
                "-Command".to_string(),
                cmd_str.to_string(),
            ];
        }
        return vec![cmd_str.to_string()];
    }

    tracing::debug!("shell command missing command field, using silent success");
    vec![
        "powershell.exe".to_string(),
        "-Command".to_string(),
        "exit 0".to_string(),
    ]
}

/// Generate output_item.added event for a function call
pub fn generate_item_added_event(name: &str, args_obj: &Value, call_id: &str) -> Option<Value> {
    let name_str = name.to_string();

    if name_str == "shell" || name_str == "local_shell" {
        let cmd_vec = parse_shell_command(args_obj);
        tracing::debug!("Shell command parsed: {:?}", cmd_vec);
        Some(json!({
            "type": "response.output_item.added",
            "item": {
                "type": "local_shell_call",
                "status": "in_progress",
                "call_id": call_id,
                "action": {
                    "type": "exec",
                    "command": cmd_vec
                }
            }
        }))
    } else if matches!(
        name_str.as_str(),
        "googleSearch" | "web_search" | "google_search"
    ) {
        let query_val = args_obj.get("query").and_then(|v| v.as_str()).unwrap_or("");
        Some(json!({
            "type": "response.output_item.added",
            "item": {
                "type": "web_search_call",
                "status": "in_progress",
                "call_id": call_id,
                "action": {
                    "type": "search",
                    "query": query_val
                }
            }
        }))
    } else {
        let args_str = args_obj.to_string();
        Some(json!({
            "type": "response.output_item.added",
            "item": {
                "type": "function_call",
                "name": name,
                "arguments": args_str,
                "call_id": call_id
            }
        }))
    }
}

/// Generate output_item.done event for a function call
pub fn generate_item_done_event(name: &str, args_obj: &Value, call_id: &str) -> Value {
    let name_str = name.to_string();

    if name_str == "shell" || name_str == "local_shell" {
        let cmd_vec = parse_shell_command(args_obj);
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "local_shell_call",
                "status": "in_progress",
                "call_id": call_id,
                "action": {
                    "type": "exec",
                    "command": cmd_vec
                }
            }
        })
    } else if matches!(
        name_str.as_str(),
        "googleSearch" | "web_search" | "google_search"
    ) {
        let query_val = args_obj.get("query").and_then(|v| v.as_str()).unwrap_or("");
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "web_search_call",
                "status": "in_progress",
                "call_id": call_id,
                "action": {
                    "type": "search",
                    "query": query_val
                }
            }
        })
    } else {
        let args_str = args_obj.to_string();
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "function_call",
                "name": name,
                "arguments": args_str,
                "call_id": call_id
            }
        })
    }
}

/// Process a function call and return (added_event_bytes, done_event_bytes)
pub fn process_function_call(func_call: &Value) -> Option<(Bytes, Bytes)> {
    let name = func_call
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let fallback_args = json!({});
    let args_obj = func_call.get("args").unwrap_or(&fallback_args);
    let call_id = generate_call_id(func_call);

    let added_ev = generate_item_added_event(name, args_obj, &call_id)?;
    let done_ev = generate_item_done_event(name, args_obj, &call_id);

    let added_bytes = Bytes::from(format!(
        "data: {}\n\n",
        serde_json::to_string(&added_ev).unwrap_or_default()
    ));
    let done_bytes = Bytes::from(format!(
        "data: {}\n\n",
        serde_json::to_string(&done_ev).unwrap_or_default()
    ));

    Some((added_bytes, done_bytes))
}
