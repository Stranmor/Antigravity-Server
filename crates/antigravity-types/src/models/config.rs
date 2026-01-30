//! Application and proxy configuration models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use validator::Validate;

// ============================================================================
// Enums
// ============================================================================

/// Proxy authentication mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyAuthMode {
    /// No authentication required
    #[default]
    Off,
    /// Always require API key
    Strict,
    /// Require API key for all except health checks
    AllExceptHealth,
    /// Automatic mode (detect from request)
    Auto,
}

impl fmt::Display for ProxyAuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyAuthMode::Off => write!(f, "off"),
            ProxyAuthMode::Strict => write!(f, "strict"),
            ProxyAuthMode::AllExceptHealth => write!(f, "all_except_health"),
            ProxyAuthMode::Auto => write!(f, "auto"),
        }
    }
}

impl ProxyAuthMode {
    /// Parse from string.
    pub fn from_string(s: &str) -> Self {
        match s {
            "strict" => ProxyAuthMode::Strict,
            "all_except_health" => ProxyAuthMode::AllExceptHealth,
            "auto" => ProxyAuthMode::Auto,
            _ => ProxyAuthMode::Off,
        }
    }
}

/// Z.ai dispatch mode for routing requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ZaiDispatchMode {
    /// Z.ai disabled
    #[default]
    Off,
    /// Route exclusively to Z.ai
    Exclusive,
    /// Pool Z.ai with other providers
    Pooled,
    /// Use Z.ai as fallback
    Fallback,
}

impl fmt::Display for ZaiDispatchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZaiDispatchMode::Off => write!(f, "off"),
            ZaiDispatchMode::Exclusive => write!(f, "exclusive"),
            ZaiDispatchMode::Pooled => write!(f, "pooled"),
            ZaiDispatchMode::Fallback => write!(f, "fallback"),
        }
    }
}

/// API protocol type.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    OpenAI,
    Anthropic,
    Gemini,
}

/// Account scheduling mode for sticky sessions.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum SchedulingMode {
    /// Prioritize cache hits
    CacheFirst,
    /// Balance between cache and load
    #[default]
    Balance,
    /// Prioritize performance (lowest latency)
    PerformanceFirst,
}

impl fmt::Display for SchedulingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchedulingMode::CacheFirst => write!(f, "CacheFirst"),
            SchedulingMode::Balance => write!(f, "Balance"),
            SchedulingMode::PerformanceFirst => write!(f, "PerformanceFirst"),
        }
    }
}

// ============================================================================
// Z.ai Configuration
// ============================================================================

/// Z.ai default model mappings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct ZaiModelDefaults {
    /// Model for Opus tier
    #[validate(length(min = 1))]
    #[serde(default = "default_zai_opus_model")]
    pub opus: String,
    /// Model for Sonnet tier
    #[validate(length(min = 1))]
    #[serde(default = "default_zai_sonnet_model")]
    pub sonnet: String,
    /// Model for Haiku tier
    #[validate(length(min = 1))]
    #[serde(default = "default_zai_haiku_model")]
    pub haiku: String,
}

impl Default for ZaiModelDefaults {
    fn default() -> Self {
        Self {
            opus: default_zai_opus_model(),
            sonnet: default_zai_sonnet_model(),
            haiku: default_zai_haiku_model(),
        }
    }
}

/// Z.ai MCP (Model Context Protocol) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, Validate)]
pub struct ZaiMcpConfig {
    /// Enable MCP features
    #[serde(default)]
    pub enabled: bool,
    /// Enable web search tool
    #[serde(default)]
    pub web_search_enabled: bool,
    /// Enable web reader tool
    #[serde(default)]
    pub web_reader_enabled: bool,
    /// Enable vision tool
    #[serde(default)]
    pub vision_enabled: bool,
}

