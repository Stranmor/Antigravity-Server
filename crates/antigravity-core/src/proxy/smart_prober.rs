//! Smart Probing System for Adaptive Rate Limiting
//!
//! Implements speculative hedging and cheap probing strategies to discover
//! rate limit changes without incurring 429 latency penalties.
//!
//! Key strategies:
//! - **Cheap Probe**: Fire-and-forget minimal request (1 token) to test limits
//! - **Delayed Hedge**: Launch secondary request after P95 latency
//! - **Immediate Hedge**: Parallel requests when near limit

use crate::proxy::adaptive_limit::{AdaptiveLimitManager, ProbeStrategy};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Configuration for smart probing
#[derive(Debug, Clone)]
pub struct SmartProberConfig {
    /// P95 latency for LLM requests (delay before hedge)
    pub p95_latency: Duration,
    /// Jitter percentage for hedge delay (±20% = 0.2)
    pub jitter_percent: f64,
    /// Whether to use cheap probes (1 token requests)
    pub enable_cheap_probes: bool,
    /// Whether to use hedging (parallel requests)
    pub enable_hedging: bool,
}

impl Default for SmartProberConfig {
    fn default() -> Self {
        Self {
            p95_latency: Duration::from_millis(2500),
            jitter_percent: 0.2,
            enable_cheap_probes: true,
            enable_hedging: true,
        }
    }
}

/// Result of a hedged request execution
#[derive(Debug)]
pub enum HedgeResult<T> {
    /// Primary request completed first
    PrimaryWon(T),
    /// Hedge (secondary) request completed first
    HedgeWon(T),
    /// No hedging was performed (single request)
    NoHedge(T),
}

impl<T> HedgeResult<T> {
    pub fn into_inner(self) -> T {
        match self {
            HedgeResult::PrimaryWon(v) | HedgeResult::HedgeWon(v) | HedgeResult::NoHedge(v) => v,
        }
    }

    pub fn was_hedged(&self) -> bool {
        !matches!(self, HedgeResult::NoHedge(_))
    }
}

/// Smart prober for adaptive rate limiting
pub struct SmartProber {
    config: SmartProberConfig,
    limits: Arc<AdaptiveLimitManager>,

    // Metrics
    probes_fired: AtomicU64,
    hedges_fired: AtomicU64,
    hedge_wins: AtomicU64,
    primary_wins: AtomicU64,
}

impl SmartProber {
    pub fn new(config: SmartProberConfig, limits: Arc<AdaptiveLimitManager>) -> Self {
        Self {
            config,
            limits,
            probes_fired: AtomicU64::new(0),
            hedges_fired: AtomicU64::new(0),
            hedge_wins: AtomicU64::new(0),
            primary_wins: AtomicU64::new(0),
        }
    }

    /// Get probe strategy for an account
    pub fn strategy_for(&self, account_id: &str) -> ProbeStrategy {
        self.limits.probe_strategy(account_id)
    }

    /// Check if request should be allowed for account
    pub fn should_allow(&self, account_id: &str) -> bool {
        self.limits.should_allow(account_id)
    }

    /// Record successful request
    pub fn record_success(&self, account_id: &str) {
        self.limits.record_success(account_id);
    }

    /// Record 429 error
    pub fn record_429(&self, account_id: &str) {
        self.limits.record_429(account_id);
    }

    /// Calculate delay with jitter for hedging
    fn calculate_hedge_delay(&self) -> Duration {
        let base_ms = self.config.p95_latency.as_millis() as f64;
        let jitter_range = base_ms * self.config.jitter_percent;
        let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
        Duration::from_millis((base_ms + jitter).max(0.0) as u64)
    }

    /// Execute with cheap probe (fire-and-forget calibration)
    pub async fn execute_with_cheap_probe<F, Fut, T, E, PF, PFut>(
        &self,
        account_id: &str,
        primary_fn: F,
        probe_fn: PF,
    ) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        PF: FnOnce() -> PFut + Send + 'static,
        PFut: Future<Output = Result<(), E>> + Send + 'static,
        E: Send + 'static,
    {
        if !self.config.enable_cheap_probes {
            return primary_fn().await;
        }

        let result = primary_fn().await;

        if result.is_ok() {
            self.probes_fired.fetch_add(1, Ordering::Relaxed);
            let limits = self.limits.clone();
            let account = account_id.to_string();

            tokio::spawn(async move {
                match probe_fn().await {
                    Ok(()) => {
                        limits.force_expand(&account);
                        tracing::debug!("🔬 Cheap probe succeeded for {}, limit expanded", account);
                    }
                    Err(_) => {
                        tracing::debug!("🔬 Cheap probe hit limit for {}", account);
                    }
                }
            });
        }

        result
    }

    /// Execute with delayed hedging (secondary fires after P95)
    pub async fn execute_with_delayed_hedge<F, Fut, T, E>(
        &self,
        primary_account: &str,
        secondary_account: &str,
        primary_fn: F,
        secondary_fn: F,
    ) -> Result<HedgeResult<T>, E>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        if !self.config.enable_hedging {
            return primary_fn().await.map(HedgeResult::NoHedge);
        }

        self.hedges_fired.fetch_add(1, Ordering::Relaxed);

        let hedge_delay = self.calculate_hedge_delay();
        let limits = self.limits.clone();
        let primary_id = primary_account.to_string();
        let secondary_id = secondary_account.to_string();

