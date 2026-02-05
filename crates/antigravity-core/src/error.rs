//! Unified error types for Antigravity Core.

use serde::Serialize;
use thiserror::Error;

/// Main error type for all Antigravity operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum AppError {
    /// Database operation failed (SQLite).
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Network request failed (HTTP client).
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// File system I/O operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// OAuth authentication or token refresh failed.
    #[error("OAuth error: {0}")]
    OAuth(String),

    /// Configuration loading or validation failed.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Account operation failed (not found, disabled, etc.).
    #[error("Account error: {0}")]
    Account(String),

    /// Proxy operation failed (upstream error, transformation error).
    #[error("Proxy error: {0}")]
    Proxy(String),

    /// Rate limit exceeded for an account or provider.
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Unclassified error with message.
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

/// Result type alias for Antigravity operations.
pub type AppResult<T> = Result<T, AppError>;

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Unknown(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Unknown(s.to_string())
    }
}
