//! Account Health Monitoring Module
//!
//! Provides comprehensive health tracking for proxy accounts with:
//! - Consecutive error tracking per account
//! - Auto-disable on error threshold exceeded
//! - Automatic recovery after cooldown period
//! - State transition logging (enabled -> disabled -> enabled)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  HealthMonitor                                               │
//! │  ├── accounts: DashMap<String, AccountHealth>               │
//! │  ├── recovery_task: Background task for auto-recovery       │
//! │  └── config: HealthConfig                                    │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Health status for an account
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Account is healthy and available for use
    Healthy,
    /// Account has some errors but still usable
    Degraded,
    /// Account is disabled due to excessive errors
    Disabled,
    /// Account is in recovery cooldown period
    Recovering,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Disabled => write!(f, "disabled"),
            HealthStatus::Recovering => write!(f, "recovering"),
        }
    }
}

/// Error types that trigger health monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// 403 Forbidden - Account may be suspended or lack permissions
    Forbidden,
    /// 429 Rate Limited - Too many requests
    RateLimited,
    /// 5xx Server Error - Upstream issues
    ServerError,
    /// 401 Unauthorized - Token may be invalid
    Unauthorized,
}

impl ErrorType {
    pub fn from_status_code(code: u16) -> Option<Self> {
        match code {
            401 => Some(ErrorType::Unauthorized),
            403 => Some(ErrorType::Forbidden),
            429 => Some(ErrorType::RateLimited),
            500..=599 => Some(ErrorType::ServerError),
            _ => None,
        }
    }
}

/// Configuration for health monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Number of consecutive errors before auto-disable (default: 5)
    pub error_threshold: u32,
    /// Cooldown period in seconds before auto-recovery (default: 300 = 5 minutes)
    pub cooldown_seconds: u64,
    /// Whether to track 429 errors (may want to ignore for rate limiting)
    pub track_rate_limits: bool,
    /// Background recovery check interval in seconds (default: 30)
    pub recovery_check_interval_seconds: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            error_threshold: 5,
            cooldown_seconds: 300,    // 5 minutes
            track_rate_limits: false, // 429s are handled by rate_limit.rs
            recovery_check_interval_seconds: 30,
        }
    }
}

/// Health state for a single account
#[derive(Debug)]
pub struct AccountHealth {
    /// Account ID
    pub account_id: String,
    /// Email (for logging)
    pub email: String,
    /// Current consecutive error count
    consecutive_errors: AtomicU32,
    /// Whether account is currently disabled
    is_disabled: AtomicBool,
    /// Timestamp when disabled (for cooldown calculation)
    disabled_at: RwLock<Option<Instant>>,
    /// Last error type encountered
    last_error_type: RwLock<Option<ErrorType>>,
    /// Last error message
    last_error_message: RwLock<Option<String>>,
    /// Total success count (for stats)
    total_successes: AtomicU32,
    /// Total error count (for stats)
    total_errors: AtomicU32,
}

impl AccountHealth {
    pub fn new(account_id: String, email: String) -> Self {
        Self {
            account_id,
            email,
            consecutive_errors: AtomicU32::new(0),
            is_disabled: AtomicBool::new(false),
            disabled_at: RwLock::new(None),
            last_error_type: RwLock::new(None),
            last_error_message: RwLock::new(None),
            total_successes: AtomicU32::new(0),
            total_errors: AtomicU32::new(0),
        }
    }

    /// Get current consecutive error count
    pub fn consecutive_errors(&self) -> u32 {
        self.consecutive_errors.load(Ordering::SeqCst)
    }

    /// Check if account is disabled
    pub fn is_disabled(&self) -> bool {
        self.is_disabled.load(Ordering::SeqCst)
    }

    /// Get total success count
    pub fn total_successes(&self) -> u32 {
        self.total_successes.load(Ordering::SeqCst)
    }

    /// Get total error count
    pub fn total_errors(&self) -> u32 {
        self.total_errors.load(Ordering::SeqCst)
    }
}

