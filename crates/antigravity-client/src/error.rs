//! Error types for the Antigravity client.

use thiserror::Error;

/// Errors that can occur when using the Antigravity client.
#[derive(Error, Debug)]
pub enum ClientError {
    /// Failed to establish connection to the server.
    #[error("Connection failed: {0}")]
    Connection(String),

    /// No Antigravity server found at any discovery location.
    #[error("Server not found at any discovery location")]
    ServerNotFound,

    /// HTTP request failed.
    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),

    /// Server returned an invalid or unparseable response.
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Server returned 429 Too Many Requests.
    #[error("Rate limited (429): retry after {retry_after:?}s")]
    RateLimited {
        /// Seconds to wait before retrying, if provided by server.
        retry_after: Option<u64>,
    },

    /// Server returned a 5xx error.
    #[error("Server error ({status}): {message}")]
    ServerError {
        /// HTTP status code.
        status: u16,
        /// Error message from server.
        message: String,
    },

    /// Request timed out after maximum retry attempts.
    #[error("Timeout after {0} attempts")]
    Timeout(u32),

    /// Error occurred during SSE streaming.
    #[error("Stream error: {0}")]
    Stream(String),
}
