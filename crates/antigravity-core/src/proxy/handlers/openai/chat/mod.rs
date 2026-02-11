mod error_handler;
mod signature_preload;
mod stream_handler;
mod token_acquisition;
mod upstream_request;

use super::responses_format::{convert_responses_to_chat, is_responses_format};
use super::MAX_RETRY_ATTEMPTS;
use crate::proxy::common::header_constants::{
    X_ACCOUNT_EMAIL, X_FORCE_ACCOUNT, X_MAPPED_MODEL, X_MAPPING_REASON,
};
use axum::http::HeaderMap;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::proxy::common::{sanitize_upstream_error, UpstreamError};
use crate::proxy::handlers::openai::completions::request_parser::ensure_non_empty_messages;
use crate::proxy::mappers::openai::{transform_openai_request, OpenAIRequest};
use crate::proxy::retry::{build_exhaustion_response, extract_error_info, record_request_success};
use crate::proxy::server::AppState;
use crate::proxy::session_manager::SessionManager;

use error_handler::{
    handle_auth_errors, handle_grace_retry, handle_rate_limit_errors, handle_service_disabled,
    OpenAIErrorAction,
};
use stream_handler::{handle_stream_response, OpenAIStreamResult};
use token_acquisition::acquire_token;
use upstream_request::{call_upstream_with_retry, UpstreamResult};

