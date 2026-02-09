// Usage extraction from SSE and JSON responses for monitoring.
// All arithmetic is on usize for buffer sizes, bounded by MAX_*_LOG_SIZE constants.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "Monitoring middleware: bounded buffer sizes, safe byte operations"
)]

use crate::proxy::monitor::ProxyRequestLog;
use serde_json::Value;

pub(super) fn extract_usage_from_json(usage: &Value, log: &mut ProxyRequestLog) {
    log.input_tokens = usage
        .get("prompt_tokens")
        .or(usage.get("input_tokens"))
        .or(usage.get("promptTokenCount"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    log.output_tokens = usage
        .get("completion_tokens")
        .or(usage.get("output_tokens"))
        .or(usage.get("candidatesTokenCount"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    log.cached_tokens = usage
        .get("cachedContentTokenCount")
        .or(usage.get("cache_read_input_tokens"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_u64())
        })
        .map(|v| v as u32);

    if log.input_tokens.is_none() && log.output_tokens.is_none() {
        log.output_tokens = usage
            .get("total_tokens")
            .or(usage.get("totalTokenCount"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
    }
}

pub(super) fn extract_text_from_sse_line(
    json: &Value,
    collected_text: &mut String,
    max_len: usize,
) {
    // Claude/Anthropic: content_block_delta with delta.text or delta.thinking
    if let Some(delta) = json.get("delta") {
        if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
            append_bounded(collected_text, text, max_len);
        }
        if let Some(thinking) = delta.get("thinking").and_then(|v| v.as_str()) {
            append_bounded(collected_text, thinking, max_len);
        }
    }
    // OpenAI: choices[0].delta.content + reasoning_content + tool_calls
    if let Some(choices) = json.get("choices").and_then(|v| v.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(delta) = choice.get("delta") {
                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                    append_bounded(collected_text, content, max_len);
                }
                if let Some(reasoning) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                    append_bounded(collected_text, reasoning, max_len);
                }
                extract_tool_calls_text(delta, collected_text, max_len);
            }
        }
    }
    // Gemini: candidates[0].content.parts[0].text + thought
    if let Some(candidates) = json.get("candidates").and_then(|v| v.as_array()) {
        if let Some(candidate) = candidates.first() {
            if let Some(parts) =
                candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array())
            {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        append_bounded(collected_text, text, max_len);
                    }
                }
            }
        }
    }
    // Codex/Responses API: {"type":"response.output_text.delta","delta":"text"}
    if json.get("type").and_then(|v| v.as_str()) == Some("response.output_text.delta") {
        if let Some(delta_str) = json.get("delta").and_then(|v| v.as_str()) {
            append_bounded(collected_text, delta_str, max_len);
        }
    }
}

fn append_bounded(buf: &mut String, text: &str, max_len: usize) {
    if buf.len() + text.len() <= max_len {
        buf.push_str(text);
    }
}

fn extract_tool_calls_text(delta: &Value, collected_text: &mut String, max_len: usize) {
    let tool_calls = match delta.get("tool_calls").and_then(|v| v.as_array()) {
        Some(tc) => tc,
        None => return,
    };
    for tc in tool_calls {
        if let Some(func) = tc.get("function") {
            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                let label = format!("[tool_call: {}] ", name);
                append_bounded(collected_text, &label, max_len);
            }
            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                append_bounded(collected_text, args, max_len);
            }
        }
    }
}
