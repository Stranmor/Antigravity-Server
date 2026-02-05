//! Anthropic Claude Messages API types.

use serde::{Deserialize, Serialize};

/// Claude message role.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClaudeRole {
    /// Human user message.
    User,
    /// AI assistant response.
    Assistant,
}

/// Claude message content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeContentBlock {
    /// Plain text content.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// Image content with base64 source.
    #[serde(rename = "image")]
    Image {
        /// Image source data.
        source: ClaudeImageSource,
    },
    /// Tool use request from the model.
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Unique tool use identifier.
        id: String,
        /// Tool name to invoke.
        name: String,
        /// Tool input parameters as JSON.
        input: serde_json::Value,
    },
    /// Tool execution result.
    #[serde(rename = "tool_result")]
    ToolResult {
        /// ID of the tool use this result corresponds to.
        tool_use_id: String,
        /// Tool execution output.
        content: String,
    },
}

/// Claude image source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeImageSource {
    /// Source type (e.g., "base64").
    #[serde(rename = "type")]
    pub source_type: String,
    /// MIME type (e.g., "image/png").
    pub media_type: String,
    /// Base64-encoded image data.
    pub data: String,
}

/// Claude usage statistics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ClaudeUsage {
    /// Number of input tokens consumed.
    pub input_tokens: u32,
    /// Number of output tokens generated.
    pub output_tokens: u32,
    /// Tokens used to create prompt cache.
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    /// Tokens read from prompt cache.
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}
