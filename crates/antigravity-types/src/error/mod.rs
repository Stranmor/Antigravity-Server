//! Typed error definitions for Antigravity.
//!
//! This module provides a structured error hierarchy with specific error types
//! for different domains. All errors are designed to be:
//!
//! - **Serializable** for API responses via serde
//! - **Displayable** for logging via Display trait
//! - **Matchable** for error handling logic via enum variants
//! - **Composable** via thiserror derive macros

mod account;
mod config;
mod proxy;

pub use account::AccountError;
pub use config::ConfigError;
pub use proxy::ProxyError;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Unified error type that wraps all domain-specific errors.
///
/// Use this when you need a single error type that can represent
/// any Antigravity error.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "domain", content = "error")]
pub enum TypedError {
    /// Wraps an account-related error
    #[error("Account error: {0}")]
    Account(#[from] AccountError),

    /// Wraps a proxy operation error
    #[error("Proxy error: {0}")]
    Proxy(#[from] ProxyError),

    /// Wraps a configuration error
    #[error("Config error: {0}")]
    Config(#[from] ConfigError),
}

/// Standard Result type using TypedError.
pub type Result<T> = std::result::Result<T, TypedError>;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serialization() {
        let err = TypedError::Account(AccountError::NotFound { id: "test-123".to_string() });

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Account"));
        assert!(json.contains("test-123"));

        let deserialized: TypedError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, deserialized);
    }

    #[test]
    fn test_error_display() {
        let err =
            ProxyError::RateLimited { provider: "claude".to_string(), retry_after_secs: Some(60) };

        let msg = format!("{}", err);
        assert!(msg.contains("claude"));
        assert!(msg.contains("60"));
    }
}
