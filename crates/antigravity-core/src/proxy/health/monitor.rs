//! Health Monitor implementation.
#![allow(clippy::arithmetic_side_effects, reason = "atomic counter operations")]

use dashmap::DashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::response::build_health_response;
use super::types::{AccountHealth, AccountHealthResponse, ErrorType, HealthConfig};

/// Health Monitor for tracking account health
pub struct HealthMonitor {
    /// Per-account health tracking
    accounts: DashMap<String, Arc<AccountHealth>>,
    /// Configuration
    config: RwLock<HealthConfig>,
    /// Shutdown signal for recovery task
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl HealthMonitor {
    /// Create a new health monitor with default configuration
    pub fn new() -> Arc<Self> {
        Self::with_config(HealthConfig::default())
    }

    /// Create a new health monitor with custom config
    pub fn with_config(config: HealthConfig) -> Arc<Self> {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);

        Arc::new(Self { accounts: DashMap::new(), config: RwLock::new(config), shutdown_tx })
    }

    /// Start the background recovery task
    pub fn start_recovery_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let monitor = Arc::clone(self);
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            loop {
                let interval = {
                    let config = monitor.config.read().await;
                    Duration::from_secs(config.recovery_check_interval_seconds)
                };

                tokio::select! {
                    () = tokio::time::sleep(interval) => {
                        monitor.check_and_recover().await;
                    }
                    _ = shutdown_rx.changed() => {
                        tracing::info!("Health monitor recovery task shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Check and recover accounts that have passed cooldown
    async fn check_and_recover(&self) {
        let config = self.config.read().await;
        let cooldown = Duration::from_secs(config.cooldown_seconds);
        drop(config);

        for entry in &self.accounts {
            let health = entry.value();

            if health.is_disabled() {
                let disabled_at = health.disabled_at.read().await;

                if let Some(disabled_time) = *disabled_at {
                    if disabled_time.elapsed() >= cooldown {
                        drop(disabled_at);

                        // Re-enable the account
                        health.is_disabled.store(false, Ordering::SeqCst);
                        health.consecutive_errors.store(0, Ordering::SeqCst);

                        {
                            let mut disabled_at = health.disabled_at.write().await;
                            *disabled_at = None;
                        }

                        tracing::info!(
                            "🔄 Account {} ({}) auto-recovered after cooldown",
                            health.account_id,
                            health.email
                        );
                    }
                }
            }
        }
    }

    /// Register an account for health monitoring
    pub fn register_account(&self, account_id: String, email: String) {
        self.accounts
            .entry(account_id.clone())
            .or_insert_with(|| Arc::new(AccountHealth::new(account_id, email)));
    }

    /// Remove an account from health monitoring
    pub fn unregister_account(&self, account_id: &str) {
        self.accounts.remove(account_id);
    }

    /// Record a successful request
    pub fn record_success(&self, account_id: &str) {
        if let Some(health) = self.accounts.get(account_id) {
            health.consecutive_errors.store(0, Ordering::SeqCst);
            health.total_successes.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Record an error and potentially disable the account
    /// Returns true if the account was disabled as a result
    pub async fn record_error(
        &self,
        account_id: &str,
        status_code: u16,
        error_message: &str,
    ) -> bool {
        let config = self.config.read().await;

        let Some(error_type) = ErrorType::from_status_code(status_code) else {
            return false;
        };

        if error_type == ErrorType::RateLimited && !config.track_rate_limits {
            return false;
        }

        let threshold = config.error_threshold;
        drop(config);

        if let Some(health) = self.accounts.get(account_id) {
            let errors = health.consecutive_errors.fetch_add(1, Ordering::SeqCst) + 1;
            health.total_errors.fetch_add(1, Ordering::SeqCst);

            {
                let mut last_type = health.last_error_type.write().await;
                *last_type = Some(error_type);
            }
            {
                let mut last_msg = health.last_error_message.write().await;
                *last_msg = Some(truncate_string(error_message, 500));
            }

            tracing::debug!(
                "Account {} ({}) error #{}: {:?} - {}",
                account_id,
                health.email,
                errors,
                error_type,
                error_message
            );

            if errors >= threshold && !health.is_disabled() {
                health.is_disabled.store(true, Ordering::SeqCst);

                {
                    let mut disabled_at = health.disabled_at.write().await;
                    *disabled_at = Some(Instant::now());
                }

                tracing::warn!(
                    "⛔ Account {} ({}) auto-disabled: {} consecutive errors (threshold: {}). \
                     Last error: {:?} - {}",
                    account_id,
                    health.email,
                    errors,
                    threshold,
                    error_type,
                    error_message
                );

                return true;
            }
        }

        false
    }

    /// Force re-enable an account (manual recovery)
    pub async fn force_enable(&self, account_id: &str) -> bool {
        if let Some(health) = self.accounts.get(account_id) {
            if health.is_disabled() {
                health.is_disabled.store(false, Ordering::SeqCst);
                health.consecutive_errors.store(0, Ordering::SeqCst);

                {
                    let mut disabled_at = health.disabled_at.write().await;
                    *disabled_at = None;
                }

                tracing::info!("✅ Account {} ({}) manually re-enabled", account_id, health.email);

                return true;
            }
        }
        false
    }

    pub async fn get_health(&self, account_id: &str) -> Option<AccountHealthResponse> {
        let health = self.accounts.get(account_id)?;
        let config = self.config.read().await;
        let cooldown_secs = config.cooldown_seconds;
        drop(config);

        Some(build_health_response(&health, cooldown_secs).await)
    }

    /// Get health for all accounts
    pub async fn get_all_health(&self) -> Vec<AccountHealthResponse> {
        let mut results = Vec::new();

        for entry in &self.accounts {
            if let Some(health) = self.get_health(entry.key()).await {
                results.push(health);
            }
        }

        results
    }

    /// Check if an account is available (not disabled)
    pub fn is_available(&self, account_id: &str) -> bool {
        self.accounts.get(account_id).is_none_or(|h| !h.is_disabled())
    }

    /// Get current configuration
    pub async fn get_config(&self) -> HealthConfig {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update_config(&self, new_config: HealthConfig) {
        *self.config.write().await = new_config;
        tracing::info!("Health monitor configuration updated");
    }

    /// Clear all health data
    pub fn clear(&self) {
        self.accounts.clear();
        tracing::debug!("Health monitor data cleared");
    }

    /// Get count of disabled accounts
    pub fn disabled_count(&self) -> usize {
        self.accounts.iter().filter(|e| e.value().is_disabled()).count()
    }

    /// Get count of healthy accounts
    pub fn healthy_count(&self) -> usize {
        self.accounts.iter().filter(|e| !e.value().is_disabled()).count()
    }

    pub fn get_score(&self, account_id: &str) -> f32 {
        self.accounts.get(account_id).map(|h| h.score()).unwrap_or(1.0)
    }

    /// Shutdown the monitor
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

impl Drop for HealthMonitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Truncate a string to a maximum length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len).collect();
        result.push('…');
        result
    }
}
