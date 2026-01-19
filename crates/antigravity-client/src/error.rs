use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Server not found at any discovery location")]
    ServerNotFound,

    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Rate limited (429): retry after {retry_after:?}s")]
    RateLimited { retry_after: Option<u64> },

    #[error("Server error ({status}): {message}")]
    ServerError { status: u16, message: String },

    #[error("Timeout after {0} attempts")]
    Timeout(usize),

    #[error("Stream error: {0}")]
    Stream(String),
}
