//! Account repository trait for storage abstraction.

use crate::models::{Account, QuotaData, TokenData};
use async_trait::async_trait;

/// Types of account lifecycle events for event sourcing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AccountEventType {
    /// Account was created.
    Created,
    /// Account metadata was updated.
    Updated,
    /// Account was deleted.
    Deleted,
    /// Account was disabled.
    Disabled,
    /// Account was re-enabled.
    Enabled,
    /// OAuth token was refreshed.
    TokenRefreshed,
    /// Quota data was updated.
    QuotaUpdated,
    /// Account hit rate limit.
    RateLimited,
    /// Model was added to protected list.
    ModelProtected,
    /// Model was removed from protected list.
    ModelUnprotected,
    /// Phone verification is required.
    PhoneVerificationRequired,
}

impl AccountEventType {
    /// Convert event type to database string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "account_created",
            Self::Updated => "account_updated",
            Self::Deleted => "account_deleted",
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

/// Account lifecycle event for event sourcing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AccountEvent {
    /// Account identifier.
    pub account_id: String,
    /// Type of event.
    pub event_type: AccountEventType,
    /// Event-specific metadata as JSON.
    pub metadata: serde_json::Value,
    /// When the event occurred.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Request log entry for analytics.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RequestLog {
    /// Account that handled the request.
    pub account_id: String,
    /// Model used for the request.
    pub model: String,
    /// Input tokens consumed.
    pub tokens_in: Option<i32>,
    /// Output tokens generated.
    pub tokens_out: Option<i32>,
    /// Cached input tokens (from prompt cache).
    pub cached_tokens: Option<i32>,
    /// Request latency in milliseconds.
    pub latency_ms: Option<i32>,
    /// HTTP status code returned.
    pub status_code: i32,
    /// Error type if request failed.
    pub error_type: Option<String>,
}

/// Account health metrics aggregated from request logs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AccountHealth {
    /// Account identifier.
    pub account_id: String,
    /// Account email.
    pub email: String,
    /// Total requests handled.
    pub total_requests: i64,
    /// Successful requests (2xx).
    pub successful_requests: i64,
    /// Requests that hit rate limits.
    pub rate_limited_requests: i64,
    /// Success rate as percentage.
    pub success_rate_pct: f64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: Option<f64>,
}

/// Result type for repository operations.
pub type RepoResult<T> = Result<T, RepositoryError>;

/// Repository operation errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RepositoryError {
    /// Account with given ID was not found.
    #[error("Account not found: {0}")]
    NotFound(String),
    /// Account with given email already exists.
    #[error("Account already exists: {0}")]
    AlreadyExists(String),
    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(String),
    /// JSON serialization/deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Abstract repository for account storage operations.
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// List all accounts.
    async fn list_accounts(&self) -> RepoResult<Vec<Account>>;
    /// Get account by ID.
    async fn get_account(&self, id: &str) -> RepoResult<Account>;
    /// Find account by email address.
    async fn get_account_by_email(&self, email: &str) -> RepoResult<Option<Account>>;
    /// Create a new account.
    async fn create_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account>;
    /// Update existing account.
    async fn update_account(&self, account: &Account) -> RepoResult<()>;
    /// Create or update account by email.
    async fn upsert_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account>;
    /// Delete account by ID.
    async fn delete_account(&self, id: &str) -> RepoResult<()>;
    /// Delete multiple accounts by IDs.
    async fn delete_accounts(&self, ids: &[String]) -> RepoResult<()>;
    /// Update quota data for account.
    async fn update_quota(&self, account_id: &str, quota: QuotaData) -> RepoResult<()>;
    /// Get currently selected account ID.
    async fn get_current_account_id(&self) -> RepoResult<Option<String>>;
    /// Set currently selected account ID.
    async fn set_current_account_id(&self, id: &str) -> RepoResult<()>;
    /// Log an account event.
    async fn log_event(&self, event: AccountEvent) -> RepoResult<()>;
    /// Log a request for analytics.
    async fn log_request(&self, request: RequestLog) -> RepoResult<()>;
    /// Get health metrics for account.
    async fn get_account_health(&self, account_id: &str) -> RepoResult<AccountHealth>;
    /// Get recent events for account.
    async fn get_events(&self, account_id: &str, limit: i64) -> RepoResult<Vec<AccountEvent>>;

    /// Update only token credentials (atomic, no read-modify-write).
    async fn update_token_credentials(
        &self,
        account_id: &str,
        access_token: &str,
        expires_in: i64,
        expiry_timestamp: i64,
    ) -> RepoResult<()>;

    /// Update only the project_id on a token (atomic, no read-modify-write).
    async fn update_project_id(&self, account_id: &str, project_id: &str) -> RepoResult<()>;

    /// Disable an account (atomic, no read-modify-write).
    async fn set_account_disabled(
        &self,
        account_id: &str,
        reason: &str,
        disabled_at: i64,
    ) -> RepoResult<()>;
}
