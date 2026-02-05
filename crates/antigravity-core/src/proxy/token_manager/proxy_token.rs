//! ProxyToken - OAuth token representation for authenticated accounts.

use std::collections::HashSet;
use std::path::PathBuf;

/// Account subscription tier with explicit ordering.
/// Lower numeric value = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum AccountTier {
    UltraBusiness = 0,
    Ultra = 1,
    Pro = 2,
    Free = 3,
    Unknown = 4,
}

impl AccountTier {
    /// Returns true for premium ultra-class tiers (UltraBusiness, Ultra).
    /// These tiers get priority over sticky session bindings.
    #[inline]
    pub fn is_ultra(self) -> bool {
        matches!(self, Self::UltraBusiness | Self::Ultra)
    }

    /// Numeric priority for sorting (lower = better).
    #[inline]
    pub fn priority(self) -> u8 {
        self as u8
    }
}

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
    /// Parse subscription tier string into typed enum.
    pub fn account_tier(&self) -> AccountTier {
        match self.subscription_tier.as_deref() {
            Some(t) if t.contains("ultra-business") => AccountTier::UltraBusiness,
            Some(t) if t.contains("ultra") => AccountTier::Ultra,
            Some(t) if t.contains("pro") => AccountTier::Pro,
            Some(t) if t.contains("free") => AccountTier::Free,
            _ => AccountTier::Unknown,
        }
    }

    /// Returns true for premium ultra-class accounts (UltraBusiness, Ultra).
    /// These accounts get priority over sticky session bindings.
    #[inline]
    pub fn is_ultra_tier(&self) -> bool {
        self.account_tier().is_ultra()
    }

    /// Business Ultra tier: high daily quota, strict RPM limits.
    pub fn is_business_ultra(&self) -> bool {
        self.subscription_tier.as_ref().is_some_and(|t| t.contains("ultra-business"))
    }

    /// Numeric tier priority for sorting (lower = better).
    /// Backwards-compatible with existing sort logic.
    pub fn tier_priority(&self) -> u8 {
        self.account_tier().priority()
    }

    pub fn tier_weight(&self) -> f32 {
        match self.account_tier() {
            AccountTier::UltraBusiness => 0.1,
            AccountTier::Ultra => 0.25,
            AccountTier::Pro => 0.8,
            AccountTier::Free => 1.0,
            AccountTier::Unknown => 1.25,
        }
    }
}
