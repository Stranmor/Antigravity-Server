//! Proxy-related errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during proxy operations.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "details")]
pub enum ProxyError {
    /// Upstream provider is unavailable (network error, 5xx, etc)
    #[error("Upstream {provider} unavailable: {message}")]
    UpstreamUnavailable {
        /// Name of the upstream provider (e.g., "gemini", "claude")
        provider: String,
        /// Detailed error message from the upstream
        message: String,
    },

    /// Rate limited by upstream (429)
    #[error("Rate limited by {provider}{}", retry_after_secs.map(|s| format!(", retry after {s}s")).unwrap_or_default())]
    RateLimited {
        /// Name of the provider that rate limited the request
        provider: String,
        /// Seconds to wait before retrying (from Retry-After header)
        retry_after_secs: Option<u64>,
    },

    /// No available accounts for this request
    #[error("No available accounts: {reason}")]
    NoAvailableAccounts {
        /// Explanation of why no accounts are available
        reason: String,
    },

    /// Request validation failed
    #[error("Invalid request: {message}")]
    InvalidRequest {
        /// Description of what validation failed
        message: String,
    },

    /// Model not supported or not found
    #[error("Unsupported model: {model}")]
    UnsupportedModel {
        /// The model identifier that was requested
        model: String,
    },

    /// Model routing failed (no route configured)
    #[error("No route for model: {model}")]
    NoRoute {
        /// The model identifier with no configured route
        model: String,
    },

    /// Circuit breaker is open (too many failures)
    #[error("Circuit breaker open for {provider}")]
    CircuitOpen {
        /// Provider whose circuit breaker has tripped
        provider: String,
    },

    /// Request timed out
    #[error("Request timeout after {duration_secs}s")]
    Timeout {
        /// How long the request waited before timing out
        duration_secs: u64,
    },

    /// Stream error during SSE transmission
    #[error("Stream error: {message}")]
    StreamError {
        /// Description of the streaming failure
        message: String,
    },

    /// Authentication error (invalid token, etc)
    #[error("Authentication failed for {provider}: {message}")]
    AuthenticationFailed {
        /// Provider that rejected authentication
        provider: String,
        /// Details about the authentication failure
        message: String,
    },

    /// Internal proxy error (bugs, unexpected states)
    #[error("Internal proxy error: {message}")]
    Internal {
        /// Description of the internal error
        message: String,
    },
}

impl ProxyError {
    /// Check if this error should trigger account rotation.
    pub const fn should_rotate_account(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::AuthenticationFailed { .. })
    }

    /// Check if this error should trigger circuit breaker.
    pub const fn should_trip_circuit(&self) -> bool {
        matches!(self, Self::UpstreamUnavailable { .. } | Self::Timeout { .. })
    }

    /// Check if this is a client error (4xx equivalent).
    pub const fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidRequest { .. } | Self::UnsupportedModel { .. } | Self::NoRoute { .. }
        )
    }

    /// Get HTTP status code for this error.
    pub const fn http_status_code(&self) -> u16 {
        match *self {
            Self::UpstreamUnavailable { .. } | Self::StreamError { .. } => 502,
            Self::RateLimited { .. } => 429,
            Self::NoAvailableAccounts { .. } | Self::CircuitOpen { .. } => 503,
            Self::InvalidRequest { .. } => 400,
            Self::UnsupportedModel { .. } | Self::NoRoute { .. } => 404,
            Self::Timeout { .. } => 504,
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
            ProxyError::RateLimited { provider: "claude".to_string(), retry_after_secs: None }
                .http_status_code(),
            429
        );

        assert_eq!(
            ProxyError::InvalidRequest { message: "bad".to_string() }.http_status_code(),
            400
        );
    }

    #[test]
    fn test_should_rotate() {
        let rate_limited =
            ProxyError::RateLimited { provider: "claude".to_string(), retry_after_secs: Some(60) };
        let timeout = ProxyError::Timeout { duration_secs: 30 };

        assert!(rate_limited.should_rotate_account());
        assert!(!timeout.should_rotate_account());
    }
}