        let primary_handle = tokio::spawn(async move { primary_fn().await });

        let secondary_handle = tokio::spawn(async move {
            tokio::time::sleep(hedge_delay).await;
            secondary_fn().await
        });

        tokio::pin!(primary_handle);
        tokio::pin!(secondary_handle);

        tokio::select! {
            biased;

            primary_result = &mut primary_handle => {
                secondary_handle.abort();

                match primary_result {
                    Ok(Ok(value)) => {
                        self.primary_wins.fetch_add(1, Ordering::Relaxed);
                        limits.record_success(&primary_id);
                        Ok(HedgeResult::PrimaryWon(value))
                    }
                    Ok(Err(e)) => {
                        limits.record_429(&primary_id);
                        Err(e)
                    }
                    Err(e) => panic!("Primary task panicked: {e}"),
                }
            }
            secondary_result = &mut secondary_handle => {
                match secondary_result {
                    Ok(Ok(value)) => {
                        self.hedge_wins.fetch_add(1, Ordering::Relaxed);
                        limits.record_success(&secondary_id);
                        Ok(HedgeResult::HedgeWon(value))
                    }
                    Ok(Err(e)) => {
                        limits.record_429(&secondary_id);
                        Err(e)
                    }
                    Err(e) => panic!("Secondary task panicked: {e}"),
                }
            }
        }
    }

    /// Execute with immediate hedging (both fire immediately)
    pub async fn execute_with_immediate_hedge<F, Fut, T, E>(
        &self,
        primary_account: &str,
        secondary_account: &str,
        primary_fn: F,
        secondary_fn: F,
    ) -> Result<HedgeResult<T>, E>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        if !self.config.enable_hedging {
            return primary_fn().await.map(HedgeResult::NoHedge);
        }

        self.hedges_fired.fetch_add(1, Ordering::Relaxed);

        let limits = self.limits.clone();
        let primary_id = primary_account.to_string();
        let secondary_id = secondary_account.to_string();

        let primary_handle = tokio::spawn(async move { primary_fn().await });
        let secondary_handle = tokio::spawn(async move { secondary_fn().await });

        tokio::pin!(primary_handle);
        tokio::pin!(secondary_handle);

        tokio::select! {
            biased;

            primary_result = &mut primary_handle => {
                secondary_handle.abort();

                match primary_result {
                    Ok(Ok(value)) => {
                        self.primary_wins.fetch_add(1, Ordering::Relaxed);
                        limits.record_success(&primary_id);
                        Ok(HedgeResult::PrimaryWon(value))
                    }
                    Ok(Err(e)) => {
                        limits.record_429(&primary_id);
                        Err(e)
                    }
                    Err(e) => panic!("Primary task panicked: {e}"),
                }
            }
            secondary_result = &mut secondary_handle => {
                match secondary_result {
                    Ok(Ok(value)) => {
                        self.hedge_wins.fetch_add(1, Ordering::Relaxed);
                        limits.record_success(&secondary_id);
                        Ok(HedgeResult::HedgeWon(value))
                    }
                    Ok(Err(e)) => {
                        limits.record_429(&secondary_id);
                        Err(e)
                    }
                    Err(e) => panic!("Secondary task panicked: {e}"),
                }
            }
        }
    }

    // Metrics getters
    pub fn probes_fired(&self) -> u64 {
        self.probes_fired.load(Ordering::Relaxed)
    }

    pub fn hedges_fired(&self) -> u64 {
        self.hedges_fired.load(Ordering::Relaxed)
    }

    pub fn hedge_wins(&self) -> u64 {
        self.hedge_wins.load(Ordering::Relaxed)
    }

    pub fn primary_wins(&self) -> u64 {
        self.primary_wins.load(Ordering::Relaxed)
    }

    pub fn hedge_win_rate(&self) -> f64 {
        let total = self.hedge_wins() + self.primary_wins();
        if total == 0 {
            0.0
        } else {
            self.hedge_wins() as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hedge_result_into_inner() {
        let primary: HedgeResult<i32> = HedgeResult::PrimaryWon(42);
        assert_eq!(primary.into_inner(), 42);

        let hedge: HedgeResult<i32> = HedgeResult::HedgeWon(99);
        assert_eq!(hedge.into_inner(), 99);

        let no_hedge: HedgeResult<i32> = HedgeResult::NoHedge(1);
        assert_eq!(no_hedge.into_inner(), 1);
    }

    #[test]
    fn test_hedge_result_was_hedged() {
        assert!(HedgeResult::PrimaryWon(1).was_hedged());
        assert!(HedgeResult::HedgeWon(1).was_hedged());
        assert!(!HedgeResult::NoHedge(1).was_hedged());
    }

    #[test]
    fn test_config_defaults() {
        let config = SmartProberConfig::default();
        assert_eq!(config.p95_latency, Duration::from_millis(2500));
        assert_eq!(config.jitter_percent, 0.2);
        assert!(config.enable_cheap_probes);
        assert!(config.enable_hedging);
    }

    #[tokio::test]
    async fn test_prober_metrics() {
        let limits = Arc::new(AdaptiveLimitManager::default());
        let prober = SmartProber::new(SmartProberConfig::default(), limits);

        assert_eq!(prober.probes_fired(), 0);
        assert_eq!(prober.hedges_fired(), 0);
        assert_eq!(prober.hedge_win_rate(), 0.0);
    }
}
