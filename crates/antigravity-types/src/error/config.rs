//! Configuration-related errors.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during configuration operations.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "details")]
pub enum ConfigError {
    /// Config file not found at expected path
    #[error("Config not found: {path}")]
    NotFound { path: String },

    /// Config file parse error (JSON/YAML)
    #[error("Config parse error: {message}")]
    ParseError { message: String },

    /// Config validation error (invalid values)
    #[error("Config validation error for {field}: {message}")]
    ValidationError { field: String, message: String },

    /// Config write error (permission denied, disk full, etc)
    #[error("Config write error: {message}")]
    WriteError { message: String },

    /// Config migration failed (version upgrade)
    #[error("Config migration failed from v{from} to v{to}: {message}")]
    MigrationFailed {
        from: u32,
        to: u32,
        message: String,
    },
}

impl ConfigError {
    /// Create a parse error from a serde_json error.
    pub fn from_json_error(e: serde_json::Error) -> Self {
        Self::ParseError {
            message: e.to_string(),
        }
    }

    /// Create a write error from an IO error.
    pub fn from_io_error(e: std::io::Error) -> Self {
        Self::WriteError {
            message: e.to_string(),
        }
    }
}
