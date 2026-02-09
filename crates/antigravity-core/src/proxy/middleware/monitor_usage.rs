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
        .and_then(|v| v.as_u64())
        .or_else(|| usage.get("input_tokens").and_then(|v| v.as_u64()))
        .or_else(|| usage.get("promptTokenCount").and_then(|v| v.as_u64()))
        .map(|v| v as u32);
    log.output_tokens = usage
        .get("completion_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| usage.get("output_tokens").and_then(|v| v.as_u64()))
        .or_else(|| usage.get("candidatesTokenCount").and_then(|v| v.as_u64()))
        .map(|v| v as u32);
    log.cached_tokens = usage
        .get("cachedContentTokenCount")
        .and_then(|v| v.as_u64())
        .or_else(|| usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()))
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_u64())
        })
        .map(|v| v as u32);

    if log.input_tokens.is_none() && log.output_tokens.is_none() {
        // Fallback for providers that only return total tokens without breakdown.
        // We store it in output_tokens for monitoring visibility.
        log.output_tokens = usage
            .get("total_tokens")
            .or(usage.get("totalTokenCount"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
    }
}
