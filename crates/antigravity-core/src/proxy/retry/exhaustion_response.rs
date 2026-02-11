//! Shared exhaustion response builder for protocol handlers.

use crate::proxy::common::header_constants::X_ACCOUNT_EMAIL;
use crate::proxy::common::{sanitize_exhaustion_error, UpstreamError};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Builds the standard "all accounts exhausted" response.
pub fn build_exhaustion_response(last_error: &UpstreamError, last_email: Option<&str>) -> Response {
    let msg =
        format!("All accounts exhausted. Last error: {}", sanitize_exhaustion_error(last_error));
    match last_email {
        Some(email) => (StatusCode::TOO_MANY_REQUESTS, [(X_ACCOUNT_EMAIL, email.to_owned())], msg)
            .into_response(),
        None => (StatusCode::TOO_MANY_REQUESTS, msg).into_response(),
    }
}
