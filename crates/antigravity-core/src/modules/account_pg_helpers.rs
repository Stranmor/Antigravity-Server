//! Helper functions for PostgreSQL account operations.

use crate::models::{Account, ModelQuota, QuotaData, TokenData};
use crate::modules::repository::{AccountEventType, RepoResult, RepositoryError};
use sqlx::Row;
use std::collections::HashSet;
use uuid::Uuid;

/// Convert a PostgreSQL row to an Account struct.
pub(crate) fn row_to_account(row: &sqlx::postgres::PgRow) -> RepoResult<Account> {
    let id: Uuid = row.get("id");
    let protected_json: serde_json::Value = row.get("protected_models");
    let protected: HashSet<String> = serde_json::from_value(protected_json)
        .map_err(|err| RepositoryError::Serialization(err.to_string()))?;

    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let last_used: chrono::DateTime<chrono::Utc> = row.get("last_used_at");
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("disabled_at");
    let proxy_disabled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("proxy_disabled_at");
    let expiry_timestamp: i64 = row.get("expiry_timestamp");

    let now = chrono::Utc::now().timestamp();
    let expires_in = (expiry_timestamp.saturating_sub(now)).max(0);

    let token_tier: Option<String> = row.get("token_tier");

    let quota_models: Option<serde_json::Value> = row.get("quota_models");
    let quota_is_forbidden: Option<bool> = row.get("quota_is_forbidden");
    let quota_fetched_at: Option<chrono::DateTime<chrono::Utc>> = row.get("quota_fetched_at");

    let quota = match quota_models {
        Some(models_json) => {
            let models: Vec<ModelQuota> = serde_json::from_value(models_json)
                .map_err(|err| RepositoryError::Serialization(err.to_string()))?;
            Some(QuotaData {
                models,
                last_updated: quota_fetched_at.map_or(0, |dt| dt.timestamp()),
                is_forbidden: quota_is_forbidden.unwrap_or(false),
                subscription_tier: token_tier,
            })
        },
        None => None,
    };

    Ok(Account {
        id: id.to_string(),
        email: row.get("email"),
        name: row.get("name"),
        token: TokenData {
            access_token: row.get("access_token"),
            refresh_token: row.get("refresh_token"),
            expires_in,
            expiry_timestamp,
            token_type: "Bearer".to_owned(),
            email: row.get("token_email"),
            project_id: row.get("project_id"),
            session_id: None,
        },
        quota,
        disabled: row.get("disabled"),
        disabled_reason: row.get("disabled_reason"),
        disabled_at: disabled_at.map(|dt| dt.timestamp()),
        proxy_disabled: row.get("proxy_disabled"),
        proxy_disabled_reason: row.get("proxy_disabled_reason"),
        proxy_disabled_at: proxy_disabled_at.map(|dt| dt.timestamp()),
        protected_models: protected,
        proxy_url: row.get("proxy_url"),
        created_at: created_at.timestamp(),
        last_used: last_used.timestamp(),
    })
}

/// Parse event type string to enum.
pub(crate) fn parse_event_type(event_str: &str) -> AccountEventType {
    match event_str {
        "account_created" => AccountEventType::Created,
        "account_updated" => AccountEventType::Updated,
        "account_deleted" => AccountEventType::Deleted,
        "account_disabled" => AccountEventType::Disabled,
        "account_enabled" => AccountEventType::Enabled,
        "token_refreshed" => AccountEventType::TokenRefreshed,
        "quota_updated" => AccountEventType::QuotaUpdated,
        "rate_limited" => AccountEventType::RateLimited,
        "model_protected" => AccountEventType::ModelProtected,
        "model_unprotected" => AccountEventType::ModelUnprotected,
        "phone_verification_required" => AccountEventType::PhoneVerificationRequired,
        _ => AccountEventType::Updated,
    }
}

/// Map sqlx error to repository error.
pub(crate) fn map_sqlx_err(err: sqlx::Error) -> RepositoryError {
    RepositoryError::Database(err.to_string())
}
