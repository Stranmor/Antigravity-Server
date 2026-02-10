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

use serde_json::{json, Value};
use std::collections::HashSet;

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

        for content in contents.iter_mut() {
            let is_empty = content
                .get("parts")
                .and_then(|p| p.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(false);
            if is_empty {
                let role = content.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role == "model" {
                    tracing::debug!(
                        "[Claude-Vertex] Model message became empty after thinking removal. Inserting placeholder."
                    );
                    if let Some(parts) = content.get_mut("parts").and_then(|p| p.as_array_mut()) {
                        parts.push(json!({"text": "..."}));
                    }
                }
            }
        }

        // Only drop genuinely empty user messages (model messages preserve role alternation)
        contents.retain(|c| {
            let role = c.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if role == "model" {
                return true;
            }
            c.get("parts").and_then(|p| p.as_array()).is_none_or(|arr| !arr.is_empty())
        });
    }
}

/// Validate that functionCall/functionResponse pairing is intact after signature stripping.
/// Logs warnings for any orphaned parts but does NOT remove them.
pub fn validate_tool_pairing_after_strip(value: &mut Value) {
    if let Some(contents) = value.pointer("/request/contents").and_then(|c| c.as_array()) {
        let len = contents.len();
        for i in 0..len {
            let role = contents[i].get("role").and_then(|r| r.as_str()).unwrap_or("");
            if role != "model" {
                continue;
            }

            let fc_ids: Vec<String> = contents[i]
                .get("parts")
                .and_then(|p| p.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| {
                            part.get("functionCall")
                                .and_then(|fc| fc.get("id"))
                                .and_then(|id| id.as_str())
                                .map(|id| id.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();

            if fc_ids.is_empty() {
                continue;
            }

            let next_is_user_with_responses = if i + 1 < len {
                let next_role = contents[i + 1].get("role").and_then(|r| r.as_str()).unwrap_or("");
                if next_role == "user" {
                    let fr_ids: HashSet<String> = contents[i + 1]
                        .get("parts")
                        .and_then(|p| p.as_array())
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| {
                                    part.get("functionResponse")
                                        .and_then(|fr| fr.get("id"))
                                        .and_then(|id| id.as_str())
                                        .map(|id| id.to_string())
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    fc_ids.iter().all(|id| fr_ids.contains(id))
                } else {
                    false
                }
            } else {
                true
            };

            if !next_is_user_with_responses && i + 1 < len {
                tracing::warn!(
                    "[Claude-Vertex] TOOL PAIRING VIOLATION at contents[{}]: functionCall IDs {:?} have no matching functionResponse in next message",
                    i,
                    fc_ids
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_model_message_after_thought_stripping() {
        let mut value = json!({
            "request": {
                "contents": [
                    {
                        "role": "model",
                        "parts": [
                            {"thought": true, "text": "thinking"}
                        ]
                    },
                    {
                        "role": "user",
                        "parts": [
                            {"text": "hi"}
                        ]
                    }
                ]
            }
        });

        strip_non_claude_thought_signatures(&mut value);

        let contents = value
            .pointer("/request/contents")
            .and_then(|c| c.as_array())
            .map(|c| c.len())
            .unwrap_or(0);
        assert_eq!(contents, 2);

        let parts = value.pointer("/request/contents/0/parts").and_then(|p| p.as_array());
        if let Some(parts) = parts {
            assert_eq!(parts.len(), 1);
            let text = parts[0].get("text").and_then(|t| t.as_str()).unwrap_or("");
            assert_eq!(text, "...");
        } else {
            panic!("expected model parts array to exist");
        }
    }

    #[test]
    fn validation_does_not_mutate_contents() {
        let mut value = json!({
            "request": {
                "contents": [
                    {
                        "role": "model",
                        "parts": [
                            {"functionCall": {"id": "call-1"}}
                        ]
                    },
                    {
                        "role": "user",
                        "parts": [
                            {"text": "reply"}
                        ]
                    }
                ]
            }
        });
        let before = value.clone();

        validate_tool_pairing_after_strip(&mut value);

        assert_eq!(value, before);
    }
}
