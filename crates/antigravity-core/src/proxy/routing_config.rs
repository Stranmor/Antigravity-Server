//! Smart routing configuration for token selection.
//!
//! Unified algorithm that maximizes cache hits while preventing thundering herd.
//! Replaces the old 3-mode system (CacheFirst/Balance/PerformanceFirst).

/// Unified smart routing configuration.
///
/// Controls how requests are distributed across accounts to optimize
/// cache hit rates while preventing thundering herd on any single account.
#[derive(Debug, Clone)]
pub struct SmartRoutingConfig {
    /// Maximum concurrent requests per account (prevents thundering herd)
    /// Default: 5
    pub max_concurrent_per_account: u32,
    /// AIMD usage ratio threshold for pre-emptive queueing.
    /// When ratio > threshold, wait instead of switching accounts.
    /// Default: 0.8
    pub preemptive_throttle_ratio: f32,
    /// Minimum delay (ms) before retrying same account after soft throttle.
    /// Default: 100
    pub throttle_delay_ms: u64,
    /// Enable session affinity (sticky sessions for cache optimization).
    /// Default: true
    pub enable_session_affinity: bool,
}

impl Default for SmartRoutingConfig {
    fn default() -> Self {
        Self {
            max_concurrent_per_account: 5,
            preemptive_throttle_ratio: 0.8,
            throttle_delay_ms: 100,
            enable_session_affinity: true,
        }
    }
}
