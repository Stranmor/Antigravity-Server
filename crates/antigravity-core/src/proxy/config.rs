//! Proxy configuration types.

pub use crate::utils::http::UpstreamProxyConfig;
use serde::{Deserialize, Serialize};

/// Proxy server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// TCP port to listen on.
    pub port: u16,

    /// Auto-start proxy on app launch.
    pub auto_start: bool,

    /// Allow LAN access (bind to 0.0.0.0 instead of 127.0.0.1).
    pub allow_lan_access: bool,

    /// API key for authentication.
    pub api_key: String,

    /// Upstream proxy configuration.
    #[serde(default)]
    pub upstream_proxy: UpstreamProxyConfig,

    /// Request timeout in seconds.
    #[serde(default = "default_timeout")]
    pub request_timeout: u64,

    /// Enable request logging.
    #[serde(default)]
    pub enable_logging: bool,
}

fn default_timeout() -> u64 {
    120
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            port: 8045,
            auto_start: true,
            allow_lan_access: false,
            api_key: String::new(),
            upstream_proxy: UpstreamProxyConfig::default(),
            request_timeout: 120,
            enable_logging: false,
        }
    }
}
