use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

impl Tool {
    pub fn is_web_search(&self) -> bool {
        if let Some(ref t) = self.type_ {
            if t.starts_with("web_search") {
                return true;
            }
        }
        if let Some(ref n) = self.name {
            if n == "web_search" {
                return true;
            }
        }
        false
    }

    #[allow(dead_code)]
    pub fn get_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            if let Some(ref t) = self.type_ {
                if t.starts_with("web_search") {
                    return "web_search".to_string();
                }
            }
            "unknown".to_string()
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<serde_json::Value>,
}

use super::content_block::ContentBlock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub role: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}