/// Z.ai provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, Validate)]
pub struct ZaiConfig {
    /// Enable Z.ai integration
    #[serde(default)]
    pub enabled: bool,
    /// Z.ai API base URL
    #[validate(url)]
    #[serde(default = "default_zai_base_url")]
    pub base_url: String,
    /// Z.ai API key
    #[serde(default)]
    pub api_key: String,
    /// Request dispatch mode
    #[serde(default)]
    pub dispatch_mode: ZaiDispatchMode,
    /// Custom model mappings
    #[serde(default)]
    pub model_mapping: HashMap<String, String>,
    /// Default model mappings
    #[serde(default)]
    #[validate(nested)]
    pub models: ZaiModelDefaults,
    /// MCP configuration
    #[serde(default)]
    #[validate(nested)]
    pub mcp: ZaiMcpConfig,
}

// ============================================================================
// Session & Experimental Config
// ============================================================================

/// Experimental features configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, Validate)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Validate)]
pub struct StickySessionConfig {
    /// Enable sticky sessions
    #[serde(default)]
    pub enabled: bool,
    /// Scheduling mode
    #[serde(default)]
    pub mode: SchedulingMode,
    /// Session TTL in seconds
    #[validate(range(min = 1))]
    #[serde(default = "default_sticky_ttl", alias = "max_wait_seconds")]
    pub ttl: u32,
}

/// Upstream proxy mode for routing requests.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamProxyMode {
    /// Direct connection (no proxy)
    #[default]
    Direct,
    /// Use system proxy settings (HTTP_PROXY, HTTPS_PROXY, ALL_PROXY for SOCKS)
    System,
    /// Use custom proxy URL
    Custom,
}

/// Quota protection configuration.
/// Prevents account exhaustion by monitoring quota thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, Validate)]
pub struct QuotaProtectionConfig {
    /// Enable quota protection
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Threshold percentage (1-99) - accounts below this are considered low
    #[validate(range(min = 1, max = 99))]
    #[serde(default = "default_quota_threshold")]
    pub threshold_percentage: u8,
    /// Models to monitor for quota protection
    #[serde(default)]
    pub monitored_models: Vec<String>,
    /// Auto-restore accounts when quota resets
    #[serde(default = "default_true")]
    pub auto_restore: bool,
}

fn default_quota_threshold() -> u8 {
    20
}

/// Smart warmup configuration.
/// Pre-warms accounts to maintain active sessions and quotas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, Validate)]
pub struct SmartWarmupConfig {
    /// Enable smart warmup
    #[serde(default)]
    pub enabled: bool,
    /// Models to warmup
    #[serde(default)]
    pub models: Vec<String>,
    /// Warmup interval in minutes
    #[validate(range(min = 5, max = 1440))]
    #[serde(default = "default_warmup_interval")]
    pub interval_minutes: u32,
    /// Only warmup accounts below quota threshold
    #[serde(default)]
    pub only_low_quota: bool,
}

fn default_warmup_interval() -> u32 {
    60
}

/// Upstream proxy configuration for outbound requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct UpstreamProxyConfig {
    /// Proxy mode: direct, system, or custom
    #[serde(default)]
    pub mode: UpstreamProxyMode,
    /// Enable upstream proxy (legacy, kept for compatibility)
    #[serde(default)]
    pub enabled: bool,
    /// Custom proxy URL (e.g., socks5://127.0.0.1:1080 or http://vps:8045)
    /// Only used when mode is Custom
    #[serde(default)]
    pub url: String,
}

impl Default for UpstreamProxyConfig {
    fn default() -> Self {
        Self {
            mode: UpstreamProxyMode::Direct,
            enabled: false,
            url: String::new(),
        }
    }
}

// ============================================================================
// Main Configurations
// ============================================================================

