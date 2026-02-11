//! Claude API request model types.
//!
//! This module defines the request structures for the Claude Messages API,
//! including messages, system prompts, and configuration options.

use serde::{Deserialize, Serialize};

use super::claude_response::{Metadata, OutputConfig, Tool};
use super::content_block::ContentBlock;

/// Request body for Claude Messages API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeRequest {
    /// Model identifier to use for completion.
    pub model: String,
    /// List of messages in the conversation.
    pub messages: Vec<Message>,
    /// Optional system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    /// Optional list of tools available to the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p nucleus sampling parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Extended thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Client-provided stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Request metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Output configuration options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
}

/// Configuration for extended thinking feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    /// Type of thinking (e.g., "enabled").
    #[serde(rename = "type")]
    pub type_: String,
    /// Token budget for thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

/// System prompt that can be a string or structured blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    /// Simple string system prompt.
    String(String),
    /// Array of structured system blocks.
    Array(Vec<SystemBlock>),
}

/// A structured block within a system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    /// Block type (e.g., "text").
    #[serde(rename = "type")]
    pub block_type: String,
    /// Text content of the block.
    pub text: String,
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message author (e.g., "user", "assistant").
    pub role: String,
    /// Content of the message.
    pub content: MessageContent,
}

/// Message content that can be a string or array of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple string content.
    String(String),
    /// Array of content blocks.
    Array(Vec<ContentBlock>),
}
