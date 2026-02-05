//! Google Gemini GenerateContent API types.

use serde::{Deserialize, Serialize};

/// Gemini content role.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GeminiRole {
    /// User-provided content in the conversation.
    User,
    /// Model-generated response content.
    Model,
}

/// Gemini content part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    /// Text content part.
    Text {
        /// The text content.
        text: String,
    },
    /// Binary data content part (images, audio, etc).
    InlineData {
        /// The inline data payload.
        inline_data: GeminiInlineData,
    },
}

/// Gemini inline data (for images, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiInlineData {
    /// MIME type of the data (e.g., "image/png", "audio/wav").
    pub mime_type: String,
    /// Base64-encoded binary data.
    pub data: String,
}

/// Gemini usage metadata.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct GeminiUsageMetadata {
    /// Number of tokens in the input prompt.
    #[serde(default)]
    pub prompt_token_count: u32,
    /// Number of tokens in the generated candidates.
    #[serde(default)]
    pub candidates_token_count: u32,
    /// Total token count (prompt + candidates).
    #[serde(default)]
    pub total_token_count: u32,
    /// Number of tokens served from cache.
    #[serde(default)]
    pub cached_content_token_count: u32,
}
