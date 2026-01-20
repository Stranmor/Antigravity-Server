//! Prometheus metrics for Antigravity proxy observability.
//!
//! Exposes metrics compatible with Prometheus/OpenMetrics format:
//! - `antigravity_requests_total{provider,model,status}` - Counter of total requests
//! - `antigravity_request_duration_seconds` - Histogram of request durations
//! - `antigravity_accounts_total` - Gauge of total accounts
//! - `antigravity_accounts_available` - Gauge of available accounts
//! - `antigravity_uptime_seconds` - Gauge of server uptime
//! - `antigravity_log_files_total` - Gauge of total log files
//! - `antigravity_log_disk_bytes` - Gauge of log disk usage in bytes
//! - `antigravity_log_rotations_total` - Counter of log rotation events
//! - `antigravity_log_cleanup_removed_total` - Counter of files removed by cleanup
//! - `antigravity_adaptive_probes_total{strategy}` - Counter of probes by strategy
//! - `antigravity_aimd_rewards_total` - Counter of AIMD limit expansions
//! - `antigravity_aimd_penalties_total` - Counter of AIMD limit contractions
//! - `antigravity_hedge_wins_total` - Counter of hedge request wins
//! - `antigravity_primary_wins_total` - Counter of primary request wins after hedge

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

/// Global Prometheus handle for rendering metrics
static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Global server start time for uptime calculation
static METRICS_START_TIME: OnceLock<Instant> = OnceLock::new();

/// Custom histogram buckets optimized for LLM API latency distribution.
///
/// LLM APIs have bimodal latency patterns:
/// - Fast responses (cache hits, short prompts): 100ms - 1s
/// - Slow responses (long generation, complex reasoning): 5s - 60s+
///
/// These buckets provide granular visibility into both patterns.
const LLM_LATENCY_BUCKETS: &[f64] = &[
    0.1,  // 100ms - very fast (cache hits)
    0.25, // 250ms - fast
    0.5,  // 500ms - typical short response
    1.0,  // 1s - normal response
    2.0,  // 2s - moderate generation
    5.0,  // 5s - longer generation
    10.0, // 10s - complex reasoning
    30.0, // 30s - extended generation
    60.0, // 60s - very long operations
];

/// Initialize Prometheus metrics recorder.
/// Must be called once at application startup before any metrics are recorded.
///
/// Returns the handle that can be used to render metrics as text.
pub fn init_metrics() -> PrometheusHandle {
    let _ = METRICS_START_TIME.get_or_init(Instant::now);

    let handle = PROMETHEUS_HANDLE.get_or_init(|| {
        let builder = PrometheusBuilder::new()
            .set_buckets(LLM_LATENCY_BUCKETS)
            .expect("Failed to set histogram buckets");
        let handle = builder
            .install_recorder()
            .expect("Failed to install Prometheus metrics recorder");

        // Register metric descriptions
        describe_counter!(
            "antigravity_requests_total",
            "Total number of proxy requests processed"
        );
        describe_histogram!(
            "antigravity_request_duration_seconds",
            "Request duration in seconds"
        );
        describe_gauge!(
            "antigravity_accounts_total",
            "Total number of registered accounts"
        );
        describe_gauge!(
            "antigravity_accounts_available",
            "Number of accounts currently available for use"
        );
        describe_gauge!("antigravity_uptime_seconds", "Server uptime in seconds");

        // Log rotation metrics
        describe_gauge!(
            "antigravity_log_files_total",
            "Total number of log files in logs directory"
        );
        describe_gauge!(
            "antigravity_log_disk_bytes",
            "Total disk space used by log files in bytes"
        );
        describe_counter!(
            "antigravity_log_rotations_total",
            "Total number of log file rotations"
        );
        describe_counter!(
            "antigravity_log_cleanup_removed_total",
            "Total number of log files removed by cleanup"
        );

        describe_counter!(
            "antigravity_adaptive_probes_total",
            "Total adaptive rate limit probes by strategy"
        );
        describe_counter!(
            "antigravity_aimd_rewards_total",
            "Total AIMD limit expansions (success above threshold)"
        );
        describe_counter!(
            "antigravity_aimd_penalties_total",
            "Total AIMD limit contractions (429 received)"
        );
        describe_counter!(
            "antigravity_hedge_wins_total",
            "Total times hedge request completed before primary"
        );
        describe_counter!(
            "antigravity_primary_wins_total",
            "Total times primary request won after hedge fired"
        );
        describe_counter!(
            "antigravity_truncations_total",
            "Total output truncations detected (upstream ~4K token limit)"
        );
        describe_gauge!(
            "antigravity_adaptive_limit_gauge",
            "Current working threshold per account"
        );
        describe_counter!(
            "antigravity_peek_retries_total",
            "Total peek phase retries by reason (timeout, heartbeats, error)"
        );
        describe_counter!(
            "antigravity_peek_heartbeats_total",
            "Total heartbeats skipped during peek phase"
        );

        handle
    });

    handle.clone()
}

/// Get the Prometheus handle for rendering metrics.
/// Returns None if metrics have not been initialized.
pub fn get_prometheus_handle() -> Option<&'static PrometheusHandle> {
    PROMETHEUS_HANDLE.get()
}

