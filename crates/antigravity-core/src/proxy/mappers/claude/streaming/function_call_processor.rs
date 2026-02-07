use bytes::Bytes;
use serde_json::json;

use super::tool_remapping::remap_function_call_args;
use super::{BlockType, StreamingState};
use crate::proxy::mappers::claude::models::FunctionCall;
use crate::proxy::SignatureCache;

pub fn process_function_call(
    state: &mut StreamingState,
    fc: &FunctionCall,
    signature: Option<String>,
) -> Vec<Bytes> {
    let mut chunks = Vec::new();

    state.mark_tool_used();

    let tool_id = fc.id.clone().unwrap_or_else(|| {
        format!("{}-{}", fc.name, crate::proxy::common::random_id::generate_random_id())
    });

    let mut tool_name = fc.name.clone();
    if tool_name.to_lowercase() == "search" {
        tool_name = "grep".to_string();
        tracing::debug!("[Streaming] Normalizing tool name: Search → grep");
    }

    let mut tool_use = json!({
        "type": "tool_use",
        "id": tool_id,
        "name": tool_name,
        "input": {}
    });

    if let Some(ref sig) = signature {
        tool_use["signature"] = json!(sig);

        SignatureCache::global().cache_tool_signature(&tool_id, sig.clone());

        if let Some(session_id) = &state.session_id {
            SignatureCache::global().cache_session_signature(session_id, sig.clone());
        }

        tracing::debug!(
            "[Claude-SSE] Captured thought_signature for function call (length: {})",
            sig.len()
        );
    }

    chunks.extend(state.start_block(BlockType::Function, tool_use));

    if let Some(args) = &fc.args {
        let mut remapped_args = args.clone();

        let tool_name_title = fc.name.clone();
        let mut final_tool_name = tool_name_title;
        if final_tool_name.to_lowercase() == "search" {
            final_tool_name = "Grep".to_string();
        }
        remap_function_call_args(&final_tool_name, &mut remapped_args);

        let json_str = serde_json::to_string(&remapped_args).unwrap_or_else(|_| "{}".to_string());
        chunks.push(state.emit_delta("input_json_delta", json!({ "partial_json": json_str })));
    }

    chunks.extend(state.end_block());

    chunks
}
