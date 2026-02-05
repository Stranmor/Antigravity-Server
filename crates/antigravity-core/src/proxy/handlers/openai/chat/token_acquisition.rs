// Token acquisition logic for chat handler

use crate::proxy::active_request_guard::ActiveRequestGuard;
use crate::proxy::mappers::request_config::RequestConfig;
use crate::proxy::token_manager::TokenManager;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;

/// Result of token acquisition: (access_token, project_id, email, guard)
pub type TokenResult = (String, String, String, ActiveRequestGuard);

/// Acquire token with optional forced account and exclusions.
pub async fn acquire_token(
    token_manager: Arc<TokenManager>,
    force_account: Option<&str>,
    config: &RequestConfig,
    session_id: &str,
    is_retry: bool,
    attempted_accounts: &HashSet<String>,
) -> Result<TokenResult, String> {
    if let Some(forced) = force_account {
        match token_manager.get_token_forced(forced, &config.final_model).await {
            Ok((token, project, email, guard)) => {
                return Ok((token, project, email, guard));
            },
            Err(e) => {
                warn!("[OpenAI] Forced account {} failed: {}, using smart routing", forced, e);
            },
        }
    }

    let exclusions = if attempted_accounts.is_empty() { None } else { Some(attempted_accounts) };

    token_manager
        .get_token_with_exclusions(
            &config.request_type,
            is_retry,
            Some(session_id),
            &config.final_model,
            exclusions,
        )
        .await
        .map_err(|e| format!("Token error: {}", e))
}