/// Full proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct ProxyConfig {
    /// Enable proxy server
    pub enabled: bool,
    /// Allow LAN access (bind to 0.0.0.0)
    #[serde(default)]
    pub allow_lan_access: bool,
    /// Authentication mode
    #[serde(default)]
    pub auth_mode: ProxyAuthMode,
    /// Port to listen on
    #[validate(range(min = 1024, max = 65535))]
    pub port: u16,
    /// API key for authentication
    #[validate(length(min = 1))]
    pub api_key: String,
    /// Auto-start proxy on app launch
    pub auto_start: bool,
    /// Custom model mappings
    #[serde(default)]
    pub custom_mapping: HashMap<String, String>,
    /// Request timeout in seconds
    #[validate(range(min = 30, max = 3600))]
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    /// Enable request logging
    #[serde(default)]
    pub enable_logging: bool,
    /// Upstream proxy configuration
    #[serde(default)]
    #[validate(nested)]
    pub upstream_proxy: UpstreamProxyConfig,
    /// Z.ai configuration
    #[serde(default)]
    #[validate(nested)]
    pub zai: ZaiConfig,
    /// Sticky session configuration
    #[serde(default)]
    #[validate(nested)]
    pub scheduling: StickySessionConfig,
    /// Experimental features
    #[serde(default)]
    #[validate(nested)]
    pub experimental: ExperimentalConfig,
    /// Fixed account mode: use this account for all requests
    /// None = round-robin, Some(account_id) = always use this account
    #[serde(default)]
    pub preferred_account_id: Option<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allow_lan_access: false,
            auth_mode: ProxyAuthMode::default(),
            port: 8045,
            api_key: String::new(),
            auto_start: true,
            custom_mapping: HashMap::new(),
            request_timeout: 120,
            enable_logging: false,
            upstream_proxy: UpstreamProxyConfig::default(),
            zai: ZaiConfig::default(),
            scheduling: StickySessionConfig::default(),
            experimental: ExperimentalConfig::default(),
            preferred_account_id: None,
        }
    }
}

impl ProxyConfig {
    /// Get the bind address based on LAN access setting.
    pub fn get_bind_address(&self) -> String {
        if self.allow_lan_access {
            "0.0.0.0".to_string()
        } else {
            "127.0.0.1".to_string()
        }
    }

    /// Get the full bind socket address.
    pub fn get_socket_addr(&self) -> String {
        format!("{}:{}", self.get_bind_address(), self.port)
    }
}

/// Full application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// UI language
    pub language: String,
    /// UI theme
    pub theme: String,
    /// Enable automatic quota refresh
    pub auto_refresh: bool,
    /// Refresh interval in minutes
    pub refresh_interval: i32,
    /// Enable automatic sync
    pub auto_sync: bool,
    /// Sync interval in minutes
    pub sync_interval: i32,
    /// Default export path
    pub default_export_path: Option<String>,
    /// Proxy configuration
    #[serde(default)]
    pub proxy: ProxyConfig,
    /// Custom Antigravity executable path
    pub antigravity_executable: Option<String>,
    /// Antigravity launch arguments
    pub antigravity_args: Option<Vec<String>>,
    /// Enable auto-launch on system startup
    #[serde(default)]
    pub auto_launch: bool,
    /// Quota protection configuration
    #[serde(default)]
    pub quota_protection: QuotaProtectionConfig,
    /// Smart warmup configuration
    #[serde(default)]
    pub smart_warmup: SmartWarmupConfig,
}

impl AppConfig {
    /// Create default configuration.
    pub fn new() -> Self {
        Self {
            language: "zh".to_string(),
            theme: "system".to_string(),
            auto_refresh: false,
            refresh_interval: 15,
            auto_sync: false,
            sync_interval: 5,
            default_export_path: None,
            proxy: ProxyConfig::default(),
            antigravity_executable: None,
            antigravity_args: None,
            auto_launch: false,
            quota_protection: QuotaProtectionConfig::default(),
            smart_warmup: SmartWarmupConfig::default(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Default Value Functions
// ============================================================================

fn default_true() -> bool {
    true
}

fn default_zai_base_url() -> String {
    "https://api.z.ai/api/anthropic".to_string()
}

fn default_zai_opus_model() -> String {
    "glm-4.7".to_string()
}

fn default_zai_sonnet_model() -> String {
    "glm-4.7".to_string()
}

fn default_zai_haiku_model() -> String {
    "glm-4.5-air".to_string()
}

fn default_request_timeout() -> u64 {
    120
}

fn default_sticky_ttl() -> u32 {
    300 // 5 minutes default TTL for sticky sessions
}
