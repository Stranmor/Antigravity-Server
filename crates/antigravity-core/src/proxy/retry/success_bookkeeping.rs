//! Shared success bookkeeping for all protocol handlers.
//!
//! After a successful upstream response, all handlers must update
//! the same three tracking systems. This module consolidates that logic.

use crate::proxy::server::AppState;
use crate::proxy::token_manager::TokenManager;
use std::sync::Arc;

/// Records successful request completion across all tracking systems.
///
/// Updates:
/// 1. Account health (mark success in token manager)
/// 2. Session failure counter (clear failures for this session)
/// 3. Adaptive rate limits (record success for AIMD algorithm)
pub fn record_request_success(
    token_manager: &Arc<TokenManager>,
    state: &AppState,
    email: &str,
    session_id: &str,
) {
    token_manager.mark_account_success(email);
    token_manager.clear_session_failures(session_id);
    state.adaptive_limits.record_success(email);
}
