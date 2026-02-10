//! Quota fetching with retry logic.
//!
//! Uses atomic per-field persistence via `AccountRepository` when available,
//! falling back to whole-struct JSON writes when no repo is configured.

use std::sync::Arc;

use reqwest::StatusCode;

use crate::error::{AppError, AppResult};
use crate::models::{Account, QuotaData, TokenData};
use crate::modules::logger;
use crate::modules::repository::AccountRepository;

use super::async_wrappers::save_account_async;
use super::crud::upsert_account;

/// Result of a quota fetch — contains fetched data without mutating the source account.
#[derive(Debug, Clone)]
pub struct QuotaFetchResult {
    /// Fetched quota data.
    pub quota: QuotaData,
    /// If token was refreshed during fetch, contains the new token.
    pub refreshed_token: Option<TokenData>,
    /// If project_id was discovered/changed.
    pub project_id: Option<String>,
    /// If account name was fetched.
    pub name: Option<String>,
    /// Whether account was disabled during fetch (e.g., invalid_grant).
    pub disabled: bool,
    /// Reason for disabling, if applicable.
    pub disabled_reason: Option<String>,
}

pub async fn upsert_account_async(
    email: String,
    name: Option<String>,
    token: TokenData,
) -> Result<Account, String> {
    tokio::task::spawn_blocking(move || upsert_account(email, name, token))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

/// Fetch quota for an account, retrying on 401 with token refresh.
///
/// Takes an immutable reference — never mutates the passed-in account.
/// Persists changes atomically via repo when available, JSON fallback otherwise.
pub async fn fetch_quota_with_retry(
    account: &Account,
    repo: Option<&Arc<dyn AccountRepository>>,
) -> AppResult<QuotaFetchResult> {
    use crate::modules::{oauth, quota};

    let token = match oauth::ensure_fresh_token(&account.token).await {
        Ok(t) => t,
        Err(e) => {
            if e.contains("invalid_grant") {
                logger::log_error(&format!(
                    "Disabling account {} due to invalid_grant during token refresh (quota check)",
                    account.email
                ));
                persist_disabled(repo, account, &format!("invalid_grant: {}", e)).await;
            }
            return Err(AppError::OAuth(e));
        },
    };

    // Track whether token was refreshed
    let refreshed_token = if token.access_token != account.token.access_token {
        logger::log_info(&format!("Time-based token refresh: {}", account.email));
        persist_token_refresh(repo, account, &token).await;
        Some(token.clone())
    } else {
        None
    };

    // Fetch name if missing
    let active_token = refreshed_token.as_ref().unwrap_or(&account.token);
    let name =
        if account.name.is_none() || account.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
            logger::log_info(&format!("Account {} missing name, fetching...", account.email));
            let fetched =
                if let Ok(user_info) = oauth::get_user_info(&active_token.access_token).await {
                    user_info.get_display_name()
                } else {
                    None
                };
            if fetched.is_some() {
                persist_name(repo, account, fetched.as_deref(), refreshed_token.as_ref()).await;
            }
            fetched
        } else {
            None
        };

    let result = quota::fetch_quota(&active_token.access_token, &account.email).await;

    if let Ok((ref quota_data, ref project_id)) = result {
        persist_quota_data(
            repo,
            account,
            quota_data,
            project_id.as_deref(),
            refreshed_token.as_ref(),
        )
        .await;

        return Ok(QuotaFetchResult {
            quota: quota_data.clone(),
            refreshed_token,
            project_id: project_id.clone(),
            name,
            disabled: false,
            disabled_reason: None,
        });
    }

    if let Err(AppError::Network(ref e)) = result {
        if let Some(status) = e.status() {
            if status == StatusCode::UNAUTHORIZED {
                return handle_unauthorized_retry(account, repo).await;
            }
        }
    }

    result.map(|(q, pid)| QuotaFetchResult {
        quota: q,
        refreshed_token,
        project_id: pid,
        name,
        disabled: false,
        disabled_reason: None,
    })
}

