//! Thinking mode detection.

use super::super::models::{ContentBlock, Message, MessageContent};
use super::safety::MIN_SIGNATURE_LENGTH;

// [REMOVED-2026-02-07] should_disable_thinking_due_to_history was removed because it caused
// an infinite degradation loop:
//   1. Thinking disabled → response has ToolUse but no Thinking block
//   2. Next request: detects "ToolUse without Thinking" → disables thinking again
//   3. Repeat forever, producing 87-token micro-responses
//
// For thinking models, thinking must ALWAYS remain enabled. The upstream API handles
// mixed tool-use/thinking history natively.

/// Check if thinking mode should be enabled by default for a given model
///
/// Claude Code v2.0.67+ enables thinking by default for Opus 4.5 models.
/// This function determines if the model should have thinking enabled
/// when no explicit thinking configuration is provided.
pub fn should_enable_thinking_by_default(model: &str) -> bool {
    let model_lower = model.to_lowercase();

    // Enable thinking by default for Opus 4.5 variants
    if model_lower.contains("opus-4-5") || model_lower.contains("opus-4.5") {
        tracing::debug!("[Thinking-Mode] Auto-enabling thinking for Opus 4.5 model: {}", model);
        return true;
    }

    // Also enable for explicit thinking model variants
    if model_lower.contains("-thinking") {
        return true;
    }

    false
}

/// [FIX #295] Check if we have any valid signature available for function calls
/// This prevents Gemini 3 Pro from rejecting requests due to missing thought_signature
/// Now also checks Session Cache to support retry scenarios.
pub fn has_valid_signature_for_function_calls(
    messages: &[Message],
    global_sig: &Option<String>,
    session_id: &str,
) -> bool {
    // 1. Check global store
    if let Some(sig) = global_sig {
        if sig.len() >= MIN_SIGNATURE_LENGTH {
            tracing::debug!(
                "[Signature-Check] Found valid signature in global store (len: {})",
                sig.len()
            );
            return true;
        }
    }

    // 2. Check Session Cache - critical for retry scenarios
    if let Some(sig) =
        crate::proxy::signature_cache::SignatureCache::global().get_session_signature(session_id)
    {
        if sig.len() >= MIN_SIGNATURE_LENGTH {
            tracing::info!(
                "[Signature-Check] Found valid signature in SESSION cache (session: {}, len: {})",
                session_id,
                sig.len()
            );
            return true;
        }
    }

    // 3. Check if any message has a thinking block with valid signature
    for msg in messages.iter().rev() {
        if msg.role == "assistant" {
            if let MessageContent::Array(blocks) = &msg.content {
                for block in blocks {
                    if let ContentBlock::Thinking { signature: Some(sig), .. } = block {
                        if sig.len() >= MIN_SIGNATURE_LENGTH {
                            tracing::debug!(
                                "[Signature-Check] Found valid signature in message history (len: {})",
                                sig.len()
                            );
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
