use crate::proxy::mappers::claude::{
    clean_cache_control_from_messages, close_tool_loop_for_thinking,
    filter_invalid_thinking_blocks_with_family, merge_consecutive_messages, ClaudeRequest,
};
use crate::proxy::server::AppState;
use axum::http::{HeaderMap, StatusCode};
use axum::{
    extract::{Json, State},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};
use tracing::{debug, info};

use super::dispatch::{decide_dispatch_mode, forward_to_zai};
use super::error_handling::{handle_upstream_error, ClaudeErrorAction, ErrorContext};
use super::preprocessing::{extract_meaningful_message, log_request_debug, log_request_info};
use super::request_preparation::prepare_request;
use super::request_validation::{all_retries_exhausted_error, generate_trace_id, parse_request};
use super::response_handler::{handle_nonstreaming_success, ResponseContext};
use super::retry_logic::MAX_RETRY_ATTEMPTS;
use super::streaming::{handle_streaming_response, ClaudeStreamResult, StreamingContext};
use super::token_selection::acquire_token;
use super::upstream_call::prepare_upstream_call;
use super::warmup::{create_warmup_response, is_warmup_request};
use crate::proxy::session_manager::SessionManager;

pub async fn handle_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let force_account =
        headers.get("X-Force-Account").and_then(|v| v.to_str().ok()).map(|s| s.to_string());

    tracing::debug!("handle_messages called. Body JSON len: {}", body.to_string().len());

    let trace_id = generate_trace_id();

    let mut request: ClaudeRequest = match parse_request(body) {
        Ok(r) => r,
        Err(response) => return response,
    };

    let dispatch = decide_dispatch_mode(&state, &request, &trace_id).await;

    clean_cache_control_from_messages(&mut request.messages);
    merge_consecutive_messages(&mut request.messages);
    filter_invalid_thinking_blocks_with_family(&mut request.messages, None);

    if state.experimental.read().await.enable_tool_loop_recovery {
        close_tool_loop_for_thinking(&mut request.messages);
    }

    if is_warmup_request(&request) {
        tracing::info!("[{}] Intercepted warmup request", trace_id);
        return create_warmup_response(&request, request.stream);
    }

    if dispatch.use_zai {
        match forward_to_zai(&state, &headers, &request).await {
            Ok(response) => return response,
            Err(e) => {
                tracing::error!("{}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            },
        }
    }

    let scaling_enabled = state.experimental.read().await.enable_usage_scaling;

    let latest_msg = extract_meaningful_message(&request);
    log_request_info(&trace_id, &request);
    log_request_debug(&trace_id, &request, &latest_msg);

    let upstream = state.upstream.clone();
    let mut request_for_body = request.clone();
    let token_manager = state.token_manager.clone();

    let pool_size = token_manager.len();
    let max_attempts = MAX_RETRY_ATTEMPTS.min(pool_size).max(1);

    let mut last_error = String::new();
    let mut retried_without_thinking = false;
    let mut last_email: Option<String> = None;
    let mut grace_retry_used = false;
    let mut attempt = 0usize;
    let mut attempted_accounts: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    while attempt < max_attempts {
        let session_id_str = SessionManager::extract_session_id(&request_for_body);
        let session_id = Some(session_id_str.as_str());

        let (mapped_model_temp, reason) = match crate::proxy::common::resolve_model_route(
            &request_for_body.model,
            &*state.custom_mapping.read().await,
        ) {
            Ok(result) => result,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": {"message": e, "type": "invalid_request_error"}})),
                )
                    .into_response()
            },
        };

        let tools_val: Option<Vec<Value>> = request_for_body
            .tools
            .as_ref()
            .map(|list| list.iter().filter_map(|t| serde_json::to_value(t).ok()).collect());

        let config = crate::proxy::mappers::request_config::resolve_request_config(
            &request_for_body.model,
            &mapped_model_temp,
            &tools_val,
            None,
            None,
        );

        let force_rotate_token = attempt > 0;
        let token_result = match acquire_token(
            token_manager.clone(),
            force_account.as_deref(),
            &config.request_type,
            &config.final_model,
            session_id,
            force_rotate_token,
            &attempted_accounts,
        )
        .await
        {
            Ok(r) => r,
            Err(response) => return response,
        };
        let access_token = token_result.access_token;
        let project_id = token_result.project_id;
        let email = token_result.email;
        let _guard = token_result.guard;

        last_email = Some(email.clone());
        info!("✓ Using account: {} (type: {})", email, config.request_type);

        let prepared = match prepare_request(
            &state,
            &request_for_body,
            &project_id,
            &trace_id,
            retried_without_thinking,
        )
        .await
        {
            Ok(p) => p,
            Err(response) => return response,
        };
        let mapped_model = prepared.mapped_model;
        let request_with_mapped = prepared.request_with_mapped;
        let gemini_body = prepared.gemini_body;

        let call_config = prepare_upstream_call(&request, &request_with_mapped, &trace_id);

        let warp_proxy = state.warp_isolation.get_proxy_for_email(&email).await;
        if warp_proxy.is_some() {
            tracing::debug!("[{}] Using WARP proxy for account {}", trace_id, email);
        }

        let response = match upstream
            .call_v1_internal_with_warp(
                call_config.method,
                &access_token,
                gemini_body,
                call_config.query,
                call_config.extra_headers.clone(),
                warp_proxy.as_deref(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = e.clone();
                debug!("Request failed on attempt {}/{}: {}", attempt + 1, max_attempts, e);
                attempt += 1;
                grace_retry_used = false;
                continue;
            },
        };

        let status = response.status();

        // success
        if status.is_success() {
            token_manager.mark_account_success(&email);
            token_manager.clear_session_failures(&session_id_str);
            state.adaptive_limits.record_success(&email);

            let context_limit =
                crate::proxy::mappers::claude::token_scaling::get_context_limit_for_model(
                    &request_with_mapped.model,
                );

            if call_config.actual_stream {
                let estimated_tokens = {
                    use crate::proxy::mappers::context_manager::ContextManager;
                    use crate::proxy::mappers::estimation_calibrator::get_calibrator;
                    let raw_estimate = ContextManager::estimate_token_usage(&request);
                    Some(get_calibrator().calibrate(raw_estimate))
                };
                let ctx = StreamingContext {
                    trace_id: trace_id.clone(),
                    email: email.clone(),
                    session_id: session_id_str.clone(),
                    mapped_model: mapped_model.clone(),
                    reason: reason.clone(),
                    scaling_enabled,
                    context_limit,
                    estimated_tokens,
                    client_wants_stream: call_config.client_wants_stream,
                };
                match handle_streaming_response(response, &ctx).await {
                    ClaudeStreamResult::Success(resp) => return resp,
                    ClaudeStreamResult::Retry(err) => {
                        last_error = err;
                        attempt += 1;
                        grace_retry_used = false;
                        continue;
                    },
                }
            } else {
                let ctx = ResponseContext {
                    trace_id: trace_id.clone(),
                    email: email.clone(),
                    session_id: session_id_str.clone(),
                    mapped_model: mapped_model.clone(),
                    reason: reason.clone(),
                    scaling_enabled,
                    context_limit,
                };
                return handle_nonstreaming_success(response, &request_with_mapped, &ctx).await;
            }
        }

        let status_code = status.as_u16();
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let error_text = response.text().await.unwrap_or_else(|_| format!("HTTP {}", status));
        last_error = format!("HTTP {}: {}", status_code, error_text);
        debug!("[{}] Upstream Error Response: {}", trace_id, error_text);

        let err_ctx = ErrorContext {
            status,
            status_code,
            error_text,
            retry_after,
            email: &email,
            session_id_str: &session_id_str,
            model: &request_with_mapped.model,
            trace_id: &trace_id,
            attempt,
        };

        match handle_upstream_error(
            &state,
            token_manager.clone(),
            &err_ctx,
            &mut request_for_body,
            &mut attempted_accounts,
            &mut retried_without_thinking,
            &mut grace_retry_used,
        )
        .await
        {
            ClaudeErrorAction::Retry => {
                attempt += 1;
                continue;
            },
            ClaudeErrorAction::Return(resp) => return resp,
        }
    }

    all_retries_exhausted_error(max_attempts, &last_error, last_email.as_deref())
}
