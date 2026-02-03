//! Quota fetching with retry logic.

use reqwest::StatusCode;

use crate::error::{AppError, AppResult};
use crate::models::{Account, QuotaData, TokenData};
use crate::modules::logger;

use super::async_wrappers::{load_account_async, save_account_async, update_account_quota_async};
use super::crud::upsert_account;

pub async fn upsert_account_async(
    email: String,
    name: Option<String>,
    token: TokenData,
) -> Result<Account, String> {
    tokio::task::spawn_blocking(move || upsert_account(email, name, token))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

pub async fn fetch_quota_with_retry(account: &mut Account) -> AppResult<QuotaData> {
    use crate::modules::{oauth, quota};

    let token = match oauth::ensure_fresh_token(&account.token).await {
        Ok(t) => t,
        Err(e) => {
            if e.contains("invalid_grant") {
                logger::log_error(&format!(
                    "Disabling account {} due to invalid_grant during token refresh (quota check)",
                    account.email
                ));
                account.disabled = true;
                account.disabled_at = Some(chrono::Utc::now().timestamp());
                account.disabled_reason = Some(format!("invalid_grant: {}", e));
                let _ = save_account_async(account.clone()).await;
            }
            return Err(AppError::OAuth(e));
        }
    };

    if token.access_token != account.token.access_token {
        logger::log_info(&format!("Time-based token refresh: {}", account.email));
        account.token = token.clone();

        let name = if account.name.is_none()
            || account.name.as_ref().is_some_and(|n| n.trim().is_empty())
        {
            match oauth::get_user_info(&token.access_token).await {
                Ok(user_info) => user_info.get_display_name(),
                Err(_) => None,
            }
        } else {
            account.name.clone()
        };

        account.name = name.clone();
        upsert_account_async(account.email.clone(), name, token.clone())
            .await
            .map_err(AppError::Account)?;
    }

    if account.name.is_none() || account.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
        logger::log_info(&format!(
            "Account {} missing name, fetching...",
            account.email
        ));
        match oauth::get_user_info(&account.token.access_token).await {
            Ok(user_info) => {
                let display_name = user_info.get_display_name();
                logger::log_info(&format!("Got name: {:?}", display_name));
                account.name = display_name.clone();
                if let Err(e) =
                    upsert_account_async(account.email.clone(), display_name, account.token.clone())
                        .await
                {
                    logger::log_warn(&format!("Failed to save name: {}", e));
                }
            }
            Err(e) => {
                logger::log_warn(&format!("Failed to get name: {}", e));
            }
        }
    }

    let result = quota::fetch_quota(&account.token.access_token, &account.email).await;

    if let Ok((ref quota_data, ref project_id)) = result {
        account.update_quota(quota_data.clone());

        if project_id.is_some() && *project_id != account.token.project_id {
            logger::log_info(&format!(
                "Project ID updated ({}), saving...",
                account.email
            ));
            account.token.project_id = project_id.clone();
            if let Err(e) = save_account_async(account.clone()).await {
                logger::log_warn(&format!(
                    "Failed to save project_id for {}: {}",
                    account.email, e
                ));
            }
        }

        if let Err(e) = update_account_quota_async(account.id.clone(), quota_data.clone()).await {
            logger::log_warn(&format!(
                "Failed to save quota for {}: {}",
                account.email, e
            ));
        } else if let Ok(updated) = load_account_async(account.id.clone()).await {
            *account = updated;
        }
    }

    if let Err(AppError::Network(ref e)) = result {
        if let Some(status) = e.status() {
            if status == StatusCode::UNAUTHORIZED {
                return handle_unauthorized_retry(account).await;
            }
        }
    }

    result.map(|(q, _)| q)
}

async fn handle_unauthorized_retry(account: &mut Account) -> AppResult<QuotaData> {
    use crate::modules::{oauth, quota};

    logger::log_warn(&format!(
        "401 Unauthorized for {}, forcing refresh...",
        account.email
    ));

    let token_res = match oauth::refresh_access_token(&account.token.refresh_token).await {
        Ok(t) => t,
        Err(e) => {
            if e.contains("invalid_grant") {
                logger::log_error(&format!(
                    "Disabling account {} due to invalid_grant during forced refresh",
                    account.email
                ));
                account.disabled = true;
                account.disabled_at = Some(chrono::Utc::now().timestamp());
                account.disabled_reason = Some(format!("invalid_grant: {}", e));
                let _ = save_account_async(account.clone()).await;
            }
            return Err(AppError::OAuth(e));
        }
    };

    let new_token = TokenData::new(
        token_res.access_token.clone(),
        account.token.refresh_token.clone(),
        token_res.expires_in,
        account.token.email.clone(),
        account.token.project_id.clone(),
        None,
    );

    let name =
        if account.name.is_none() || account.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
            match oauth::get_user_info(&token_res.access_token).await {
                Ok(user_info) => user_info.get_display_name(),
                Err(_) => None,
            }
        } else {
            account.name.clone()
        };

    account.token = new_token.clone();
    account.name = name.clone();
    upsert_account_async(account.email.clone(), name, new_token.clone())
        .await
        .map_err(AppError::Account)?;

    let retry_result = quota::fetch_quota(&new_token.access_token, &account.email).await;

    if let Ok((ref quota_data, ref project_id)) = retry_result {
        if project_id.is_some() && *project_id != account.token.project_id {
            logger::log_info(&format!(
                "Project ID updated after retry ({}), saving...",
                account.email
            ));
            account.token.project_id = project_id.clone();
            let _ = save_account_async(account.clone()).await;
        }

        let _ = update_account_quota_async(account.id.clone(), quota_data.clone()).await;
        if let Ok(updated) = load_account_async(account.id.clone()).await {
            *account = updated;
        }
    }

    if let Err(AppError::Network(ref e)) = retry_result {
        if let Some(s) = e.status() {
            if s == StatusCode::FORBIDDEN {
                let mut q = QuotaData::new();
                q.is_forbidden = true;
                return Ok(q);
            }
        }
    }

    retry_result.map(|(q, _)| q)
}
