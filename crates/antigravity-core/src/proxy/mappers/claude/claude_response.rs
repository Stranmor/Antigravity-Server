//! Claude API response model types.
//!
//! This module defines the response structures for the Claude Messages API,
//! including tools, usage statistics, and the main response type.

use serde::{Deserialize, Serialize};

use super::content_block::ContentBlock;

/// Tool definition for Claude API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Tool type (e.g., "function", "web_search").
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    /// Tool name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for tool input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

impl Tool {
    /// Checks if this tool is a web search tool.
    pub fn is_web_search(&self) -> bool {
        if let Some(ref t) = self.type_ {
            if t.starts_with("web_search") {
                return true;
            }
        }
        if let Some(ref n) = self.name {
            if n == "web_search" {
                return true;
            }
        }
        false
    }
}

/// Request metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    /// User identifier for tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Output configuration options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Effort level for generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Number of input tokens.
    pub input_tokens: u32,
    /// Number of output tokens.
    pub output_tokens: u32,
    /// Tokens read from cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    /// Tokens written to cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    /// Server tool usage data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<serde_json::Value>,
}

/// Response from Claude Messages API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeResponse {
    /// Unique response identifier.
    pub id: String,
    /// Response type (always "message").
    #[serde(rename = "type")]
    pub type_: String,
    /// Role (always "assistant").
    pub role: String,
    /// Model that generated the response.
    pub model: String,
    /// Content blocks in the response.
    pub content: Vec<ContentBlock>,
    /// Reason generation stopped.
    pub stop_reason: String,
    /// Stop sequence that triggered stop, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    /// Token usage statistics.
    pub usage: Usage,
}
