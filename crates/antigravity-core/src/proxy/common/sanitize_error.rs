//! Upstream error sanitization — prevents leaking internal project IDs,
//! account emails, and Google API URLs to clients.
//!
//! Pattern: log the raw error server-side, return only an opaque message
//! with the HTTP status code to the client.

/// Sanitize an upstream error for client consumption.
///
/// Returns a generic message that includes only the HTTP status code
/// and a high-level error category (if detectable), without any
/// internal details from the upstream response body.
pub fn sanitize_upstream_error(status_code: u16, raw_text: &str) -> String {
    let category = classify_error(status_code, raw_text);
    match category {
        ErrorCategory::RateLimited => format!("Rate limited (HTTP {})", status_code),
        ErrorCategory::QuotaExhausted => format!("Quota exhausted (HTTP {})", status_code),
        ErrorCategory::Unauthorized => format!("Authentication failed (HTTP {})", status_code),
        ErrorCategory::ModelNotFound => format!("Model not available (HTTP {})", status_code),
        ErrorCategory::PromptTooLong => format!("Prompt too long (HTTP {})", status_code),
        ErrorCategory::ServiceDisabled => format!("Service unavailable (HTTP {})", status_code),
        ErrorCategory::ServerError => format!("Upstream server error (HTTP {})", status_code),
        ErrorCategory::Unknown => format!("Upstream error (HTTP {})", status_code),
    }
}

/// Sanitize the `last_error` string used in "all accounts exhausted" messages.
///
/// The `last_error` typically contains `"HTTP {code}: {raw_body}"` — we strip
/// the raw body and return only the sanitized version.
pub fn sanitize_exhaustion_error(last_error: &str) -> String {
    if let Some(code_str) = last_error.strip_prefix("HTTP ") {
        if let Some((code_part, raw_body)) = code_str.split_once(": ") {
            if let Ok(code) = code_part.parse::<u16>() {
                return sanitize_upstream_error(code, raw_body);
            }
        }
        // "HTTP {code}" without body — already safe
        if let Ok(code) = code_str.parse::<u16>() {
            return format!("Upstream error (HTTP {})", code);
        }
    }
    // Fallback: not in "HTTP xxx: ..." format — could be a connection error
    // (these don't contain sensitive data, but sanitize defensively)
    "Upstream request failed".to_string()
}

enum ErrorCategory {
    RateLimited,
    QuotaExhausted,
    Unauthorized,
    ModelNotFound,
    PromptTooLong,
    ServiceDisabled,
    ServerError,
    Unknown,
}

fn classify_error(status_code: u16, raw_text: &str) -> ErrorCategory {
    match status_code {
        429 | 529 => {
            if raw_text.contains("QUOTA_EXHAUSTED") {
                ErrorCategory::QuotaExhausted
            } else {
                ErrorCategory::RateLimited
            }
        },
        401 => ErrorCategory::Unauthorized,
        403 => {
            if raw_text.contains("SERVICE_DISABLED")
                || raw_text.contains("CONSUMER_INVALID")
                || raw_text.contains("Permission denied")
            {
                ErrorCategory::ServiceDisabled
            } else {
                ErrorCategory::Unauthorized
            }
        },
        404 => ErrorCategory::ModelNotFound,
        400 => {
            if raw_text.contains("too long")
                || raw_text.contains("exceeds")
                || raw_text.contains("prompt is too long")
            {
                ErrorCategory::PromptTooLong
            } else {
                ErrorCategory::Unknown
            }
        },
        500 | 502 | 503 => ErrorCategory::ServerError,
        _ => ErrorCategory::Unknown,
    }
}
