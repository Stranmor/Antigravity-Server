//! Account model and related types.

use super::{QuotaData, TokenData};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Account data structure representing a user's API account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    /// Unique identifier for the account
    pub id: String,
    /// Email address associated with the account
    pub email: String,
    /// Optional display name
    pub name: Option<String>,
    /// Authentication token data
    pub token: TokenData,
    /// Current quota information
    pub quota: Option<QuotaData>,
    /// Whether the account is disabled globally
    #[serde(default)]
    pub disabled: bool,
    /// Reason for global disable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    /// Timestamp when account was disabled
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<i64>,
    /// Whether the account is disabled for proxy use only
    #[serde(default)]
    pub proxy_disabled: bool,
    /// Reason for proxy disable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_disabled_reason: Option<String>,
    /// Timestamp when proxy was disabled
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_disabled_at: Option<i64>,
    /// Models protected by quota protection (disabled for this account due to low quota)
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub protected_models: HashSet<String>,
    /// Timestamp when account was created
    pub created_at: i64,
    /// Timestamp when account was last used
    pub last_used: i64,
}

impl Account {
    /// Create a new account with the given ID, email, and token.
    pub fn new(id: String, email: String, token: TokenData) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id,
            email,
            name: None,
            token,
            quota: None,
            disabled: false,
            disabled_reason: None,
            disabled_at: None,
            proxy_disabled: false,
            proxy_disabled_reason: None,
            proxy_disabled_at: None,
            protected_models: HashSet::new(),
            created_at: now,
            last_used: now,
        }
    }

    /// Update the last used timestamp to now.
    pub fn update_last_used(&mut self) {
        self.last_used = chrono::Utc::now().timestamp();
    }

    /// Update the quota data.
    pub fn update_quota(&mut self, quota: QuotaData) {
        self.quota = Some(quota);
    }

    /// Check if the account is available for proxy use.
    pub const fn is_available_for_proxy(&self) -> bool {
        !self.disabled && !self.proxy_disabled
    }

    /// Disable the account for proxy use with a reason.
    pub fn disable_for_proxy(&mut self, reason: impl Into<String>) {
        self.proxy_disabled = true;
        self.proxy_disabled_reason = Some(reason.into());
        self.proxy_disabled_at = Some(chrono::Utc::now().timestamp());
    }

    /// Re-enable the account for proxy use.
    pub fn enable_for_proxy(&mut self) {
        self.proxy_disabled = false;
        self.proxy_disabled_reason = None;
        self.proxy_disabled_at = None;
    }

    /// Check if a specific model is protected (disabled due to low quota).
    pub fn is_model_protected(&self, model: &str) -> bool {
        self.protected_models.contains(model)
    }

    /// Add a model to the protected set (disable it for this account due to low quota).
    pub fn protect_model(&mut self, model: &str) {
        let _ = self.protected_models.insert(model.to_string());
    }

    /// Remove a model from the protected set (re-enable it for this account).
    pub fn unprotect_model(&mut self, model: &str) {
        let _ = self.protected_models.remove(model);
    }

    /// Check if any models are currently protected.
    pub fn has_protected_models(&self) -> bool {
        !self.protected_models.is_empty()
    }

    /// Get the set of protected models.
    pub const fn protected_models(&self) -> &HashSet<String> {
        &self.protected_models
    }

    /// Clear all protected models.
    pub fn clear_protected_models(&mut self) {
        self.protected_models.clear();
    }
}

/// Account index data structure (accounts.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountIndex {
    /// Schema version
    pub version: String,
    /// List of account summaries
    pub accounts: Vec<AccountSummary>,
    /// Currently active account ID
    pub current_account_id: Option<String>,
}

impl AccountIndex {
    /// Create a new empty account index.
    pub fn new() -> Self {
        Self { version: "2.0".to_string(), accounts: Vec::new(), current_account_id: None }
    }
}

impl Default for AccountIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Account summary for the index file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    /// Unique identifier
    pub id: String,
    /// Email address
    pub email: String,
    /// Optional display name
    pub name: Option<String>,
    /// Creation timestamp
    pub created_at: i64,
    /// Last used timestamp
    pub last_used: i64,
}

impl From<&Account> for AccountSummary {
    fn from(account: &Account) -> Self {
        Self {
            id: account.id.clone(),
            email: account.email.clone(),
            name: account.name.clone(),
            created_at: account.created_at,
            last_used: account.last_used,
        }
    }
}
