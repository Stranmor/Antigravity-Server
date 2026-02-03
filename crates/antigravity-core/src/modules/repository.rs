//! Account repository trait for storage abstraction.

use crate::models::{Account, QuotaData, TokenData};
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountEventType {
    Created,
    Updated,
    Disabled,
    Enabled,
    TokenRefreshed,
    QuotaUpdated,
    RateLimited,
    ModelProtected,
    ModelUnprotected,
    PhoneVerificationRequired,
}

impl AccountEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "account_created",
            Self::Updated => "account_updated",
            Self::Disabled => "account_disabled",
            Self::Enabled => "account_enabled",
            Self::TokenRefreshed => "token_refreshed",
            Self::QuotaUpdated => "quota_updated",
            Self::RateLimited => "rate_limited",
            Self::ModelProtected => "model_protected",
            Self::ModelUnprotected => "model_unprotected",
            Self::PhoneVerificationRequired => "phone_verification_required",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AccountEvent {
    pub account_id: String,
    pub event_type: AccountEventType,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct RequestLog {
    pub account_id: String,
    pub model: String,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub latency_ms: Option<i32>,
    pub status_code: i32,
    pub error_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccountHealth {
    pub account_id: String,
    pub email: String,
    pub total_requests: i64,
    pub successful_requests: i64,
    pub rate_limited_requests: i64,
    pub success_rate_pct: f64,
    pub avg_latency_ms: Option<f64>,
}

pub type RepoResult<T> = Result<T, RepositoryError>;

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Account not found: {0}")]
    NotFound(String),
    #[error("Account already exists: {0}")]
    AlreadyExists(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn list_accounts(&self) -> RepoResult<Vec<Account>>;
    async fn get_account(&self, id: &str) -> RepoResult<Account>;
    async fn get_account_by_email(&self, email: &str) -> RepoResult<Option<Account>>;
    async fn create_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account>;
    async fn update_account(&self, account: &Account) -> RepoResult<()>;
    async fn upsert_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account>;
    async fn delete_account(&self, id: &str) -> RepoResult<()>;
    async fn delete_accounts(&self, ids: &[String]) -> RepoResult<()>;
    async fn update_quota(&self, account_id: &str, quota: QuotaData) -> RepoResult<()>;
    async fn get_current_account_id(&self) -> RepoResult<Option<String>>;
    async fn set_current_account_id(&self, id: &str) -> RepoResult<()>;
    async fn log_event(&self, event: AccountEvent) -> RepoResult<()>;
    async fn log_request(&self, request: RequestLog) -> RepoResult<()>;
    async fn get_account_health(&self, account_id: &str) -> RepoResult<AccountHealth>;
    async fn get_events(&self, account_id: &str, limit: i64) -> RepoResult<Vec<AccountEvent>>;
}
