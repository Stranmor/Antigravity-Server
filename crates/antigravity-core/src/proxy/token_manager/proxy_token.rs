//! ProxyToken - OAuth token representation for authenticated accounts.

use std::collections::HashSet;
use std::path::PathBuf;

/// Token representing an authenticated account with OAuth credentials.
///
/// Contains access/refresh tokens, account metadata, and quota information
/// for routing requests to the appropriate Google/Anthropic backend.
#[derive(Debug, Clone)]
pub struct ProxyToken {
    pub account_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub timestamp: i64,
    pub email: String,
    pub account_path: PathBuf,
    pub project_id: Option<String>,
    pub subscription_tier: Option<String>,
    pub remaining_quota: Option<i32>,
    pub protected_models: HashSet<String>,
    pub health_score: f32,
}

impl ProxyToken {
    /// Business Ultra tier: high daily quota, strict RPM limits
    pub fn is_business_ultra(&self) -> bool {
        self.subscription_tier
            .as_ref()
            .is_some_and(|t| t.contains("ultra-business"))
    }

    pub fn tier_priority(&self) -> u8 {
        match self.subscription_tier.as_deref() {
            Some(t) if t.contains("ultra-business") => 0,
            Some(t) if t.contains("ultra") => 1,
            Some(t) if t.contains("pro") => 2,
            Some(t) if t.contains("free") => 3,
            _ => 4,
        }
    }

    pub fn tier_weight(&self) -> f32 {
        match self.subscription_tier.as_deref() {
            Some(t) if t.contains("ultra-business") => 0.1,
            Some(t) if t.contains("ultra") => 0.25,
            Some(t) if t.contains("pro") => 0.8,
            Some(t) if t.contains("free") => 1.0,
            _ => 1.25,
        }
    }
}
