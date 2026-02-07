// Claude mapper module
// Handles Claude ↔ Gemini protocol transformation

pub mod claude_models;
pub mod claude_response;
pub mod collector;
pub mod content_block;
pub mod gemini_models;
pub mod grounding;
pub mod grounding_models;
pub mod models;
pub mod request;
pub mod response;
pub mod sse_stream;
pub mod streaming;
pub mod thinking_utils;
pub mod token_scaling;

#[cfg(test)]
mod collector_tests;
#[cfg(test)]
mod signature_tests;
#[cfg(test)]
mod tests_request;

pub use collector::collect_stream_to_json;
pub use grounding::process_grounding_metadata;
pub use models::*;
pub use request::{
    clean_cache_control_from_messages, merge_consecutive_messages, transform_claude_request_in,
};
pub use response::transform_response;
pub use sse_stream::{create_claude_sse_stream, emit_force_stop};
pub use streaming::{PartProcessor, StreamingState};
pub use thinking_utils::{
    close_tool_loop_for_thinking, filter_invalid_thinking_blocks_with_family, has_valid_signature,
    remove_trailing_unsigned_thinking,
};
