//! Session, experimental, and protection configuration types.

use serde::{Deserialize, Serialize};
use validator::Validate;

use super::enums::{ProxyRotationStrategy, SchedulingMode, UpstreamProxyMode};

/// Experimental features configuration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Validate)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Configuration struct - bools are intentional feature flags"
)]
pub struct ExperimentalConfig {
    /// Enable signature caching for prompt reuse
    #[serde(default = "default_true")]
    pub enable_signature_cache: bool,
    /// Enable tool loop recovery
    #[serde(default = "default_true")]
    pub enable_tool_loop_recovery: bool,
    /// Enable cross-model consistency checks
    #[serde(default = "default_true")]
    pub enable_cross_model_checks: bool,
    /// Enable usage scaling for context window optimization
    #[serde(default)]
    pub enable_usage_scaling: bool,
}

/// Sticky session configuration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Validate)]
pub struct StickySessionConfig {
    /// Enable sticky sessions
    #[serde(default)]
    pub enabled: bool,
    /// Scheduling mode
    #[serde(default)]
    pub mode: SchedulingMode,
    /// Session TTL in seconds
    #[validate(range(min = 1_u32))]
    #[serde(default = "default_sticky_ttl", alias = "max_wait_seconds")]
    pub ttl: u32,
}

/// Quota protection configuration.
/// Prevents account exhaustion by monitoring quota thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, Validate)]
pub struct QuotaProtectionConfig {
    /// Enable quota protection
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Threshold percentage (1-99) - accounts below this are considered low
    #[validate(range(min = 1_u8, max = 99_u8))]
    #[serde(default = "default_quota_threshold")]
    pub threshold_percentage: u8,
    /// Models to monitor for quota protection
    #[serde(default)]
    pub monitored_models: Vec<String>,
    /// Auto-restore accounts when quota resets
    #[serde(default = "default_true")]
    pub auto_restore: bool,
}

/// Smart warmup configuration.
/// Pre-warms accounts to maintain active sessions and quotas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, Validate)]
pub struct SmartWarmupConfig {
    /// Enable smart warmup
    #[serde(default)]
    pub enabled: bool,
    /// Models to warmup
    #[serde(default)]
    pub models: Vec<String>,
    /// Warmup interval in minutes
    #[validate(range(min = 5_u32, max = 1440_u32))]
    #[serde(default = "default_warmup_interval")]
    pub interval_minutes: u32,
    /// Only warmup accounts below quota threshold
    #[serde(default)]
    pub only_low_quota: bool,
}

/// Upstream proxy configuration for outbound requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Validate)]
pub struct UpstreamProxyConfig {
    /// Proxy mode: direct, system, custom, or pool
    #[serde(default)]
    pub mode: UpstreamProxyMode,
    /// Enable upstream proxy (legacy, kept for compatibility)
    #[serde(default)]
    pub enabled: bool,
    /// Custom proxy URL (e.g., socks5://127.0.0.1:1080 or http://vps:8045)
    /// Only used when mode is Custom
    #[serde(default)]
    pub url: String,
    /// List of proxy URLs for pool rotation (used when mode is Pool)
    /// Supports http://, https://, socks5:// protocols
    /// Example: ["socks5://proxy1:1080", "http://proxy2:8080", "socks5://proxy3:1080"]
    #[serde(default)]
    pub proxy_urls: Vec<String>,
    /// Rotation strategy for proxy pool
    #[serde(default)]
    pub rotation_strategy: ProxyRotationStrategy,
    /// When true, ALL outbound requests MUST go through a proxy.
    /// Requests without a proxy (no per-account proxy_url, no pool, Direct/System mode)
    /// are BLOCKED with an error instead of falling back to direct connection.
    /// Prevents IP leaks when proxy infrastructure is required.
    #[serde(default)]
    pub enforce_proxy: bool,
}

impl Default for UpstreamProxyConfig {
    fn default() -> Self {
        Self {
            mode: UpstreamProxyMode::Direct,
            enabled: false,
            url: String::new(),
            proxy_urls: Vec::new(),
            rotation_strategy: ProxyRotationStrategy::default(),
            enforce_proxy: false,
        }
    }
}

// Default value functions
pub const fn default_true() -> bool {
    true
}

pub const fn default_sticky_ttl() -> u32 {
    300 // 5 minutes default TTL for sticky sessions
}

pub const fn default_quota_threshold() -> u8 {
    20
}

pub const fn default_warmup_interval() -> u32 {
    60
}

/// Strategy for assigning proxies from the pool to new accounts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyAssignmentStrategy {
    /// Assign proxies in round-robin order, balancing across accounts.
    #[default]
    RoundRobin,
    /// Assign the proxy with the fewest accounts currently using it.
    LeastUsed,
    /// Assign a random proxy from the pool.
    Random,
}

/// Per-account proxy pool configuration.
/// When enabled, newly added accounts without an explicit proxy_url
/// are automatically assigned one from this pool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, Validate)]
pub struct AccountProxyPoolConfig {
    /// Enable automatic proxy assignment for new accounts
    #[serde(default)]
    pub enabled: bool,
    /// List of proxy URLs available for assignment
    /// Supports socks5://, http://, https://
    #[serde(default)]
    pub urls: Vec<String>,
    /// Strategy for assigning proxies to accounts
    #[serde(default)]
    pub strategy: ProxyAssignmentStrategy,
}
