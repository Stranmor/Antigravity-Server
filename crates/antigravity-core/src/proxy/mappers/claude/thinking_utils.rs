use super::models::{ContentBlock, Message, MessageContent};
use crate::proxy::SignatureCache;
use tracing::{debug, info, warn};

pub const MIN_SIGNATURE_LENGTH: usize = 50;
pub const GEMINI_SKIP_SIGNATURE: &str = "skip_thought_signature_validator";

#[derive(Debug, Default)]
pub struct ConversationState {
    pub in_tool_loop: bool,
    pub interrupted_tool: bool,
    pub last_assistant_idx: Option<usize>,
}

/// Analyze the conversation to detect tool loops or interrupted tool calls
pub fn analyze_conversation_state(messages: &[Message]) -> ConversationState {
    let mut state = ConversationState::default();

    if messages.is_empty() {
        return state;
    }

    // Find last assistant message index
    for (i, msg) in messages.iter().enumerate().rev() {
        if msg.role == "assistant" {
            state.last_assistant_idx = Some(i);
            break;
        }
    }

    // A tool loop starts if the assistant message has tool use blocks
    let has_tool_use = if let Some(idx) = state.last_assistant_idx {
        if let Some(msg) = messages.get(idx) {
            if let MessageContent::Array(blocks) = &msg.content {
                blocks
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolUse { .. }))
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    if !has_tool_use {
        return state;
    }

    // Check what follows the assistant's tool use
    if let Some(last_msg) = messages.last() {
        if last_msg.role == "user" {
            if let MessageContent::Array(blocks) = &last_msg.content {
                // Case 1: Final message is ToolResult -> Active Tool Loop
                if blocks
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
                {
                    state.in_tool_loop = true;
                    debug!(
                        "[Thinking-Recovery] Active tool loop detected (last msg is ToolResult)."
                    );
                } else {
                    // Case 2: Final message is Text (User) -> Interrupted Tool
                    state.interrupted_tool = true;
                    debug!(
                        "[Thinking-Recovery] Interrupted tool detected (last msg is Text user)."
                    );
                }
            } else if let MessageContent::String(_) = &last_msg.content {
                // Case 2: Final message is String (User) -> Interrupted Tool
                state.interrupted_tool = true;
                debug!("[Thinking-Recovery] Interrupted tool detected (last msg is String user).");
            }
        }
    }

    // Check for interrupted tool: Last assistant message has ToolUse, but no corresponding ToolResult in next user msg
    // (This is harder to detect perfectly on a stateless request, but usually if we are
    //  in a state where we have ToolUse but the conversation seems "broken" or stripped)
    // Actually, in the proxy context, we typically see:
    // ... Assistant (ToolUse) -> User (ToolResult) : Normal Loop
    // ... Assistant (ToolUse) -> User (Text) : Interrupted (User cancelled)

    // For "Thinking Utils", we care about the case where valid signatures are missing.
    // If we are in a tool loop (last msg is ToolResult), and the *preceding* Assistant message
    // had its Thinking block stripped (due to invalid sig), then we are in a "Broken Tool Loop".
    // Gemini/Claude will reject a ToolResult if the preceding Assistant message didn't start with Thinking.

    state
}

/// Recover from broken tool loops or interrupted tool calls by injecting synthetic messages
pub fn close_tool_loop_for_thinking(messages: &mut Vec<Message>) {
    let state = analyze_conversation_state(messages);

    if !state.in_tool_loop && !state.interrupted_tool {
        return;
    }

    // Check if the last assistant message has a valid thinking block
    let mut has_valid_thinking = false;
    if let Some(idx) = state.last_assistant_idx {
        if let Some(msg) = messages.get(idx) {
            if let MessageContent::Array(blocks) = &msg.content {
                for block in blocks {
                    if let ContentBlock::Thinking {
                        thinking,
                        signature,
                        ..
                    } = block
                    {
                        if !thinking.is_empty()
                            && signature
                                .as_ref()
                                .map(|s| s.len() >= MIN_SIGNATURE_LENGTH)
                                .unwrap_or(false)
                        {
                            has_valid_thinking = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    if !has_valid_thinking {
        if state.in_tool_loop {
            info!(
                "[Thinking-Recovery] Broken tool loop (ToolResult without preceding Thinking). Recovery triggered."
            );

            // Insert acknowledging message to "close" the history turn
            messages.push(Message {
                role: "assistant".to_string(),
                content: MessageContent::Array(vec![ContentBlock::Text {
                    text: "[System: Tool execution completed. Proceeding to final response.]"
                        .to_string(),
                }]),
            });
            messages.push(Message {
                role: "user".to_string(),
                content: MessageContent::Array(vec![ContentBlock::Text {
                    text: "Please provide the final result based on the tool output above."
                        .to_string(),
                }]),
            });
        } else if state.interrupted_tool {
            info!(
                "[Thinking-Recovery] Interrupted tool call detected. Injecting synthetic closure."
            );

            // For interrupted tool, we need to insert the closure AFTER the assistant's tool use
            // but BEFORE the user's latest message.
            if let Some(idx) = state.last_assistant_idx {
                messages.insert(
                    idx + 1,
                    Message {
                        role: "assistant".to_string(),
                        content: MessageContent::Array(vec![ContentBlock::Text {
                            text: "[Tool call was interrupted by user.]".to_string(),
                        }]),
                    },
                );
            }
        }
    }
}

/// Cache the relationship between a signature and its model family
pub fn cache_signature_family(signature: &str, family: &str) {
    SignatureCache::global().cache_thinking_family(signature.to_string(), family.to_string());
}

/// Get the model family origin of a signature
pub fn get_signature_family(signature: &str) -> Option<String> {
    SignatureCache::global().get_signature_family(signature)
}

/// [CRITICAL] Sanitize thinking blocks and check cross-model compatibility
pub fn filter_invalid_thinking_blocks_with_family(
    messages: &mut [Message],
    target_family: Option<&str>,
) {
    let mut stripped_count = 0;

    for msg in messages.iter_mut() {
        if msg.role != "assistant" {
            continue;
        }

        if let MessageContent::Array(blocks) = &mut msg.content {
            let original_len = blocks.len();
            blocks.retain(|block| {
                if let ContentBlock::Thinking { signature, .. } = block {
                    // 1. Basic length check
                    let sig = match signature {
                        Some(s) if s.len() >= MIN_SIGNATURE_LENGTH => s,
                        _ => {
                            stripped_count += 1;
                            return false;
                        }
                    };

                    // 2. Family compatibility check (Prevents SONNET-Thinking sig being sent to OPUS-Thinking)
                    if let Some(target) = target_family {
                        if let Some(origin_family) = get_signature_family(sig) {
                            if origin_family != target {
                                warn!("[Thinking-Sanitizer] Dropping signature from family '{}' for target '{}'", origin_family, target);
                                stripped_count += 1;
                                return false;
                            }
                        }
                    }
                }
                true
            });

            // SAFETY: Claude API requires at least one block
            if blocks.is_empty() && original_len > 0 {
                blocks.push(ContentBlock::Text {
                    text: ".".to_string(),
                });
            }
        }
    }

    if stripped_count > 0 {
        info!(
            "[Thinking-Sanitizer] Stripped {} invalid or incompatible thinking blocks",
            stripped_count
        );
    }
}

/// Check if a thinking block has a valid signature
pub fn has_valid_signature(block: &ContentBlock) -> bool {
    match block {
        ContentBlock::Thinking {
            signature,
            thinking,
            ..
        } => {
            // Empty thinking + any signature = valid (trailing signature case)
            if thinking.is_empty() && signature.is_some() {
                return true;
            }

            // Strict validation: signature must be in cache
            if let Some(sig) = signature {
                // Check length
                if sig.len() < MIN_SIGNATURE_LENGTH {
                    debug!(
                        "[Signature-Validation] Signature too short: {} chars",
                        sig.len()
                    );
                    return false;
                }

                // Check if in cache (ADVISORY - don't reject valid signatures on cache miss)
                // FIX: Previously rejected valid signatures when family cache expired/missed
                let cached_family = SignatureCache::global().get_signature_family(sig);
                if cached_family.is_none() {
                    // Advisory only - valid length signature accepted even without cached family
                    // This prevents signature loss on cache TTL expiry or cold start
                    debug!(
                        "[Signature-Validation] Unknown signature origin (len: {}). Accepting anyway (valid length).",
                        sig.len()
                    );
                }

                // Signature valid if length check passed (family check is advisory)
                true
            } else {
                // No signature
                false
            }
        }
        _ => true, // Non-thinking blocks are valid by default
    }
}

/// Remove trailing unsigned thinking blocks from a message
pub fn remove_trailing_unsigned_thinking(blocks: &mut Vec<ContentBlock>) {
    if blocks.is_empty() {
        return;
    }

    // Scan backwards
    let mut end_index = blocks.len();
    for i in (0..blocks.len()).rev() {
        match &blocks[i] {
            ContentBlock::Thinking { .. } => {
                if !has_valid_signature(&blocks[i]) {
                    end_index = i;
                } else {
                    break; // Found valid signed thinking block, stop
                }
            }
            _ => break, // Non-thinking block, stop
        }
    }

    if end_index < blocks.len() {
        let removed = blocks.len() - end_index;
        blocks.truncate(end_index);
        debug!("Removed {} trailing unsigned thinking block(s)", removed);
    }
}