pub async fn handle_chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let force_account =
        headers.get(X_FORCE_ACCOUNT).and_then(|v| v.to_str().ok()).map(|s| s.to_string());

    if is_responses_format(&body) {
        convert_responses_to_chat(&mut body);
    }

    let mut openai_req: OpenAIRequest = serde_json::from_value(body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;

    ensure_non_empty_messages(&mut openai_req);

    debug!("Received OpenAI request for model: {}", openai_req.model);

    let upstream = state.upstream.clone();
    let token_manager = state.token_manager.clone();
    let pool_size = token_manager.len();
    let max_attempts = MAX_RETRY_ATTEMPTS.min(pool_size).max(1);

    let mut last_error = UpstreamError::EmptyStream;
    let mut last_email: Option<String> = None;
    let trace_id = format!("oai_{}", chrono::Utc::now().timestamp_micros());
    let mut grace_retry_used = false;
    let mut attempt = 0usize;
    let mut attempted_accounts: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    while attempt < max_attempts {
        let (mapped_model, reason) = match crate::proxy::common::resolve_model_route(
            &openai_req.model,
            &*state.custom_mapping.read().await,
        ) {
            Ok(result) => result,
            Err(e) => return Err((StatusCode::BAD_REQUEST, e)),
        };
        let tools_val: Option<Vec<Value>> = openai_req.tools.as_ref().map(|list| list.to_vec());
        let config = crate::proxy::mappers::request_config::resolve_request_config(
            &openai_req.model,
            &mapped_model,
            &tools_val,
            None,
            None,
        );

        let session_id = SessionManager::extract_openai_session_id(&openai_req);

        let (access_token, project_id, email, _active_guard) = match acquire_token(
            token_manager.clone(),
            force_account.as_deref(),
            &config,
            &session_id,
            attempt > 0,
            &attempted_accounts,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => return Err((StatusCode::SERVICE_UNAVAILABLE, e)),
        };

        last_email = Some(email.clone());
        info!("✓ Using account: {} (type: {})", email, config.request_type);

        signature_preload::preload_signatures(&openai_req).await;

        let is_claude_model = mapped_model.starts_with("claude-");

        let gemini_body = if is_claude_model {
            let mut claude_req =
                crate::proxy::mappers::openai::request::claude_bridge::openai_to_claude_request(
                    &openai_req,
                );
            claude_req.model = mapped_model.clone();

            let mut body = crate::proxy::mappers::claude::transform_claude_request_in(
                &claude_req,
                &project_id,
                false,
            )
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

            if let Some(fmt) = &openai_req.response_format {
                if fmt.r#type == "json_object" {
                    if let Some(gen_config) = body.get_mut("generationConfig") {
                        gen_config["responseMimeType"] = serde_json::json!("application/json");
                    }
                }
            }

            body
        } else {
            transform_openai_request(&openai_req, &project_id, &mapped_model)
        };

        debug!("[OpenAI-Request] Transformed Gemini Body");

        let client_wants_stream = openai_req.stream;
        if !client_wants_stream {
            info!("[OpenAI] 🔄 Auto-converting non-stream request to stream for better quota");
        }

        let response = match call_upstream_with_retry(
            upstream.clone(),
            "streamGenerateContent",
            &access_token,
            gemini_body,
            Some("alt=sse"),
            None,
            &email,
            attempt,
            max_attempts,
        )
        .await
        {
            UpstreamResult::Success(r) => r,
            UpstreamResult::ConnectionError(e) => {
                last_error = UpstreamError::ConnectionError(e);
                attempted_accounts.insert(email.clone());
                attempt += 1;
                grace_retry_used = false;
                continue;
            },
        };

        let status = response.status();
        if status.is_success() {
            record_request_success(&token_manager, &state, &email, &session_id);

            let gemini_stream = response.bytes_stream();
            match handle_stream_response(
                gemini_stream,
                openai_req.model.clone(),
                email.clone(),
                mapped_model.clone(),
                reason.clone(),
                client_wants_stream,
                &trace_id,
                session_id.clone(),
            )
            .await
            {
                OpenAIStreamResult::StreamingResponse(resp) => return Ok(resp.into_response()),
                OpenAIStreamResult::JsonResponse(st, em, model, rsn, json) => {
                    return Ok((
                        st,
                        [
                            (X_ACCOUNT_EMAIL, em.as_str()),
                            (X_MAPPED_MODEL, model.as_str()),
                            (X_MAPPING_REASON, rsn.as_str()),
                        ],
                        Json(json),
                    )
                        .into_response());
                },
                OpenAIStreamResult::Retry(err) => {
                    last_error = UpstreamError::ConnectionError(err);
                    attempted_accounts.insert(email.clone());
                    attempt += 1;
                    grace_retry_used = false;
                    continue;
                },
                OpenAIStreamResult::EmptyStream => {
                    warn!("[{}] Stream ended immediately, rotating...", trace_id);
                    last_error = UpstreamError::EmptyStream;
                    attempted_accounts.insert(email.clone());
                    attempt += 1;
                    grace_retry_used = false;
                    continue;
                },
            }
        }

        let (err_info, upstream_err) = extract_error_info(response).await;
        let status_code = err_info.status_code;
        let error_text = err_info.error_text;
        let retry_after = err_info.retry_after;
        last_error = upstream_err;

        error!("[OpenAI-Upstream] Error Response {}: {}", status_code, error_text);

        if let Some(new_grace) = handle_grace_retry(
            status_code,
            &error_text,
            grace_retry_used,
            token_manager.clone(),
            &email,
            &trace_id,
        )
        .await
        {
            grace_retry_used = new_grace;
            continue;
        }

        if handle_service_disabled(status_code, &error_text, token_manager.clone(), &email).await {
            attempted_accounts.insert(email.clone());
            attempt += 1;
            continue;
        }

        match handle_rate_limit_errors(
            status_code,
            &error_text,
            retry_after.as_deref(),
            token_manager.clone(),
            &state,
            &email,
            &session_id,
            &config.final_model,
            attempt,
            max_attempts,
        )
        .await
        {
            OpenAIErrorAction::Continue => {
                if status_code == 429
                    || status_code == 529
                    || status_code == 503
                    || status_code == 500
                {
                    attempted_accounts.insert(email.clone());
                    attempt += 1;
                    grace_retry_used = false;
                    continue;
                }
            },
            OpenAIErrorAction::ReturnError(code, email, text) => {
                return Ok((code, [(X_ACCOUNT_EMAIL, email)], text).into_response());
            },
        }

        if handle_auth_errors(
            status_code,
            token_manager.clone(),
            &email,
            &config.final_model,
            attempt,
            max_attempts,
        ) {
            attempted_accounts.insert(email.clone());
            attempt += 1;
            grace_retry_used = false;
            continue;
        }

        if status_code == 404 {
            warn!(
                "OpenAI Upstream 404 on account {} (model not available on this tier), rotating",
                email
            );
            attempted_accounts.insert(email.clone());
            attempt += 1;
            grace_retry_used = false;
            continue;
        }

        error!(
            "OpenAI Upstream non-retryable error {} on account {}: {}",
            status_code, email, error_text
        );
        return Ok((
            status,
            [(X_ACCOUNT_EMAIL, email.as_str())],
            sanitize_upstream_error(status_code, &error_text),
        )
            .into_response());
    }

    Ok(build_exhaustion_response(&last_error, last_email.as_deref()))
}