/// Record a completed request with labels.
///
/// # Arguments
/// * `provider` - The API provider (e.g., "anthropic", "openai", "gemini")
/// * `model` - The model name (e.g., "claude-3-opus", "gpt-4")
/// * `status` - HTTP status code category ("2xx", "4xx", "5xx")
/// * `duration_ms` - Request duration in milliseconds
pub fn record_request(provider: &str, model: &str, status: &str, duration_ms: u64) {
    let labels = [
        ("provider", provider.to_string()),
        ("model", model.to_string()),
        ("status", status.to_string()),
    ];

    counter!("antigravity_requests_total", &labels).increment(1);

    // Convert milliseconds to seconds for histogram
    let duration_seconds = duration_ms as f64 / 1000.0;
    histogram!("antigravity_request_duration_seconds", &labels).record(duration_seconds);
}

/// Update account gauges.
///
/// # Arguments
/// * `total` - Total number of accounts
/// * `available` - Number of available accounts
pub fn update_account_gauges(total: usize, available: usize) {
    gauge!("antigravity_accounts_total").set(total as f64);
    gauge!("antigravity_accounts_available").set(available as f64);
}

/// Update uptime gauge.
/// Should be called periodically or on metrics render.
pub fn update_uptime_gauge() {
    if let Some(start) = METRICS_START_TIME.get() {
        let uptime = start.elapsed().as_secs_f64();
        gauge!("antigravity_uptime_seconds").set(uptime);
    }
}

/// Update log rotation metrics by scanning the log directory.
///
/// # Arguments
/// * `log_dir` - Path to the logs directory
///
/// # Returns
/// * `(file_count, disk_bytes)` - Tuple of file count and total disk usage
pub fn update_log_rotation_gauges(log_dir: &Path) -> (usize, u64) {
    let mut file_count = 0usize;
    let mut disk_bytes = 0u64;

    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "log" | "gz") {
                    file_count += 1;
                    if let Ok(metadata) = path.metadata() {
                        disk_bytes += metadata.len();
                    }
                }
            }
        }
    }

    gauge!("antigravity_log_files_total").set(file_count as f64);
    gauge!("antigravity_log_disk_bytes").set(disk_bytes as f64);

    (file_count, disk_bytes)
}

/// Increment the log rotation counter.
/// Call this after a log file rotation occurs.
pub fn record_log_rotation() {
    counter!("antigravity_log_rotations_total").increment(1);
}

/// Increment the log cleanup counter by the number of files removed.
/// Call this after cleanup removes old log files.
///
/// # Arguments
/// * `count` - Number of files removed
pub fn record_log_cleanup(count: usize) {
    if count > 0 {
        counter!("antigravity_log_cleanup_removed_total").increment(count as u64);
    }
}

pub fn record_adaptive_probe(strategy: &str) {
    let labels = [("strategy", strategy.to_string())];
    counter!("antigravity_adaptive_probes_total", &labels).increment(1);
}

pub fn record_aimd_reward() {
    counter!("antigravity_aimd_rewards_total").increment(1);
}

pub fn record_aimd_penalty() {
    counter!("antigravity_aimd_penalties_total").increment(1);
}

pub fn record_hedge_win() {
    counter!("antigravity_hedge_wins_total").increment(1);
}

pub fn record_primary_win() {
    counter!("antigravity_primary_wins_total").increment(1);
}

pub fn record_truncation() {
    counter!("antigravity_truncations_total").increment(1);
}

pub fn update_adaptive_limit_gauge(account_id: &str, working_threshold: u64) {
    let labels = [("account_id", account_id.to_string())];
    gauge!("antigravity_adaptive_limit_gauge", &labels).set(working_threshold as f64);
}

pub fn record_peek_retry(reason: &str) {
    let labels = [("reason", reason.to_string())];
    counter!("antigravity_peek_retries_total", &labels).increment(1);
}

pub fn record_peek_heartbeat() {
    counter!("antigravity_peek_heartbeats_total").increment(1);
}

/// Render all metrics in Prometheus text format.
pub fn render_metrics() -> String {
    update_uptime_gauge();

    if let Some(handle) = get_prometheus_handle() {
        handle.render()
    } else {
        String::from("# Metrics not initialized\n")
    }
}

/// Determine the provider from URL path.
pub fn detect_provider_from_url(url: &str) -> &'static str {
    if url.contains("/v1/messages") || url.contains("/v1/models/claude") {
        "anthropic"
    } else if url.contains("/v1beta/models") {
        "gemini"
    } else if url.contains("/v1/chat/completions")
        || url.contains("/v1/completions")
        || url.contains("/v1/models")
        || url.contains("/v1/images")
    {
        "openai"
    } else if url.contains("/mcp/") {
        "mcp"
    } else {
        "unknown"
    }
}

/// Convert HTTP status code to category for metrics labels.
pub fn status_category(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_category() {
        assert_eq!(status_category(200), "2xx");
        assert_eq!(status_category(404), "4xx");
        assert_eq!(status_category(500), "5xx");
        assert_eq!(status_category(301), "3xx");
    }

    #[test]
    fn test_detect_provider() {
        assert_eq!(detect_provider_from_url("/v1/messages"), "anthropic");
        assert_eq!(detect_provider_from_url("/v1/chat/completions"), "openai");
        assert_eq!(
            detect_provider_from_url("/v1beta/models/gemini-pro"),
            "gemini"
        );
        assert_eq!(detect_provider_from_url("/mcp/web_search"), "mcp");
    }
}
