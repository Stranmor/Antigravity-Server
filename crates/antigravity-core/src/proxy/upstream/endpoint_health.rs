//! Endpoint Health Tracking (Circuit Breaker for transport errors)
//!
//! Tracks health of upstream endpoints to skip unhealthy ones temporarily.
//! After 5 consecutive failures, an endpoint is skipped for 30 seconds.
//!
//! Note: This map uses String keys and is bounded by the number of endpoints
//! (1 custom or 2 default URLs). No eviction is needed as entries are reused.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub struct EndpointHealth {
    failures: AtomicU32,
    last_failure: std::sync::RwLock<Option<Instant>>,
}

impl EndpointHealth {
    pub fn new() -> Self {
        Self { failures: AtomicU32::new(0), last_failure: std::sync::RwLock::new(None) }
    }

    pub fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut lock) = self.last_failure.write() {
            *lock = Some(Instant::now());
        }
    }

    pub fn record_success(&self) {
        self.failures.store(0, Ordering::Relaxed);
    }

    pub fn should_skip(&self) -> bool {
        let failures = self.failures.load(Ordering::Relaxed);
        if failures < 5 {
            return false;
        }
        if let Ok(lock) = self.last_failure.read() {
            if let Some(last) = *lock {
                return last.elapsed() < Duration::from_secs(30);
            }
        }
        false
    }
}

impl Default for EndpointHealth {
    fn default() -> Self {
        Self::new()
    }
}

pub static ENDPOINT_HEALTH: LazyLock<DashMap<String, EndpointHealth>> = LazyLock::new(DashMap::new);

pub const TRANSPORT_RETRY_DELAY_MS: u64 = 500;
pub const MAX_TRANSPORT_RETRIES_PER_ENDPOINT: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_behavior() {
        let health = EndpointHealth::new();

        assert!(!health.should_skip());

        for _ in 0..4 {
            health.record_failure();
        }
        assert!(!health.should_skip());

        health.record_failure();
        assert!(health.should_skip());

        health.record_success();
        assert!(!health.should_skip());
    }
}
