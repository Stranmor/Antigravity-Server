//! Token selection and account acquisition logic

use crate::proxy::active_request_guard::ActiveRequestGuard;
use crate::proxy::token_manager::TokenManager;
use axum::response::Response;
use std::collections::HashSet;
use std::sync::Arc;

use super::request_validation::no_accounts_error;

pub struct TokenAcquisitionResult {
    pub access_token: String,
    pub project_id: String,
    pub email: String,
    pub guard: ActiveRequestGuard,
}

pub async fn acquire_token(
    token_manager: Arc<TokenManager>,
    force_account: Option<&str>,
    request_type: &str,
    final_model: &str,
    session_id: Option<&str>,
    force_rotate: bool,
    attempted_accounts: &HashSet<String>,
) -> Result<TokenAcquisitionResult, Response> {
    let exclusions = if attempted_accounts.is_empty() { None } else { Some(attempted_accounts) };

    if let Some(forced) = force_account {
        match token_manager.get_token_forced(forced, final_model).await {
            Ok((token, project, email, guard)) => {
                return Ok(TokenAcquisitionResult {
                    access_token: token,
                    project_id: project,
                    email,
                    guard,
                });
            },
            Err(e) => {
                tracing::warn!(
                    "[Claude] Forced account {} failed: {}, using smart routing",
                    forced,
                    e
                );
            },
        }
    }

    match token_manager
        .get_token_with_exclusions(request_type, force_rotate, session_id, final_model, exclusions)
        .await
    {
        Ok((token, project, email, guard)) => {
            Ok(TokenAcquisitionResult { access_token: token, project_id: project, email, guard })
        },
        Err(e) => Err(no_accounts_error(e)),
    }
}
