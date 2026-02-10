//! Gemini API handlers.

mod models;
mod streaming;

pub use models::{handle_count_tokens, handle_get_model, handle_list_models};

use crate::proxy::common::{sanitize_exhaustion_error, sanitize_upstream_error, UpstreamError};
use crate::proxy::{
    mappers::gemini::{unwrap_response, wrap_request},
    server::AppState,
    session_manager::SessionManager,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::Value;
use std::collections::HashSet;
use streaming::{build_stream_response, extract_signature, peek_first_chunk};
use tracing::{debug, error, info, warn};

const MAX_RETRY_ATTEMPTS: usize = 64;

pub async fn handle_generate(
    State(state): State<AppState>,
    Path(model_action): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let force_account =
        headers.get("X-Force-Account").and_then(|v| v.to_str().ok()).map(|s| s.to_string());

    let (model_name, method) = match model_action.rsplit_once(':') {
        Some((m, action)) => (m.to_string(), action.to_string()),
        None => (model_action, "generateContent".to_string()),
    };

    info!("[Gemini] Request: {}/{}", model_name, method);

    if method != "generateContent" && method != "streamGenerateContent" {
        return Err((StatusCode::BAD_REQUEST, format!("Unsupported method: {}", method)));
    }
    let is_stream = method == "streamGenerateContent";

    let token_manager = state.token_manager.clone();
    let max_attempts = MAX_RETRY_ATTEMPTS.min(token_manager.len()).max(1);

    let mut last_error = UpstreamError::EmptyStream;
    let mut last_email: Option<String> = None;
    let mut attempt = 0usize;
    let mut grace_retry_used = false;
    let mut attempted_accounts: HashSet<String> = HashSet::new();

    while attempt < max_attempts {
        let (mapped_model, _reason) = match crate::proxy::common::resolve_model_route(
            &model_name,
            &*state.custom_mapping.read().await,
        ) {
            Ok(result) => result,
            Err(e) => return Err((StatusCode::BAD_REQUEST, e)),
        };

        let tools_val: Option<Vec<Value>> =
            body.get("tools").and_then(|t| t.as_array()).map(|arr| {
                arr.iter()
                    .flat_map(|entry| {
                        entry
                            .get("functionDeclarations")
                            .and_then(|v| v.as_array())
                            .map(|decls| decls.to_vec())
                            .unwrap_or_else(|| vec![entry.clone()])
                    })
                    .collect()
            });

        let config = crate::proxy::mappers::request_config::resolve_request_config(
            &model_name,
            &mapped_model,
            &tools_val,
            None,
            None,
        );
        let session_id = SessionManager::extract_gemini_session_id(&body, &model_name);

        let (access_token, project_id, email, _guard) = if let Some(ref forced) = force_account {
            match token_manager.get_token_forced(forced, &config.final_model).await {
                Ok((token, project, email, guard)) => (token, project, email, guard),
                Err(e) => {
                    warn!("[Gemini] Forced account {} failed: {}, using smart routing", forced, e);
                    match token_manager
                        .get_token_with_exclusions(
                            &config.request_type,
                            attempt > 0,
                            Some(&session_id),
                            &config.final_model,
                            if attempted_accounts.is_empty() {
                                None
                            } else {
                                Some(&attempted_accounts)
                            },
                        )
                        .await
                    {
                        Ok(t) => t,
                        Err(e) => {
                            return Err((
                                StatusCode::SERVICE_UNAVAILABLE,
                                format!("Token error: {}", e),
                            ))
                        },
                    }
                },
            }
        } else {
            match token_manager
                .get_token_with_exclusions(
                    &config.request_type,
                    attempt > 0,
                    Some(&session_id),
                    &config.final_model,
                    if attempted_accounts.is_empty() { None } else { Some(&attempted_accounts) },
                )
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    return Err((StatusCode::SERVICE_UNAVAILABLE, format!("Token error: {}", e)))
                },
            }
        };

        last_email = Some(email.clone());
        info!("[Gemini] Account: {} (type: {})", email, config.request_type);

        let wrapped_body = wrap_request(&body, &project_id, &mapped_model, Some(&session_id));
        let query_string = if is_stream { Some("alt=sse") } else { None };
        let upstream_method = if is_stream { "streamGenerateContent" } else { "generateContent" };

        let response = match state
            .upstream
            .call_v1_internal(upstream_method, &access_token, wrapped_body, query_string)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = UpstreamError::TokenAcquisition(e.clone());
                debug!("[Gemini] Attempt {}/{} failed: {}", attempt + 1, max_attempts, e);
                attempt += 1;
                continue;
            },
        };

        let status = response.status();
        if status.is_success() {
            if is_stream {
                let mut response_stream = response.bytes_stream();

                let first_chunk = match peek_first_chunk(&mut response_stream).await {
                    Ok(chunk) => chunk,
                    Err(peek_err) => {
                        warn!("[Gemini] Peek failed: {}, rotating account", peek_err);
                        last_error = UpstreamError::ConnectionError(peek_err);
                        attempted_accounts.insert(email.clone());
                        attempt += 1;
                        continue;
                    },
                };

                return build_stream_response(
                    response_stream,
                    first_chunk,
                    session_id,
                    email,
                    mapped_model,
                )
                .await;
            }

            let resp: Value = response
                .json()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

            extract_signature(&resp, &session_id);
            let unwrapped = unwrap_response(&resp);
            return Ok((
                StatusCode::OK,
                [("X-Account-Email", email.as_str()), ("X-Mapped-Model", mapped_model.as_str())],
                Json(unwrapped),
            )
                .into_response());
        }

        let code = status.as_u16();
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        let error_text = response.text().await.unwrap_or_else(|_| format!("HTTP {}", code));
        last_error = UpstreamError::HttpResponse { status_code: code, body: error_text.clone() };

        if crate::proxy::retry::is_rate_limit_code(code) || code == 401 || code == 403 {
            token_manager.mark_rate_limited(&email, code, retry_after.as_deref(), &error_text);

            if code == 403
                && (error_text.contains("SERVICE_DISABLED")
                    || error_text.contains("CONSUMER_INVALID")
                    || error_text.contains("Permission denied on resource project")
                    || error_text.contains("verify your account"))
            {
                warn!(
                    "[Gemini] Account {} needs verification or has project issue. 1h lockout.",
                    email
                );
                token_manager.rate_limit_tracker().set_lockout_until(
                    &email,
                    std::time::SystemTime::now() + std::time::Duration::from_secs(3600),
                    crate::proxy::rate_limit::RateLimitReason::ServerError,
                    None,
                );
                let email_clone = email.clone();
                tokio::spawn(async move {
                    let _ = crate::modules::account::mark_needs_verification_by_email(&email_clone)
                        .await;
                });
                attempted_accounts.insert(email.clone());
                attempt += 1;
                continue;
            }

            if code == 429 && error_text.contains("QUOTA_EXHAUSTED") {
                error!("[Gemini] Quota exhausted on {}, rotating to next account", email);
                attempted_accounts.insert(email.clone());
                attempt += 1;
                continue;
            }
            if code == 429 && !grace_retry_used && error_text.contains("RATE_LIMIT_EXCEEDED") {
                grace_retry_used = true;
                warn!("[Gemini] 429 RATE_LIMIT_EXCEEDED on {}, grace retry", email);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
            warn!("[Gemini] {} on {}, rotating", code, email);
            grace_retry_used = false;
            attempted_accounts.insert(email.clone());
            attempt += 1;
            continue;
        }

        error!("[Gemini] Non-retryable {}: {}", code, error_text);
        if code == 404 {
            warn!("[Gemini] 404 on {} (model not available on this tier), rotating", email);
            attempted_accounts.insert(email.clone());
            attempt += 1;
            continue;
        }
        return Ok((
            status,
            [("X-Account-Email", email.as_str())],
            sanitize_upstream_error(code, &error_text),
        )
            .into_response());
    }

    let msg = format!("All accounts exhausted. Last: {}", sanitize_exhaustion_error(&last_error));
    match last_email {
        Some(email) => {
            Ok((StatusCode::TOO_MANY_REQUESTS, [("X-Account-Email", email)], msg).into_response())
        },
        None => Ok((StatusCode::TOO_MANY_REQUESTS, msg).into_response()),
    }
}
