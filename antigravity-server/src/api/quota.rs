//! Quota and warmup handlers

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use antigravity_core::modules::account;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct RefreshQuotaRequest {
    pub account_id: String,
}

#[derive(Serialize)]
pub struct QuotaResponse {
    pub account_id: String,
    pub quota: Option<antigravity_types::models::QuotaData>,
}

pub async fn refresh_account_quota(
    State(state): State<AppState>,
    Json(payload): Json<RefreshQuotaRequest>,
) -> Result<Json<QuotaResponse>, (StatusCode, String)> {
    let acc = account::load_account(&payload.account_id).map_err(|e| (StatusCode::NOT_FOUND, e))?;

    match account::fetch_quota_with_retry(&acc, state.repository()).await {
        Ok(result) => {
            let quota = result.quota;
            let updated_account = match account::update_account_quota_async(
                payload.account_id.clone(),
                quota.clone(),
            )
            .await
            {
                Ok(updated) => Some(updated),
                Err(e) => {
                    tracing::warn!("Failed to update quota protection: {}", e);
                    None
                },
            };
            let protected_models =
                updated_account.map(|updated| updated.protected_models.iter().cloned().collect());
            if let Some(repo) = state.repository() {
                match repo.get_account_by_email(&acc.email).await {
                    Ok(Some(pg_account)) => {
                        if let Err(e) =
                            repo.update_quota(&pg_account.id, quota.clone(), protected_models).await
                        {
                            tracing::warn!("DB quota update failed for {}: {}", acc.email, e);
                        }
                    },
                    Ok(None) => {
                        tracing::warn!("PG account lookup failed for {}", acc.email);
                    },
                    Err(e) => {
                        tracing::warn!("PG account lookup error for {}: {}", acc.email, e);
                    },
                }
            }
            if let Err(e) = state.reload_accounts().await {
                tracing::warn!("Failed to reload accounts: {}", e);
            }

            Ok(Json(QuotaResponse { account_id: payload.account_id, quota: Some(quota) }))
        },
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn refresh_all_quotas(
    State(state): State<AppState>,
) -> Result<Json<antigravity_types::models::RefreshStats>, (StatusCode, String)> {
    let accounts = account::list_accounts().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let total = accounts.len();
    let mut join_set: JoinSet<
        Result<(String, String, antigravity_types::models::QuotaData), String>,
    > = JoinSet::new();

    let semaphore = Arc::new(Semaphore::new(10));
    let repo = state.repository().cloned();

    for acc in accounts {
        if acc.disabled {
            continue;
        }

        let account_id = acc.id.clone();
        let email = acc.email.clone();
        let permit =
            semaphore.clone().acquire_owned().await.map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Semaphore error: {}", e))
            })?;
        let repo_clone = repo.clone();

        join_set.spawn(async move {
            let _permit = permit;
            let result = account::fetch_quota_with_retry(&acc, repo_clone.as_ref())
                .await
                .map_err(|e| format!("{}: {}", email, e))?;
            Ok((account_id, email, result.quota))
        });
    }

    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok((account_id, email, quota))) => {
                let protected_models =
                    match account::update_account_quota_async(account_id.clone(), quota.clone())
                        .await
                    {
                        Ok(updated) => Some(updated.protected_models.iter().cloned().collect()),
                        Err(e) => {
                            tracing::warn!(
                                "Quota protection update failed for {}: {}",
                                account_id,
                                e
                            );
                            None
                        },
                    };
                if let Some(repo) = state.repository() {
                    match repo.get_account_by_email(&email).await {
                        Ok(Some(pg_account)) => {
                            if let Err(e) =
                                repo.update_quota(&pg_account.id, quota, protected_models).await
                            {
                                tracing::warn!("DB quota update failed for {}: {}", email, e);
                            }
                        },
                        Ok(None) => {
                            tracing::warn!("PG account lookup failed for {}", email);
                        },
                        Err(e) => {
                            tracing::warn!("PG account lookup error for {}: {}", email, e);
                        },
                    }
                }
                success += 1;
            },
            Ok(Err(e)) => {
                tracing::warn!("Quota refresh failed: {}", e);
                failed += 1;
            },
            Err(e) => {
                tracing::error!("Task panicked: {}", e);
                failed += 1;
            },
        }
    }

    if let Err(e) = state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after quota refresh: {}", e);
    }

    Ok(Json(antigravity_types::models::RefreshStats { total, success, failed }))
}

#[derive(Deserialize)]
pub struct ToggleProxyRequest {
    pub account_id: String,
    pub enable: bool,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct ToggleProxyResponse {
    pub success: bool,
    pub account_id: String,
    pub proxy_disabled: bool,
}

pub async fn toggle_proxy_status(
    State(state): State<AppState>,
    Json(payload): Json<ToggleProxyRequest>,
) -> Result<Json<ToggleProxyResponse>, (StatusCode, String)> {
    tracing::info!(
        account_id = %payload.account_id,
        enable = %payload.enable,
        reason = ?payload.reason,
        "Toggling proxy status"
    );

    let mut acc =
        account::load_account(&payload.account_id).map_err(|e| (StatusCode::NOT_FOUND, e))?;

    acc.proxy_disabled = !payload.enable;

    if let Some(repo) = state.repository() {
        match repo.get_account_by_email(&acc.email).await {
            Ok(Some(mut pg_account)) => {
                pg_account.proxy_disabled = acc.proxy_disabled;
                repo.update_account(&pg_account)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            },
            Ok(None) => {
                return Err((StatusCode::NOT_FOUND, format!("Account not found: {}", acc.email)));
            },
            Err(e) => {
                return Err((StatusCode::NOT_FOUND, e.to_string()));
            },
        }
    }

    account::save_account(&acc).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Err(e) = state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after proxy toggle: {}", e);
    }

