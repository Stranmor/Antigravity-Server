//! Shared types re-exported from antigravity-shared crate

pub use antigravity_shared::models::{
    Account, AppConfig, DashboardStats, ProxyRequestLog, ProxyStats, ProxyStatus, QuotaData,
    RefreshStats, UpdateInfo, UpstreamProxyMode,
};
pub use antigravity_shared::proxy::config::{
    Protocol, ProxyAuthMode, ProxyConfig, ZaiConfig, ZaiDispatchMode,
};
pub use antigravity_shared::utils::http::UpstreamProxyConfig;
