//! Typed error definitions for Antigravity.
//!
//! These errors provide structured error handling with specific error types
//! for different domains. They are designed to be:
//! - Serializable for API responses
//! - Displayable for logging
//! - Matchable for error handling logic

use serde::{Deserialize, Serialize};

/// Account-related errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "details")]
pub enum AccountError {
    /// Account with given ID not found
    NotFound { id: String },
    /// Account is disabled (manually or due to rate limits)
    Disabled { id: String, reason: Option<String> },
    /// Account token has expired
    TokenExpired { id: String },
    /// Account token refresh failed
    TokenRefreshFailed { id: String, message: String },
    /// Account storage/filesystem error
    StorageError { message: String },
    /// Account validation error
    ValidationError { field: String, message: String },
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { id } => write!(f, "Account not found: {}", id),
            Self::Disabled { id, reason } => {
                write!(f, "Account {} is disabled", id)?;
                if let Some(r) = reason {
                    write!(f, ": {}", r)?;
                }
                Ok(())
            }
            Self::TokenExpired { id } => write!(f, "Token expired for account: {}", id),
            Self::TokenRefreshFailed { id, message } => {
                write!(f, "Failed to refresh token for {}: {}", id, message)
            }
            Self::StorageError { message } => write!(f, "Account storage error: {}", message),
            Self::ValidationError { field, message } => {
                write!(f, "Validation error for {}: {}", field, message)
            }
        }
    }
}

impl std::error::Error for AccountError {}

/// Proxy-related errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "details")]
pub enum ProxyError {
    /// Upstream provider is unavailable
    UpstreamUnavailable { provider: String, message: String },
    /// Rate limited by upstream
    RateLimited {
        provider: String,
        retry_after_secs: Option<u64>,
    },
    /// No available accounts for this request
    NoAvailableAccounts { reason: String },
    /// Request validation failed
    InvalidRequest { message: String },
    /// Model not supported
    UnsupportedModel { model: String },
    /// Circuit breaker is open
    CircuitOpen { provider: String },
    /// Request timeout
    Timeout { duration_secs: u64 },
    /// Internal proxy error
    Internal { message: String },
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UpstreamUnavailable { provider, message } => {
                write!(f, "Upstream {} unavailable: {}", provider, message)
            }
            Self::RateLimited {
                provider,
                retry_after_secs,
            } => {
                write!(f, "Rate limited by {}", provider)?;
                if let Some(secs) = retry_after_secs {
                    write!(f, ", retry after {}s", secs)?;
                }
                Ok(())
            }
            Self::NoAvailableAccounts { reason } => {
                write!(f, "No available accounts: {}", reason)
            }
            Self::InvalidRequest { message } => write!(f, "Invalid request: {}", message),
            Self::UnsupportedModel { model } => write!(f, "Unsupported model: {}", model),
            Self::CircuitOpen { provider } => {
                write!(f, "Circuit breaker open for {}", provider)
            }
            Self::Timeout { duration_secs } => {
                write!(f, "Request timeout after {}s", duration_secs)
            }
            Self::Internal { message } => write!(f, "Internal proxy error: {}", message),
        }
    }
}

impl std::error::Error for ProxyError {}

/// Configuration-related errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "details")]
pub enum ConfigError {
    /// Config file not found
    NotFound { path: String },
    /// Config file parse error
    ParseError { message: String },
    /// Config validation error
    ValidationError { field: String, message: String },
    /// Config write error
    WriteError { message: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { path } => write!(f, "Config not found: {}", path),
            Self::ParseError { message } => write!(f, "Config parse error: {}", message),
            Self::ValidationError { field, message } => {
                write!(f, "Config validation error for {}: {}", field, message)
            }
            Self::WriteError { message } => write!(f, "Config write error: {}", message),
        }
    }
}

impl std::error::Error for ConfigError {}
