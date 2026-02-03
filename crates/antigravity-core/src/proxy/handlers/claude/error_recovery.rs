//! Error recovery and retry handling for Claude messages

use crate::proxy::mappers::claude::models::{ContentBlock, MessageContent};
use crate::proxy::mappers::claude::{close_tool_loop_for_thinking, ClaudeRequest};

pub fn handle_thinking_signature_error(
    request: &mut ClaudeRequest,
    session_id: Option<&str>,
    trace_id: &str,
) {
    let mut preserved_sig: Option<String> = None;
    for msg in request.messages.iter().rev() {
        if let MessageContent::Array(blocks) = &msg.content {
            for block in blocks.iter().rev() {
                if let ContentBlock::Thinking {
                    signature: Some(sig),
                    ..
                } = block
                {
                    if sig.len() >= 50 {
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
                "[{}] Preserved signature (len={}) to session cache before stripping thinking blocks",
                trace_id,
                sig.len()
            );
        }
    }

    tracing::warn!(
        "[{}] Unexpected thinking signature error (should have been filtered). \
         Retrying with all thinking blocks removed.",
        trace_id
    );

    for msg in request.messages.iter_mut() {
        if let MessageContent::Array(blocks) = &mut msg.content {
            let mut new_blocks = Vec::with_capacity(blocks.len());
            for block in blocks.drain(..) {
                match block {
                    ContentBlock::Thinking { thinking, .. } => {
                        if !thinking.is_empty() {
                            tracing::debug!(
                                "[Fallback] Converting thinking block to text (len={})",
                                thinking.len()
                            );
                            new_blocks.push(ContentBlock::Text { text: thinking });
                        }
                    }
                    ContentBlock::RedactedThinking { .. } => {}
                    _ => new_blocks.push(block),
                }
            }
            *blocks = new_blocks;
        }
    }

    close_tool_loop_for_thinking(&mut request.messages);

    if request.model.contains("claude-") {
        let mut m = request.model.clone();
        m = m.replace("-thinking", "");
        if m.contains("claude-sonnet-4-5-") {
            m = "claude-sonnet-4-5".to_string();
        } else if m.contains("claude-opus-4-5-") || m.contains("claude-opus-4-") {
            m = "claude-opus-4-5".to_string();
        }
        request.model = m;
    }
}

pub fn apply_background_task_cleanup(
    request: &mut ClaudeRequest,
    downgrade_model: &str,
    trace_id: &str,
    original_model: &str,
) {
    tracing::info!(
        "[{}][AUTO] 检测到后台任务,强制降级: {} -> {}",
        trace_id,
        original_model,
        downgrade_model
    );

    request.tools = None;
    request.thinking = None;

    for msg in request.messages.iter_mut() {
        if let MessageContent::Array(blocks) = &mut msg.content {
            blocks.retain(|b| {
                !matches!(
                    b,
                    ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. }
                )
            });
        }
    }

    request.model = downgrade_model.to_string();
}

pub fn apply_user_request_cleanup(request: &mut ClaudeRequest, trace_id: &str, mapped_model: &str) {
    use crate::proxy::mappers::claude::remove_trailing_unsigned_thinking;

    tracing::debug!(
        "[{}][USER] 用户交互请求,保持映射: {}",
        trace_id,
        mapped_model
    );

    for msg in request.messages.iter_mut() {
        if msg.role == "assistant" || msg.role == "model" {
            if let MessageContent::Array(blocks) = &mut msg.content {
                remove_trailing_unsigned_thinking(blocks);
            }
        }
    }
}
