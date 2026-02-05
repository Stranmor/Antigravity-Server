//! Claude content block types for message content representation.
//!
//! This module defines the various content block types used in Claude API
//! messages, including text, images, documents, tool use, and thinking blocks.

use serde::{Deserialize, Serialize};

/// Content block types for Claude API messages.
///
/// Represents different types of content that can appear in a message,
/// including text, images, documents, tool calls, and thinking blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum ContentBlock {
    /// Plain text content block.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },

    /// Thinking/reasoning content block (extended thinking feature).
    #[serde(rename = "thinking")]
    Thinking {
        /// The thinking/reasoning text.
        thinking: String,
        /// Optional cryptographic signature for verification.
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
        /// Optional cache control settings.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<serde_json::Value>,
    },

    /// Image content block with base64-encoded data.
    #[serde(rename = "image")]
    Image {
        /// The image source containing type, media type, and data.
        source: ImageSource,
        /// Optional cache control settings.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<serde_json::Value>,
    },

    /// Document content block (PDF, etc.).
    #[serde(rename = "document")]
    Document {
        /// The document source containing type, media type, and data.
        source: DocumentSource,
        /// Optional cache control settings.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<serde_json::Value>,
    },

    /// Redacted thinking block (content hidden for safety).
    #[serde(rename = "redacted_thinking")]
    RedactedThinking {
        /// Opaque data representing redacted content.
        data: String,
    },

    /// Tool use request from the model.
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Unique identifier for this tool use.
        id: String,
        /// Name of the tool being called.
        name: String,
        /// Input arguments for the tool.
        input: serde_json::Value,
        /// Optional signature for verification.
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
        /// Optional cache control settings.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<serde_json::Value>,
    },

    /// Result from a tool execution.
    #[serde(rename = "tool_result")]
    ToolResult {
        /// ID of the tool use this result corresponds to.
        tool_use_id: String,
        /// The result content from the tool.
        content: serde_json::Value,
        /// Whether the tool execution resulted in an error.
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },

    /// Server-side tool use (internal tools).
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        /// Unique identifier for this tool use.
        id: String,
        /// Name of the server tool.
        name: String,
        /// Input arguments for the tool.
        input: serde_json::Value,
    },

    /// Web search tool result.
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult {
        /// ID of the tool use this result corresponds to.
        tool_use_id: String,
        /// The search result content.
        content: serde_json::Value,
    },
}

/// Source information for image content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    /// The source type (e.g., "base64").
    #[serde(rename = "type")]
    pub source_type: String,
    /// MIME type of the image (e.g., "image/png").
    pub media_type: String,
    /// Base64-encoded image data.
    pub data: String,
}

/// Source information for document content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSource {
    /// The source type (e.g., "base64").
    #[serde(rename = "type")]
    pub source_type: String,
    /// MIME type of the document (e.g., "application/pdf").
    pub media_type: String,
    /// Base64-encoded document data.
    pub data: String,
}
