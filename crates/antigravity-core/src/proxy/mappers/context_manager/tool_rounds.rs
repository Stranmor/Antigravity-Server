//! Tool round identification for context trimming
#![allow(
    clippy::unwrap_used,
    reason = "current_round.take().unwrap() is guarded by is_some() check"
)]

use crate::proxy::mappers::claude::models::{ContentBlock, Message, MessageContent};
use tracing::debug;

/// Represents a tool use/result round in conversation
#[derive(Debug)]
pub struct ToolRound {
    /// Index of the assistant message that initiated this tool round
    pub assistant_index: usize,
    pub tool_result_indices: Vec<usize>,
    pub indices: Vec<usize>,
}

/// Identify tool use/result rounds in message history
pub fn identify_tool_rounds(messages: &[Message]) -> Vec<ToolRound> {
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
                        assistant_index: i,
                        tool_result_indices: Vec::new(),
                        indices: vec![i],
                    });
                }
            },
            "user" => {
                if let Some(ref mut round) = current_round {
                    if has_tool_result(&msg.content) {
                        round.tool_result_indices.push(i);
                        round.indices.push(i);
                    } else {
                        rounds.push(current_round.take().unwrap());
                    }
                }
            },
            _ => {},
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
        blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. }))
    } else {
        false
    }
}

fn has_tool_result(content: &MessageContent) -> bool {
    if let MessageContent::Array(blocks) = content {
        blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
    } else {
        false
    }
}
