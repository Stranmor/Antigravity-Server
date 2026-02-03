//! Claude protocol handlers
//!
//! This module handles Claude API requests, transforming them to Gemini format
//! and handling streaming/non-streaming responses.

mod background_detection;
mod dispatch;
mod error_recovery;
mod messages;
mod preprocessing;
mod request_validation;
mod response_handler;
mod retry_logic;
mod streaming;
mod token_selection;
mod warmup;

pub use messages::{handle_count_tokens, handle_list_models, handle_messages};
