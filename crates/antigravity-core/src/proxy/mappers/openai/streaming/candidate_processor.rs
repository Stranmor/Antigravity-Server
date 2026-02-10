// Candidate and part processing for OpenAI SSE stream transformation.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "SSE streaming: bounded buffer operations, validated indices"
)]

use bytes::Bytes;
use serde_json::{json, Value};

use super::stream_formatters::{
    content_chunk, format_grounding_metadata, generate_call_id, map_finish_reason, reasoning_chunk,
    sse_line, tool_call_chunk,
};
use crate::proxy::SignatureCache;

pub(super) struct CandidateContext<'a> {
    pub stream_id: &'a str,
    pub created_ts: i64,
    pub model: &'a str,
    pub session_id: &'a Option<String>,
    pub accumulated_thinking: &'a mut String,
    pub emitted_tool_calls: &'a mut std::collections::HashSet<String>,
}

pub(super) fn process_candidate(
    candidate: &Value,
    idx: usize,
    ctx: &mut CandidateContext<'_>,
) -> Vec<Bytes> {
    let mut output = Vec::new();
    let parts = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array());

    let mut content_out = String::new();
    let mut thought_out = String::new();

    if let Some(parts_list) = parts {
        for part in parts_list {
            process_part(part, ctx, &mut content_out, &mut thought_out, &mut output, idx);
        }
    }

    if let Some(grounding) = candidate.get("groundingMetadata") {
        let grounding_text = format_grounding_metadata(grounding);
        if !grounding_text.is_empty() {
            content_out.push_str(&grounding_text);
        }
    }

    if content_out.is_empty() && thought_out.is_empty() && candidate.get("finishReason").is_none() {
        return output;
    }

    let finish_reason =
        candidate.get("finishReason").and_then(|f| f.as_str()).map(map_finish_reason);

    if !thought_out.is_empty() {
        let chunk =
            reasoning_chunk(ctx.stream_id, ctx.created_ts, ctx.model, idx as u32, &thought_out);
        output.push(Bytes::from(sse_line(&chunk)));
    }

    if !content_out.is_empty() || finish_reason.is_some() {
        emit_content_chunks(&mut output, ctx, idx, &content_out, finish_reason);
    }

    output
}

fn process_part(
    part: &Value,
    ctx: &mut CandidateContext<'_>,
    content_out: &mut String,
    thought_out: &mut String,
    output: &mut Vec<Bytes>,
    idx: usize,
) {
    let is_thought_part = part.get("thought").and_then(|v| v.as_bool()).unwrap_or(false);

    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
        if is_thought_part {
            thought_out.push_str(text);
            const MAX_THINKING_SIZE: usize = 10 * 1024 * 1024; // 10MB
            if ctx.accumulated_thinking.len() < MAX_THINKING_SIZE {
                let remaining = MAX_THINKING_SIZE.saturating_sub(ctx.accumulated_thinking.len());
                let safe_len = utf8_safe_prefix_len(text, remaining);
                if safe_len > 0 {
                    ctx.accumulated_thinking.push_str(&text[..safe_len]);
                }
            }
        } else {
            content_out.push_str(text);
        }
    }

    if let Some(sig) =
        part.get("thoughtSignature").or(part.get("thought_signature")).and_then(|s| s.as_str())
    {
        cache_signature(sig, ctx);
    }

    if let Some(img) = part.get("inlineData") {
        emit_inline_image(img, output, ctx, idx);
    }

    if let Some(func_call) = part.get("functionCall") {
        emit_function_call(func_call, output, ctx, idx);
    }
}

fn cache_signature(sig: &str, ctx: &mut CandidateContext<'_>) {
    if ctx.accumulated_thinking.is_empty() {
        return;
    }
    let model_family = antigravity_types::ModelFamily::from_model_name(ctx.model);
    SignatureCache::global().cache_content_signature(
        ctx.accumulated_thinking,
        sig.to_string(),
        model_family.as_str().to_string(),
    );
    SignatureCache::global()
        .cache_thinking_family(sig.to_string(), model_family.as_str().to_string());
    if let Some(ref sid) = ctx.session_id {
        SignatureCache::global().cache_session_signature(sid, sig.to_string());
        tracing::debug!(
            "[OpenAI-SSE] Cached session signature (session={}, sig_len={})",
            sid,
            sig.len()
        );
    }
    tracing::debug!(
        "[OpenAI-SSE] Cached content signature (thinking_len={}, sig_len={})",
        ctx.accumulated_thinking.len(),
        sig.len()
    );
}

