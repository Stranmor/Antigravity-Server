//! Core domain models for Antigravity Manager.
//!
//! This module contains all shared data structures used across the Antigravity ecosystem.

pub mod account;
pub mod config;
pub mod device;
pub mod quota;
pub mod stats;
pub mod sync;
pub mod token;

// Re-export all models
pub use account::{Account, AccountIndex, AccountSummary};
pub use config::{
    AppConfig, ExperimentalConfig, Protocol, ProxyAuthMode, ProxyConfig, QuotaProtectionConfig,
    SchedulingMode, SmartWarmupConfig, StickySessionConfig, UpstreamProxyConfig, UpstreamProxyMode,
    ZaiConfig, ZaiDispatchMode, ZaiMcpConfig, ZaiModelDefaults,
};
pub use device::{DeviceProfile, DeviceProfileVersion, DeviceProfiles};
pub use quota::{ModelQuota, QuotaData};
pub use stats::{
    DashboardStats, ProxyRequestLog, ProxyStats, ProxyStatus, RefreshStats, UpdateInfo,
};
pub use sync::{MappingEntry, SyncableMapping};
pub use token::TokenData;
