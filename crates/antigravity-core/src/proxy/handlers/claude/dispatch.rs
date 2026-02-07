//! Dispatch mode decision logic for Claude messages handler

use crate::proxy::mappers::claude::ClaudeRequest;
use crate::proxy::server::AppState;
use axum::http::HeaderMap;
use axum::response::Response;

pub struct DispatchDecision {
    pub use_zai: bool,
}

pub async fn decide_dispatch_mode(
    _state: &AppState,
    _request: &ClaudeRequest,
    _trace_id: &str,
) -> DispatchDecision {
    DispatchDecision { use_zai: false }
}

/// Strip Gemini-specific dummy signatures from thinking blocks before sending to Anthropic API.
///
/// The `DUMMY_SIGNATURE` ("skip_thought_signature_validator") is a Gemini/Vertex-specific bypass
/// value that Anthropic Claude API doesn't recognize, causing "Invalid `signature` in `thinking`
/// block" errors. This function removes such signatures so Anthropic skips validation entirely.
fn sanitize_signatures_for_anthropic(request: &ClaudeRequest) -> ClaudeRequest {
    use crate::proxy::mappers::claude::request::signature_validator::DUMMY_SIGNATURE;
    use crate::proxy::mappers::claude::{ContentBlock, MessageContent};

    let mut sanitized = request.clone();

    for msg in sanitized.messages.iter_mut() {
        if msg.role != "assistant" {
            continue;
        }
        if let MessageContent::Array(blocks) = &mut msg.content {
            for block in blocks.iter_mut() {
                match block {
                    ContentBlock::Thinking { signature, .. } => {
                        if let Some(sig) = signature.as_ref() {
                            if sig == DUMMY_SIGNATURE {
                                tracing::debug!(
                                    "[ZAI-Sanitize] Removing Gemini dummy signature from thinking block"
                                );
                                *signature = None;
                            }
                        }
                    },
                    ContentBlock::ToolUse { signature, .. } => {
                        if let Some(sig) = signature.as_ref() {
                            if sig == DUMMY_SIGNATURE {
                                tracing::debug!(
                                    "[ZAI-Sanitize] Removing Gemini dummy signature from tool_use block"
                                );
                                *signature = None;
                            }
                        }
                    },
                    _ => {},
                }
            }
        }
    }

    sanitized
}

pub async fn forward_to_zai(
    state: &AppState,
    headers: &HeaderMap,
    request: &ClaudeRequest,
) -> Result<Response, String> {
    let sanitized = sanitize_signatures_for_anthropic(request);

    let new_body = serde_json::to_value(&sanitized)
        .map_err(|e| format!("Failed to serialize fixed request for z.ai: {}", e))?;

    Ok(crate::proxy::providers::zai_anthropic::forward_anthropic_json(
        state,
        axum::http::Method::POST,
        "/v1/messages",
        headers,
        new_body,
    )
    .await)
}
