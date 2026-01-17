//! Google Gemini GenerateContent API types.

use serde::{Deserialize, Serialize};

/// Gemini content role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GeminiRole {
    User,
    Model,
}

/// Gemini content part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    Text { text: String },
    InlineData { inline_data: GeminiInlineData },
}

/// Gemini inline data (for images, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiInlineData {
    pub mime_type: String,
    pub data: String,
}

/// Gemini usage metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeminiUsageMetadata {
    #[serde(default)]
    pub prompt_token_count: u32,
    #[serde(default)]
    pub candidates_token_count: u32,
    #[serde(default)]
    pub total_token_count: u32,
    #[serde(default)]
    pub cached_content_token_count: u32,
}
