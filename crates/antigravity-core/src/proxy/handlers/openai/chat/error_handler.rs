use crate::proxy::common::sanitize_upstream_error;
use crate::proxy::rate_limit::RateLimitReason;
use crate::proxy::server::AppState;
use crate::proxy::token_manager::TokenManager;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{error, info, warn};

pub enum OpenAIErrorAction {
    Continue,
    ReturnError(axum::http::StatusCode, String, String),
}

pub async fn handle_grace_retry(
    status_code: u16,
    error_text: &str,
    grace_retry_used: bool,
    token_manager: Arc<TokenManager>,
    email: &str,
    trace_id: &str,
) -> Option<bool> {
    if status_code == 429 && !grace_retry_used {
        let reason = token_manager.rate_limit_tracker().parse_rate_limit_reason(error_text);
        if reason == RateLimitReason::RateLimitExceeded {
            info!(
                "[{}] 🔄 Grace retry: RATE_LIMIT_EXCEEDED on {}, waiting 1s before retry on same account",
                trace_id, email
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
            return Some(true);
        }
    }
    None
}

pub async fn handle_service_disabled(
    status_code: u16,
    error_text: &str,
    token_manager: Arc<TokenManager>,
    email: &str,
) -> bool {
    if status_code != 403 {
        return false;
    }

    use crate::proxy::common::tos_ban::{classify_403, ForbiddenReason, TOS_BAN_LOCKOUT_SECS};
    match classify_403(error_text) {
        ForbiddenReason::TosBanned => {
            tracing::error!("[OpenAI] 🚫 Account {} TOS-BANNED! 24h lockout.", email);
            token_manager.rate_limit_tracker().set_lockout_until(
                email,
                SystemTime::now() + Duration::from_secs(TOS_BAN_LOCKOUT_SECS),
                RateLimitReason::ServerError,
                None,
            );
            let email_clone = email.to_string();
            tokio::spawn(async move {
                let _ =
                    crate::modules::account::mark_needs_verification_by_email(&email_clone).await;
            });
            true
        },
        ForbiddenReason::NeedsVerification => {
            warn!(
                "[OpenAI] 🚫 Account {} needs verification or has project issue. 1h lockout.",
                email
            );
            token_manager.rate_limit_tracker().set_lockout_until(
                email,
                SystemTime::now() + Duration::from_secs(3600),
                RateLimitReason::ServerError,
                None,
            );
            let email_clone = email.to_string();
            tokio::spawn(async move {
                let _ =
                    crate::modules::account::mark_needs_verification_by_email(&email_clone).await;
            });
            true
        },
        ForbiddenReason::Other => false,
    }
}

pub async fn handle_rate_limit_errors(
    status_code: u16,
    error_text: &str,
    retry_after: Option<&str>,
    token_manager: Arc<TokenManager>,
    state: &AppState,
    email: &str,
    session_id: &str,
    final_model: &str,
    attempt: usize,
    max_attempts: usize,
) -> OpenAIErrorAction {
    if crate::proxy::retry::is_rate_limit_code(status_code) {
        token_manager
            .mark_rate_limited_async(email, status_code, retry_after, error_text, Some(final_model))
            .await;

        if status_code == 429 {
            token_manager.record_session_failure(session_id);
            state.adaptive_limits.record_429(email);
        } else {
            state.adaptive_limits.record_error(email, status_code);
        }

        if let Some(delay_ms) = crate::proxy::upstream::retry::parse_retry_delay(error_text) {
            let actual_delay = delay_ms.saturating_add(200).min(10_000);
            warn!(
                "OpenAI Upstream {} on {} attempt {}/{}, waiting {}ms then rotating",
                status_code,
                email,
                attempt + 1,
                max_attempts,
                actual_delay
            );
            tokio::time::sleep(Duration::from_millis(actual_delay)).await;
            return OpenAIErrorAction::Continue;
        }

        if error_text.contains("QUOTA_EXHAUSTED") {
            error!(
                "OpenAI Quota exhausted (429) on account {} attempt {}/{}, stopping to protect pool.",
                email,
                attempt + 1,
                max_attempts
            );
            return OpenAIErrorAction::ReturnError(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                email.to_string(),
                sanitize_upstream_error(status_code, error_text),
            );
        }

        warn!(
            "OpenAI Upstream {} on {} attempt {}/{}, rotating account",
            status_code,
            email,
            attempt + 1,
            max_attempts
        );
        return OpenAIErrorAction::Continue;
    }
    OpenAIErrorAction::Continue
}

pub fn handle_auth_errors(
    status_code: u16,
    token_manager: Arc<TokenManager>,
    email: &str,
    final_model: &str,
    attempt: usize,
    max_attempts: usize,
) -> bool {
    if status_code == 403 || status_code == 401 {
        token_manager.rate_limit_tracker().set_model_lockout(
            email,
            final_model,
            SystemTime::now() + Duration::from_secs(30),
            RateLimitReason::ServerError,
        );
        warn!(
            "OpenAI Upstream {} on account {} attempt {}/{}, locking for 30s and rotating",
            status_code,
            email,
            attempt + 1,
            max_attempts
        );
        return true;
    }
    false
}
