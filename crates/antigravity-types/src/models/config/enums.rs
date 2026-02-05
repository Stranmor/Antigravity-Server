//! Configuration enums for proxy and scheduling modes.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Proxy authentication mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
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
        match *self {
            Self::Off => write!(f, "off"),
            Self::Strict => write!(f, "strict"),
            Self::AllExceptHealth => write!(f, "all_except_health"),
            Self::Auto => write!(f, "auto"),
        }
    }
}

impl ProxyAuthMode {
    /// Parse from string.
    pub fn from_string(s: &str) -> Self {
        match s {
            "strict" => Self::Strict,
            "all_except_health" => Self::AllExceptHealth,
            "auto" => Self::Auto,
            _ => Self::Off,
        }
    }
}

/// Z.ai dispatch mode for routing requests.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
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
        match *self {
            Self::Off => write!(f, "off"),
            Self::Exclusive => write!(f, "exclusive"),
            Self::Pooled => write!(f, "pooled"),
            Self::Fallback => write!(f, "fallback"),
        }
    }
}

/// API protocol type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Protocol {
    /// OpenAI ChatCompletions API format.
    #[default]
    OpenAI,
    /// Anthropic Claude Messages API format.
    Anthropic,
    /// Google Gemini GenerateContent API format.
    Gemini,
}

/// Account scheduling mode for sticky sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
        match *self {
            Self::CacheFirst => write!(f, "CacheFirst"),
            Self::Balance => write!(f, "Balance"),
            Self::PerformanceFirst => write!(f, "PerformanceFirst"),
        }
    }
}

/// Upstream proxy mode for routing requests.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
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
