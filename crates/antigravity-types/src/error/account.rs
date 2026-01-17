//! Account-related errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during account operations.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "details")]
pub enum AccountError {
    /// Account with given ID not found
    #[error("Account not found: {id}")]
    NotFound { id: String },

    /// Account is disabled (manually or due to rate limits)
    #[error("Account {id} is disabled: {}", reason.as_deref().unwrap_or("no reason provided"))]
    Disabled { id: String, reason: Option<String> },

    /// Account token has expired and needs refresh
    #[error("Token expired for account: {id}")]
    TokenExpired { id: String },

    /// Account token refresh failed
    #[error("Failed to refresh token for {id}: {message}")]
    TokenRefreshFailed { id: String, message: String },

    /// Account storage/filesystem error
    #[error("Account storage error: {message}")]
    StorageError { message: String },

    /// Account validation error (e.g., invalid config format)
    #[error("Validation error for {field}: {message}")]
    ValidationError { field: String, message: String },

    /// Account pool exhausted (all accounts are disabled/rate-limited)
    #[error("Account pool exhausted: {reason}")]
    PoolExhausted { reason: String },

    /// Concurrent modification conflict
    #[error("Account {id} was modified concurrently")]
    ConcurrentModification { id: String },
}

impl AccountError {
    /// Check if this is a temporary error that may resolve on retry.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::TokenExpired { .. }
                | Self::TokenRefreshFailed { .. }
                | Self::PoolExhausted { .. }
        )
    }

    /// Check if the account should be disabled due to this error.
    pub fn should_disable_account(&self) -> bool {
        matches!(
            self,
            Self::TokenExpired { .. } | Self::TokenRefreshFailed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transient() {
        let transient = AccountError::TokenExpired {
            id: "x".to_string(),
        };
        let permanent = AccountError::NotFound {
            id: "x".to_string(),
        };

        assert!(transient.is_transient());
        assert!(!permanent.is_transient());
    }
}
