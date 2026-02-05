//! OpenAI ChatCompletions API types.

use serde::{Deserialize, Serialize};

/// OpenAI message role.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpenAIRole {
    /// System message providing context or instructions.
    System,
    /// User message from the human.
    User,
    /// Assistant message from the model.
    Assistant,
    /// Tool result message.
    Tool,
    /// Function call result (deprecated, use Tool).
    Function,
}

/// OpenAI chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    /// Role of the message author.
    pub role: OpenAIRole,
    /// Text content of the message.
    pub content: Option<String>,
    /// Optional name for the participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// ID of the tool call this message responds to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// OpenAI usage statistics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct OpenAIUsage {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,
    /// Number of tokens in the completion.
    pub completion_tokens: u32,
    /// Total tokens used (prompt + completion).
    pub total_tokens: u32,
}
