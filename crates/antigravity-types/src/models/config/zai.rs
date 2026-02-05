//! Z.ai provider configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

use super::enums::ZaiDispatchMode;

/// Z.ai default model mappings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Validate)]
pub struct ZaiModelDefaults {
    /// Model for Opus tier
    #[validate(length(min = 1_u64))]
    #[serde(default = "default_zai_opus_model")]
    pub opus: String,
    /// Model for Sonnet tier
    #[validate(length(min = 1_u64))]
    #[serde(default = "default_zai_sonnet_model")]
    pub sonnet: String,
    /// Model for Haiku tier
    #[validate(length(min = 1_u64))]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Validate)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Configuration struct - bools are intentional feature flags"
)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, Validate)]
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

// Default value functions
pub fn default_zai_base_url() -> String {
    "https://api.z.ai/api/anthropic".to_string()
}

pub fn default_zai_opus_model() -> String {
    "glm-4.7".to_string()
}

pub fn default_zai_sonnet_model() -> String {
    "glm-4.7".to_string()
}

pub fn default_zai_haiku_model() -> String {
    "glm-4.5-air".to_string()
}