    Ok(Json(ToggleProxyResponse {
        success: true,
        account_id: payload.account_id,
        proxy_disabled: acc.proxy_disabled,
    }))
}

#[derive(Deserialize)]
pub struct WarmupAccountRequest {
    pub account_id: String,
}

#[derive(Serialize)]
pub struct WarmupResponse {
    pub success: bool,
    pub message: String,
}

pub async fn warmup_account(
    State(state): State<AppState>,
    Json(payload): Json<WarmupAccountRequest>,
) -> Result<Json<WarmupResponse>, (StatusCode, String)> {
    let acc = account::load_account(&payload.account_id).map_err(|e| (StatusCode::NOT_FOUND, e))?;

    match account::fetch_quota_with_retry(&acc, state.repository()).await {
        Ok(result) => {
            let quota = result.quota;
            let protected_models =
                match account::update_account_quota_async(acc.id.clone(), quota.clone()).await {
                    Ok(updated) => Some(updated.protected_models.iter().cloned().collect()),
                    Err(e) => {
                        tracing::warn!("Failed to update quota for {}: {}", acc.email, e);
                        None
                    },
                };
            if let Some(repo) = state.repository() {
                match repo.get_account_by_email(&acc.email).await {
                    Ok(Some(pg_account)) => {
                        if let Err(e) =
                            repo.update_quota(&pg_account.id, quota, protected_models).await
                        {
                            tracing::warn!("DB quota update failed for {}: {}", acc.email, e);
                        }
                    },
                    Ok(None) => {
                        tracing::warn!("PG account lookup failed for {}", acc.email);
                    },
                    Err(e) => {
                        tracing::warn!("PG account lookup error for {}: {}", acc.email, e);
                    },
                }
            }
            if let Err(e) = state.reload_accounts().await {
                tracing::warn!("Failed to reload accounts after warmup: {}", e);
            }

            Ok(Json(WarmupResponse {
                success: true,
                message: format!("Account {} warmed up successfully", acc.email),
            }))
        },
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn warmup_all_accounts(
    State(state): State<AppState>,
) -> Result<Json<WarmupResponse>, (StatusCode, String)> {
    let accounts = account::list_accounts().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let total = accounts.len();

    struct WarmupResult {
        account_id: String,
        email: String,
        quota: Option<antigravity_types::models::QuotaData>,
    }

    let mut join_set: JoinSet<Result<WarmupResult, String>> = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(10));
    let repo = state.repository().cloned();

    for acc in accounts {
        if acc.disabled || acc.proxy_disabled {
            continue;
        }

        let account_id = acc.id.clone();
        let email = acc.email.clone();
        let permit =
            semaphore.clone().acquire_owned().await.map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Semaphore error: {}", e))
            })?;
        let repo_clone = repo.clone();

        join_set.spawn(async move {
            let _permit = permit;
            let result = account::fetch_quota_with_retry(&acc, repo_clone.as_ref())
                .await
                .map_err(|e| format!("{}: {}", email, e))?;

            Ok(WarmupResult { account_id, email, quota: Some(result.quota) })
        });
    }

    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(warmup_result)) => {
                if let Some(quota) = warmup_result.quota {
                    let protected_models = match account::update_account_quota_async(
                        warmup_result.account_id.clone(),
                        quota.clone(),
                    )
                    .await
                    {
                        Ok(updated) => Some(updated.protected_models.iter().cloned().collect()),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to update quota for {}: {}",
                                warmup_result.account_id,
                                e
                            );
                            None
                        },
                    };
                    if let Some(repo) = state.repository() {
                        match repo.get_account_by_email(&warmup_result.email).await {
                            Ok(Some(pg_account)) => {
                                if let Err(e) =
                                    repo.update_quota(&pg_account.id, quota, protected_models).await
                                {
                                    tracing::warn!(
                                        "DB quota update failed for {}: {}",
                                        warmup_result.email,
                                        e
                                    );
                                }
                            },
                            Ok(None) => {
                                tracing::warn!(
                                    "PG account lookup failed for {}",
                                    warmup_result.email
                                );
                            },
                            Err(e) => {
                                tracing::warn!(
                                    "PG account lookup error for {}: {}",
                                    warmup_result.email,
                                    e
                                );
                            },
                        }
                    }
                }
                success += 1;
            },
            Ok(Err(e)) => {
                tracing::warn!("Warmup failed: {}", e);
                failed += 1;
            },
            Err(e) => {
                tracing::error!("Task panicked: {}", e);
                failed += 1;
            },
        }
    }

    if let Err(e) = state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after warmup all: {}", e);
    }

    Ok(Json(WarmupResponse {
        success: true,
        message: format!(
            "Warmup complete: {}/{} accounts warmed up ({} failed)",
            success, total, failed
        ),
    }))
}
