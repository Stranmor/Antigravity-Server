// OpenAI 流式转换
use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures::{Stream, StreamExt};
use rand::Rng;
use serde_json::{json, Value};
use std::pin::Pin;
use tracing::debug;
use uuid::Uuid;

// Use unified signature storage (shared with Claude path)
pub use crate::proxy::mappers::signature_store::{get_thought_signature, store_thought_signature};

/// Extract and convert Gemini usageMetadata to OpenAI usage format
fn extract_usage_metadata(u: &Value) -> Option<super::models::OpenAIUsage> {
    use super::models::{OpenAIUsage, PromptTokensDetails};

    let prompt_tokens = u
        .get("promptTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let completion_tokens = u
        .get("candidatesTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let total_tokens = u
        .get("totalTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cached_tokens = u
        .get("cachedContentTokenCount")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    Some(OpenAIUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        prompt_tokens_details: cached_tokens.map(|ct| PromptTokensDetails {
            cached_tokens: Some(ct),
        }),
        completion_tokens_details: None,
    })
}

pub fn create_openai_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    model: String,
    _estimated_tokens: Option<u32>,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    let mut buffer = BytesMut::new();

    // 在流开始时生成固定的 ID 和 timestamp，所有 chunk 共用
    let stream_id = format!("chatcmpl-{}", Uuid::new_v4());
    let created_ts = Utc::now().timestamp();

    let stream = async_stream::stream! {
        let mut emitted_tool_calls = std::collections::HashSet::new();
        let mut final_usage: Option<super::models::OpenAIUsage> = None;
        while let Some(item) = gemini_stream.next().await {
            match item {
                Ok(bytes) => {
                    // Verbose logging for debugging image fragmentation
                    debug!("[OpenAI-SSE] Received chunk: {} bytes", bytes.len());
                    buffer.extend_from_slice(&bytes);

                    // Process complete lines from buffer
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
                                    // Log raw chunk for debugging gemini-3 thoughts
                                    tracing::debug!("Gemini SSE Chunk: {}", json_part);

                                    // Handle v1internal wrapper if present
                                    let actual_data = if let Some(inner) = json.get_mut("response").map(|v| v.take()) {
                                        inner
                                    } else {
                                        json
                                    };

                                    // Capture usageMetadata if present
                                    if let Some(u) = actual_data.get("usageMetadata") {
                                        final_usage = extract_usage_metadata(u);
                                    }

                                    // Extract candidates
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
                                                    // 捕获 thoughtSignature (Gemini 3 工具调用必需)
                                                    if let Some(sig) = part.get("thoughtSignature").or(part.get("thought_signature")).and_then(|s| s.as_str()) {
                                                        store_thought_signature(sig);
                                                    }

                                                    if let Some(img) = part.get("inlineData") {
                                                        let mime_type = img.get("mimeType").and_then(|v| v.as_str()).unwrap_or("image/png");
                                                        let data = img.get("data").and_then(|v| v.as_str()).unwrap_or("");
                                                        if !data.is_empty() {
                                                            // [FIX] Split large base64 images into smaller SSE chunks
                                                            // to avoid "Chunk too big" errors in clients like Open WebUI
                                                            const CHUNK_SIZE: usize = 32 * 1024; // 32KB per chunk
                                                            let prefix = format!("![image](data:{};base64,", mime_type);
                                                            let suffix = ")";

                                                            // Send prefix as first chunk
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

                                                            // Send base64 data in chunks
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

                                                            // Send suffix as final chunk
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

                                                    // Handle function call
                                                    if let Some(func_call) = part.get("functionCall") {
                                                        let call_key = serde_json::to_string(func_call).unwrap_or_default();
                                                        if !emitted_tool_calls.contains(&call_key) {
                                                            emitted_tool_calls.insert(call_key);

                                                            let name = func_call.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                            let args = func_call.get("args").unwrap_or(&json!({})).to_string();

                                                            // Generate stable ID
                                                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                                            use std::hash::{Hash, Hasher};
                                                            serde_json::to_string(func_call).unwrap_or_default().hash(&mut hasher);
                                                            let call_id = format!("call_{:x}", hasher.finish());

                                                            // Emit tool_calls delta
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


                                            // 处理联网搜索引文 (Grounding Metadata) - 流式
                                            if let Some(grounding) = candidate.get("groundingMetadata") {
                                                let mut grounding_text = String::new();

                                                // 1. 处理搜索词
                                                if let Some(queries) = grounding.get("webSearchQueries").and_then(|q| q.as_array()) {
                                                    let query_list: Vec<&str> = queries.iter().filter_map(|v| v.as_str()).collect();
                                                    if !query_list.is_empty() {
                                                        grounding_text.push_str("\n\n---\n**🔍 已为您搜索：** ");
                                                        grounding_text.push_str(&query_list.join(", "));
                                                    }
                                                }

                                                // 2. 处理来源链接 (Chunks)
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

                                            // 只有当 content 和 thought 都为空时才跳过
                                            if content_out.is_empty() && thought_out.is_empty() {
                                                // Skip empty chunks if no text/grounding/thought was found
                                                if candidate.get("finishReason").is_none() {
                                                    continue;
                                                }
                                            }

                                            // Extract finish reason
                                            let finish_reason = candidate.get("finishReason")
                                                .and_then(|f| f.as_str())
                                                .map(|f| match f {
                                                    "STOP" => "stop",
                                                    "MAX_TOKENS" => "length",
                                                    "SAFETY" => "content_filter",
                                                    "RECITATION" => "content_filter",
                                                    _ => f,
                                                });

                                            // Construct OpenAI SSE chunk
                                            // 如果有思考内容，先发送 reasoning_content chunk
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

                                            // 发送正常 content chunk (with chunking for large content like base64 images)
                                            if !content_out.is_empty() || finish_reason.is_some() {
                                                // SSE chunk size limit: 32KB to avoid "Chunk too big" errors in clients like Open WebUI
                                                const MAX_CHUNK_SIZE: usize = 32 * 1024;

                                                if content_out.len() > MAX_CHUNK_SIZE {
                                                    // Large content (likely base64 image) - split into multiple SSE chunks
                                                    let content_bytes = content_out.as_bytes();
                                                    let total_chunks = content_bytes.len().div_ceil(MAX_CHUNK_SIZE);

                                                    for (chunk_idx, chunk) in content_bytes.chunks(MAX_CHUNK_SIZE).enumerate() {
                                                        let is_last_chunk = chunk_idx == total_chunks - 1;

                                                        // UTF-8 safe chunking: find boundary that doesn't split multi-byte chars
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

                    // 发送友好的 SSE 错误事件(包含 i18n_key 供前端翻译)
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

        // Emit usage event if captured before [DONE]
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

        // End of stream signal for OpenAI
        yield Ok::<Bytes, String>(Bytes::from("data: [DONE]\n\n"));
    };

    Box::pin(stream)
}

pub fn create_legacy_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    model: String,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    let mut buffer = BytesMut::new();

    // Generate constant alphanumeric ID (mimics OpenAI base62 format)
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    let random_str: String = (0..28)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    let stream_id = format!("cmpl-{}", random_str);
    let created_ts = Utc::now().timestamp();

    let stream = async_stream::stream! {
        let mut final_usage: Option<super::models::OpenAIUsage> = None;
        while let Some(item) = gemini_stream.next().await {
            match item {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_raw = buffer.split_to(pos + 1);
                        if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                            let line = line_str.trim();
                            if line.is_empty() { continue; }

                            if line.starts_with("data: ") {
                                let json_part = line.trim_start_matches("data: ").trim();
                                if json_part == "[DONE]" { continue; }

                                if let Ok(mut json) = serde_json::from_str::<Value>(json_part) {
                                    let actual_data = if let Some(inner) = json.get_mut("response").map(|v| v.take()) { inner } else { json };

                                    // Capture usageMetadata if present
                                    if let Some(u) = actual_data.get("usageMetadata") {
                                        final_usage = extract_usage_metadata(u);
                                    }

                                    let mut content_out = String::new();
                                    if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                        if let Some(parts) = candidates.first().and_then(|c| c.get("content")).and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                            for part in parts {
                                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                    content_out.push_str(text);
                                                }
                                                /* 禁用思维链输出到正文
                                                if let Some(thought_text) = part.get("thought").and_then(|t| t.as_str()) {
                                                    // // content_out.push_str(thought_text);
                                                }
                                                */
                                                // 捕获 thoughtSignature
                                                // 捕获 thoughtSignature 到全局存储
                                                if let Some(sig) = part.get("thoughtSignature").or(part.get("thought_signature")).and_then(|s| s.as_str()) {
                                                    store_thought_signature(sig);
                                                }
                                            }
                                        }
                                    }

                                    let finish_reason = actual_data.get("candidates")
                                        .and_then(|c| c.as_array())
                                        .and_then(|c| c.first())
                                        .and_then(|c| c.get("finishReason"))
                                        .and_then(|f| f.as_str())
                                        .map(|f| match f {
                                            "STOP" => "stop",
                                            "MAX_TOKENS" => "length",
                                            "SAFETY" => "content_filter",
                                            _ => f,
                                        });

                                    // Construct LEGACY completion chunk - STRICT VERSION
                                    let legacy_chunk = json!({
                                        "id": &stream_id,
                                        "object": "text_completion",
                                        "created": created_ts,
                                        "model": &model,
                                        "choices": [
                                            {
                                                "text": content_out,
                                                "index": 0,
                                                "logprobs": null,
                                                "finish_reason": finish_reason // Will be null if None
                                            }
                                        ]
                                    });

                                    let json_str = serde_json::to_string(&legacy_chunk).unwrap_or_default();
                                    tracing::debug!("Legacy Stream Chunk: {}", json_str);
                                    let sse_out = format!("data: {}\n\n", json_str);
                                    yield Ok::<Bytes, String>(Bytes::from(sse_out));
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
                        "Legacy stream error occurred"
                    );

                    // 发送友好的 SSE 错误事件(包含 i18n_key 供前端翻译)
                    let error_chunk = json!({
                        "id": &stream_id,
                        "object": "text_completion",
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

        // Emit usage event if captured before [DONE]
        if let Some(usage) = final_usage {
            let usage_chunk = json!({
                "id": &stream_id,
                "object": "text_completion",
                "created": created_ts,
                "model": &model,
                "choices": [],
                "usage": usage
            });
            let sse_out = format!("data: {}\n\n", serde_json::to_string(&usage_chunk).unwrap_or_default());
            yield Ok::<Bytes, String>(Bytes::from(sse_out));
        }

        tracing::debug!("Stream finished. Yielding [DONE]");
        yield Ok::<Bytes, String>(Bytes::from("data: [DONE]\n\n"));
        // Final flush delay
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    Box::pin(stream)
}

pub fn create_codex_sse_stream(
    mut gemini_stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    _model: String,
) -> Pin<Box<dyn Stream<Item = Result<Bytes, String>> + Send>> {
    let mut buffer = BytesMut::new();

    // Generate alphanumeric ID
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    let random_str: String = (0..24)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    let response_id = format!("resp-{}", random_str);

    let stream = async_stream::stream! {
        // 1. Emit response.created
        let created_ev = json!({
            "type": "response.created",
            "response": {
                "id": &response_id,
                "object": "response"
            }
        });
        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&created_ev).unwrap_or_default())));

        let mut full_content = String::new();
        let mut emitted_tool_calls = std::collections::HashSet::new();
        let mut last_finish_reason = "stop".to_string();
        let mut _accumulated_usage: Option<super::models::OpenAIUsage> = None;

        while let Some(item) = gemini_stream.next().await {
            match item {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_raw = buffer.split_to(pos + 1);
                        if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                            let line = line_str.trim();
                            if line.is_empty() || !line.starts_with("data: ") { continue; }

                            let json_part = line.trim_start_matches("data: ").trim();
                            if json_part == "[DONE]" { continue; }

                            if let Ok(mut json) = serde_json::from_str::<Value>(json_part) {
                                let actual_data = if let Some(inner) = json.get_mut("response").map(|v| v.take()) { inner } else { json };

                                // Capture usageMetadata if present
                                if let Some(u) = actual_data.get("usageMetadata") {
                                    _accumulated_usage = extract_usage_metadata(u);
                                }

                                // Capture finish reason
                                if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                    if let Some(candidate) = candidates.first() {
                                        if let Some(reason) = candidate.get("finishReason").and_then(|r| r.as_str()) {
                                            last_finish_reason = match reason {
                                                "STOP" => "stop".to_string(),
                                                "MAX_TOKENS" => "length".to_string(),
                                                _ => "stop".to_string(),
                                            };
                                        }
                                    }
                                }

                                // text delta
                                let mut delta_text = String::new();
                                if let Some(candidates) = actual_data.get("candidates").and_then(|c| c.as_array()) {
                                    if let Some(candidate) = candidates.first() {
                                        if let Some(parts) = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                                            for part in parts {
                                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                                    // Sanitize smart quotes to standard quotes for JSON compatibility
                                                    let clean_text = text.replace(['“', '”'], "\"");
                                                    delta_text.push_str(&clean_text);
                                                }
                                                /* 禁用思维链输出到正文
                                                if let Some(thought_text) = part.get("thought").and_then(|t| t.as_str()) {
                                                    let clean_thought = thought_text.replace('"', "\"").replace('"', "\"");
                                                    // delta_text.push_str(&clean_thought);
                                                }
                                                */
                                                // 捕获 thoughtSignature (Gemini 3 工具调用必需)
                                                // 存储到全局状态，不再嵌入到用户可见的文本中
                                                if let Some(sig) = part.get("thoughtSignature").or(part.get("thought_signature")).and_then(|s| s.as_str()) {
                                                    tracing::debug!("[Codex-SSE] 捕获 thoughtSignature (长度: {})", sig.len());
                                                    store_thought_signature(sig);
                                                }
                                                // Handle function call in chunk with deduplication
                                                if let Some(func_call) = part.get("functionCall") {
                                                    let call_key = serde_json::to_string(func_call).unwrap_or_default();
                                                    if !emitted_tool_calls.contains(&call_key) {
                                                        emitted_tool_calls.insert(call_key);

                                                                                let name = func_call.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                                                let _args = func_call.get("args").unwrap_or(&json!({})).to_string();
                                                        // Stable ID generation based on hashed content to be consistent
                                                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                                        use std::hash::{Hash, Hasher};
                                                        serde_json::to_string(func_call).unwrap_or_default().hash(&mut hasher);
                                                        let call_id = format!("call_{:x}", hasher.finish());

                                                        // Parse args once
                                                        let fallback_args = json!({});
                                                        let args_obj = func_call.get("args").unwrap_or(&fallback_args);
                                                        // Fallback for function_call arguments string
                                                        let args_str = args_obj.to_string();

                                                        let name_str = name.to_string();

                                                        // Determine event type based on tool name
                                                        // 使用 Option 来允许某些情况跳过工具调用
                                                        let maybe_item_added_ev: Option<Value> = if name_str == "shell" || name_str == "local_shell" {
                                                            // Map to local_shell_call
                                                            tracing::debug!("[Debug] func_call: {}", serde_json::to_string(&func_call).unwrap_or_default());
                                                            tracing::debug!("[Debug] args_obj: {}", serde_json::to_string(&args_obj).unwrap_or_default());

                                                            // 解析命令：支持数组格式、字符串格式，以及空 args 情况
                                                            let cmd_vec: Vec<String> = if args_obj.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                                                                // args 为空时使用静默成功命令，避免任务中断
                                                                tracing::debug!("shell command args 为空，使用静默成功命令继续流程");
                                                                vec!["powershell.exe".to_string(), "-Command".to_string(), "exit 0".to_string()]
                                                            } else if let Some(arr) = args_obj.get("command").and_then(|v| v.as_array()) {
                                                                // 数组格式
                                                                arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect()
                                                            } else if let Some(cmd_str) = args_obj.get("command").and_then(|v| v.as_str()) {
                                                                // 字符串格式
                                                                if cmd_str.contains(' ') {
                                                                    vec!["powershell.exe".to_string(), "-Command".to_string(), cmd_str.to_string()]
                                                                } else {
                                                                    vec![cmd_str.to_string()]
                                                                }
                                                            } else {
                                                                // command 字段缺失，使用静默成功命令
                                                                tracing::debug!("shell command 缺少 command 字段，使用静默成功命令");
                                                                vec!["powershell.exe".to_string(), "-Command".to_string(), "exit 0".to_string()]
                                                            };

                                                            tracing::debug!("Shell 命令解析: {:?}", cmd_vec);
                                                            Some(json!({
                                                                "type": "response.output_item.added",
                                                                "item": {
                                                                    "type": "local_shell_call",
                                                                    "status": "in_progress",
                                                                    "call_id": &call_id,
                                                                    "action": {
                                                                        "type": "exec",
                                                                        "command": cmd_vec
                                                                    }
                                                                }
                                                            }))
                                                        } else if name_str == "googleSearch" || name_str == "web_search" || name_str == "google_search" {
                                                            // Map to web_search_call
                                                            let query_val = args_obj.get("query").and_then(|v| v.as_str()).unwrap_or("");
                                                            Some(json!({
                                                                "type": "response.output_item.added",
                                                                "item": {
                                                                    "type": "web_search_call",
                                                                    "status": "in_progress",
                                                                    "call_id": &call_id,
                                                                    "action": {
                                                                        "type": "search",
                                                                        "query": query_val
                                                                    }
                                                                }
                                                            }))
                                                        } else {
                                                            // Default function_call
                                                            Some(json!({
                                                                "type": "response.output_item.added",
                                                                "item": {
                                                                    "type": "function_call",
                                                                    "name": name,
                                                                    "arguments": args_str,
                                                                    "call_id": &call_id
                                                                }
                                                            }))
                                                        };

                                                        if let Some(item_added_ev) = maybe_item_added_ev {
                                                            yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&item_added_ev).unwrap_or_default())));

                                                        // Emit response.output_item.done (matching the added event)
                                                        // 复用相同的 cmd_vec 逻辑
                                                        let item_done_ev = if name_str == "shell" || name_str == "local_shell" {
                                                            let cmd_vec_done: Vec<String> = if let Some(arr) = args_obj.get("command").and_then(|v| v.as_array()) {
                                                                arr.iter()
                                                                    .filter_map(|v| v.as_str())
                                                                    .map(|s| s.to_string())
                                                                    .collect()
                                                            } else if let Some(cmd_str) = args_obj.get("command").and_then(|v| v.as_str()) {
                                                                if cmd_str.contains(' ') {
                                                                    vec!["powershell.exe".to_string(), "-Command".to_string(), cmd_str.to_string()]
                                                                } else {
                                                                    vec![cmd_str.to_string()]
                                                                }
                                                            } else {
                                                                vec!["powershell.exe".to_string(), "-Command".to_string(), "echo 'Invalid command'".to_string()]
                                                            };
                                                            json!({
                                                                "type": "response.output_item.done",
                                                                "item": {
                                                                    "type": "local_shell_call",
                                                                    "status": "in_progress",
                                                                    "call_id": call_id,
                                                                     "action": {
                                                                        "type": "exec",
                                                                        "command": cmd_vec_done
                                                                    }
                                                                }
                                                            })
                                                        } else if name_str == "googleSearch" || name_str == "web_search" || name_str == "google_search" {
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
                                                            json!({
                                                                "type": "response.output_item.done",
                                                                "item": {
                                                                    "type": "function_call",
                                                                    "name": name,
                                                                    "arguments": args_str,
                                                                    "call_id": call_id
                                                                }
                                                            })
                                                        };

                                                        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&item_done_ev).unwrap_or_default())));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if !delta_text.is_empty() {
                                    full_content.push_str(&delta_text);
                                    // 2. Emit response.output_text.delta
                                    let delta_ev = json!({
                                        "type": "response.output_text.delta",
                                        "delta": delta_text
                                    });
                                    yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&delta_ev).unwrap_or_default())));
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
                        "Codex stream error occurred"
                    );

                    // 发送友好的错误事件(包含 i18n_key 供前端翻译)
                    let error_ev = json!({
                        "type": "error",
                        "error": {
                            "type": error_type,
                            "message": user_message,
                            "code": "stream_error",
                            "i18n_key": i18n_key
                        }
                    });
                    yield Ok(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&error_ev).unwrap_or_default())));
                    break;
                }
            }
        }

        // 3. Emit response.output_item.done
        let item_done_ev = json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": full_content
                    }
                ]
            }
        });
        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&item_done_ev).unwrap_or_default())));

        // SSOP: Check full_content for embedded JSON command signatures if no tools were emitted natively
        if emitted_tool_calls.is_empty() {
            // Try to find a JSON block containing "command"
            // Simple heuristic: look for { and }
            // We search for the *last* valid JSON block that has a "command" field, as the model might output reasoning first.

            let mut detected_cmd_val = None;
            let mut detected_cmd_type = "unknown";

            // Find all potential JSON start/end indices
            let chars: Vec<char> = full_content.chars().collect();
            let mut depth = 0;
            let mut start_idx = 0;

            // Scan for top-level JSON objects
            for (i, c) in chars.iter().enumerate() {
                if *c == '{' {
                    if depth == 0 { start_idx = i; }
                    depth += 1;
                } else if *c == '}'
                    && depth > 0 {
                        depth -= 1;
                        if depth == 0 {
                            // Found a potential JSON object block [start_idx..=i]
                            let json_str: String = chars[start_idx..=i].iter().collect();
                            if let Ok(val) = serde_json::from_str::<Value>(&json_str) {
                                // Check for "command" field
                                if let Some(cmd_val) = val.get("command") {
                                    // Found a command! Identify type.
                                    // Case 1: "command": ["shell", ...] or ["ls", ...]
                                    if let Some(arr) = cmd_val.as_array() {
                                        if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                                            if first == "shell" || first == "powershell" || first == "cmd" || first == "ls" || first == "git" || first == "echo" {
                                                detected_cmd_type = "shell";
                                                detected_cmd_val = Some(cmd_val.clone());
                                            }
                                        }
                                    }
                                    // Case 2: "command": "shell" (String) and "args": { "command": "..." }
                                    // This matches the user's latest screenshot which failed SSOP.
                                    else if let Some(cmd_str) = cmd_val.as_str() {
                                        if cmd_str == "shell" || cmd_str == "local_shell" {
                                             // Enhanced matching for params/argument
                                             if let Some(args) = val.get("args").or(val.get("arguments")).or(val.get("params")) {
                                                  if let Some(inner_cmd) = args.get("command").or(args.get("code")).or(args.get("argument")) {
                                                      // We construct a synthetic array: ["shell", inner_cmd]
                                                      // So subsequent logic can process it.
                                                      // Actually, let's just grab the inner command string.
                                                      if let Some(inner_cmd_str) = inner_cmd.as_str() {
                                                          detected_cmd_type = "shell";
                                                          detected_cmd_val = Some(json!([inner_cmd_str]));
                                                      }
                                                  }
                                              }
                                        }
                                    }
                                }
                            } else {
                                // Fallback for malformed JSON (e.g. unescaped quotes)
                                // 注意: 使用安全的切片方法避免 UTF-8 边界 panic
                                if (json_str.contains("\"command\": \"shell\"") || json_str.contains("\"command\": \"local_shell\""))
                                   && (json_str.contains("\"argument\":") || json_str.contains("\"code\":")) {

                                    let keys = ["\"argument\":", "\"code\":", "\"command\":"];
                                    for key in keys {
                                        if let Some(pos) = json_str.find(key) {
                                            // 使用安全的 get() 方法替代直接索引
                                            let slice_start = pos + key.len();
                                            if let Some(slice_after_key) = json_str.get(slice_start..) {
                                                if let Some(quote_idx) = slice_after_key.find('"') {
                                                    let val_start_abs = slice_start + quote_idx + 1;
                                                    if let Some(last_quote_idx) = json_str.rfind('"') {
                                                        if last_quote_idx > val_start_abs {
                                                            // 使用 get() 安全获取子字符串
                                                            if let Some(raw_cmd) = json_str.get(val_start_abs..last_quote_idx) {
                                                                detected_cmd_type = "shell";
                                                                detected_cmd_val = Some(json!([raw_cmd]));
                                                                tracing::debug!("SSOP: Recovered malformed JSON command: {}", raw_cmd);
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
            }

            if let Some(cmd_val) = detected_cmd_val {
                if detected_cmd_type == "shell" {
                     let mut hasher = std::collections::hash_map::DefaultHasher::new();
                     use std::hash::{Hash, Hasher};
                     "ssop_shell_call".hash(&mut hasher); // Unique seed
                     serde_json::to_string(&cmd_val).unwrap_or_default().hash(&mut hasher);
                     let call_id = format!("call_{:x}", hasher.finish());

                     let mut cmd_vec: Vec<String> = cmd_val
                         .as_array()
                         .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
                         .unwrap_or_default();

                     // Helper to ensure it runs in shell properly
                     // Problem: Model often outputs ["shell", "powershell", "-Command", ...]
                     // "shell" is not a valid executable on Windows. We must strip it if it's acting as a label.
                     if !cmd_vec.is_empty() && (cmd_vec[0] == "shell" || cmd_vec[0] == "local_shell") {
                         cmd_vec.remove(0);
                     }

                     // Now check if empty or needs wrapping
                     let final_cmd_vec = if cmd_vec.is_empty() {
                         vec!["powershell".to_string(), "-Command".to_string(), "echo 'Empty command'".to_string()]
                     } else if cmd_vec[0] == "powershell" || cmd_vec[0] == "cmd" || cmd_vec[0] == "git" || cmd_vec[0] == "python" || cmd_vec[0] == "node" {
                         cmd_vec
                     } else {
                         // Wrap generic commands (ls, dir, echo, etc) in powershell for Windows safety
                        // Use EncodedCommand to avoid quoting hell
                        // AND pipe to Out-String to avoid CLIXML object output which breaks Gemini
                        let raw_cmd = cmd_vec.join(" ");
                        let joined = format!("& {{ {} }} | Out-String", raw_cmd);
                        let utf16: Vec<u16> = joined.encode_utf16().collect();
                        let mut bytes = Vec::with_capacity(utf16.len() * 2);
                        for c in utf16 {
                            bytes.extend_from_slice(&c.to_le_bytes());
                        }
                        use base64::Engine as _;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

                        vec!["powershell".to_string(), "-EncodedCommand".to_string(), b64]
                    };

                     tracing::debug!("SSOP: Detected Shell Command in Text, Injecting Event: {:?}", final_cmd_vec);

                     // Emit added
                     let item_added_ev = json!({
                        "type": "response.output_item.added",
                        "item": {
                            "type": "local_shell_call",
                            "status": "in_progress",
                            "call_id": &call_id,
                            "action": {
                                "type": "exec",
                                "command": final_cmd_vec
                            }
                        }
                    });
                    yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&item_added_ev).unwrap_or_default())));

                    // Emit done
                    let item_done_ev = json!({
                        "type": "response.output_item.done",
                        "item": {
                            "type": "local_shell_call",
                            "status": "in_progress",
                            "call_id": &call_id,
                             "action": {
                                "type": "exec",
                                "command": final_cmd_vec
                            }
                        }
                    });
        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&item_done_ev).unwrap_or_default())));
                }
            }
        }

        // 4. Emit response.completed
        let completed_ev = json!({
            "type": "response.completed",
            "response": {
                "id": &response_id,
                "object": "response",
                "status": "completed",
                "finish_reason": last_finish_reason,
                "usage": {
                    "input_tokens": 0,
                    "input_tokens_details": { "cached_tokens": 0 },
                    "output_tokens": 0,
                    "output_tokens_details": { "reasoning_tokens": 0 },
                    "total_tokens": 0
                }
            }
        });
        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&completed_ev).unwrap_or_default())));
    };

    Box::pin(stream)
}
