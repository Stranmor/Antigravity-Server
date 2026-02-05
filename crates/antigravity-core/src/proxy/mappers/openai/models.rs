//! OpenAI API data models for request/response transformation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// OpenAI chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OpenAIRequest {
    /// Model identifier (e.g., "gpt-4", "gemini-3-pro").
    pub model: String,
    /// Conversation messages.
    #[serde(default)]
    pub messages: Vec<OpenAIMessage>,
    /// Legacy prompt field for completions API.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Enable streaming response.
    #[serde(default)]
    pub stream: bool,
    /// Number of completions to generate.
    #[serde(default)]
    pub n: Option<u32>,
    /// Maximum tokens in response.
    #[serde(rename = "max_tokens")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature (0.0-2.0).
    pub temperature: Option<f32>,
    /// Nucleus sampling parameter.
    #[serde(rename = "top_p")]
    pub top_p: Option<f32>,
    /// Stop sequences.
    pub stop: Option<Value>,
    /// Response format specification.
    pub response_format: Option<ResponseFormat>,
    /// Tool definitions for function calling.
    #[serde(default)]
    pub tools: Option<Vec<Value>>,
    /// Tool choice strategy.
    #[serde(rename = "tool_choice")]
    pub tool_choice: Option<Value>,
    /// Allow parallel tool calls.
    #[serde(rename = "parallel_tool_calls")]
    pub parallel_tool_calls: Option<bool>,
    /// Codex instructions field.
    pub instructions: Option<String>,
    /// Codex input field.
    pub input: Option<Value>,
    /// Image size for generation.
    #[serde(default)]
    pub size: Option<String>,
    /// Image quality setting.
    #[serde(default)]
    pub quality: Option<String>,
    /// Person generation mode.
    #[serde(default, rename = "personGeneration")]
    pub person_generation: Option<String>,
}

/// Response format specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ResponseFormat {
    /// Format type (e.g., "json_object", "text").
    pub r#type: String,
}

/// Content in OpenAI message (string or array of blocks).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
#[non_exhaustive]
pub enum OpenAIContent {
    /// Plain text content.
    String(String),
    /// Array of content blocks (text, images, audio).
    Array(Vec<OpenAIContentBlock>),
}

/// Content block types in OpenAI messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum OpenAIContentBlock {
    /// Text content block.
    #[serde(rename = "text")]
    Text {
        /// Text content.
        text: String,
    },
    /// Image URL content block.
    #[serde(rename = "image_url")]
    ImageUrl {
        /// Image URL data.
        image_url: OpenAIImageUrl,
    },
    /// Audio input content block (nested or flat format).
    #[serde(rename = "input_audio")]
    InputAudio {
        /// Nested format audio data.
        #[serde(skip_serializing_if = "Option::is_none")]
        input_audio: Option<InputAudioContent>,
        /// Flat format: base64 audio data.
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<String>,
        /// Flat format: audio format (wav, mp3, etc.).
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
    /// Video URL content block.
    #[serde(rename = "video_url")]
    VideoUrl {
        /// Video URL data.
        video_url: VideoUrlContent,
    },
}

impl OpenAIContentBlock {
    /// Extract audio content from either nested or flat format.
    #[must_use]
    pub fn extract_audio(&self) -> Option<InputAudioContent> {
        match self {
            OpenAIContentBlock::InputAudio { input_audio, data, format } => {
                if let Some(nested) = input_audio {
                    return Some(nested.clone());
                }
                if let (Some(audio_data), Some(audio_format)) = (data, format) {
                    return Some(InputAudioContent {
                        data: audio_data.clone(),
                        format: audio_format.clone(),
                    });
                }
                None
            },
            OpenAIContentBlock::Text { text: _ }
            | OpenAIContentBlock::ImageUrl { image_url: _ }
            | OpenAIContentBlock::VideoUrl { video_url: _ } => None,
        }
    }
}

/// Image URL with optional detail level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpenAIImageUrl {
    /// Image URL (data URI or HTTP URL).
    pub url: String,
    /// Detail level ("low", "high", "auto").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Audio content with base64 data and format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct InputAudioContent {
    /// Base64-encoded audio data.
    pub data: String,
    /// Audio format (wav, mp3, ogg, flac, m4a, aac).
    pub format: String,
}

/// Video URL content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct VideoUrlContent {
    /// Video URL (data URI or HTTP URL).
    pub url: String,
}

/// Message in OpenAI conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OpenAIMessage {
    /// Role (system, user, assistant, tool).
    pub role: String,
    /// Message content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<OpenAIContent>,
    /// Reasoning content for o1 models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// Tool calls made by assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID for tool responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Function name for tool messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Tool call made by assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCall {
    /// Unique tool call identifier.
    pub id: String,
    /// Tool type (always "function").
    pub r#type: String,
    /// Function call details.
    pub function: ToolFunction,
}

/// Function call details in tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolFunction {
    /// Function name.
    pub name: String,
    /// JSON-encoded function arguments.
    pub arguments: String,
}

/// OpenAI chat completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OpenAIResponse {
    /// Response identifier.
    pub id: String,
    /// Object type ("chat.completion").
    pub object: String,
    /// Unix timestamp of creation.
    pub created: u64,
    /// Model used for completion.
    pub model: String,
    /// Completion choices.
    pub choices: Vec<Choice>,
    /// Token usage statistics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAIUsage>,
}

/// Single completion choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Choice {
    /// Choice index.
    pub index: u32,
    /// Generated message.
    pub message: OpenAIMessage,
    /// Reason for completion (stop, length, tool_calls).
    pub finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OpenAIUsage {
    /// Tokens in prompt.
    pub prompt_tokens: u32,
    /// Tokens in completion.
    pub completion_tokens: u32,
    /// Total tokens used.
    pub total_tokens: u32,
    /// Prompt token breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    /// Completion token breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

/// Prompt token details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PromptTokensDetails {
    /// Tokens from cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
}

/// Completion token details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CompletionTokensDetails {
    /// Tokens used for reasoning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
}
