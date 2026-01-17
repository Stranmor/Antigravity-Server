//! Core domain models for Antigravity Manager.
//!
//! This module contains all shared data structures used across the Antigravity ecosystem.

mod account;
mod config;
mod quota;
mod stats;
mod token;

// Re-export all models
pub use account::{Account, AccountIndex, AccountSummary};
pub use config::{
    AppConfig, ExperimentalConfig, Protocol, ProxyAuthMode, ProxyConfig, SchedulingMode,
    StickySessionConfig, UpstreamProxyConfig, ZaiConfig, ZaiDispatchMode, ZaiMcpConfig,
    ZaiModelDefaults,
};
pub use quota::{ModelQuota, QuotaData};
pub use stats::{DashboardStats, ProxyRequestLog, ProxyStats, ProxyStatus, RefreshStats, UpdateInfo};
pub use token::TokenData;
