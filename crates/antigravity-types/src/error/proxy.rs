//! Proxy-related errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during proxy operations.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "details")]
pub enum ProxyError {
    /// Upstream provider is unavailable (network error, 5xx, etc)
    #[error("Upstream {provider} unavailable: {message}")]
    UpstreamUnavailable { provider: String, message: String },

    /// Rate limited by upstream (429)
    #[error("Rate limited by {provider}{}", retry_after_secs.map(|s| format!(", retry after {}s", s)).unwrap_or_default())]
    RateLimited {
        provider: String,
        retry_after_secs: Option<u64>,
    },

    /// No available accounts for this request
    #[error("No available accounts: {reason}")]
    NoAvailableAccounts { reason: String },

    /// Request validation failed
    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    /// Model not supported or not found
    #[error("Unsupported model: {model}")]
    UnsupportedModel { model: String },

    /// Model routing failed (no route configured)
    #[error("No route for model: {model}")]
    NoRoute { model: String },

    /// Circuit breaker is open (too many failures)
    #[error("Circuit breaker open for {provider}")]
    CircuitOpen { provider: String },

    /// Request timed out
    #[error("Request timeout after {duration_secs}s")]
    Timeout { duration_secs: u64 },

    /// Stream error during SSE transmission
    #[error("Stream error: {message}")]
    StreamError { message: String },

    /// Authentication error (invalid token, etc)
    #[error("Authentication failed for {provider}: {message}")]
    AuthenticationFailed { provider: String, message: String },

    /// Internal proxy error (bugs, unexpected states)
    #[error("Internal proxy error: {message}")]
    Internal { message: String },
}

impl ProxyError {
    /// Check if this error should trigger account rotation.
    pub fn should_rotate_account(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::AuthenticationFailed { .. }
        )
    }

    /// Check if this error should trigger circuit breaker.
    pub fn should_trip_circuit(&self) -> bool {
        matches!(
            self,
            Self::UpstreamUnavailable { .. } | Self::Timeout { .. }
        )
    }

    /// Check if this is a client error (4xx equivalent).
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidRequest { .. } | Self::UnsupportedModel { .. } | Self::NoRoute { .. }
        )
    }

    /// Get HTTP status code for this error.
    pub fn http_status_code(&self) -> u16 {
        match self {
            Self::UpstreamUnavailable { .. } => 502,
            Self::RateLimited { .. } => 429,
            Self::NoAvailableAccounts { .. } => 503,
            Self::InvalidRequest { .. } => 400,
            Self::UnsupportedModel { .. } | Self::NoRoute { .. } => 404,
            Self::CircuitOpen { .. } => 503,
            Self::Timeout { .. } => 504,
            Self::StreamError { .. } => 502,
            Self::AuthenticationFailed { .. } => 401,
            Self::Internal { .. } => 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_status_codes() {
        assert_eq!(
            ProxyError::RateLimited {
                provider: "claude".to_string(),
                retry_after_secs: None
            }
            .http_status_code(),
            429
        );

        assert_eq!(
            ProxyError::InvalidRequest {
                message: "bad".to_string()
            }
            .http_status_code(),
            400
        );
    }

    #[test]
    fn test_should_rotate() {
        let rate_limited = ProxyError::RateLimited {
            provider: "claude".to_string(),
            retry_after_secs: Some(60),
        };
        let timeout = ProxyError::Timeout { duration_secs: 30 };

        assert!(rate_limited.should_rotate_account());
        assert!(!timeout.should_rotate_account());
    }
}
