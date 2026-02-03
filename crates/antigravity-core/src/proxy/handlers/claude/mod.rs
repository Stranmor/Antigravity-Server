//! Claude protocol handlers
//!
//! This module handles Claude API requests, transforming them to Gemini format
//! and handling streaming/non-streaming responses.

mod background_detection;
mod messages;
mod retry_logic;
mod warmup;

pub use messages::{handle_count_tokens, handle_list_models, handle_messages};
