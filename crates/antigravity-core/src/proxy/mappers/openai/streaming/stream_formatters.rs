// OpenAI SSE stream formatting helpers
use serde_json::{json, Value};

/// Format an SSE data line
#[inline]
pub fn sse_line(data: &Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::to_string(data).unwrap_or_default()
    )
}

/// Create a content delta chunk
pub fn content_chunk(
    stream_id: &str,
    created_ts: i64,
    model: &str,
    index: u32,
    content: &str,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "id": stream_id,
        "object": "chat.completion.chunk",
        "created": created_ts,
        "model": model,
        "choices": [{
            "index": index,
            "delta": { "content": content },
            "finish_reason": finish_reason
        }]
    })
}

/// Create a reasoning content chunk (for thought/thinking)
pub fn reasoning_chunk(
    stream_id: &str,
    created_ts: i64,
    model: &str,
    index: u32,
    reasoning_content: &str,
) -> Value {
    json!({
        "id": stream_id,
        "object": "chat.completion.chunk",
        "created": created_ts,
        "model": model,
        "choices": [{
            "index": index,
            "delta": {
                "role": "assistant",
                "content": Value::Null,
                "reasoning_content": reasoning_content
            },
            "finish_reason": Value::Null
        }]
    })
}

/// Create a tool call chunk
pub fn tool_call_chunk(
    stream_id: &str,
    created_ts: i64,
    model: &str,
    index: u32,
    call_id: &str,
    name: &str,
    arguments: &str,
) -> Value {
    json!({
        "id": stream_id,
        "object": "chat.completion.chunk",
        "created": created_ts,
        "model": model,
        "choices": [{
            "index": index,
            "delta": {
                "role": "assistant",
                "tool_calls": [{
                    "index": 0,
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments
                    }
                }]
            },
            "finish_reason": Value::Null
        }]
    })
}

/// Create an error chunk
pub fn error_chunk(
    stream_id: &str,
    created_ts: i64,
    model: &str,
    error_type: &str,
    message: &str,
    i18n_key: &str,
) -> Value {
    json!({
        "id": stream_id,
        "object": "chat.completion.chunk",
        "created": created_ts,
        "model": model,
        "choices": [],
        "error": {
            "type": error_type,
            "message": message,
            "code": "stream_error",
            "i18n_key": i18n_key
        }
    })
}

/// Create a usage chunk
pub fn usage_chunk(
    stream_id: &str,
    created_ts: i64,
    model: &str,
    usage: &impl serde::Serialize,
) -> Value {
    json!({
        "id": stream_id,
        "object": "chat.completion.chunk",
        "created": created_ts,
        "model": model,
        "choices": [],
        "usage": usage
    })
}

/// Format grounding metadata as markdown text
pub fn format_grounding_metadata(grounding: &Value) -> String {
    let mut result = String::new();

    if let Some(queries) = grounding.get("webSearchQueries").and_then(|q| q.as_array()) {
        let query_list: Vec<&str> = queries.iter().filter_map(|v| v.as_str()).collect();
        if !query_list.is_empty() {
            result.push_str("\n\n---\n**🔍 Searched for you:** ");
            result.push_str(&query_list.join(", "));
        }
    }

    if let Some(chunks) = grounding.get("groundingChunks").and_then(|c| c.as_array()) {
        let mut links = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            if let Some(web) = chunk.get("web") {
                let title = web
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Web Source");
                let uri = web.get("uri").and_then(|v| v.as_str()).unwrap_or("#");
                links.push(format!("[{}] [{}]({})", i + 1, title, uri));
            }
        }
        if !links.is_empty() {
            result.push_str("\n\n**🌐 Source Citations:**\n");
            result.push_str(&links.join("\n"));
        }
    }

    result
}

/// Map Gemini finish reason to OpenAI format
pub fn map_finish_reason(gemini_reason: &str) -> &'static str {
    match gemini_reason {
        "STOP" => "stop",
        "MAX_TOKENS" => "length",
        "SAFETY" => "content_filter",
        "RECITATION" => "content_filter",
        _ => "stop",
    }
}

/// Generate a tool call ID from function call JSON
pub fn generate_call_id(func_call: &Value) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    serde_json::to_string(func_call)
        .unwrap_or_default()
        .hash(&mut hasher);
    format!("call_{:x}", hasher.finish())
}
