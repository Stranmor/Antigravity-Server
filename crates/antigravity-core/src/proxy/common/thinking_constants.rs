//! Thinking mode budget constants shared across protocol handlers.

/// Default thinking budget in tokens when not specified by client.
pub const THINKING_BUDGET: u64 = 16000;
/// Overhead added to thinking budget for maxOutputTokens calculation.
pub const THINKING_OVERHEAD: u64 = 32768;
/// Minimum overhead used when client specifies a small max_tokens.
pub const THINKING_MIN_OVERHEAD: u64 = 8192;
