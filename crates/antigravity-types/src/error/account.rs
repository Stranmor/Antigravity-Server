//! Account-related errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during account operations.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "details")]
pub enum AccountError {
    /// Account with given ID not found
    #[error("Account not found: {id}")]
    NotFound {
        /// Unique identifier of the missing account
        id: String,
    },

    /// Account is disabled (manually or due to rate limits)
    #[error("Account {id} is disabled: {}", reason.as_deref().unwrap_or("no reason provided"))]
    Disabled {
        /// Unique identifier of the disabled account
        id: String,
        /// Optional explanation for why the account was disabled
        reason: Option<String>,
    },

    /// Account token has expired and needs refresh
    #[error("Token expired for account: {id}")]
    TokenExpired {
        /// Unique identifier of the account with expired token
        id: String,
    },

    /// Account token refresh failed
    #[error("Failed to refresh token for {id}: {message}")]
    TokenRefreshFailed {
        /// Unique identifier of the account
        id: String,
        /// Details about the refresh failure
        message: String,
    },

    /// Account storage/filesystem error
    #[error("Account storage error: {message}")]
    StorageError {
        /// Description of the storage failure
        message: String,
    },

    /// Account validation error (e.g., invalid config format)
    #[error("Validation error for {field}: {message}")]
    ValidationError {
        /// Name of the field that failed validation
        field: String,
        /// Description of the validation failure
        message: String,
    },

    /// Account pool exhausted (all accounts are disabled/rate-limited)
    #[error("Account pool exhausted: {reason}")]
    PoolExhausted {
        /// Explanation of why no accounts are available
        reason: String,
    },

    /// Concurrent modification conflict
    #[error("Account {id} was modified concurrently")]
    ConcurrentModification {
        /// Unique identifier of the account with conflict
        id: String,
    },
}

impl AccountError {
    /// Check if this is a temporary error that may resolve on retry.
    pub const fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::TokenExpired { .. }
                | Self::TokenRefreshFailed { .. }
                | Self::PoolExhausted { .. }
        )
    }

    /// Check if the account should be disabled due to this error.
    pub const fn should_disable_account(&self) -> bool {
        matches!(self, Self::TokenExpired { .. } | Self::TokenRefreshFailed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transient() {
        let transient = AccountError::TokenExpired { id: "x".to_string() };
        let permanent = AccountError::NotFound { id: "x".to_string() };

        assert!(transient.is_transient());
        assert!(!permanent.is_transient());
    }
}
