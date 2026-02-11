//! Shared error info extraction from upstream HTTP responses.

use crate::proxy::common::UpstreamError;

/// Extracted error information from an upstream HTTP response.
pub struct ErrorInfo {
    pub status_code: u16,
    pub retry_after: Option<String>,
    pub error_text: String,
}

/// Extracts error details from a failed upstream response.
pub async fn extract_error_info(response: reqwest::Response) -> (ErrorInfo, UpstreamError) {
    let status_code = response.status().as_u16();
    let retry_after =
        response.headers().get("Retry-After").and_then(|h| h.to_str().ok()).map(|s| s.to_string());
    let error_text = response.text().await.unwrap_or_else(|_| format!("HTTP {}", status_code));
    let error = UpstreamError::HttpResponse { status_code, body: error_text.clone() };
    (ErrorInfo { status_code, retry_after, error_text }, error)
}
