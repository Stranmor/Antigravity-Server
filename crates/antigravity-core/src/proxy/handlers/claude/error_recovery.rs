//! Error recovery and retry handling for Claude messages

use crate::proxy::mappers::claude::models::{ContentBlock, MessageContent};
use crate::proxy::mappers::claude::request::DUMMY_SIGNATURE;
use crate::proxy::mappers::claude::ClaudeRequest;

pub fn handle_thinking_signature_error(
    request: &mut ClaudeRequest,
    session_id: Option<&str>,
    trace_id: &str,
) {
    let mut preserved_sig: Option<String> = None;
    let mut fixed_thinking = 0usize;
    let mut fixed_tool_use = 0usize;

    // First pass: find any existing valid signature to preserve in session cache
    for msg in request.messages.iter().rev() {
        if let MessageContent::Array(blocks) = &msg.content {
            for block in blocks.iter().rev() {
                if let ContentBlock::Thinking { signature: Some(sig), .. } = block {
                    if sig.len() >= 50 && sig != DUMMY_SIGNATURE {
                        preserved_sig = Some(sig.clone());
                        break;
                    }
                }
            }
        }
        if preserved_sig.is_some() {
            break;
        }
    }

    if let Some(sig) = &preserved_sig {
        if let Some(sid) = session_id {
            crate::proxy::SignatureCache::global().cache_session_signature(sid, sig.clone());
            tracing::info!(
                "[{}] Preserved signature (len={}) to session cache before fixing signatures",
                trace_id,
                sig.len()
            );
        }
    }

    // Second pass: inject dummy signatures into thinking and tool_use blocks that lack valid ones
    for msg in request.messages.iter_mut() {
        if let MessageContent::Array(blocks) = &mut msg.content {
            for block in blocks.iter_mut() {
                match block {
                    ContentBlock::Thinking { signature, .. } => {
                        let needs_fix = match signature.as_ref() {
                            None => true,
                            Some(s) => s.len() < 50,
                        };
                        if needs_fix {
                            *signature = Some(DUMMY_SIGNATURE.to_string());
                            fixed_thinking += 1;
                        }
                    },
                    ContentBlock::ToolUse { signature, .. } => {
                        if signature.is_none() {
                            *signature = Some(DUMMY_SIGNATURE.to_string());
                            fixed_tool_use += 1;
                        }
                    },
                    // Intentionally ignored: only Thinking and ToolUse blocks need signature injection
                    _ => {},
                }
            }
        }
    }

    tracing::info!(
        "[{}] Injected dummy signatures: {} thinking blocks, {} tool_use blocks. \
         Retrying with thinking PRESERVED (no model downgrade).",
        trace_id,
        fixed_thinking,
        fixed_tool_use
    );

    // IMPORTANT: Do NOT downgrade the model. Thinking must stay enabled.
}

pub fn apply_background_task_cleanup(
    request: &mut ClaudeRequest,
    downgrade_model: &str,
    trace_id: &str,
    original_model: &str,
) {
    tracing::info!(
        "[{}][AUTO] detecttoafterbackgroundtask,forcefallback: {} -> {}",
        trace_id,
        original_model,
        downgrade_model
    );

    request.tools = None;
    request.thinking = None;

    for msg in request.messages.iter_mut() {
        if let MessageContent::Array(blocks) = &mut msg.content {
            blocks.retain(|b| {
                !matches!(b, ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. })
            });
        }
    }

    request.model = downgrade_model.to_string();
}

pub fn apply_user_request_cleanup(request: &mut ClaudeRequest, trace_id: &str, mapped_model: &str) {
    tracing::debug!(
        "[{}][USER] userinteractiverequest,maintainmapping: {}",
        trace_id,
        mapped_model
    );

    // Instead of removing unsigned thinking, inject dummy signatures to preserve them
    for msg in request.messages.iter_mut() {
        if msg.role == "assistant" || msg.role == "model" {
            if let MessageContent::Array(blocks) = &mut msg.content {
                for block in blocks.iter_mut() {
                    if let ContentBlock::Thinking { signature, .. } = block {
                        let needs_fix = match signature.as_ref() {
                            None => true,
                            Some(s) => s.len() < 50,
                        };
                        if needs_fix {
                            *signature = Some(DUMMY_SIGNATURE.to_string());
                        }
                    }
                }
            }
        }
    }
}
