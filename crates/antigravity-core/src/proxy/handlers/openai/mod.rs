// OpenAI-compatible API handlers
// Split from monolithic openai.rs for maintainability

mod chat;
mod completions;
mod models;
mod responses_format;

pub use chat::handle_chat_completions;
pub use completions::handle_completions;
pub use models::handle_list_models;

// Shared imports for submodules
use super::retry_strategy::{
    apply_retry_strategy, determine_retry_strategy, peek_first_data_chunk, PeekConfig, PeekResult,
};
use axum::{extract::Json, extract::State, http::StatusCode, response::IntoResponse};
use bytes::Bytes;
use serde_json::{json, Value};
use tracing::{debug, info};

use crate::proxy::mappers::openai::{
    transform_openai_request, transform_openai_response, OpenAIRequest,
};
use crate::proxy::server::AppState;
use crate::proxy::session_manager::SessionManager;

const MAX_RETRY_ATTEMPTS: usize = 64; // Capped by pool_size - tries ALL accounts with quota
