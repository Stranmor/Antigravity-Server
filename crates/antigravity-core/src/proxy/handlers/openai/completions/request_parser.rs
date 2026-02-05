// Request parsing and normalization for completions handler

use super::codex_parser;
use crate::proxy::mappers::openai::{OpenAIContent, OpenAIMessage, OpenAIRequest};
use serde_json::{json, Value};

/// Normalize Codex-style or legacy prompt request to messages format.
/// Returns true if the request was Codex-style.
pub fn normalize_request_body(body: &mut Value) -> bool {
    let is_codex = body.get("input").is_some() || body.get("instructions").is_some();
    if is_codex {
        let instructions = body.get("instructions").and_then(|v| v.as_str()).unwrap_or_default();
        let input_items = body.get("input").and_then(|v| v.as_array());
        let messages = codex_parser::parse_codex_input_to_messages(instructions, input_items);
        if let Some(obj) = body.as_object_mut() {
            obj.insert("messages".to_string(), json!(messages));
        }
    } else if let Some(prompt_val) = body.get("prompt") {
        let prompt_str = match prompt_val {
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n")
            },
            _ => prompt_val.to_string(),
        };
        if let Some(obj) = body.as_object_mut() {
            obj.remove("prompt");
            obj.insert(
                "messages".to_string(),
                json!([ { "role": "user", "content": prompt_str } ]),
            );
        }
    }
    is_codex
}

/// Ensure the request has at least one message (safety for empty requests).
pub fn ensure_non_empty_messages(req: &mut OpenAIRequest) {
    if req.messages.is_empty() {
        req.messages.push(OpenAIMessage {
            role: "user".to_string(),
            content: Some(OpenAIContent::String(" ".to_string())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
}
