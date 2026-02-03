// Responses API format conversion to Chat Completions format
use serde_json::{json, Value};
use tracing::debug;

/// Detects if request is in Responses API format (has instructions/input but no messages)
pub fn is_responses_format(body: &Value) -> bool {
    body.get("messages").is_none()
        && (body.get("instructions").is_some() || body.get("input").is_some())
}

/// Converts Responses API format to Chat Completions format in-place
pub fn convert_responses_to_chat(body: &mut Value) {
    debug!("Detected Responses API format, converting to Chat Completions format");

    // Convert instructions to system message
    if let Some(instructions) = body.get("instructions").and_then(|v| v.as_str()) {
        if !instructions.is_empty() {
            let system_msg = json!({
                "role": "system",
                "content": instructions
            });

            // Initialize messages array if needed
            if body.get("messages").is_none() {
                body["messages"] = json!([]);
            }

            // Insert system message at the beginning
            if let Some(messages) = body.get_mut("messages").and_then(|v| v.as_array_mut()) {
                messages.insert(0, system_msg);
            }
        }
    }

    // Convert input to user message
    if let Some(input) = body.get("input") {
        let user_msg = if input.is_string() {
            json!({
                "role": "user",
                "content": input.as_str().unwrap_or("")
            })
        } else {
            // input is array format, simplified handling
            json!({
                "role": "user",
                "content": input.to_string()
            })
        };

        if let Some(messages) = body.get_mut("messages").and_then(|v| v.as_array_mut()) {
            messages.push(user_msg);
        }
    }
}
