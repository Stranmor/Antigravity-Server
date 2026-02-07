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
    // Claude/Anthropic: content_block_delta with delta.text
    if let Some(delta) = json.get("delta") {
        if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
            if collected_text.len() + text.len() <= max_len {
                collected_text.push_str(text);
            }
        }
    }
    // OpenAI: choices[0].delta.content
    if let Some(choices) = json.get("choices").and_then(|v| v.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(content) =
                choice.get("delta").and_then(|d| d.get("content")).and_then(|v| v.as_str())
            {
                if collected_text.len() + content.len() <= max_len {
                    collected_text.push_str(content);
                }
            }
        }
    }
    // Gemini: candidates[0].content.parts[0].text
    if let Some(candidates) = json.get("candidates").and_then(|v| v.as_array()) {
        if let Some(candidate) = candidates.first() {
            if let Some(parts) =
                candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array())
            {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        if collected_text.len() + text.len() <= max_len {
                            collected_text.push_str(text);
                        }
                    }
                }
            }
        }
    }
}
