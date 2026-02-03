use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    pub role: String,
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "thoughtSignature")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "functionCall")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "functionResponse")]
    pub function_response: Option<FunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "inlineData")]
    pub inline_data: Option<InlineData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineData {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<Candidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<UsageMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "modelVersion")]
    pub model_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "responseId")]
    pub response_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "groundingMetadata")]
    pub grounding_metadata: Option<super::grounding_models::GroundingMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "promptTokenCount")]
    pub prompt_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "candidatesTokenCount")]
    pub candidates_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "totalTokenCount")]
    pub total_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "cachedContentTokenCount")]
    pub cached_content_token_count: Option<u32>,
}