fn emit_inline_image(img: &Value, output: &mut Vec<Bytes>, ctx: &CandidateContext<'_>, idx: usize) {
    let mime_type = img.get("mimeType").and_then(|v| v.as_str()).unwrap_or("image/png");
    let data = img.get("data").and_then(|v| v.as_str()).unwrap_or("");
    if data.is_empty() {
        return;
    }

    const CHUNK_SIZE: usize = 32 * 1024;
    let prefix = format!("![image](data:{};base64,", mime_type);

    push_content_chunk(output, ctx, idx, &prefix, None);
    for chunk in data.as_bytes().chunks(CHUNK_SIZE) {
        if let Ok(chunk_str) = std::str::from_utf8(chunk) {
            push_content_chunk(output, ctx, idx, chunk_str, None);
        }
    }
    push_content_chunk(output, ctx, idx, ")", None);

    tracing::info!(
        "[OpenAI-SSE] Sent image in {} chunks ({} bytes total)",
        (data.len() / CHUNK_SIZE) + 2,
        data.len()
    );
}

fn emit_function_call(
    func_call: &Value,
    output: &mut Vec<Bytes>,
    ctx: &mut CandidateContext<'_>,
    idx: usize,
) {
    let call_key = serde_json::to_string(func_call).unwrap_or_default();
    if !ctx.emitted_tool_calls.insert(call_key) {
        return;
    }

    let name = func_call.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
    let empty_args = json!({});
    let args_value = func_call.get("args").filter(|value| !value.is_null()).unwrap_or(&empty_args);
    let args = args_value.to_string();
    let call_id = generate_call_id(func_call);

    let chunk = tool_call_chunk(
        ctx.stream_id,
        ctx.created_ts,
        ctx.model,
        idx as u32,
        &call_id,
        name,
        &args,
    );
    output.push(Bytes::from(sse_line(&chunk)));
}

fn emit_content_chunks(
    output: &mut Vec<Bytes>,
    ctx: &CandidateContext<'_>,
    idx: usize,
    content_out: &str,
    finish_reason: Option<&'static str>,
) {
    const MAX_CHUNK_SIZE: usize = 32 * 1024;

    if content_out.len() > MAX_CHUNK_SIZE {
        let chunks = chunk_text(content_out, MAX_CHUNK_SIZE);
        for (chunk_idx, chunk_str) in chunks.iter().enumerate() {
            let is_last_chunk = chunk_idx + 1 == chunks.len();
            let chunk_finish_reason = if is_last_chunk { finish_reason } else { None };
            push_content_chunk(output, ctx, idx, chunk_str, chunk_finish_reason);
        }
    } else {
        push_content_chunk(output, ctx, idx, content_out, finish_reason);
    }
}

fn push_content_chunk(
    output: &mut Vec<Bytes>,
    ctx: &CandidateContext<'_>,
    idx: usize,
    content: &str,
    finish_reason: Option<&'static str>,
) {
    let chunk =
        content_chunk(ctx.stream_id, ctx.created_ts, ctx.model, idx as u32, content, finish_reason);
    match serde_json::to_string(&chunk) {
        Ok(serialized) => output.push(Bytes::from(format!("data: {}\n\n", serialized))),
        Err(error) => {
            tracing::warn!("[OpenAI-SSE] Failed to serialize content chunk: {}", error);
        },
    }
}

fn utf8_safe_prefix_len(text: &str, max_len: usize) -> usize {
    let mut len = max_len.min(text.len());
    while len > 0 && !text.is_char_boundary(len) {
        len = len.saturating_sub(1);
    }
    len
}

fn chunk_text(content: &str, max_chunk_size: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < content.len() {
        let mut end = (start + max_chunk_size).min(content.len());
        while end > start && !content.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        if end == start {
            if let Some((offset, ch)) = content[start..].char_indices().next() {
                end = start + offset + ch.len_utf8();
            } else {
                break;
            }
        }
        chunks.push(&content[start..end]);
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::{chunk_text, utf8_safe_prefix_len};

    #[test]
    fn utf8_safe_prefix_len_respects_boundary() {
        let text = "a😀";
        assert_eq!(utf8_safe_prefix_len(text, 2), 1);
        assert_eq!(utf8_safe_prefix_len(text, 5), 5);
    }

    #[test]
    fn chunk_text_preserves_content() {
        let text = "ab😀cd";
        let chunks = chunk_text(text, 3);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|chunk| !chunk.is_empty()));
    }
}
