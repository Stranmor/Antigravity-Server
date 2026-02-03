//! Account management handlers: list, get, switch, delete, add

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use antigravity_core::modules::{account, oauth as core_oauth};
use antigravity_types::models::TokenData;

use crate::state::{get_model_quota, AppState};

#[derive(Serialize)]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub disabled: bool,
    pub proxy_disabled: bool,
    pub is_current: bool,
    pub gemini_quota: Option<i32>,
    pub claude_quota: Option<i32>,
    pub subscription_tier: Option<String>,
    pub quota: Option<antigravity_types::models::QuotaData>,
}

pub async fn list_accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<AccountInfo>>, (StatusCode, String)> {
    let current_id = state.get_current_account().ok().flatten().map(|a| a.id);

    let accounts = state
        .list_accounts()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let infos: Vec<AccountInfo> = accounts
        .into_iter()
        .map(|a| AccountInfo {
            id: a.id.clone(),
            email: a.email.clone(),
            name: a.name.clone(),
            disabled: a.disabled,
            proxy_disabled: a.proxy_disabled,
            is_current: current_id.as_ref() == Some(&a.id),
            gemini_quota: get_model_quota(&a, "gemini"),
            claude_quota: get_model_quota(&a, "claude"),
            subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
            quota: a.quota.clone(),
        })
        .collect();
    Ok(Json(infos))
}

pub async fn get_current_account(
    State(state): State<AppState>,
) -> Result<Json<Option<AccountInfo>>, (StatusCode, String)> {
    match state.get_current_account() {
        Ok(Some(a)) => Ok(Json(Some(AccountInfo {
            id: a.id.clone(),
            email: a.email.clone(),
            name: a.name.clone(),
            disabled: a.disabled,
            proxy_disabled: a.proxy_disabled,
            is_current: true,
            gemini_quota: get_model_quota(&a, "gemini"),
            claude_quota: get_model_quota(&a, "claude"),
            subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
            quota: a.quota.clone(),
        }))),
        Ok(None) => Ok(Json(None)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[derive(Deserialize)]
pub struct SwitchAccountRequest {
    pub account_id: String,
}

pub async fn switch_account(
    State(state): State<AppState>,
    Json(payload): Json<SwitchAccountRequest>,
) -> Result<Json<bool>, (StatusCode, String)> {
    match state.switch_account(&payload.account_id).await {
        Ok(()) => Ok(Json(true)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[derive(Deserialize)]
pub struct DeleteAccountRequest {
    pub account_id: String,
}

pub async fn delete_account_handler(
    State(state): State<AppState>,
    Json(payload): Json<DeleteAccountRequest>,
) -> Result<Json<bool>, (StatusCode, String)> {
    if let Some(repo) = state.repository() {
        repo.delete_account(&payload.account_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        account::delete_account(&payload.account_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }
    let _ = state.reload_accounts().await;
    Ok(Json(true))
}

#[derive(Deserialize)]
pub struct DeleteAccountsRequest {
    pub account_ids: Vec<String>,
}

pub async fn delete_accounts_handler(
    State(state): State<AppState>,
    Json(payload): Json<DeleteAccountsRequest>,
) -> Result<Json<bool>, (StatusCode, String)> {
    if let Some(repo) = state.repository() {
        repo.delete_accounts(&payload.account_ids)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        account::delete_accounts(&payload.account_ids)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }
    let _ = state.reload_accounts().await;
    Ok(Json(true))
}

#[derive(Deserialize)]
pub struct AddByTokenRequest {
    pub refresh_tokens: Vec<String>,
}

#[derive(Serialize)]
pub struct AddByTokenResponse {
    pub success_count: usize,
    pub fail_count: usize,
    pub accounts: Vec<AccountInfo>,
}

pub async fn add_account_by_token(
    State(state): State<AppState>,
    Json(payload): Json<AddByTokenRequest>,
) -> Result<Json<AddByTokenResponse>, (StatusCode, String)> {
    struct TokenResult {
        email: String,
        name: Option<String>,
        token_data: TokenData,
    }

    let mut join_set: JoinSet<Result<TokenResult, String>> = JoinSet::new();

    for token in payload.refresh_tokens {
        join_set.spawn(async move {
            let token_response = core_oauth::refresh_access_token(&token)
                .await
                .map_err(|e| format!("Token refresh failed: {}", e))?;

            let user_info = core_oauth::get_user_info(&token_response.access_token)
                .await
                .map_err(|e| format!("User info failed: {}", e))?;

            let refresh_token = token_response
                .refresh_token
                .clone()
                .unwrap_or_else(|| token.clone());

            let token_data = TokenData::new(
                token_response.access_token,
                refresh_token,
                token_response.expires_in,
                Some(user_info.email.clone()),
                None,
                None,
            );

            Ok(TokenResult {
                name: user_info.get_display_name(),
                email: user_info.email,
                token_data,
            })
        });
    }

    let mut success_count = 0;
    let mut fail_count = 0;
    let mut added_accounts = Vec::new();
    let mut errors = Vec::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(token_result)) => {
                let upsert_result = if let Some(repo) = state.repository() {
                    repo.upsert_account(
                        token_result.email.clone(),
                        token_result.name,
                        token_result.token_data,
                    )
                    .await
                    .map_err(|e| e.to_string())
                } else {
                    account::upsert_account(
                        token_result.email.clone(),
                        token_result.name,
                        token_result.token_data,
                    )
                };

                match upsert_result {
                    Ok(acc) => {
                        success_count += 1;
                        added_accounts.push(AccountInfo {
                            id: acc.id.clone(),
                            email: acc.email.clone(),
                            name: acc.name.clone(),
                            disabled: acc.disabled,
                            proxy_disabled: acc.proxy_disabled,
                            is_current: false,
                            gemini_quota: get_model_quota(&acc, "gemini"),
                            claude_quota: get_model_quota(&acc, "claude"),
                            subscription_tier: acc
                                .quota
                                .as_ref()
                                .and_then(|q| q.subscription_tier.clone()),
                            quota: acc.quota.clone(),
                        });
                    }
                    Err(e) => {
                        fail_count += 1;
                        errors.push(format!("{}: {}", token_result.email, e));
                        tracing::warn!("Failed to upsert account: {}", errors.last().unwrap());
                    }
                }
            }
            Ok(Err(e)) => {
                fail_count += 1;
                errors.push(e);
                tracing::warn!("Failed to add account: {}", errors.last().unwrap());
            }
            Err(e) => {
                fail_count += 1;
                tracing::error!("Task panicked: {}", e);
            }
        }
    }

    let _ = state.reload_accounts().await;

    Ok(Json(AddByTokenResponse {
        success_count,
        fail_count,
        accounts: added_accounts,
    }))
}
