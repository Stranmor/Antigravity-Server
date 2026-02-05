//! Rate limit tracking types and keys.
//!
//! This module provides types for tracking rate limits per account
//! and per model, with support for different rate limit reasons.

use std::time::SystemTime;

/// Reason for a rate limit being applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RateLimitReason {
    /// Daily or monthly quota exhausted.
    QuotaExhausted,
    /// Too many requests per minute/second.
    RateLimitExceeded,
    /// Model-specific capacity exhausted.
    ModelCapacityExhausted,
    /// Server returned 5xx error.
    ServerError,
    /// Unknown rate limit reason.
    Unknown,
}

/// Key for identifying rate-limited resources.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum RateLimitKey {
    /// Rate limit applies to entire account.
    Account(String),
    /// Rate limit applies to specific model on account.
    Model {
        /// Account identifier.
        account: String,
        /// Model name.
        model: String,
    },
}

impl RateLimitKey {
    /// Creates an account-level rate limit key.
    pub fn account(account_id: &str) -> Self {
        RateLimitKey::Account(account_id.to_string())
    }

    /// Creates a model-specific rate limit key.
    pub fn model(account_id: &str, model: &str) -> Self {
        RateLimitKey::Model { account: account_id.to_string(), model: model.to_string() }
    }

    /// Creates a rate limit key from optional model.
    pub fn from_optional_model(account_id: &str, model: Option<&str>) -> Self {
        match model {
            Some(m) => RateLimitKey::model(account_id, m),
            None => RateLimitKey::account(account_id),
        }
    }

    /// Returns the account ID.
    pub fn account_id(&self) -> &str {
        match self {
            RateLimitKey::Account(acc) => acc,
            RateLimitKey::Model { account, .. } => account,
        }
    }

    /// Returns the model name if this is a model-specific key.
    pub fn model_name(&self) -> Option<&str> {
        match self {
            RateLimitKey::Account(_) => None,
            RateLimitKey::Model { model, .. } => Some(model),
        }
    }
}

impl std::fmt::Display for RateLimitKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitKey::Account(acc) => write!(f, "{}", acc),
            RateLimitKey::Model { account, model } => write!(f, "{}:{}", account, model),
        }
    }
}

/// Information about an active rate limit.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// When the rate limit resets.
    pub reset_time: SystemTime,
    /// Seconds until reset (from Retry-After header).
    #[allow(dead_code)]
    pub retry_after_sec: u64,
    /// When the rate limit was detected.
    #[allow(dead_code)]
    pub detected_at: SystemTime,
    /// Reason for the rate limit.
    #[allow(dead_code)]
    pub reason: RateLimitReason,
    /// Model that triggered the limit, if any.
    #[allow(dead_code)]
    pub model: Option<String>,
}
