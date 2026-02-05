//! Gemini API model types for request/response handling.
//!
//! This module defines the data structures used when communicating with
//! the Gemini API, including content, parts, function calls, and responses.

use serde::{Deserialize, Serialize};

/// Gemini content structure containing role and parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    /// The role of the content author (e.g., "user", "model").
    pub role: String,
    /// The parts that make up this content.
    pub parts: Vec<GeminiPart>,
}

/// A single part within Gemini content.
///
/// Parts can contain text, function calls, function responses, or inline data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiPart {
    /// Optional text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Whether this is a thought/reasoning part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    /// Signature for thought verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "thoughtSignature")]
    pub thought_signature: Option<String>,
    /// Function call request from the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "functionCall")]
    pub function_call: Option<FunctionCall>,
    /// Response to a function call.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "functionResponse")]
    pub function_response: Option<FunctionResponse>,
    /// Inline binary data (images, audio, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "inlineData")]
    pub inline_data: Option<InlineData>,
}

/// Function call request from the Gemini model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function to call.
    pub name: String,
    /// Optional unique identifier for this call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Optional arguments to pass to the function.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Response to a function call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    /// Name of the function that was called.
    pub name: String,
    /// The response data from the function.
    pub response: serde_json::Value,
    /// Optional identifier matching the original call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Inline binary data with MIME type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineData {
    /// MIME type of the data (e.g., "image/png").
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// Base64-encoded binary data.
    pub data: String,
}

/// Response from the Gemini API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiResponse {
    /// List of response candidates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<Candidate>>,
    /// Token usage metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<UsageMetadata>,
    /// Version of the model that generated this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "modelVersion")]
    pub model_version: Option<String>,
    /// Unique identifier for this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "responseId")]
    pub response_id: Option<String>,
}

/// A single candidate response from Gemini.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// The content of this candidate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<GeminiContent>,
    /// Reason why generation finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
    /// Index of this candidate in the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// Grounding metadata for search-augmented responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "groundingMetadata")]
    pub grounding_metadata: Option<super::grounding_models::GroundingMetadata>,
}

/// Token usage metadata from Gemini API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetadata {
    /// Number of tokens in the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "promptTokenCount")]
    pub prompt_token_count: Option<u32>,
    /// Number of tokens in the response candidates.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "candidatesTokenCount")]
    pub candidates_token_count: Option<u32>,
    /// Total token count (prompt + candidates).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "totalTokenCount")]
    pub total_token_count: Option<u32>,
    /// Number of tokens served from cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "cachedContentTokenCount")]
    pub cached_content_token_count: Option<u32>,
}
