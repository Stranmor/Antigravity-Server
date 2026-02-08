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

/// Typed upstream error for compile-time safe `last_error` tracking.
///
/// Replaces the fragile `String`-based `last_error` variable in handler retry
/// loops — eliminates string parsing in `sanitize_exhaustion_error`.
pub enum UpstreamError {
    /// HTTP response with status code and raw body (both logged server-side, sanitized for client).
    HttpResponse { status_code: u16, body: String },
    /// Token acquisition / OAuth error from token manager.
    TokenAcquisition(String),
    /// Connection / transport error (reqwest failures, DNS, TLS).
    ConnectionError(String),
    /// Upstream returned empty response stream.
    EmptyStream,
}

/// Sanitize the typed `last_error` used in "all accounts exhausted" messages.
///
/// Each variant maps directly to a sanitized client-facing message — no string
/// parsing required.
pub fn sanitize_exhaustion_error(error: &UpstreamError) -> String {
    match error {
        UpstreamError::HttpResponse { status_code, body } => {
            sanitize_upstream_error(*status_code, body)
        },
        UpstreamError::TokenAcquisition(_) => "Token acquisition failed".to_string(),
        UpstreamError::ConnectionError(_) => "Upstream request failed".to_string(),
        UpstreamError::EmptyStream => "Empty response from upstream".to_string(),
    }
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
            if raw_text.contains("prompt is too long")
                || raw_text.contains("exceeds the maximum")
                || raw_text.contains("token limit")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_raw_google_error_body() {
        let raw = r#"{"error":{"code":429,"message":"Resource exhausted","status":"RESOURCE_EXHAUSTED","details":[{"reason":"RATE_LIMIT_EXCEEDED","metadata":{"project":"bamboo-precept-lgxtn"}}]}}"#;
        let result = sanitize_upstream_error(429, raw);
        assert_eq!(result, "Rate limited (HTTP 429)");
        assert!(!result.contains("bamboo"));
        assert!(!result.contains("project"));
    }

    #[test]
    fn sanitize_quota_exhausted() {
        let raw = r#"{"error":{"details":[{"reason":"QUOTA_EXHAUSTED"}]}}"#;
        assert_eq!(sanitize_upstream_error(429, raw), "Quota exhausted (HTTP 429)");
    }

    #[test]
    fn sanitize_529_rate_limit() {
        assert_eq!(sanitize_upstream_error(529, "overloaded"), "Rate limited (HTTP 529)");
    }

    #[test]
    fn sanitize_401_strips_email() {
        let raw = r#"{"error":"user@gmail.com has no access"}"#;
        let result = sanitize_upstream_error(401, raw);
        assert_eq!(result, "Authentication failed (HTTP 401)");
        assert!(!result.contains("gmail"));
    }

    #[test]
    fn sanitize_403_service_disabled() {
        assert_eq!(
            sanitize_upstream_error(403, "SERVICE_DISABLED for project X"),
            "Service unavailable (HTTP 403)"
        );
        assert_eq!(
            sanitize_upstream_error(403, "CONSUMER_INVALID"),
            "Service unavailable (HTTP 403)"
        );
        assert_eq!(
            sanitize_upstream_error(403, "Permission denied on resource project bamboo-precept"),
            "Service unavailable (HTTP 403)"
        );
    }

    #[test]
    fn sanitize_403_other_is_auth_error() {
        assert_eq!(
            sanitize_upstream_error(403, "some other 403 reason"),
            "Authentication failed (HTTP 403)"
        );
    }

    #[test]
    fn sanitize_404_model_not_found() {
        assert_eq!(
            sanitize_upstream_error(404, "model not found"),
            "Model not available (HTTP 404)"
        );
    }

    #[test]
    fn sanitize_400_prompt_too_long() {
        assert_eq!(
            sanitize_upstream_error(400, "prompt is too long: 278399 tokens > 200000 maximum"),
            "Prompt too long (HTTP 400)"
        );
        assert_eq!(
            sanitize_upstream_error(400, "input exceeds the maximum context length"),
            "Prompt too long (HTTP 400)"
        );
        assert_eq!(
            sanitize_upstream_error(400, "token limit exceeded"),
            "Prompt too long (HTTP 400)"
        );
    }

    #[test]
    fn sanitize_400_generic() {
        assert_eq!(
            sanitize_upstream_error(400, "INVALID_ARGUMENT: missing field"),
            "Upstream error (HTTP 400)"
        );
    }

    #[test]
    fn sanitize_server_errors() {
        assert_eq!(sanitize_upstream_error(500, "internal"), "Upstream server error (HTTP 500)");
        assert_eq!(sanitize_upstream_error(502, "bad gw"), "Upstream server error (HTTP 502)");
        assert_eq!(sanitize_upstream_error(503, "unavail"), "Upstream server error (HTTP 503)");
    }

    #[test]
    fn sanitize_unknown_status() {
        assert_eq!(sanitize_upstream_error(418, "teapot"), "Upstream error (HTTP 418)");
    }

    #[test]
    fn exhaustion_http_response_quota_exhausted() {
        let err = UpstreamError::HttpResponse {
            status_code: 429,
            body: r#"{"error":{"details":[{"reason":"QUOTA_EXHAUSTED"}]}}"#.to_string(),
        };
        assert_eq!(sanitize_exhaustion_error(&err), "Quota exhausted (HTTP 429)");
    }

    #[test]
    fn exhaustion_http_response_server_error() {
        let err = UpstreamError::HttpResponse {
            status_code: 500,
            body: "internal server error".to_string(),
        };
        assert_eq!(sanitize_exhaustion_error(&err), "Upstream server error (HTTP 500)");
    }

    #[test]
    fn exhaustion_connection_error() {
        let err = UpstreamError::ConnectionError(
            "HTTP request failed at https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro".to_string(),
        );
        assert_eq!(sanitize_exhaustion_error(&err), "Upstream request failed");
    }

    #[test]
    fn exhaustion_token_acquisition() {
        let err = UpstreamError::TokenAcquisition("invalid_grant".to_string());
        assert_eq!(sanitize_exhaustion_error(&err), "Token acquisition failed");
    }

    #[test]
    fn exhaustion_empty_stream() {
        assert_eq!(
            sanitize_exhaustion_error(&UpstreamError::EmptyStream),
            "Empty response from upstream"
        );
    }

    #[test]
    fn no_internal_urls_in_any_output() {
        let dangerous_inputs = [
            (429, "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro"),
            (403, "Permission denied on resource project bamboo-precept-lgxtn"),
            (500, "Internal error at https://us-central1-aiplatform.googleapis.com/v1/projects/my-project"),
            (401, "user@example.com unauthorized"),
        ];
        for (code, raw) in dangerous_inputs {
            let result = sanitize_upstream_error(code, raw);
            assert!(!result.contains("googleapis"), "Leaked URL in: {}", result);
            assert!(!result.contains("@"), "Leaked email in: {}", result);
            assert!(!result.contains("bamboo"), "Leaked project in: {}", result);
            assert!(!result.contains("my-project"), "Leaked project in: {}", result);
        }
    }
}
