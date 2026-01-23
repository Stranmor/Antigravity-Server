//! Context Manager Module
//!
//! Responsible for estimating token usage and purifying context (stripping thinking blocks)
//! to prevent "Prompt is too long" errors and avoid invalid signatures.

use super::claude::models::{ClaudeRequest, ContentBlock, Message, MessageContent, SystemPrompt};
use tracing::{debug, info};

/// Strategy for context purification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurificationStrategy {
    /// Soft purification: Retains recent thinking blocks (~2 turns), removes older ones
    Soft,
    /// Aggressive purification: Removes ALL thinking blocks to save maximum tokens
    Aggressive,
}

/// Context Statistics (our local addition)
#[derive(Debug, Clone)]
pub struct ContextStats {
    pub estimated_tokens: u32,
    pub limit: u32,
    pub usage_ratio: f32,
}

/// Helper to estimate tokens from text with multi-language awareness
///
/// Improved estimation algorithm:
/// - ASCII/English: ~4 characters per token
/// - Unicode/CJK: ~1.5 characters per token (Chinese, Japanese, Korean are tokenized differently)
/// - Adds 15% safety margin to prevent underestimation
fn estimate_tokens_from_str(s: &str) -> u32 {
    if s.is_empty() {
        return 0;
    }

    let mut ascii_chars = 0u32;
    let mut unicode_chars = 0u32;

    for c in s.chars() {
        if c.is_ascii() {
            ascii_chars += 1;
        } else {
            unicode_chars += 1;
        }
    }

    // ASCII: ~4 chars/token, Unicode/CJK: ~1.5 chars/token
    let ascii_tokens = (ascii_chars as f32 / 4.0).ceil() as u32;
    let unicode_tokens = (unicode_chars as f32 / 1.5).ceil() as u32;

    // Add 15% safety margin to account for tokenizer variations
    ((ascii_tokens + unicode_tokens) as f32 * 1.15).ceil() as u32
}

/// Context Manager implementation
pub struct ContextManager;

impl ContextManager {
    /// Purify message history based on the selected strategy
    ///
    /// This removes Thinking blocks completely (unlike compression which keeps placeholders/signatures)
    /// Used when context is critical or signatures are invalid.
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
}

impl ContextManager {
    pub fn estimate_token_usage(request: &ClaudeRequest) -> u32 {
        let mut total = 0;

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

        for msg in &request.messages {
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
                                total += 100;
                            }
                            ContentBlock::RedactedThinking { data } => {
                                total += estimate_tokens_from_str(data);
                            }
                            ContentBlock::ToolUse { name, input, .. } => {
                                total += 20;
                                total += estimate_tokens_from_str(name);
                                if let Ok(json_str) = serde_json::to_string(input) {
                                    total += estimate_tokens_from_str(&json_str);
                                }
                            }
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
                            }
                            _ => {}
                        }
                    }
                }
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
                        if let ContentBlock::Thinking {
                            thinking,
                            signature,
                            ..
                        } = block
                        {
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
                        if let ContentBlock::Thinking {
                            signature: Some(sig),
                            ..
                        } = block
                        {
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

#[derive(Debug)]
struct ToolRound {
    _assistant_index: usize,
    tool_result_indices: Vec<usize>,
    indices: Vec<usize>,
}

fn identify_tool_rounds(messages: &[Message]) -> Vec<ToolRound> {
    let mut rounds = Vec::new();
    let mut current_round: Option<ToolRound> = None;

    for (i, msg) in messages.iter().enumerate() {
        match msg.role.as_str() {
            "assistant" => {
                if has_tool_use(&msg.content) {
                    if let Some(round) = current_round.take() {
                        rounds.push(round);
                    }
                    current_round = Some(ToolRound {
                        _assistant_index: i,
                        tool_result_indices: Vec::new(),
                        indices: vec![i],
                    });
                }
            }
            "user" => {
                if let Some(ref mut round) = current_round {
                    if has_tool_result(&msg.content) {
                        round.tool_result_indices.push(i);
                        round.indices.push(i);
                    } else {
                        rounds.push(current_round.take().unwrap());
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(round) = current_round {
        rounds.push(round);
    }

    debug!(
        "[ContextManager] Identified {} tool rounds in {} messages",
        rounds.len(),
        messages.len()
    );

    rounds
}

fn has_tool_use(content: &MessageContent) -> bool {
    if let MessageContent::Array(blocks) = content {
        blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }))
    } else {
        false
    }
}

fn has_tool_result(content: &MessageContent) -> bool {
    if let MessageContent::Array(blocks) = content {
        blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        if let MessageContent::Array(blocks) = &messages[0].content {
            assert_eq!(blocks.len(), 1);
            if let ContentBlock::Text { text } = &blocks[0] {
                assert_eq!(text, "A0");
            } else {
                panic!("Wrong block");
            }
        }

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
