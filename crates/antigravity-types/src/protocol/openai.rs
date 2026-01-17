//! OpenAI ChatCompletions API types.

use serde::{Deserialize, Serialize};

/// OpenAI message role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OpenAIRole {
    System,
    User,
    Assistant,
    Tool,
    Function,
}

/// OpenAI chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: OpenAIRole,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// OpenAI usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenAIUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}
