//! Remove thinking blocks entirely from previous turns for Claude on Vertex AI.
//!
//! The Gemini generateContent endpoint's `thoughtSignature` field is Gemini's own
//! signature system. Claude API signatures (from Anthropic's `signature` field)
//! are incompatible when placed in Gemini's `thoughtSignature`.
//!
//! Per Anthropic docs:
//!   "you can omit thinking blocks from previous turns, or let the API strip
//!    them for you if you pass them back"
//!
//! But we can't "let the API strip them" because the API rejects them BEFORE
//! stripping (Invalid signature / Field required).
//!
//! Solution: Remove thinking parts entirely from all turns EXCEPT the very last
//! assistant turn (which doesn't have prior thinking blocks — it's being generated).
//! This preserves all non-thinking content (text, tool_use, tool_result).

use serde_json::Value;

pub fn strip_non_claude_thought_signatures(value: &mut Value) {
    // Walk into contents array
    if let Some(contents) = value.pointer_mut("/request/contents").and_then(|c| c.as_array_mut()) {
        for content in contents.iter_mut() {
            if let Some(parts) = content.get_mut("parts").and_then(|p| p.as_array_mut()) {
                // Remove parts that are thinking blocks (have `thought` field)
                let before = parts.len();
                parts.retain(|part| {
                    let is_thought = part
                        .get("thought")
                        .map(|v| v.as_bool().unwrap_or(false) || v.as_str().is_some())
                        .unwrap_or(false);

                    if is_thought {
                        let text_len = part
                            .get("text")
                            .and_then(|t| t.as_str())
                            .map(|s| s.len())
                            .unwrap_or(0);
                        tracing::info!(
                            "[Claude-Vertex] Removing thinking part (text_len: {}) from previous turn",
                            text_len
                        );
                        return false;
                    }

                    // Also strip thoughtSignature from functionCall parts
                    // (tool_use blocks) — same incompatibility
                    // NOTE: we mutate in retain via unsafe but actually
                    // we just skip signature stripping here and do it below
                    true
                });

                if parts.len() != before {
                    tracing::debug!(
                        "[Claude-Vertex] Removed {} thinking parts from content",
                        before - parts.len()
                    );
                }

                // Strip thoughtSignature from remaining functionCall parts
                for part in parts.iter_mut() {
                    if part.get("functionCall").is_some()
                        && part
                            .as_object_mut()
                            .is_some_and(|m| m.remove("thoughtSignature").is_some())
                    {
                        tracing::debug!(
                            "[Claude-Vertex] Removed thoughtSignature from functionCall part"
                        );
                    }
                }
            }
        }

        // Remove any content entries that now have empty parts arrays
        contents.retain(|c| {
            c.get("parts").and_then(|p| p.as_array()).is_none_or(|arr| !arr.is_empty())
        });
    }
}
