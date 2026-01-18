//! Shared models module.
//!
//! This module re-exports all model types from `antigravity-types`.
//! It exists for backwards compatibility - new code should import from
//! `antigravity-types` directly.
//!
//! ## Migration Note
//!
//! Previously, this module contained duplicate definitions of domain models.
//! As of the Phase 3 consolidation, this is now a pure re-export layer.

// Re-export all models from antigravity-types as the single source of truth
pub use antigravity_types::models::{
    // Account types
    Account,
    AccountIndex,
    AccountSummary,
    // Config types
    AppConfig,
    // Stats types
    DashboardStats,
    ExperimentalConfig,
    // Quota types
    ModelQuota,
    Protocol,
    // Enums
    ProxyAuthMode,
    ProxyConfig,
    ProxyRequestLog,
    ProxyStats,
    ProxyStatus,
    QuotaData,
    QuotaProtectionConfig,
    RefreshStats,
    SchedulingMode,
    SmartWarmupConfig,
    StickySessionConfig,
    // Token types
    TokenData,
    UpdateInfo,
    UpstreamProxyConfig,
    UpstreamProxyMode,
    // Z.ai config types
    ZaiConfig,
    ZaiDispatchMode,
    ZaiMcpConfig,
    ZaiModelDefaults,
};

// Backwards compatibility: re-export submodules as well
// (some code may do `use antigravity_shared::models::account::Account`)
pub mod account {
    pub use antigravity_types::models::{Account, AccountIndex, AccountSummary};
}

pub mod config {
    pub use antigravity_types::models::AppConfig;
}

pub mod quota {
    pub use antigravity_types::models::{ModelQuota, QuotaData};
}

pub mod stats {
    pub use antigravity_types::models::{
        DashboardStats, ProxyRequestLog, ProxyStats, ProxyStatus, RefreshStats, UpdateInfo,
    };
}

pub mod token {
    pub use antigravity_types::models::TokenData;
}
