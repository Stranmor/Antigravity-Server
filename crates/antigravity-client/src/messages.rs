//! Message types for chat completions API.

use serde::{Deserialize, Serialize};

/// Request body for chat completions endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    /// Model identifier (e.g., "gemini-3-pro", "claude-opus-4-5").
    pub model: String,
    /// Conversation messages.
    pub messages: Vec<ChatMessage>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature (0.0-2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Enable SSE streaming response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message author ("user", "assistant", "system").
    pub role: String,
    /// Text content of the message.
    pub content: String,
}

/// Response from chat completions endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    /// Unique response identifier.
    pub id: Option<String>,
    /// Model that generated the response.
    pub model: Option<String>,
    /// Generated completion choices.
    pub choices: Vec<ChatChoice>,
    /// Token usage statistics.
    pub usage: Option<Usage>,
}

/// A single completion choice in the response.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatChoice {
    /// Index of this choice in the list.
    pub index: u32,
    /// Complete message (non-streaming responses).
    pub message: Option<ChatMessage>,
    /// Incremental content (streaming responses).
    pub delta: Option<ChatDelta>,
    /// Reason generation stopped ("stop", "length", etc.).
    pub finish_reason: Option<String>,
}

/// Incremental content delta for streaming responses.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatDelta {
    /// Role of the message author (first chunk only).
    pub role: Option<String>,
    /// Incremental text content.
    pub content: Option<String>,
}

/// Token usage statistics for a request.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Usage {
    /// Tokens in the input prompt.
    pub prompt_tokens: u32,
    /// Tokens in the generated completion.
    pub completion_tokens: u32,
    /// Total tokens used (prompt + completion).
    pub total_tokens: u32,
}

/// A single SSE chunk in a streaming response.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    /// Unique response identifier.
    pub id: Option<String>,
    /// Model that generated the response.
    pub model: Option<String>,
    /// Generated completion choices.
    pub choices: Vec<ChatChoice>,
}

/// Configuration for retry behavior on transient errors.
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial delay between retries in milliseconds.
    pub base_delay_ms: u64,
    /// Maximum delay between retries in milliseconds.
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self { max_retries: 3, base_delay_ms: 500, max_delay_ms: 30_000 }
    }
}

/// Configuration for the Antigravity client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the Antigravity server.
    pub base_url: String,
    /// API key for authentication.
    pub api_key: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Retry configuration for transient errors.
    pub retry: RetryConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8045".to_string(),
            api_key: "sk-antigravity".to_string(),
            timeout_secs: 120,
            retry: RetryConfig::default(),
        }
    }
}
