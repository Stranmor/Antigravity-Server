//! Core domain models for Antigravity Manager.
//!
//! This module contains all shared data structures used across the Antigravity ecosystem.

pub mod account;
pub mod config;
pub mod device;
pub mod model_family;
pub mod quota;
pub mod stats;
pub mod sync;
pub mod token;

// Re-export all models
pub use account::{Account, AccountIndex, AccountSummary};
pub use config::{
    AppConfig, ExperimentalConfig, Protocol, ProxyAuthMode, ProxyConfig, ProxyRotationStrategy,
    QuotaProtectionConfig, SchedulingMode, SmartWarmupConfig, StickySessionConfig,
    ThinkingBudgetConfig, ThinkingBudgetMode, UpstreamProxyConfig, UpstreamProxyMode, ZaiConfig,
    ZaiDispatchMode, ZaiMcpConfig, ZaiModelDefaults,
};
pub use device::{DeviceProfile, DeviceProfileVersion, DeviceProfiles};
pub use model_family::ModelFamily;
pub use quota::{ModelQuota, QuotaData};
pub use stats::{
    DashboardStats, ProxyRequestLog, ProxyStats, ProxyStatus, RefreshStats, TokenUsageStats,
    UpdateInfo,
};
pub use sync::{MappingEntry, SyncableMapping};
pub use token::TokenData;