/// Response structure for account health endpoint
#[derive(Debug, Clone, Serialize)]
pub struct AccountHealthResponse {
    pub account_id: String,
    pub email: String,
    pub status: HealthStatus,
    pub consecutive_errors: u32,
    pub is_disabled: bool,
    pub disabled_at_unix: Option<u64>,
    pub cooldown_remaining_seconds: Option<u64>,
    pub last_error_type: Option<ErrorType>,
    pub last_error_message: Option<String>,
    pub total_successes: u32,
    pub total_errors: u32,
    pub success_rate: f64,
}

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

        Arc::new(Self {
            accounts: DashMap::new(),
            config: RwLock::new(config),
            shutdown_tx,
        })
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
            // Reset consecutive errors on success
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

        // Determine error type
        let Some(error_type) = ErrorType::from_status_code(status_code) else {
            return false; // Not a trackable error
        };

        // Skip rate limits if configured
        if error_type == ErrorType::RateLimited && !config.track_rate_limits {
            return false;
        }

        let threshold = config.error_threshold;
        drop(config);

        if let Some(health) = self.accounts.get(account_id) {
            // Increment consecutive errors
            let errors = health.consecutive_errors.fetch_add(1, Ordering::SeqCst) + 1;
            health.total_errors.fetch_add(1, Ordering::SeqCst);

            // Update last error info
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

            // Check if we should disable
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

                tracing::info!(
                    "✅ Account {} ({}) manually re-enabled",
                    account_id,
                    health.email
                );

                return true;
            }
        }
        false
    }

    /// Get health status for an account
    pub async fn get_health(&self, account_id: &str) -> Option<AccountHealthResponse> {
        let health = self.accounts.get(account_id)?;
        let config = self.config.read().await;
        let cooldown_secs = config.cooldown_seconds;
        drop(config);

        let consecutive_errors = health.consecutive_errors();
        let is_disabled = health.is_disabled();
        let total_successes = health.total_successes();
        let total_errors = health.total_errors();

        // Calculate status
        let status = if is_disabled {
            let disabled_at = health.disabled_at.read().await;
            if disabled_at.is_some() {
                HealthStatus::Recovering
            } else {
                HealthStatus::Disabled
            }
        } else if consecutive_errors > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        // Calculate cooldown remaining
        let (disabled_at_unix, cooldown_remaining) = {
            let disabled_at = health.disabled_at.read().await;
            if let Some(instant) = *disabled_at {
                let elapsed = instant.elapsed().as_secs();
                let remaining = cooldown_secs.saturating_sub(elapsed);

                // Convert Instant to Unix timestamp (approximate)
                let unix_ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() - elapsed)
                    .ok();

                (unix_ts, Some(remaining))
            } else {
                (None, None)
            }
        };

        let last_error_type = *health.last_error_type.read().await;
        let last_error_message = health.last_error_message.read().await.clone();

        // Calculate success rate
        let total = total_successes + total_errors;
        let success_rate = if total > 0 {
            (f64::from(total_successes) / f64::from(total)) * 100.0
        } else {
            100.0
        };

        Some(AccountHealthResponse {
            account_id: health.account_id.clone(),
            email: health.email.clone(),
            status,
            consecutive_errors,
            is_disabled,
            disabled_at_unix,
            cooldown_remaining_seconds: cooldown_remaining,
            last_error_type,
            last_error_message,
            total_successes,
            total_errors,
            success_rate,
        })
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
        self.accounts
            .get(account_id)
            .is_none_or(|h| !h.is_disabled()) // Unknown accounts are considered available
    }

    /// Get current configuration
    pub async fn get_config(&self) -> HealthConfig {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update_config(&self, new_config: HealthConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
        tracing::info!("Health monitor configuration updated");
    }

    /// Clear all health data
    pub fn clear(&self) {
        self.accounts.clear();
        tracing::debug!("Health monitor data cleared");
    }

    /// Get count of disabled accounts
    pub fn disabled_count(&self) -> usize {
        self.accounts
            .iter()
            .filter(|e| e.value().is_disabled())
            .count()
    }

    /// Get count of healthy accounts
    pub fn healthy_count(&self) -> usize {
        self.accounts
            .iter()
            .filter(|e| !e.value().is_disabled())
            .count()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_threshold() {
        let _config = HealthConfig {
            error_threshold: 3,
            cooldown_seconds: 60,
            track_rate_limits: true,
            recovery_check_interval_seconds: 30,
        };
        let monitor = HealthMonitor::new();

        monitor.register_account("test-1".to_string(), "test@example.com".to_string());

        // First 2 errors should not disable
        assert!(!monitor.record_error("test-1", 500, "Error 1").await);
        assert!(!monitor.record_error("test-1", 500, "Error 2").await);

        // Third error should disable
        assert!(monitor.record_error("test-1", 500, "Error 3").await);

        // Account should be disabled
        assert!(!monitor.is_available("test-1"));
    }

    #[tokio::test]
    async fn test_success_resets_errors() {
        let _config = HealthConfig {
            error_threshold: 3,
            ..Default::default()
        };
        let monitor = HealthMonitor::new();

        monitor.register_account("test-1".to_string(), "test@example.com".to_string());

        // 2 errors
        monitor.record_error("test-1", 500, "Error 1").await;
        monitor.record_error("test-1", 500, "Error 2").await;

        // Success resets
        monitor.record_success("test-1");

        // Need 3 more errors to disable
        monitor.record_error("test-1", 500, "Error 1").await;
        monitor.record_error("test-1", 500, "Error 2").await;

        // Should still be available
        assert!(monitor.is_available("test-1"));
    }

    #[tokio::test]
    async fn test_force_enable() {
        let _config = HealthConfig {
            error_threshold: 1,
            ..Default::default()
        };
        let monitor = HealthMonitor::new();

        monitor.register_account("test-1".to_string(), "test@example.com".to_string());

        // Disable
        monitor.record_error("test-1", 500, "Error").await;
        assert!(!monitor.is_available("test-1"));

        // Force enable
        assert!(monitor.force_enable("test-1").await);
        assert!(monitor.is_available("test-1"));
    }

    #[test]
    fn test_error_type_from_status() {
        assert_eq!(
            ErrorType::from_status_code(401),
            Some(ErrorType::Unauthorized)
        );
        assert_eq!(ErrorType::from_status_code(403), Some(ErrorType::Forbidden));
        assert_eq!(
            ErrorType::from_status_code(429),
            Some(ErrorType::RateLimited)
        );
        assert_eq!(
            ErrorType::from_status_code(500),
            Some(ErrorType::ServerError)
        );
        assert_eq!(
            ErrorType::from_status_code(502),
            Some(ErrorType::ServerError)
        );
        assert_eq!(ErrorType::from_status_code(200), None);
        assert_eq!(ErrorType::from_status_code(400), None);
    }
}
