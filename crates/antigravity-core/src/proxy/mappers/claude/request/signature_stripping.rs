//! Strip non-Claude thinking signatures for Claude models on Vertex AI.
//!
//! Claude on Vertex only accepts Claude-generated signatures. Gemini-generated
//! signatures (even long, valid-looking ones) are incompatible and will be
//! rejected with "Invalid `signature` in `thinking` block".
//!
//! Strategy:
//!   - Claude-origin signatures (family cache hit with "claude-*") → KEEP (valid)
//!   - Gemini-origin signatures → STRIP (incompatible)
//!   - Unknown-origin signatures (cache miss) → STRIP (safer than crashing)
//!   - Dummy/missing/short signatures → STRIP

use serde_json::Value;

use super::signature_validator::DUMMY_SIGNATURE;
use super::MIN_SIGNATURE_LENGTH;

pub fn strip_non_claude_thought_signatures(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let is_thought = map
                .get("thought")
                .map(|v| v.as_bool().unwrap_or(false) || v.as_str().is_some())
                .unwrap_or(false);

            if is_thought {
                let sig = map.get("thoughtSignature").and_then(|s| s.as_str());

                let keep = match sig {
                    Some(s) if s == DUMMY_SIGNATURE => false,
                    Some(s) if s.len() < MIN_SIGNATURE_LENGTH => false,
                    Some(s) => {
                        // Check signature family cache to determine origin
                        let family = crate::proxy::SignatureCache::global().get_signature_family(s);
                        match family {
                            Some(f) if f.starts_with("claude") => {
                                tracing::info!(
                                    "[Claude-Vertex] Keeping Claude-origin signature (family: {}, len: {})",
                                    f, s.len()
                                );
                                true
                            },
                            Some(f) => {
                                tracing::info!(
                                    "[Claude-Vertex] Stripping non-Claude signature (family: {}, len: {})",
                                    f, s.len()
                                );
                                false
                            },
                            None => {
                                tracing::info!(
                                    "[Claude-Vertex] Stripping unknown-origin signature (len: {}). \
                                     Cache miss — safer to strip than risk invalid signature error.",
                                    s.len()
                                );
                                false
                            },
                        }
                    },
                    None => false,
                };

                if !keep {
                    map.remove("thoughtSignature");
                    map.remove("thought");
                }
            }

            for v in map.values_mut() {
                strip_non_claude_thought_signatures(v);
            }
        },
        Value::Array(arr) => {
            for v in arr {
                strip_non_claude_thought_signatures(v);
            }
        },
        // Primitive JSON values (string, number, bool, null) have no nested
        // thought signatures to strip — nothing to do.
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {},
    }
}
