//! Health monitoring types and data structures.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;
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
    pub(crate) consecutive_errors: AtomicU32,
    /// Whether account is currently disabled
    pub(crate) is_disabled: AtomicBool,
    /// Timestamp when disabled (for cooldown calculation)
    pub(crate) disabled_at: RwLock<Option<Instant>>,
    /// Last error type encountered
    pub(crate) last_error_type: RwLock<Option<ErrorType>>,
    /// Last error message
    pub(crate) last_error_message: RwLock<Option<String>>,
    /// Total success count (for stats)
    pub(crate) total_successes: AtomicU32,
    /// Total error count (for stats)
    pub(crate) total_errors: AtomicU32,
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
