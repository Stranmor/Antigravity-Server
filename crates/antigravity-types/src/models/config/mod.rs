//! Application and proxy configuration models.

mod app;
mod enums;
mod proxy;
mod session;
mod zai;

pub use app::AppConfig;
pub use enums::{Protocol, ProxyAuthMode, SchedulingMode, UpstreamProxyMode, ZaiDispatchMode};
pub use proxy::ProxyConfig;
pub use session::{
    ExperimentalConfig, QuotaProtectionConfig, SmartWarmupConfig, StickySessionConfig,
    UpstreamProxyConfig,
};
pub use zai::{ZaiConfig, ZaiMcpConfig, ZaiModelDefaults};
