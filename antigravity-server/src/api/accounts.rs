//! Account management handlers: list, get, switch, delete, add

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use antigravity_core::modules::{account, oauth as core_oauth};
use antigravity_types::models::TokenData;

use crate::state::AppState;

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
    let current_id = state.get_current_account().await.ok().flatten().map(|a| a.id);

    let accounts =
        state.list_accounts().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let infos: Vec<AccountInfo> = accounts
        .into_iter()
        .map(|a| AccountInfo {
            id: a.id.clone(),
            email: a.email.clone(),
            name: a.name.clone(),
            disabled: a.disabled,
            proxy_disabled: a.proxy_disabled,
            is_current: current_id.as_ref() == Some(&a.id),
            gemini_quota: a
                .quota
                .as_ref()
                .and_then(|q| q.get_model_quota("gemini"))
                .map(|m| m.percentage),
            claude_quota: a
                .quota
                .as_ref()
                .and_then(|q| q.get_model_quota("claude"))
                .map(|m| m.percentage),
            subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
            quota: a.quota.clone(),
        })
        .collect();
    Ok(Json(infos))
}

pub async fn get_current_account(
    State(state): State<AppState>,
) -> Result<Json<Option<AccountInfo>>, (StatusCode, String)> {
    match state.get_current_account().await {
        Ok(Some(a)) => Ok(Json(Some(AccountInfo {
            id: a.id.clone(),
            email: a.email.clone(),
            name: a.name.clone(),
            disabled: a.disabled,
            proxy_disabled: a.proxy_disabled,
            is_current: true,
            gemini_quota: a
                .quota
                .as_ref()
                .and_then(|q| q.get_model_quota("gemini"))
                .map(|m| m.percentage),
            claude_quota: a
                .quota
                .as_ref()
                .and_then(|q| q.get_model_quota("claude"))
                .map(|m| m.percentage),
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
        let account_id = payload.account_id.clone();
        tokio::task::spawn_blocking(move || account::delete_account(&account_id))
            .await
            .map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))
            })?
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }
    drop(state.reload_accounts().await);
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
        let account_ids = payload.account_ids.clone();
        tokio::task::spawn_blocking(move || account::delete_accounts(&account_ids))
            .await
            .map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))
            })?
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }
    drop(state.reload_accounts().await);
    Ok(Json(true))
}

#[derive(Deserialize)]
pub struct AddByTokenRequest {
    pub refresh_tokens: Vec<String>,
    /// Optional proxy URL to assign to these accounts.
    /// When set, ALL requests for these accounts (including this initial setup)
    /// will be routed through this proxy. One proxy = one account.
    pub proxy_url: Option<String>,
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

    // Resolve proxy_url: explicit from request > auto-assigned from pool > None
    let proxy_url = if payload.proxy_url.is_some() {
        payload.proxy_url.clone()
    } else {
        let pool = &state.inner.proxy_config.read().await.account_proxy_pool;
        antigravity_core::modules::proxy_pool::assign_proxy_from_pool(pool, None)
    };

    if proxy_url.is_some() {
        tracing::info!("Using per-account proxy for new accounts: {:?}", proxy_url);
    }

    for token in payload.refresh_tokens {
        let proxy_url = proxy_url.clone();
        join_set.spawn(async move {
            // Use per-account proxy from the very first request
            let token_response =
                core_oauth::refresh_access_token_with_proxy(&token, proxy_url.as_deref())
                    .await
                    .map_err(|e| format!("Token refresh failed: {}", e))?;

            let user_info = core_oauth::get_user_info_with_proxy(
                &token_response.access_token,
                proxy_url.as_deref(),
            )
            .await
            .map_err(|e| format!("User info failed: {}", e))?;

            let refresh_token =
                token_response.refresh_token.clone().unwrap_or_else(|| token.clone());

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

    let mut success_count = 0_usize;
    let mut fail_count = 0_usize;
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
                    let email = token_result.email.clone();
                    let name = token_result.name;
                    let token_data = token_result.token_data;
                    match tokio::task::spawn_blocking(move || {
                        account::upsert_account(email, name, token_data)
                    })
                    .await
                    {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(e) => Err(format!("spawn_blocking panicked: {e}")),
                    }
                };

                match upsert_result {
                    Ok(mut acc) => {
                        // Immediately set proxy_url if provided — before reload_accounts
                        // so the first token manager operation uses the proxy
                        if let Some(ref purl) = proxy_url {
                            acc.proxy_url = Some(purl.clone());
                            if let Some(repo) = state.repository() {
                                if let Err(e) =
                                    repo.update_proxy_url(&acc.id, Some(purl.as_str())).await
                                {
                                    tracing::warn!(
                                        "Failed to persist proxy_url to DB for {}: {}",
                                        acc.email,
                                        e
                                    );
                                }
                            }
                            // Also persist to JSON file so both paths are covered
                            let acc_clone = acc.clone();
                            if let Err(e) = tokio::task::spawn_blocking(move || {
                                account::save_account(&acc_clone)
                            })
                            .await
                            .unwrap_or_else(|e| Err(format!("spawn_blocking panicked: {e}")))
                            {
                                tracing::warn!(
                                    "Failed to persist proxy_url to JSON for {}: {}",
                                    acc.email,
                                    e
                                );
                            }
                        }

                        success_count = success_count.saturating_add(1);
                        added_accounts.push(AccountInfo {
                            id: acc.id.clone(),
                            email: acc.email.clone(),
                            name: acc.name.clone(),
                            disabled: acc.disabled,
                            proxy_disabled: acc.proxy_disabled,
                            is_current: false,
                            gemini_quota: acc
                                .quota
                                .as_ref()
                                .and_then(|q| q.get_model_quota("gemini"))
                                .map(|m| m.percentage),
                            claude_quota: acc
                                .quota
                                .as_ref()
                                .and_then(|q| q.get_model_quota("claude"))
                                .map(|m| m.percentage),
                            subscription_tier: acc
                                .quota
                                .as_ref()
                                .and_then(|q| q.subscription_tier.clone()),
                            quota: acc.quota.clone(),
                        });
                    },
                    Err(e) => {
                        fail_count = fail_count.saturating_add(1);
                        errors.push(format!("{}: {}", token_result.email, e));
                        if let Some(last_err) = errors.last() {
                            tracing::warn!("Failed to upsert account: {last_err}");
                        }
                    },
                }
            },
            Ok(Err(e)) => {
                fail_count = fail_count.saturating_add(1);
                errors.push(e);
                if let Some(last_err) = errors.last() {
                    tracing::warn!("Failed to add account: {last_err}");
                }
            },
            Err(e) => {
                fail_count = fail_count.saturating_add(1);
                tracing::error!("Task panicked: {e}");
            },
        }
    }

    drop(state.reload_accounts().await);

    Ok(Json(AddByTokenResponse { success_count, fail_count, accounts: added_accounts }))
}
