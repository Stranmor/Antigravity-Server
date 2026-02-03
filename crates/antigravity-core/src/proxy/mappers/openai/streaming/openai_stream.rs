use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::pin::Pin;
use tracing::debug;
use uuid::Uuid;

use super::usage::extract_usage_metadata;
use crate::proxy::mappers::openai::models::OpenAIUsage;
use crate::proxy::mappers::signature_store::store_thought_signature;

pub fn create_openai_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    model: String,
    _estimated_tokens: Option<u32>,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    let mut buffer = BytesMut::new();
    let stream_id = format!("chatcmpl-{}", Uuid::new_v4());
    let created_ts = Utc::now().timestamp();

    let stream = async_stream::stream! {
        let mut emitted_tool_calls = std::collections::HashSet::new();
        let mut final_usage: Option<OpenAIUsage> = None;
        while let Some(item) = gemini_stream.next().await {
            match item {
                Ok(bytes) => {
                    debug!("[OpenAI-SSE] Received chunk: {} bytes", bytes.len());
                    buffer.extend_from_slice(&bytes);

                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_raw = buffer.split_to(pos + 1);
                        if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                            let line = line_str.trim();
                            if line.is_empty() { continue; }

                            if line.starts_with("data: ") {
                                let json_part = line.trim_start_matches("data: ").trim();
                                if json_part == "[DONE]" {
                                    continue;
                                }

                                if let Ok(mut json) = serde_json::from_str::<Value>(json_part) {
                                    tracing::debug!("Gemini SSE Chunk: {}", json_part);

                                    let actual_data = if let Some(inner) = json.get_mut("response").map(|v| v.take()) {
                                        inner
                                    } else {
                                        json
                                    };

                                    if let Some(u) = actual_data.get("usageMetadata") {
                                        final_usage = extract_usage_metadata(u);
                                    }

                                    if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                        for (idx, candidate) in candidates.iter().enumerate() {
                                            let parts = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array());

                                            let mut content_out = String::new();
                                            let mut thought_out = String::new();

                                            if let Some(parts_list) = parts {
                                                for part in parts_list {
                                                    let is_thought_part = part.get("thought")
                                                        .and_then(|v| v.as_bool())
                                                        .unwrap_or(false);

                                                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                        if is_thought_part {
                                                            thought_out.push_str(text);
                                                        } else {
                                                            content_out.push_str(text);
                                                        }
                                                    }
                                                    if let Some(sig) = part.get("thoughtSignature").or(part.get("thought_signature")).and_then(|s| s.as_str()) {
                                                        store_thought_signature(sig);
                                                    }

                                                    if let Some(img) = part.get("inlineData") {
                                                        let mime_type = img.get("mimeType").and_then(|v| v.as_str()).unwrap_or("image/png");
                                                        let data = img.get("data").and_then(|v| v.as_str()).unwrap_or("");
                                                        if !data.is_empty() {
                                                            const CHUNK_SIZE: usize = 32 * 1024;
                                                            let prefix = format!("![image](data:{};base64,", mime_type);
                                                            let suffix = ")";

                                                            let prefix_chunk = json!({
                                                                "id": &stream_id,
                                                                "object": "chat.completion.chunk",
                                                                "created": created_ts,
                                                                "model": &model,
                                                                "choices": [{
                                                                    "index": idx as u32,
                                                                    "delta": { "content": prefix },
                                                                    "finish_reason": serde_json::Value::Null
                                                                }]
                                                            });
                                                            let sse_out = format!("data: {}\n\n", serde_json::to_string(&prefix_chunk).unwrap_or_default());
                                                            yield Ok::<Bytes, String>(Bytes::from(sse_out));

                                                            for chunk in data.as_bytes().chunks(CHUNK_SIZE) {
                                                                if let Ok(chunk_str) = std::str::from_utf8(chunk) {
                                                                    let data_chunk = json!({
                                                                        "id": &stream_id,
                                                                        "object": "chat.completion.chunk",
                                                                        "created": created_ts,
                                                                        "model": &model,
                                                                        "choices": [{
                                                                            "index": idx as u32,
                                                                            "delta": { "content": chunk_str },
                                                                            "finish_reason": serde_json::Value::Null
                                                                        }]
                                                                    });
                                                                    let sse_out = format!("data: {}\n\n", serde_json::to_string(&data_chunk).unwrap_or_default());
                                                                    yield Ok::<Bytes, String>(Bytes::from(sse_out));
                                                                }
                                                            }

                                                            let suffix_chunk = json!({
                                                                "id": &stream_id,
                                                                "object": "chat.completion.chunk",
                                                                "created": created_ts,
                                                                "model": &model,
                                                                "choices": [{
                                                                    "index": idx as u32,
                                                                    "delta": { "content": suffix },
                                                                    "finish_reason": serde_json::Value::Null
                                                                }]
                                                            });
                                                            let sse_out = format!("data: {}\n\n", serde_json::to_string(&suffix_chunk).unwrap_or_default());
                                                            yield Ok::<Bytes, String>(Bytes::from(sse_out));

                                                            tracing::info!("[OpenAI-SSE] Sent image in {} chunks ({} bytes total)",
                                                                (data.len() / CHUNK_SIZE) + 2, data.len());
                                                        }
                                                    }

                                                    if let Some(func_call) = part.get("functionCall") {
                                                        let call_key = serde_json::to_string(func_call).unwrap_or_default();
                                                        if !emitted_tool_calls.contains(&call_key) {
                                                            emitted_tool_calls.insert(call_key);

                                                            let name = func_call.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                            let args = func_call.get("args").unwrap_or(&json!({})).to_string();

                                                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                                            use std::hash::{Hash, Hasher};
                                                            serde_json::to_string(func_call).unwrap_or_default().hash(&mut hasher);
                                                            let call_id = format!("call_{:x}", hasher.finish());

                                                            let tool_call_chunk = json!({
                                                                "id": &stream_id,
                                                                "object": "chat.completion.chunk",
                                                                "created": created_ts,
                                                                "model": &model,
                                                                "choices": [{
                                                                    "index": idx as u32,
                                                                    "delta": {
                                                                        "role": "assistant",
                                                                        "tool_calls": [{
                                                                            "index": 0,
                                                                            "id": call_id,
                                                                            "type": "function",
                                                                            "function": {
                                                                                "name": name,
                                                                                "arguments": args
                                                                            }
                                                                        }]
                                                                    },
                                                                    "finish_reason": serde_json::Value::Null
                                                                }]
                                                            });

                                                            let sse_out = format!("data: {}\n\n", serde_json::to_string(&tool_call_chunk).unwrap_or_default());
                                                            yield Ok::<Bytes, String>(Bytes::from(sse_out));
                                                        }
                                                    }
                                                }
                                            }

                                            if let Some(grounding) = candidate.get("groundingMetadata") {
                                                let mut grounding_text = String::new();

                                                if let Some(queries) = grounding.get("webSearchQueries").and_then(|q| q.as_array()) {
                                                    let query_list: Vec<&str> = queries.iter().filter_map(|v| v.as_str()).collect();
                                                    if !query_list.is_empty() {
                                                        grounding_text.push_str("\n\n---\n**🔍 已为您搜索：** ");
                                                        grounding_text.push_str(&query_list.join(", "));
                                                    }
                                                }

                                                if let Some(chunks) = grounding.get("groundingChunks").and_then(|c| c.as_array()) {
                                                    let mut links = Vec::new();
                                                    for (i, chunk) in chunks.iter().enumerate() {
                                                        if let Some(web) = chunk.get("web") {
                                                            let title = web.get("title").and_then(|v| v.as_str()).unwrap_or("网页来源");
                                                            let uri = web.get("uri").and_then(|v| v.as_str()).unwrap_or("#");
                                                            links.push(format!("[{}] [{}]({})", i + 1, title, uri));
                                                        }
                                                    }
                                                    if !links.is_empty() {
                                                        grounding_text.push_str("\n\n**🌐 来源引文：**\n");
                                                        grounding_text.push_str(&links.join("\n"));
                                                    }
                                                }

                                                if !grounding_text.is_empty() {
                                                    content_out.push_str(&grounding_text);
                                                }
                                            }

                                            if content_out.is_empty() && thought_out.is_empty()
                                                && candidate.get("finishReason").is_none()
                                            {
                                                continue;
                                            }

                                            let finish_reason = candidate.get("finishReason")
                                                .and_then(|f| f.as_str())
                                                .map(|f| match f {
                                                    "STOP" => "stop",
                                                    "MAX_TOKENS" => "length",
                                                    "SAFETY" => "content_filter",
                                                    "RECITATION" => "content_filter",
                                                    _ => f,
                                                });

                                            if !thought_out.is_empty() {
                                                let reasoning_chunk = json!({
                                                    "id": &stream_id,
                                                    "object": "chat.completion.chunk",
                                                    "created": created_ts,
                                                    "model": model,
                                                    "choices": [
                                                        {
                                                            "index": idx as u32,
                                                            "delta": {
                                                                "role": "assistant",
                                                                "content": serde_json::Value::Null,
                                                                "reasoning_content": thought_out
                                                            },
                                                            "finish_reason": serde_json::Value::Null
                                                        }
                                                    ]
                                                });
                                                let sse_out = format!("data: {}\n\n", serde_json::to_string(&reasoning_chunk).unwrap_or_default());
                                                yield Ok::<Bytes, String>(Bytes::from(sse_out));
                                            }

                                            if !content_out.is_empty() || finish_reason.is_some() {
                                                const MAX_CHUNK_SIZE: usize = 32 * 1024;

                                                if content_out.len() > MAX_CHUNK_SIZE {
                                                    let content_bytes = content_out.as_bytes();
                                                    let total_chunks = content_bytes.len().div_ceil(MAX_CHUNK_SIZE);

                                                    for (chunk_idx, chunk) in content_bytes.chunks(MAX_CHUNK_SIZE).enumerate() {
                                                        let is_last_chunk = chunk_idx == total_chunks - 1;

                                                        let chunk_str = if is_last_chunk {
                                                            String::from_utf8_lossy(chunk).to_string()
                                                        } else {
                                                            let safe_len = (0..=chunk.len())
                                                                .rev()
                                                                .find(|&i| std::str::from_utf8(&chunk[..i]).is_ok())
                                                                .unwrap_or(0);
                                                            String::from_utf8_lossy(&chunk[..safe_len]).to_string()
                                                        };

                                                        let chunk_finish_reason = if is_last_chunk { finish_reason } else { None };

                                                        let openai_chunk = json!({
                                                            "id": &stream_id,
                                                            "object": "chat.completion.chunk",
                                                            "created": created_ts,
                                                            "model": model,
                                                            "choices": [
                                                                {
                                                                    "index": idx as u32,
                                                                    "delta": {
                                                                        "content": chunk_str
                                                                    },
                                                                    "finish_reason": chunk_finish_reason
                                                                }
                                                            ]
                                                        });

                                                        let sse_out = format!("data: {}\n\n", serde_json::to_string(&openai_chunk).unwrap_or_default());
                                                        yield Ok::<Bytes, String>(Bytes::from(sse_out));
                                                    }
                                                } else {
                                                    let openai_chunk = json!({
                                                        "id": &stream_id,
                                                        "object": "chat.completion.chunk",
                                                        "created": created_ts,
                                                        "model": model,
                                                        "choices": [
                                                            {
                                                                "index": idx as u32,
                                                                "delta": {
                                                                    "content": content_out
                                                                },
                                                                "finish_reason": finish_reason
                                                            }
                                                        ]
                                                    });

                                                    let sse_out = format!("data: {}\n\n", serde_json::to_string(&openai_chunk).unwrap_or_default());
                                                    yield Ok::<Bytes, String>(Bytes::from(sse_out));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    use crate::proxy::mappers::error_classifier::classify_stream_error;
                    let (error_type, user_message, i18n_key) = classify_stream_error(&e);

                    tracing::error!(
                        error_type = %error_type,
                        user_message = %user_message,
                        i18n_key = %i18n_key,
                        raw_error = %e,
                        "OpenAI stream error occurred"
                    );

                    let error_chunk = json!({
                        "id": &stream_id,
                        "object": "chat.completion.chunk",
                        "created": created_ts,
                        "model": &model,
                        "choices": [],
                        "error": {
                            "type": error_type,
                            "message": user_message,
                            "code": "stream_error",
                            "i18n_key": i18n_key
                        }
                    });

                    let sse_out = format!("data: {}\n\n", serde_json::to_string(&error_chunk).unwrap_or_default());
                    yield Ok(Bytes::from(sse_out));
                    yield Ok(Bytes::from("data: [DONE]\n\n"));
                    break;
                }
            }
        }

        if let Some(usage) = final_usage {
            let usage_chunk = json!({
                "id": &stream_id,
                "object": "chat.completion.chunk",
                "created": created_ts,
                "model": &model,
                "choices": [],
                "usage": usage
            });
            let sse_out = format!("data: {}\n\n", serde_json::to_string(&usage_chunk).unwrap_or_default());
            yield Ok::<Bytes, String>(Bytes::from(sse_out));
        }

        yield Ok::<Bytes, String>(Bytes::from("data: [DONE]\n\n"));
    };

    Box::pin(stream)
}
