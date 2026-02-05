//! Error handling and retry decision logic for Claude messages

use crate::proxy::mappers::claude::ClaudeRequest;
use crate::proxy::rate_limit::RateLimitReason;
use crate::proxy::server::AppState;
use crate::proxy::token_manager::TokenManager;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::time::Duration;

use super::error_recovery::handle_thinking_signature_error;
use super::request_validation::prompt_too_long_error;
use super::retry_logic::{
    apply_retry_strategy, determine_retry_strategy, is_signature_error, should_rotate_account,
    RetryStrategy,
};

pub enum ErrorAction {
    Retry,
    Return(Response),
}

pub struct ErrorContext<'a> {
    pub status: StatusCode,
    pub status_code: u16,
    pub error_text: String,
    pub retry_after: Option<String>,
    pub email: &'a str,
    pub session_id_str: &'a str,
    pub model: &'a str,
    pub trace_id: &'a str,
    pub attempt: usize,
}

pub async fn handle_upstream_error(
    state: &AppState,
    token_manager: Arc<TokenManager>,
    ctx: &ErrorContext<'_>,
    request_for_body: &mut ClaudeRequest,
    attempted_accounts: &mut HashSet<String>,
    retried_without_thinking: &mut bool,
    grace_retry_used: &mut bool,
) -> ErrorAction {
    if ctx.status_code == 429 && !*grace_retry_used {
        let reason = token_manager.rate_limit_tracker().parse_rate_limit_reason(&ctx.error_text);
        if reason == RateLimitReason::RateLimitExceeded {
            *grace_retry_used = true;
            tracing::info!(
                "[{}] 🔄 Grace retry: RATE_LIMIT_EXCEEDED on {}, waiting 1s before retry on same account",
                ctx.trace_id, ctx.email
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
            return ErrorAction::Retry;
        }
    }

    if ctx.status_code == 429
        || ctx.status_code == 529
        || ctx.status_code == 503
        || ctx.status_code == 500
    {
        token_manager
            .mark_rate_limited_async(
                ctx.email,
                ctx.status_code,
                ctx.retry_after.as_deref(),
                &ctx.error_text,
                Some(ctx.model),
            )
            .await;

        if ctx.status_code == 429 {
            token_manager.record_session_failure(ctx.session_id_str);
            state.adaptive_limits.record_429(ctx.email);
        } else {
            state.adaptive_limits.record_error(ctx.email, ctx.status_code);
        }
    }

    if ctx.status_code == 400 && !*retried_without_thinking && is_signature_error(&ctx.error_text) {
        handle_thinking_signature_error(request_for_body, Some(ctx.session_id_str), ctx.trace_id);
        *retried_without_thinking = true;

        if apply_retry_strategy(
            RetryStrategy::FixedDelay(Duration::from_millis(100)),
            ctx.attempt,
            ctx.status_code,
            ctx.trace_id,
        )
        .await
        {
            return ErrorAction::Retry;
        }
    }

    let strategy =
        determine_retry_strategy(ctx.status_code, &ctx.error_text, *retried_without_thinking);

    if apply_retry_strategy(strategy, ctx.attempt, ctx.status_code, ctx.trace_id).await {
        if should_rotate_account(ctx.status_code) {
            attempted_accounts.insert(ctx.email.to_string());
        }
        *grace_retry_used = false;
        return ErrorAction::Retry;
    }

    if ctx.status_code == 400
        && (ctx.error_text.contains("too long")
            || ctx.error_text.contains("exceeds")
            || ctx.error_text.contains("limit"))
    {
        return ErrorAction::Return(prompt_too_long_error(ctx.email));
    }

    tracing::error!(
        "[{}] Non-retryable error {}: {}",
        ctx.trace_id,
        ctx.status_code,
        ctx.error_text
    );
    ErrorAction::Return(
        (ctx.status, [("X-Account-Email", ctx.email)], ctx.error_text.clone()).into_response(),
    )
}
