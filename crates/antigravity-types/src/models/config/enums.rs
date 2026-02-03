//! Configuration enums for proxy and scheduling modes.

use serde::{Deserialize, Serialize};
use std::fmt;

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
