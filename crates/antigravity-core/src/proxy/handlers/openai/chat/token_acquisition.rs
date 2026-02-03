use crate::proxy::mappers::request_config::RequestConfig;
use crate::proxy::token_manager::{ActiveRequestGuard, TokenManager};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;

pub struct TokenAcquisitionResult {
    pub access_token: String,
    pub project_id: String,
    pub email: String,
    pub guard: ActiveRequestGuard,
}

pub enum TokenAcquisitionError {
    Unavailable(String),
}

pub async fn acquire_token(
    token_manager: Arc<TokenManager>,
    force_account: Option<&str>,
    config: &RequestConfig,
    session_id: &str,
    attempt: usize,
    attempted_accounts: &HashSet<String>,
) -> Result<TokenAcquisitionResult, TokenAcquisitionError> {
    let exclusions = if attempted_accounts.is_empty() {
        None
    } else {
        Some(attempted_accounts)
    };

    if let Some(forced) = force_account {
        match token_manager
            .get_token_forced(forced, &config.final_model)
            .await
        {
            Ok((token, email, project, guard)) => {
                return Ok(TokenAcquisitionResult {
                    access_token: token,
                    project_id: project,
                    email,
                    guard,
                });
            }
            Err(e) => {
                warn!(
                    "[OpenAI] Forced account {} failed: {}, using smart routing",
                    forced, e
                );
            }
        }
    }

    match token_manager
        .get_token_with_exclusions(
            &config.request_type,
            attempt > 0,
            Some(session_id),
            &config.final_model,
            exclusions,
        )
        .await
    {
        Ok((token, email, project, guard)) => Ok(TokenAcquisitionResult {
            access_token: token,
            project_id: project,
            email,
            guard,
        }),
        Err(e) => Err(TokenAcquisitionError::Unavailable(format!(
            "Token error: {}",
            e
        ))),
    }
}
