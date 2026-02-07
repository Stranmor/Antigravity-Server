use super::model_compat::is_model_compatible;
use super::safety::MIN_SIGNATURE_LENGTH;
use serde_json::{json, Value};

/// Dummy signature that tells Gemini to skip signature validation.
///
/// Per Google docs: https://ai.google.dev/gemini-api/docs/thought-signatures#faqs
/// Used when real signature is unavailable, incompatible, or missing.
pub const DUMMY_SIGNATURE: &str = "skip_thought_signature_validator";

pub enum SignatureAction {
    UseWithSignature { part: Value },
}

pub fn validate_thinking_signature(
    thinking: &str,
    signature: Option<&String>,
    is_retry: bool,
    mapped_model: &str,
    last_thought_signature: &mut Option<String>,
) -> SignatureAction {
    if let Some(sig) = signature {
        let cached_family = crate::proxy::SignatureCache::global().get_signature_family(sig);

        match cached_family {
            Some(family) => {
                let compatible = !is_retry && is_model_compatible(&family, mapped_model);

                if !compatible {
                    tracing::warn!(
                        "[Thinking-Signature] {} signature (Family: {}, Target: {}). Using dummy signature to preserve thinking.",
                        if is_retry { "Retry mode - historical" } else { "Incompatible" },
                        family,
                        mapped_model
                    );
                    return make_thinking_part_with_dummy(thinking, last_thought_signature);
                }
                *last_thought_signature = Some(sig.clone());
                let mut part = json!({
                    "text": thinking,
                    "thought": true,
                    "thoughtSignature": sig
                });
                crate::proxy::common::json_schema::clean_json_schema(&mut part);
                SignatureAction::UseWithSignature { part }
            },
            None => {
                if sig.len() >= MIN_SIGNATURE_LENGTH {
                    tracing::debug!(
                        "[Thinking-Signature] Unknown signature origin but valid length (len: {}), using as-is.",
                        sig.len()
                    );
                    *last_thought_signature = Some(sig.clone());
                    let mut part = json!({
                        "text": thinking,
                        "thought": true,
                        "thoughtSignature": sig
                    });
                    crate::proxy::common::json_schema::clean_json_schema(&mut part);
                    SignatureAction::UseWithSignature { part }
                } else {
                    tracing::warn!(
                        "[Thinking-Signature] Signature too short (len: {}). Using dummy signature to preserve thinking.",
                        sig.len()
                    );
                    make_thinking_part_with_dummy(thinking, last_thought_signature)
                }
            },
        }
    } else {
        // Try content cache first
        if let Some((recovered_sig, recovered_family)) =
            crate::proxy::SignatureCache::global().get_content_signature(thinking)
        {
            tracing::info!(
                "[Thinking-Signature] Recovered signature from CONTENT cache (len: {}, family: {})",
                recovered_sig.len(),
                recovered_family
            );

            if !is_retry && is_model_compatible(&recovered_family, mapped_model) {
                *last_thought_signature = Some(recovered_sig.clone());
                let mut part = json!({
                    "text": thinking,
                    "thought": true,
                    "thoughtSignature": recovered_sig
                });
                crate::proxy::common::json_schema::clean_json_schema(&mut part);
                return SignatureAction::UseWithSignature { part };
            }
        }

        tracing::warn!(
            "[Thinking-Signature] No signature provided and content cache miss. Using dummy signature to preserve thinking."
        );
        make_thinking_part_with_dummy(thinking, last_thought_signature)
    }
}

/// Creates a thinking part with the dummy signature that skips upstream validation.
fn make_thinking_part_with_dummy(
    thinking: &str,
    last_thought_signature: &mut Option<String>,
) -> SignatureAction {
    *last_thought_signature = Some(DUMMY_SIGNATURE.to_string());
    let mut part = json!({
        "text": thinking,
        "thought": true,
        "thoughtSignature": DUMMY_SIGNATURE
    });
    crate::proxy::common::json_schema::clean_json_schema(&mut part);
    SignatureAction::UseWithSignature { part }
}

pub fn resolve_tool_signature(
    id: &str,
    client_signature: Option<&String>,
    last_thought_signature: &Option<String>,
    session_id: &str,
) -> Option<String> {
    client_signature
        .or(last_thought_signature.as_ref())
        .cloned()
        .or_else(|| {
            crate::proxy::SignatureCache::global()
                .get_session_signature(session_id)
                .inspect(|s| {
                    tracing::info!(
                        "[Claude-Request] Recovered signature from SESSION cache (session: {}, len: {})",
                        session_id,
                        s.len()
                    );
                })
        })
        .or_else(|| {
            crate::proxy::SignatureCache::global()
                .get_tool_signature(id)
                .inspect(|_s| {
                    tracing::info!(
                        "[Claude-Request] Recovered signature from TOOL cache for tool_id: {}",
                        id
                    );
                })
        })
}

pub fn should_use_tool_signature(
    sig: &str,
    id: &str,
    mapped_model: &str,
    is_thinking_enabled: bool,
) -> bool {
    // Always accept dummy signatures
    if sig == DUMMY_SIGNATURE {
        return true;
    }

    if sig.len() < MIN_SIGNATURE_LENGTH {
        tracing::warn!(
            "[Tool-Signature] Signature too short for tool_use: {} (len: {})",
            id,
            sig.len()
        );
        return false;
    }

    let cached_family = crate::proxy::SignatureCache::global().get_signature_family(sig);

    match cached_family {
        Some(family) => {
            if is_model_compatible(&family, mapped_model) {
                true
            } else {
                tracing::warn!(
                    "[Tool-Signature] Incompatible signature for tool_use: {} (Family: {}, Target: {})",
                    id,
                    family,
                    mapped_model
                );
                false
            }
        },
        None => {
            if sig.len() >= MIN_SIGNATURE_LENGTH {
                tracing::debug!(
                    "[Tool-Signature] Unknown signature origin but valid length (len: {}) for tool_use: {}, using as-is.",
                    sig.len(),
                    id
                );
                true
            } else if is_thinking_enabled {
                tracing::warn!(
                    "[Tool-Signature] Unknown signature origin and too short for tool_use: {} (len: {}). Dropping in thinking mode.",
                    id,
                    sig.len()
                );
                false
            } else {
                true
            }
        },
    }
}

pub fn ensure_thinking_block_first(parts: &mut Vec<Value>) {
    let has_thought_part = parts.iter().any(|p| {
        p.get("thought").and_then(|v| v.as_bool()).unwrap_or(false)
            || p.get("thoughtSignature").is_some()
            || p.get("thought").and_then(|v| v.as_str()).is_some()
    });

    if !has_thought_part {
        parts.insert(
            0,
            json!({
                "text": "Thinking...",
                "thought": true
            }),
        );
        tracing::debug!(
            "Injected dummy thought block for historical assistant message at index {}",
            parts.len()
        );
    } else {
        let first_is_thought = parts.first().is_some_and(|p| {
            (p.get("thought").is_some() || p.get("thoughtSignature").is_some())
                && p.get("text").is_some()
        });

        if !first_is_thought {
            parts.insert(
                0,
                json!({
                    "text": "...",
                    "thought": true
                }),
            );
            tracing::debug!(
                "First part of model message at {} is not a valid thought block. Prepending dummy.",
                parts.len()
            );
        } else if let Some(p0) = parts.get_mut(0) {
            if p0.get("thought").is_none() {
                p0.as_object_mut().map(|obj| obj.insert("thought".to_string(), json!(true)));
            }
        }
    }
}
