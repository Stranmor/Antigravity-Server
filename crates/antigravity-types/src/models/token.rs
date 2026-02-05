//! Token data model.

use serde::{Deserialize, Serialize};

/// OAuth token data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenData {
    /// OAuth access token
    pub access_token: String,
    /// OAuth refresh token for renewing access
    pub refresh_token: String,
    /// Token validity duration in seconds
    pub expires_in: i64,
    /// Absolute timestamp when token expires
    pub expiry_timestamp: i64,
    /// Token type (usually "Bearer")
    pub token_type: String,
    /// Email associated with the token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Google Cloud project ID for API requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Antigravity session ID for prompt caching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl TokenData {
    /// Create new token data.
    pub fn new(
        access_token: String,
        refresh_token: String,
        expires_in: i64,
        email: Option<String>,
        project_id: Option<String>,
        session_id: Option<String>,
    ) -> Self {
        let expiry_timestamp = chrono::Utc::now().timestamp().saturating_add(expires_in);
        Self {
            access_token,
            refresh_token,
            expires_in,
            expiry_timestamp,
            token_type: "Bearer".to_string(),
            email,
            project_id,
            session_id,
        }
    }

    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() >= self.expiry_timestamp
    }

    /// Check if the token will expire within the given seconds.
    pub fn expires_within(&self, seconds: i64) -> bool {
        chrono::Utc::now().timestamp().saturating_add(seconds) >= self.expiry_timestamp
    }

    /// Get remaining validity in seconds (0 if already expired).
    pub fn remaining_seconds(&self) -> i64 {
        let remaining = self.expiry_timestamp.saturating_sub(chrono::Utc::now().timestamp());
        remaining.max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_expiry_check() {
        let token =
            TokenData::new("access".to_string(), "refresh".to_string(), 3600, None, None, None);

        assert!(!token.is_expired());
        assert!(token.expires_within(3601));
        assert!(!token.expires_within(3599));
    }
}
