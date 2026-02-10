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

/// Repair functionCall/functionResponse pairing after signature stripping.
/// If a `model` message contains `functionCall` IDs that have no matching
/// `functionResponse` in the next `user` message, inject synthetic responses
/// to prevent Claude API "tool_use ids without tool_result" errors.
pub fn repair_tool_pairing_after_strip(value: &mut Value) {
    // Collect repair actions first to avoid borrow conflicts
    struct RepairAction {
        content_idx: usize,
        missing_ids: Vec<(String, String)>, // (id, function_name)
        /// true = next message is "user" → append parts; false = insert new user message
        append_to_next: bool,
    }

    let actions: Vec<RepairAction> = {
        let Some(contents) = value.pointer("/request/contents").and_then(|c| c.as_array()) else {
            return;
        };
        let len = contents.len();
        let mut actions = Vec::new();

        for i in 0..len {
            let role = contents[i].get("role").and_then(|r| r.as_str()).unwrap_or("");
            if role != "model" {
                continue;
            }

            // Collect all functionCall IDs and names from this model message
            let fc_entries: Vec<(String, String)> = contents[i]
                .get("parts")
                .and_then(|p| p.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| {
                            let fc = part.get("functionCall")?;
                            let id = fc.get("id").and_then(|v| v.as_str())?;
                            let name = fc.get("name").and_then(|v| v.as_str()).unwrap_or(id);
                            Some((id.to_string(), name.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();

            if fc_entries.is_empty() {
                continue;
            }

            // Skip last message — it's the current turn being generated
            if i + 1 >= len {
                continue;
            }

            let next_role = contents[i + 1].get("role").and_then(|r| r.as_str()).unwrap_or("");

            if next_role == "user" {
                // Check which IDs are already answered
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

                let missing: Vec<(String, String)> =
                    fc_entries.into_iter().filter(|(id, _)| !fr_ids.contains(id)).collect();

                if !missing.is_empty() {
                    actions.push(RepairAction {
                        content_idx: i,
                        missing_ids: missing,
                        append_to_next: true,
                    });
                }
            } else {
                // Next message is model (no user message in between)
                actions.push(RepairAction {
                    content_idx: i,
                    missing_ids: fc_entries,
                    append_to_next: false,
                });
            }
        }
        actions
    };

    if actions.is_empty() {
        return;
    }

    // Apply repairs in reverse order to preserve indices
    let contents = value
        .pointer_mut("/request/contents")
        .and_then(|c| c.as_array_mut())
        .expect("contents array must exist at this point");

    for action in actions.into_iter().rev() {
        let synthetic_parts: Vec<Value> = action
            .missing_ids
            .iter()
            .map(|(id, name)| {
                json!({
                    "functionResponse": {
                        "name": name,
                        "response": {
                            "result": "Tool execution interrupted. No result provided."
                        },
                        "id": id
                    }
                })
            })
            .collect();

        let ids_debug: Vec<&str> = action.missing_ids.iter().map(|(id, _)| id.as_str()).collect();

        if action.append_to_next {
            // Append to existing user message at content_idx + 1
            tracing::warn!(
                "[Claude-Vertex] TOOL PAIRING REPAIR at contents[{}]: injecting {} synthetic functionResponse(s) into next user message for IDs: {:?}",
                action.content_idx,
                synthetic_parts.len(),
                ids_debug
            );
            if let Some(next_parts) =
                contents[action.content_idx + 1].get_mut("parts").and_then(|p| p.as_array_mut())
            {
                // Insert at beginning so tool results come before text
                for (j, part) in synthetic_parts.into_iter().enumerate() {
                    next_parts.insert(j, part);
                }
            }
        } else {
            // Insert a new synthetic user message after content_idx
            tracing::warn!(
                "[Claude-Vertex] TOOL PAIRING REPAIR at contents[{}]: inserting synthetic user message with {} functionResponse(s) for IDs: {:?}",
                action.content_idx,
                synthetic_parts.len(),
                ids_debug
            );
            let synthetic_user_msg = json!({
                "role": "user",
                "parts": synthetic_parts
            });
            contents.insert(action.content_idx + 1, synthetic_user_msg);
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
    fn repair_injects_missing_function_response_into_user_message() {
        let mut value = json!({
            "request": {
                "contents": [
                    {
                        "role": "model",
                        "parts": [
                            {"functionCall": {"id": "call-1", "name": "my_tool", "args": {}}}
                        ]
                    },
                    {
                        "role": "user",
                        "parts": [
                            {"text": "reply without tool result"}
                        ]
                    }
                ]
            }
        });

        repair_tool_pairing_after_strip(&mut value);

        let user_parts =
            value.pointer("/request/contents/1/parts").and_then(|p| p.as_array()).unwrap();
        // Should have 2 parts: injected functionResponse + original text
        assert_eq!(user_parts.len(), 2);
        // First part should be the injected functionResponse
        assert!(user_parts[0].get("functionResponse").is_some());
        let fr = &user_parts[0]["functionResponse"];
        assert_eq!(fr["id"].as_str().unwrap(), "call-1");
        assert_eq!(fr["name"].as_str().unwrap(), "my_tool");
        // Second part should be original text
        assert!(user_parts[1].get("text").is_some());
    }

    #[test]
    fn repair_inserts_synthetic_user_message_for_model_model_adjacency() {
        let mut value = json!({
            "request": {
                "contents": [
                    {
                        "role": "model",
                        "parts": [
                            {"functionCall": {"id": "call-1", "name": "tool_a", "args": {}}}
                        ]
                    },
                    {
                        "role": "model",
                        "parts": [
                            {"text": "next model response"}
                        ]
                    }
                ]
            }
        });

        repair_tool_pairing_after_strip(&mut value);

        let contents = value.pointer("/request/contents").and_then(|c| c.as_array()).unwrap();
        // Should now have 3 messages: model, synthetic user, model
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[1]["role"].as_str().unwrap(), "user");
        let user_parts = contents[1].get("parts").and_then(|p| p.as_array()).unwrap();
        assert_eq!(user_parts.len(), 1);
        assert!(user_parts[0].get("functionResponse").is_some());
        assert_eq!(user_parts[0]["functionResponse"]["id"].as_str().unwrap(), "call-1");
    }

    #[test]
    fn repair_does_not_mutate_when_pairing_is_correct() {
        let mut value = json!({
            "request": {
                "contents": [
                    {
                        "role": "model",
                        "parts": [
                            {"functionCall": {"id": "call-1", "name": "tool_a", "args": {}}}
                        ]
                    },
                    {
                        "role": "user",
                        "parts": [
                            {"functionResponse": {"id": "call-1", "name": "tool_a", "response": {"result": "ok"}}}
                        ]
                    }
                ]
            }
        });
        let before = value.clone();

        repair_tool_pairing_after_strip(&mut value);

        assert_eq!(value, before);
    }

    #[test]
    fn repair_skips_last_model_message_function_call() {
        // Last model message functionCall is the current turn being generated — should not be touched
        let mut value = json!({
            "request": {
                "contents": [
                    {
                        "role": "user",
                        "parts": [{"text": "hello"}]
                    },
                    {
                        "role": "model",
                        "parts": [
                            {"functionCall": {"id": "call-99", "name": "active_tool", "args": {}}}
                        ]
                    }
                ]
            }
        });
        let before = value.clone();

        repair_tool_pairing_after_strip(&mut value);

        assert_eq!(value, before);
    }
}
