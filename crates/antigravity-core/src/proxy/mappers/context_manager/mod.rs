//! Context Manager Module
//!
//! Responsible for estimating token usage and purifying context (stripping thinking blocks)
//! to prevent "Prompt is too long" errors and avoid invalid signatures.

// Token estimation uses arithmetic on u32 counters.
// Values are bounded by context window limits (~2M tokens max).
// Overflow is impossible in practice; saturating_add would hide bugs.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "Token estimation: bounded by context window limits, overflow impossible"
)]

mod estimation;
mod tool_rounds;

#[cfg(test)]
mod tests;

use super::claude::models::{ClaudeRequest, ContentBlock, Message, MessageContent, SystemPrompt};
use tracing::{debug, info};

pub use estimation::estimate_tokens_from_str;
pub use tool_rounds::{identify_tool_rounds, ToolRound};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurificationStrategy {
    /// Soft purification: Retains recent thinking blocks (~2 turns), removes older ones
    Soft,
    /// Aggressive purification: Removes ALL thinking blocks to save maximum tokens
    Aggressive,
}

#[derive(Debug, Clone)]
pub struct ContextStats {
    pub estimated_tokens: u32,
    pub limit: u32,
    pub usage_ratio: f32,
}

pub struct ContextManager;

impl ContextManager {
    pub fn purify_history(messages: &mut [Message], strategy: PurificationStrategy) -> bool {
        let protected_last_n = match strategy {
            PurificationStrategy::Soft => 4,
            PurificationStrategy::Aggressive => 0,
        };

        Self::strip_thinking_blocks(messages, protected_last_n)
    }

    fn strip_thinking_blocks(messages: &mut [Message], protected_last_n: usize) -> bool {
        let total_msgs = messages.len();
        if total_msgs == 0 {
            return false;
        }

        let start_protection_idx = total_msgs.saturating_sub(protected_last_n);
        let mut modified = false;

        for (i, msg) in messages.iter_mut().enumerate() {
            if i >= start_protection_idx {
                continue;
            }

            if msg.role == "assistant" {
                if let MessageContent::Array(blocks) = &mut msg.content {
                    let original_len = blocks.len();
                    blocks.retain(|b| !matches!(b, ContentBlock::Thinking { .. }));

                    if blocks.len() != original_len {
                        modified = true;
                        debug!(
                            "[ContextManager] Stripped {} thinking blocks from message {}",
                            original_len - blocks.len(),
                            i
                        );
                    }
                }
            }
        }

        modified
    }

