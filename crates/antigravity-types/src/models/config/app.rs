//! Application-level configuration.

use serde::{Deserialize, Serialize};

use super::proxy::ProxyConfig;
use super::session::{QuotaProtectionConfig, SmartWarmupConfig};

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
