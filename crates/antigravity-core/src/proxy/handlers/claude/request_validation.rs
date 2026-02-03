//! Request parsing and validation for Claude messages handler

use crate::proxy::mappers::claude::ClaudeRequest;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

/// Parse and validate incoming Claude request body
#[allow(clippy::result_large_err)]
pub fn parse_request(body: Value) -> Result<ClaudeRequest, Response> {
    serde_json::from_value(body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": format!("Invalid request body: {}", e)
                }
            })),
        )
            .into_response()
    })
}

/// Generate a random trace ID for request tracking
pub fn generate_trace_id() -> String {
    rand::Rng::sample_iter(rand::thread_rng(), &rand::distributions::Alphanumeric)
        .take(6)
        .map(char::from)
        .collect::<String>()
        .to_lowercase()
}

/// Create error response for service unavailable (no accounts)
pub fn no_accounts_error(message: String) -> Response {
    let safe_message = if message.contains("invalid_grant") {
        "OAuth refresh failed (invalid_grant): refresh_token likely revoked/expired; reauthorize account(s) to restore service.".to_string()
    } else {
        message
    };
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "type": "error",
            "error": {
                "type": "overloaded_error",
                "message": format!("No available accounts: {}", safe_message)
            }
        })),
    )
        .into_response()
}

/// Create error response for prompt too long
pub fn prompt_too_long_error(email: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [("X-Account-Email", email)],
        Json(json!({
            "id": "err_prompt_too_long",
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "Prompt is too long (server-side context limit reached).",
                "suggestion": "Please: 1) Execute '/compact' in Claude Code 2) Reduce conversation history 3) Switch to gemini-1.5-pro (2M context limit)"
            }
        })),
    )
        .into_response()
}

/// Create final error response after all retries exhausted
pub fn all_retries_exhausted_error(
    max_attempts: usize,
    last_error: &str,
    last_email: Option<&str>,
) -> Response {
    let error_json = json!({
        "type": "error",
        "error": {
            "type": "overloaded_error",
            "message": format!("All {} attempts failed. Last error: {}", max_attempts, last_error)
        }
    });

    if let Some(email) = last_email {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("X-Account-Email", email)],
            Json(error_json),
        )
            .into_response()
    } else {
        (StatusCode::TOO_MANY_REQUESTS, Json(error_json)).into_response()
    }
}
