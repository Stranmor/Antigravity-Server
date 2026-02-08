use super::safety::is_valid_or_dummy_signature;
use crate::proxy::signature_metrics::record_signature_validation;
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
    let is_claude = mapped_model.starts_with("claude-");

    if let Some(sig) = signature {
        // Dummy signature is Gemini-specific. For Claude models, strip it
        // and send thinking block without any signature.
        if sig == DUMMY_SIGNATURE {
            if is_claude {
                tracing::debug!("[Thinking-Signature] Dummy signature on Claude model — sending without signature.");
                record_signature_validation("claude_no_sig");
                return make_thinking_part_no_sig(thinking);
            }
            tracing::debug!("[Thinking-Signature] Dummy signature detected, passing through.");
            record_signature_validation("dummy_passthrough");
            *last_thought_signature = Some(DUMMY_SIGNATURE.to_string());
            return make_thinking_part_with_dummy(thinking, last_thought_signature);
        }

        // Validate signature has acceptable length
        if !is_valid_or_dummy_signature(sig) {
            if is_claude {
                tracing::warn!(
                    "[Thinking-Signature] Signature too short (len: {}) on Claude model — sending without signature.",
                    sig.len()
                );
                record_signature_validation("claude_short_no_sig");
                return make_thinking_part_no_sig(thinking);
            }
            tracing::warn!(
                "[Thinking-Signature] Signature too short (len: {}). Using dummy to preserve thinking.",
                sig.len()
            );
            record_signature_validation("dummy_short");
            return make_thinking_part_with_dummy(thinking, last_thought_signature);
        }

        // Client-supplied signature with valid length — pass through as-is.
        // Per Anthropic docs: "signature values are compatible across platforms
        // (Claude APIs, Amazon Bedrock, and Vertex AI). Values generated on one
        // platform will be compatible with another."
        tracing::debug!(
            "[Thinking-Signature] Valid client signature (len: {}), passing through as-is.",
            sig.len()
        );
        record_signature_validation("valid");
        *last_thought_signature = Some(sig.clone());
        make_thinking_part_with_sig(thinking, sig)
    } else {
        // Try content cache first
        if let Some((recovered_sig, _recovered_family)) =
            crate::proxy::SignatureCache::global().get_content_signature(thinking)
        {
            tracing::info!(
                "[Thinking-Signature] Recovered signature from CONTENT cache (len: {}), using as-is.",
                recovered_sig.len()
            );
            record_signature_validation("recovered_content");

            if !is_retry {
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

        // No signature available. For Claude models, send without signature.
        // For Gemini models, use the dummy bypass.
        if is_claude {
            tracing::debug!(
                "[Thinking-Signature] No signature for Claude model — sending without signature."
            );
            record_signature_validation("claude_no_sig");
            make_thinking_part_no_sig(thinking)
        } else {
            tracing::warn!(
                "[Thinking-Signature] No signature provided and content cache miss. Using dummy signature to preserve thinking."
            );
            record_signature_validation("dummy_no_sig");
            make_thinking_part_with_dummy(thinking, last_thought_signature)
        }
    }
}

/// Creates a thinking part with a specific signature.
fn make_thinking_part_with_sig(thinking: &str, sig: &str) -> SignatureAction {
    let mut part = json!({
        "text": thinking,
        "thought": true,
        "thoughtSignature": sig
    });
    crate::proxy::common::json_schema::clean_json_schema(&mut part);
    SignatureAction::UseWithSignature { part }
}

/// Creates a thinking part WITHOUT any signature.
/// Used for Claude models where the Gemini dummy is not valid.
fn make_thinking_part_no_sig(thinking: &str) -> SignatureAction {
    let mut part = json!({
        "text": thinking,
        "thought": true
    });
    crate::proxy::common::json_schema::clean_json_schema(&mut part);
    SignatureAction::UseWithSignature { part }
}

/// Creates a thinking part with the dummy signature that skips upstream validation.
/// Only for Gemini models — Claude does not accept this dummy.
fn make_thinking_part_with_dummy(
    thinking: &str,
    last_thought_signature: &mut Option<String>,
) -> SignatureAction {
    *last_thought_signature = Some(DUMMY_SIGNATURE.to_string());
    make_thinking_part_with_sig(thinking, DUMMY_SIGNATURE)
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
    _mapped_model: &str,
    _is_thinking_enabled: bool,
) -> bool {
    // Always accept dummy signatures
    if sig == DUMMY_SIGNATURE {
        return true;
    }

    if !is_valid_or_dummy_signature(sig) {
        tracing::warn!(
            "[Tool-Signature] Signature too short for tool_use: {} (len: {})",
            id,
            sig.len()
        );
        return false;
    }

    // Signatures are cross-platform compatible — no family check needed.
    // Per Anthropic docs, signatures work across Claude APIs, Bedrock, and Vertex AI.
    true
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