async fn handle_unauthorized_retry(
    account: &Account,
    repo: Option<&Arc<dyn AccountRepository>>,
) -> AppResult<QuotaFetchResult> {
    use crate::modules::{oauth, quota};

    logger::log_warn(&format!("401 Unauthorized for {}, forcing refresh...", account.email));

    let token_res = match oauth::refresh_access_token(&account.token.refresh_token).await {
        Ok(t) => t,
        Err(e) => {
            if e.contains("invalid_grant") {
                logger::log_error(&format!(
                    "Disabling account {} due to invalid_grant during forced refresh",
                    account.email
                ));
                persist_disabled(repo, account, &format!("invalid_grant: {}", e)).await;
            }
            return Err(AppError::OAuth(e));
        },
    };

    let new_token = TokenData::new(
        token_res.access_token.clone(),
        account.token.refresh_token.clone(),
        token_res.expires_in,
        account.token.email.clone(),
        account.token.project_id.clone(),
        None,
    );

    persist_token_refresh(repo, account, &new_token).await;

    let name =
        if account.name.is_none() || account.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
            let fetched = match oauth::get_user_info(&token_res.access_token).await {
                Ok(user_info) => user_info.get_display_name(),
                Err(_) => None,
            };
            if fetched.is_some() {
                persist_name(repo, account, fetched.as_deref(), Some(&new_token)).await;
            }
            fetched
        } else {
            None
        };

    let retry_result = quota::fetch_quota(&new_token.access_token, &account.email).await;

    if let Ok((ref quota_data, ref project_id)) = retry_result {
        persist_quota_data(repo, account, quota_data, project_id.as_deref(), Some(&new_token))
            .await;

        return Ok(QuotaFetchResult {
            quota: quota_data.clone(),
            refreshed_token: Some(new_token),
            project_id: project_id.clone(),
            name,
            disabled: false,
            disabled_reason: None,
        });
    }

    if let Err(AppError::Network(ref e)) = retry_result {
        if let Some(s) = e.status() {
            if s == StatusCode::FORBIDDEN {
                let mut q = QuotaData::new();
                q.is_forbidden = true;
                persist_quota_data(repo, account, &q, None, Some(&new_token)).await;
                return Ok(QuotaFetchResult {
                    quota: q,
                    refreshed_token: Some(new_token),
                    project_id: None,
                    name,
                    disabled: false,
                    disabled_reason: None,
                });
            }
        }
    }

    retry_result.map(|(q, pid)| QuotaFetchResult {
        quota: q,
        refreshed_token: Some(new_token),
        project_id: pid,
        name,
        disabled: false,
        disabled_reason: None,
    })
}

// --- Atomic persistence helpers ---

/// Disable an account atomically via repo, or fall back to JSON whole-struct write.
async fn persist_disabled(
    repo: Option<&Arc<dyn AccountRepository>>,
    account: &Account,
    reason: &str,
) {
    let now = chrono::Utc::now();

    if let Some(repo) = repo {
        if let Err(e) = repo.set_account_disabled(&account.id, reason, now).await {
            tracing::warn!("DB disable failed for {}: {}", account.email, e);
        }
    } else {
        // JSON fallback only when no repo configured
        let mut clone = account.clone();
        clone.disabled = true;
        clone.disabled_at = Some(now.timestamp());
        clone.disabled_reason = Some(reason.to_string());
        if let Err(e) = save_account_async(clone).await {
            tracing::warn!("Failed to save disabled account {}: {}", account.email, e);
        }
    }
}

/// Persist a refreshed token atomically via repo, or fall back to JSON.
async fn persist_token_refresh(
    repo: Option<&Arc<dyn AccountRepository>>,
    account: &Account,
    new_token: &TokenData,
) {
    if let Some(repo) = repo {
        let expiry = chrono::Utc::now() + chrono::Duration::seconds(new_token.expires_in);
        if let Err(e) =
            repo.update_token_credentials(&account.id, &new_token.access_token, None, expiry).await
        {
            tracing::warn!("DB token update failed for {}: {}", account.email, e);
        }
    } else {
        let mut clone = account.clone();
        clone.token = new_token.clone();
        if let Err(e) = save_account_async(clone).await {
            tracing::warn!("Failed to save refreshed token for {}: {}", account.email, e);
        }
    }
}

/// Persist quota data and optional project_id atomically via repo, or fall back to JSON.
///
/// `refreshed_token` prevents stale-data writes: when a token refresh preceded this call,
/// the JSON fallback must use the new token, not the original `account.token`.
async fn persist_quota_data(
    repo: Option<&Arc<dyn AccountRepository>>,
    account: &Account,
    quota: &QuotaData,
    project_id: Option<&str>,
    refreshed_token: Option<&TokenData>,
) {
    if let Some(repo) = repo {
        if let Err(e) = repo.update_quota(&account.id, quota.clone(), None).await {
            tracing::warn!("DB quota update failed for {}: {}", account.email, e);
        }
        if let Some(pid) = project_id {
            if Some(pid.to_string()) != account.token.project_id {
                if let Err(e) = repo.update_project_id(&account.id, pid).await {
                    tracing::warn!("DB project_id update failed for {}: {}", account.email, e);
                }
            }
        }
    } else {
        let mut clone = account.clone();
        if let Some(token) = refreshed_token {
            clone.token = token.clone();
        }
        clone.update_quota(quota.clone());
        if let Some(pid) = project_id {
            clone.token.project_id = Some(pid.to_string());
        }
        if let Err(e) = save_account_async(clone).await {
            tracing::warn!("Failed to save quota for {}: {}", account.email, e);
        }
    }
}

async fn persist_name(
    repo: Option<&Arc<dyn AccountRepository>>,
    account: &Account,
    name: Option<&str>,
    refreshed_token: Option<&TokenData>,
) {
    if let Some(repo) = repo {
        let mut updated = account.clone();
        updated.name = name.map(String::from);
        if let Err(e) = repo.update_account(&updated).await {
            tracing::warn!("DB name update failed for {}: {}", account.email, e);
        }
    } else {
        let mut clone = account.clone();
        clone.name = name.map(String::from);
        if let Some(token) = refreshed_token {
            clone.token = token.clone();
        }
        if let Err(e) = save_account_async(clone).await {
            tracing::warn!("Failed to save name for {}: {}", account.email, e);
        }
    }
}
