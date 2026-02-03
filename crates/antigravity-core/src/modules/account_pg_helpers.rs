use crate::models::{Account, TokenData};
use crate::modules::repository::{AccountEventType, RepoResult, RepositoryError};
use sqlx::Row;
use std::collections::HashSet;
use uuid::Uuid;

/// Convert a PostgreSQL row to an Account struct
pub fn row_to_account(row: &sqlx::postgres::PgRow) -> RepoResult<Account> {
    let id: Uuid = row.get("id");
    let protected_json: serde_json::Value = row.get("protected_models");
    let protected: HashSet<String> = serde_json::from_value(protected_json)
        .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let last_used: chrono::DateTime<chrono::Utc> = row.get("last_used_at");
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("disabled_at");
    let proxy_disabled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("proxy_disabled_at");
    let expiry_timestamp: i64 = row.get("expiry_timestamp");

    // Calculate expires_in from stored absolute timestamp
    let now = chrono::Utc::now().timestamp();
    let expires_in = (expiry_timestamp - now).max(0);

    Ok(Account {
        id: id.to_string(),
        email: row.get("email"),
        name: row.get("name"),
        token: TokenData {
            access_token: row.get("access_token"),
            refresh_token: row.get("refresh_token"),
            expires_in,
            expiry_timestamp,
            token_type: "Bearer".to_string(),
            email: row.get("token_email"),
            project_id: row.get("project_id"),
            session_id: None,
        },
        quota: None,
        disabled: row.get("disabled"),
        disabled_reason: row.get("disabled_reason"),
        disabled_at: disabled_at.map(|dt| dt.timestamp()),
        proxy_disabled: row.get("proxy_disabled"),
        proxy_disabled_reason: row.get("proxy_disabled_reason"),
        proxy_disabled_at: proxy_disabled_at.map(|dt| dt.timestamp()),
        protected_models: protected,
        created_at: created_at.timestamp(),
        last_used: last_used.timestamp(),
    })
}

/// Parse event type string to enum
pub fn parse_event_type(s: &str) -> AccountEventType {
    match s {
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

/// Map sqlx error to repository error
pub fn map_sqlx_err(e: sqlx::Error) -> RepositoryError {
    RepositoryError::Database(e.to_string())
}