    pub fn estimate_token_usage(request: &ClaudeRequest) -> u32 {
        let mut total = 0;

        if let Some(sys) = &request.system {
            match sys {
                SystemPrompt::String(s) => total += estimate_tokens_from_str(s),
                SystemPrompt::Array(blocks) => {
                    for block in blocks {
                        total += estimate_tokens_from_str(&block.text);
                    }
                },
            }
        }

        for msg in &request.messages {
            total += 4;

            match &msg.content {
                MessageContent::String(s) => {
                    total += estimate_tokens_from_str(s);
                },
                MessageContent::Array(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                total += estimate_tokens_from_str(text);
                            },
                            ContentBlock::Thinking { thinking, .. } => {
                                total += estimate_tokens_from_str(thinking);
                                total += 100;
                            },
                            ContentBlock::RedactedThinking { data } => {
                                total += estimate_tokens_from_str(data);
                            },
                            ContentBlock::ToolUse { name, input, .. } => {
                                total += 20;
                                total += estimate_tokens_from_str(name);
                                if let Ok(json_str) = serde_json::to_string(input) {
                                    total += estimate_tokens_from_str(&json_str);
                                }
                            },
                            ContentBlock::ToolResult { content, .. } => {
                                total += 10;
                                if let Some(s) = content.as_str() {
                                    total += estimate_tokens_from_str(s);
                                } else if let Some(arr) = content.as_array() {
                                    for item in arr {
                                        if let Some(text) =
                                            item.get("text").and_then(|t| t.as_str())
                                        {
                                            total += estimate_tokens_from_str(text);
                                        }
                                    }
                                } else if let Ok(s) = serde_json::to_string(content) {
                                    total += estimate_tokens_from_str(&s);
                                }
                            },
                            // Intentionally ignored: Image/Document/ServerToolUse/WebSearchToolResult
                            // contribute negligible token overhead vs. serialization cost
                            _ => {},
                        }
                    }
                },
            }
        }

        if let Some(tools) = &request.tools {
            for tool in tools {
                if let Ok(json_str) = serde_json::to_string(tool) {
                    total += estimate_tokens_from_str(&json_str);
                }
            }
        }

        if let Some(thinking) = &request.thinking {
            if let Some(budget) = thinking.budget_tokens {
                total += budget;
            }
        }

        total
    }

    /// [Layer 2] Compress thinking content while preserving signatures
    pub fn compress_thinking_preserve_signature(
        messages: &mut [Message],
        protected_last_n: usize,
    ) -> bool {
        let total_msgs = messages.len();
        if total_msgs == 0 {
            return false;
        }

        let start_protection_idx = total_msgs.saturating_sub(protected_last_n);
        let mut compressed_count = 0;
        let mut total_chars_saved = 0;

        for (i, msg) in messages.iter_mut().enumerate() {
            if i >= start_protection_idx {
                continue;
            }

            if msg.role == "assistant" {
                if let MessageContent::Array(blocks) = &mut msg.content {
                    for block in blocks.iter_mut() {
                        if let ContentBlock::Thinking { thinking, signature, .. } = block {
                            if signature.is_some() && thinking.len() > 10 {
                                let original_len = thinking.len();
                                *thinking = "...".to_string();
                                compressed_count += 1;
                                total_chars_saved += original_len - 3;

                                debug!(
                                    "[ContextManager] [Layer-2] Compressed thinking: {} → 3 chars (signature preserved)",
                                    original_len
                                );
                            }
                        }
                    }
                }
            }
        }

        if compressed_count > 0 {
            let estimated_tokens_saved = (total_chars_saved as f32 / 3.5).ceil() as u32;
            info!(
                "[ContextManager] [Layer-2] Compressed {} thinking blocks (saved ~{} tokens, signatures preserved)",
                compressed_count, estimated_tokens_saved
            );
        }

        compressed_count > 0
    }

    /// [Layer 3 Helper] Extract the last valid thinking signature from message history
    pub fn extract_last_valid_signature(messages: &[Message]) -> Option<String> {
        for msg in messages.iter().rev() {
            if msg.role == "assistant" {
                if let MessageContent::Array(blocks) = &msg.content {
                    for block in blocks {
                        if let ContentBlock::Thinking { signature: Some(sig), .. } = block {
                            if sig.len() >= 50 {
                                debug!(
                                    "[ContextManager] [Layer-3] Extracted last valid signature (len: {})",
                                    sig.len()
                                );
                                return Some(sig.clone());
                            }
                        }
                    }
                }
            }
        }

        debug!("[ContextManager] [Layer-3] No valid signature found in history");
        None
    }

    /// [Layer 1] Trim old tool messages, keeping only the last N rounds
    pub fn trim_tool_messages(messages: &mut Vec<Message>, keep_last_n_rounds: usize) -> bool {
        let tool_rounds = identify_tool_rounds(messages);

        if tool_rounds.len() <= keep_last_n_rounds {
            return false;
        }

        let rounds_to_remove = tool_rounds.len() - keep_last_n_rounds;
        let mut indices_to_remove = std::collections::HashSet::new();

        for round in tool_rounds.iter().take(rounds_to_remove) {
            for idx in &round.indices {
                indices_to_remove.insert(*idx);
            }
        }

        let mut removed_count = 0;
        for idx in (0..messages.len()).rev() {
            if indices_to_remove.contains(&idx) {
                messages.remove(idx);
                removed_count += 1;
            }
        }

        if removed_count > 0 {
            info!(
                "[ContextManager] [Layer-1] Trimmed {} tool messages, kept last {} rounds",
                removed_count, keep_last_n_rounds
            );
        }

        removed_count > 0
    }
}
