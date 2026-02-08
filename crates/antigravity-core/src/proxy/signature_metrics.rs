//! Prometheus metrics for signature validation observability.
//!
//! Tracks signature validation outcomes, cache hit/miss rates,
//! and thinking degradation events.

// Prometheus metrics: bounded counters only.
#![allow(clippy::arithmetic_side_effects, reason = "Prometheus metrics: bounded counters")]

use metrics::{counter, describe_counter};

/// Register all signature-related metric descriptions.
/// Called once from `init_metrics()` in prometheus.rs.
pub(crate) fn init_signature_metrics() {
    describe_counter!(
        "antigravity_signature_validations_total",
        "Signature validation outcomes by result type"
    );
    describe_counter!(
        "antigravity_signature_cache_total",
        "Signature cache operations by cache type and result"
    );
    describe_counter!(
        "antigravity_thinking_degradation_total",
        "Thinking responses with missing signature (quality risk)"
    );
}

/// Record a signature validation outcome.
///
/// Labels: result = "valid" | "dummy_passthrough" | "dummy_short" | "dummy_no_sig" | "recovered_content"
pub(crate) fn record_signature_validation(result: &str) {
    let labels = [("result", result.to_string())];
    counter!("antigravity_signature_validations_total", &labels).increment(1);
}

/// Record a signature cache operation.
///
/// Labels: cache = "session" | "content" | "tool" | "family", op = "hit" | "miss" | "store"
pub(crate) fn record_signature_cache(cache: &str, op: &str) {
    let labels = [("cache", cache.to_string()), ("op", op.to_string())];
    counter!("antigravity_signature_cache_total", &labels).increment(1);
}

/// Record a thinking degradation event (thinking response without cached signature).
pub(crate) fn record_thinking_degradation() {
    counter!("antigravity_thinking_degradation_total").increment(1);
}
