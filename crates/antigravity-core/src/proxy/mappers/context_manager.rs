//! Context Manager Module
//!
//! Responsible for estimating token usage and purifying context (stripping thinking blocks)
//! to prevent "Prompt is too long" errors and avoid invalid signatures.

use super::claude::models::{ClaudeRequest, ContentBlock, Message, MessageContent, SystemPrompt};
use tracing::{debug, info};

/// Purification Strategy for Context History
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PurificationStrategy {
    /// Do nothing, keep all thinking blocks
    None,
    /// Keep thinking blocks only in the last 2 turns
    Soft,
    /// Remove ALL thinking blocks in history
    Aggressive,
}

/// Context Statistics
#[derive(Debug, Clone)]
pub struct ContextStats {
    pub estimated_tokens: u32,
    pub limit: u32,
    pub usage_ratio: f32,
}

/// Helper to estimate tokens from text (approx 3.5 chars per token)
fn estimate_tokens_from_str(s: &str) -> u32 {
    (s.len() as f32 / 3.5).ceil() as u32
}

/// Context Manager implementation
pub struct ContextManager;

impl ContextManager {
    /// Estimate token usage for a Claude Request
    ///
    /// This is a lightweight estimation, not a precise count.
    /// It iterates through all messages and blocks to sum up estimated tokens.
    pub fn estimate_token_usage(request: &ClaudeRequest) -> u32 {
        let mut total = 0;

        // System prompt
        if let Some(sys) = &request.system {
            match sys {
                SystemPrompt::String(s) => total += estimate_tokens_from_str(s),
                SystemPrompt::Array(blocks) => {
                    for block in blocks {
                        total += estimate_tokens_from_str(&block.text);
                    }
                }
            }
        }

        // Messages
        for msg in &request.messages {
            // Message overhead
            total += 4;

            match &msg.content {
                MessageContent::String(s) => {
                    total += estimate_tokens_from_str(s);
                }
                MessageContent::Array(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                total += estimate_tokens_from_str(text);
                            }
                            ContentBlock::Thinking { thinking, .. } => {
                                total += estimate_tokens_from_str(thinking);
                                // Signature overhead
                                total += 100;
                            }
                            ContentBlock::RedactedThinking { data } => {
                                total += estimate_tokens_from_str(data);
                            }
                            ContentBlock::ToolUse { name, input, .. } => {
                                total += 20; // Function call overhead
                                total += estimate_tokens_from_str(name);
                                if let Ok(json_str) = serde_json::to_string(input) {
                                    total += estimate_tokens_from_str(&json_str);
                                }
                            }
                            ContentBlock::ToolResult { content, .. } => {
                                total += 10; // Result overhead
                                             // content is serde_json::Value
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
                                } else {
                                    // Fallback for objects or other types
                                    if let Ok(s) = serde_json::to_string(content) {
                                        total += estimate_tokens_from_str(&s);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Tools definition overhead (rough estimate)
        if let Some(tools) = &request.tools {
            for tool in tools {
                if let Ok(json_str) = serde_json::to_string(tool) {
                    total += estimate_tokens_from_str(&json_str);
                }
            }
        }

        // Thinking budget overhead if enabled
        if let Some(thinking) = &request.thinking {
            if let Some(budget) = thinking.budget_tokens {
                // Reserve budget in estimation
                total += budget;
            }
        }

        total
    }

    /// Purify history based on strategy
    ///
    /// Modifies the messages vector in-place.
    /// - Level 0 (None): No change
    /// - Level 1 (Soft): Keep thinking in last 2 turns, strip others
    /// - Level 2 (Aggressive): Strip ALL thinking in history (except current generation which is handled by LLM)
    pub fn purify_history(messages: &mut [Message], strategy: PurificationStrategy) -> bool {
        if strategy == PurificationStrategy::None {
            return false;
        }

        let total_msgs = messages.len();
        if total_msgs == 0 {
            return false;
        }

        let mut modified = false;

        // Determine the number of protected turns (most recent)
        let protected_count = if strategy == PurificationStrategy::Soft {
            4 // Protect last 4 messages (~2 turns)
        } else {
            0
        };

        // Protected range start index
        let start_protection_idx = total_msgs.saturating_sub(protected_count);

        for (i, msg) in messages.iter_mut().enumerate() {
            let is_protected = i >= start_protection_idx;

            // Only process Assistant messages
            if msg.role == "assistant" && !is_protected {
                if let MessageContent::Array(blocks) = &mut msg.content {
                    let initial_len = blocks.len();

                    // Filter out Thinking blocks
                    // IMPORTANT: This also removes the `signature` field inside the block
                    blocks.retain(|b| {
                        !matches!(
                            b,
                            ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. }
                        )
                    });

                    if blocks.len() != initial_len {
                        modified = true;

                        // If message becomes empty (it was only thinking), replace with placeholder
                        // to maintain valid conversation structure
                        if blocks.is_empty() {
                            blocks.push(ContentBlock::Text {
                                text: "...".to_string(),
                            });
                            debug!(
                                "[ContextManager] Replaced empty assistant message with placeholder"
                            );
                        }
                    }
                }
            }
        }

        if modified {
            info!(
                "[ContextManager] Purified history with strategy: {:?} (Protected last {} msgs)",
                strategy, protected_count
            );
        }

        modified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a request since Default is not implemented
    fn create_test_request() -> ClaudeRequest {
        ClaudeRequest {
            model: "claude-3-5-sonnet".into(),
            messages: vec![],
            system: None,
            tools: None,
            stream: false,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            thinking: None,
            metadata: None,
            output_config: None,
        }
    }

    #[test]
    fn test_estimate_tokens() {
        let mut req = create_test_request();
        req.messages = vec![Message {
            role: "user".into(),
            content: MessageContent::String("Hello World".into()),
        }];

        let tokens = ContextManager::estimate_token_usage(&req);
        assert!(tokens > 0);
        assert!(tokens < 50);
    }

    #[test]
    fn test_purify_history_soft() {
        // Construct history of 6 messages (indices 0-5)
        // 0: Assistant (Ancient) -> Should be purified
        // 1: User
        // 2: Assistant (Old) -> Should be protected (index 2 >= 6-4=2)
        // 3: User
        // 4: Assistant (Recent) -> Should be protected
        // 5: User

        let mut messages = vec![
            Message {
                role: "assistant".into(),
                content: MessageContent::Array(vec![
                    ContentBlock::Thinking {
                        thinking: "ancient".into(),
                        signature: None,
                        cache_control: None,
                    },
                    ContentBlock::Text { text: "A0".into() },
                ]),
            },
            Message {
                role: "user".into(),
                content: MessageContent::String("Q1".into()),
            },
            Message {
                role: "assistant".into(),
                content: MessageContent::Array(vec![
                    ContentBlock::Thinking {
                        thinking: "old".into(),
                        signature: None,
                        cache_control: None,
                    },
                    ContentBlock::Text { text: "A1".into() },
                ]),
            },
            Message {
                role: "user".into(),
                content: MessageContent::String("Q2".into()),
            },
            Message {
                role: "assistant".into(),
                content: MessageContent::Array(vec![
                    ContentBlock::Thinking {
                        thinking: "recent".into(),
                        signature: None,
                        cache_control: None,
                    },
                    ContentBlock::Text { text: "A2".into() },
                ]),
            },
            Message {
                role: "user".into(),
                content: MessageContent::String("current".into()),
            },
        ];

        ContextManager::purify_history(&mut messages, PurificationStrategy::Soft);

        // 0: Ancient -> Filtered
        if let MessageContent::Array(blocks) = &messages[0].content {
            assert_eq!(blocks.len(), 1);
            if let ContentBlock::Text { text } = &blocks[0] {
                assert_eq!(text, "A0");
            } else {
                panic!("Wrong block");
            }
        }

        // 2: Old -> Protected
        if let MessageContent::Array(blocks) = &messages[2].content {
            assert_eq!(blocks.len(), 2);
        }
    }

    #[test]
    fn test_purify_history_aggressive() {
        let mut messages = vec![Message {
            role: "assistant".into(),
            content: MessageContent::Array(vec![
                ContentBlock::Thinking {
                    thinking: "thought".into(),
                    signature: None,
                    cache_control: None,
                },
                ContentBlock::Text {
                    text: "text".into(),
                },
            ]),
        }];

        ContextManager::purify_history(&mut messages, PurificationStrategy::Aggressive);

        if let MessageContent::Array(blocks) = &messages[0].content {
            assert_eq!(blocks.len(), 1);
            assert!(matches!(blocks[0], ContentBlock::Text { .. }));
        }
    }
}
