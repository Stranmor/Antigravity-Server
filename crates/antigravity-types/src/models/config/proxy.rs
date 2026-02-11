//! Proxy server configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

use super::enums::ProxyAuthMode;
use super::session::{ExperimentalConfig, StickySessionConfig, UpstreamProxyConfig};
use super::thinking::ThinkingBudgetConfig;
use super::zai::ZaiConfig;

/// Full proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Validate)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Configuration struct - bools are intentional feature flags"
)]
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
    #[validate(range(min = 1024_u16, max = 65535_u16))]
    pub port: u16,
    /// API key for authentication
    #[validate(length(min = 1_u64))]
    pub api_key: String,
    /// Auto-start proxy on app launch
    pub auto_start: bool,
    /// Custom model mappings
    #[serde(default)]
    pub custom_mapping: HashMap<String, String>,
    /// Request timeout in seconds
    #[validate(range(min = 30_u64, max = 3600_u64))]
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
    /// Thinking budget configuration (Auto/Passthrough/Custom/Adaptive)
    #[serde(default)]
    pub thinking_budget: ThinkingBudgetConfig,
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
            thinking_budget: ThinkingBudgetConfig::default(),
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

pub const fn default_request_timeout() -> u64 {
    120
}
