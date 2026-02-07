use super::streaming::StreamingState;
use crate::proxy::signature_metrics::record_thinking_degradation;

pub fn validate_thinking_response(
    state: &StreamingState,
    trace_id: &str,
    finish_reason: &str,
    input_tokens: u32,
    output_tokens: u32,
    cached_tokens: u32,
) {
    let cache_info =
        if cached_tokens > 0 { format!(", Cached: {}", cached_tokens) } else { String::new() };

    let thinking_info = if state.has_thinking_received() { " | Thinking: ✓" } else { "" };

    tracing::info!(
        "[{}] ✓ Stream completed | In: {} tokens | Out: {} tokens{}{} | Reason: {}",
        trace_id,
        input_tokens.saturating_sub(cached_tokens),
        output_tokens,
        cache_info,
        thinking_info,
        finish_reason
    );

    if let Some(ref model) = state.model_name {
        let is_thinking_model =
            model.contains("thinking") || model.contains("pro-2.5") || model.contains("flash-2.5");
        if is_thinking_model && !state.has_thinking_received() {
            tracing::debug!(
                "[{}] Thinking model responded without thinking | Model: {} | \
                 Output tokens: {} (model decided thinking was not needed).",
                trace_id,
                model,
                output_tokens
            );
        }
        if is_thinking_model && state.has_thinking_received() {
            if let Some(ref sid) = state.session_id {
                let has_sig = crate::proxy::SignatureCache::global().has_session_signature(sid);
                if has_sig {
                    tracing::debug!(
                        "[{}] ✓ Thinking response validated: signature cached for session {}",
                        trace_id,
                        sid
                    );
                } else {
                    tracing::warn!(
                        "[{}] ⚠ Thinking response received but no signature cached for session {}. \
                         Next request may fail signature validation.",
                        trace_id,
                        sid
                    );
                    record_thinking_degradation();
                }
            }
        }
    }
}
