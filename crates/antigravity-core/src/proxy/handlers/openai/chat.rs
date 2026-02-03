use super::super::retry_strategy::{peek_first_data_chunk, PeekConfig, PeekResult};
use super::responses_format::{convert_responses_to_chat, is_responses_format};
use super::MAX_RETRY_ATTEMPTS;
use axum::http::HeaderMap;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use bytes::Bytes;
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::proxy::mappers::openai::{
    transform_openai_request, transform_openai_response, OpenAIRequest,
};
use crate::proxy::server::AppState;

use crate::proxy::session_manager::SessionManager;

pub async fn handle_chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let force_account = headers
        .get("X-Force-Account")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // [NEW] 自动检测并转换 Responses 格式
    // 如果请求包含 instructions 或 input 但没有 messages，则认为是 Responses 格式
    if is_responses_format(&body) {
        convert_responses_to_chat(&mut body);
    }

    let mut openai_req: OpenAIRequest = serde_json::from_value(body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;

    // Safety: Ensure messages is not empty
    if openai_req.messages.is_empty() {
        debug!("Received request with empty messages, injecting fallback...");
        openai_req
            .messages
            .push(crate::proxy::mappers::openai::OpenAIMessage {
                role: "user".to_string(),
                content: Some(crate::proxy::mappers::openai::OpenAIContent::String(
                    " ".to_string(),
                )),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
    }

    debug!("Received OpenAI request for model: {}", openai_req.model);

    // 1. 获取 UpstreamClient (Clone handle)
    let upstream = state.upstream.clone();
    let token_manager = state.token_manager;
    let pool_size = token_manager.len();
    let max_attempts = MAX_RETRY_ATTEMPTS.min(pool_size).max(1);

    let mut last_error = String::new();
    let mut last_email: Option<String> = None;
    let trace_id = format!("oai_{}", chrono::Utc::now().timestamp_subsec_millis());
    let mut grace_retry_used = false;
    let mut attempt = 0usize;
    let mut attempted_accounts: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    while attempt < max_attempts {
        // 2. 模型路由解析
        let (mapped_model, reason) = crate::proxy::common::resolve_model_route(
            &openai_req.model,
            &*state.custom_mapping.read().await,
        );
        // 将 OpenAI 工具转为 Value 数组以便探测联网
        let tools_val: Option<Vec<Value>> = openai_req.tools.as_ref().map(|list| list.to_vec());
        let config = crate::proxy::mappers::request_config::resolve_request_config(
            &openai_req.model,
            &mapped_model,
            &tools_val,
            None,
            None,
        );

        // 3. 提取 SessionId (粘性指纹)
        let session_id = SessionManager::extract_openai_session_id(&openai_req);

        let (access_token, project_id, email, _active_guard) =
            if let Some(ref forced) = force_account {
                match token_manager
                    .get_token_forced(forced, &config.final_model)
                    .await
                {
                    Ok((token, email, project, guard)) => (token, project, email, guard),
                    Err(e) => {
                        warn!(
                            "[OpenAI] Forced account {} failed: {}, using smart routing",
                            forced, e
                        );
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
                                ));
                            }
                        }
                    }
                }
            } else {
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
                        ));
                    }
                }
            };

        last_email = Some(email.clone());
        info!("✓ Using account: {} (type: {})", email, config.request_type);

        let gemini_body = transform_openai_request(&openai_req, &project_id, &mapped_model);

        // [New] 打印转换后的报文 (Gemini Body) 供调试
        if let Ok(body_json) = serde_json::to_string_pretty(&gemini_body) {
            debug!("[OpenAI-Request] Transformed Gemini Body:\n{}", body_json);
        }

        // 5. 发送请求 - 自动转换逻辑
        let client_wants_stream = openai_req.stream;
        // [AUTO-CONVERSION] 非 Stream 请求自动转换为 Stream 以享受更宽松的配额
        let force_stream_internally = !client_wants_stream;
        let actual_stream = client_wants_stream || force_stream_internally;

        if force_stream_internally {
            info!("[OpenAI] 🔄 Auto-converting non-stream request to stream for better quota");
        }

        let method = if actual_stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let query_string = if actual_stream { Some("alt=sse") } else { None };

        // Get per-account WARP proxy for IP isolation
        let warp_proxy = state.warp_isolation.get_proxy_for_email(&email).await;

        // [ARCH FIX] Inner retry loop for transient errors (503)
        // This doesn't consume main attempt counter - retries on same account
        let mut inner_retries = 0u8;
        let response = loop {
            let gemini_body_clone = gemini_body.clone();
            match upstream
                .call_v1_internal_with_warp(
                    method,
                    &access_token,
                    gemini_body_clone,
                    query_string,
                    std::collections::HashMap::new(),
                    warp_proxy.as_deref(),
                )
                .await
            {
                Ok(r) => {
                    let status = r.status();
                    // 503 = server overload, retry on same account
                    if status.as_u16() == 503 && inner_retries < 5 {
                        inner_retries += 1;
                        let delay = 300 * (1u64 << inner_retries.min(3)); // 600, 1200, 2400, 2400, 2400
                        tracing::warn!(
                            "503 server overload on {}, inner retry {}/5 in {}ms",
                            email,
                            inner_retries,
                            delay
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        continue;
                    }
                    break r;
                }
                Err(e) => {
                    last_error = e.clone();
                    debug!(
                        "OpenAI Request failed on attempt {}/{}: {}",
                        attempt + 1,
                        max_attempts,
                        e
                    );
                    break reqwest::Response::from(
                        http::Response::builder()
                            .status(500)
                            .body(format!("Connection error: {}", e))
                            .unwrap(),
                    );
                }
            }
        };

        // Check if we got a fake error response from connection failure
        if response.status().as_u16() == 500 && !last_error.is_empty() {
            attempted_accounts.insert(email.clone());
            attempt += 1;
            grace_retry_used = false;
            continue;
        }

        let status = response.status();
        if status.is_success() {
            // [AIMD] 记录成功，用于预测性限流调整
            state.adaptive_limits.record_success(&email);
            token_manager.clear_session_failures(&session_id);

            // 5. 处理流式 vs 非流式
            if actual_stream {
                use crate::proxy::mappers::openai::streaming::create_openai_sse_stream;
                use axum::body::Body;
                use axum::response::Response;
                use futures::StreamExt;

                let gemini_stream = response.bytes_stream();
                let openai_stream = create_openai_sse_stream(
                    Box::pin(gemini_stream),
                    openai_req.model.clone(),
                    None,
                );

                // [FIX #859] Enhanced Peek logic to handle heartbeats and slow start
                let peek_config = PeekConfig::openai();
                let (first_data_chunk, openai_stream) =
                    match peek_first_data_chunk(openai_stream, &peek_config, &trace_id).await {
                        PeekResult::Data(bytes, stream) => (Some(bytes), stream),
                        PeekResult::Retry(err) => {
                            last_error = err;
                            attempted_accounts.insert(email.clone());
                            attempt += 1;
                            grace_retry_used = false;
                            continue;
                        }
                    };

                match first_data_chunk {
                    Some(bytes) => {
                        // We have data! Construct the combined stream
                        let stream_rest = openai_stream;
                        let combined_stream =
                            Box::pin(futures::stream::once(async move { Ok(bytes) }).chain(
                                stream_rest.map(|result| -> Result<Bytes, std::io::Error> {
                                    match result {
                                        Ok(b) => Ok(b),
                                        Err(e) => {
                                            let err_str = e.to_string();
                                            let user_message = if err_str.contains("decoding") || err_str.contains("hyper") {
                                                "Upstream server closed connection (overload). Please retry your request."
                                            } else {
                                                "Stream interrupted by upstream. Please retry your request."
                                            };
                                            tracing::warn!("Stream error during transmission: {} (user msg: {})", err_str, user_message);
                                            // Return error in OpenAI format with retriable code + [DONE]
                                            Ok(Bytes::from(format!(
                                                "data: {{\"error\":{{\"message\":\"{}\",\"type\":\"server_error\",\"code\":\"overloaded\",\"param\":null}}}}\n\ndata: [DONE]\n\n",
                                                user_message
                                            )))
                                        }
                                    }
                                }),
                            ));

                        // 判断客户端期望的格式
                        if client_wants_stream {
                            // 客户端本就要 Stream，直接返回 SSE
                            let body = Body::from_stream(combined_stream);
                            return Ok(Response::builder()
                                .header("Content-Type", "text/event-stream")
                                .header("Cache-Control", "no-cache")
                                .header("Connection", "keep-alive")
                                .header("X-Account-Email", &email)
                                .header("X-Mapped-Model", &mapped_model)
                                .header("X-Mapping-Reason", &reason)
                                .body(body)
                                .expect("valid streaming response")
                                .into_response());
                        } else {
                            // 客户端要非 Stream，需要收集完整响应并转换为 JSON
                            use crate::proxy::mappers::openai::collect_openai_stream_to_json;

                            match collect_openai_stream_to_json(combined_stream).await {
                                Ok(full_response) => {
                                    info!("[OpenAI] ✓ Stream collected and converted to JSON");
                                    return Ok((
                                        StatusCode::OK,
                                        [
                                            ("X-Account-Email", email.as_str()),
                                            ("X-Mapped-Model", mapped_model.as_str()),
                                            ("X-Mapping-Reason", reason.as_str()),
                                        ],
                                        Json(full_response),
                                    )
                                        .into_response());
                                }
                                Err(e) => {
                                    return Err((
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        format!("Stream collection error: {}", e),
                                    ));
                                }
                            }
                        }
                    }
                    None => {
                        tracing::warn!(
                            "[{}] Stream ended immediately (Empty Response), rotating...",
                            trace_id
                        );
                        last_error = "Empty response stream (None)".to_string();
                        attempted_accounts.insert(email.clone());
                        attempt += 1;
                        grace_retry_used = false;
                        continue;
                    }
                }
            }

            let gemini_resp: Value = response
                .json()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

            let openai_response = transform_openai_response(&gemini_resp);
            return Ok((
                StatusCode::OK,
                [
                    ("X-Account-Email", email.as_str()),
                    ("X-Mapped-Model", mapped_model.as_str()),
                    ("X-Mapping-Reason", reason.as_str()),
                ],
                Json(openai_response),
            )
                .into_response());
        }

        // 处理特定错误并重试
        let status_code = status.as_u16();
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| format!("HTTP {}", status_code));
        last_error = format!("HTTP {}: {}", status_code, error_text);

        // [New] 打印错误报文日志
        tracing::error!(
            "[OpenAI-Upstream] Error Response {}: {}",
            status_code,
            error_text
        );

        // [Grace Retry] For transient 429 (RATE_LIMIT_EXCEEDED), retry once on same account before rotation
        if status_code == 429 && !grace_retry_used {
            let reason = token_manager
                .rate_limit_tracker()
                .parse_rate_limit_reason(&error_text);
            if reason == crate::proxy::rate_limit::RateLimitReason::RateLimitExceeded {
                grace_retry_used = true;
                tracing::info!(
                    "[{}] 🔄 Grace retry: RATE_LIMIT_EXCEEDED on {}, waiting 1s before retry on same account",
                    trace_id, email
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        }

        if status_code == 403
            && (error_text.contains("SERVICE_DISABLED")
                || error_text.contains("CONSUMER_INVALID")
                || error_text.contains("Permission denied on resource project")
                || error_text.contains("verify your account"))
        {
            warn!(
                "[OpenAI] 🚫 Account {} needs verification or has project issue. 1h lockout.",
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
                let _ =
                    crate::modules::account::mark_needs_verification_by_email(&email_clone).await;
            });
            attempted_accounts.insert(email.clone());
            attempt += 1;
            continue;
        }

        // 429/529/503/500 — rotate to next account (503 already retried 5x in inner loop)
        if status_code == 429 || status_code == 529 || status_code == 503 || status_code == 500 {
            token_manager
                .mark_rate_limited_async(
                    &email,
                    status_code,
                    retry_after.as_deref(),
                    &error_text,
                    Some(&config.final_model),
                )
                .await;

            if status_code == 429 {
                token_manager.record_session_failure(&session_id);
                state.adaptive_limits.record_429(&email);
            } else {
                state.adaptive_limits.record_error(&email, status_code);
            }

            // 1. 优先尝试解析 RetryInfo (由 Google Cloud 直接下发)
            if let Some(delay_ms) = crate::proxy::upstream::retry::parse_retry_delay(&error_text) {
                let actual_delay = delay_ms.saturating_add(200).min(10_000);
                tracing::warn!(
                    "OpenAI Upstream {} on {} attempt {}/{}, waiting {}ms then rotating",
                    status_code,
                    email,
                    attempt + 1,
                    max_attempts,
                    actual_delay
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(actual_delay)).await;
                attempted_accounts.insert(email.clone());
                attempt += 1;
                grace_retry_used = false;
                continue;
            }

            // 2. 只有明确包含 "QUOTA_EXHAUSTED" 才停止，避免误判频率提示 (如 "check quota")
            if error_text.contains("QUOTA_EXHAUSTED") {
                error!(
                    "OpenAI Quota exhausted (429) on account {} attempt {}/{}, stopping to protect pool.",
                    email,
                    attempt + 1,
                    max_attempts
                );
                return Ok(
                    (status, [("X-Account-Email", email.as_str())], error_text).into_response()
                );
            }

            // 3. 其他限流或服务器过载情况，轮换账号
            tracing::warn!(
                "OpenAI Upstream {} on {} attempt {}/{}, rotating account",
                status_code,
                email,
                attempt + 1,
                max_attempts
            );
            attempted_accounts.insert(email.clone());
            attempt += 1;
            grace_retry_used = false;
            continue;
        }

        // 只有 403 (权限/地区限制) 和 401 (认证失效) 触发账号轮换
        if status_code == 403 || status_code == 401 {
            // [FIX] Temporarily lock this account for this model so retry picks different account
            token_manager.rate_limit_tracker().set_model_lockout(
                &email,
                &config.final_model,
                std::time::SystemTime::now() + std::time::Duration::from_secs(30),
                crate::proxy::rate_limit::RateLimitReason::ServerError,
            );
            tracing::warn!(
                "OpenAI Upstream {} on account {} attempt {}/{}, locking for 30s and rotating",
                status_code,
                email,
                attempt + 1,
                max_attempts
            );
            attempted_accounts.insert(email.clone());
            attempt += 1;
            grace_retry_used = false;
            continue;
        }

        // 404 等由于模型配置或路径错误的 HTTP 异常，直接报错，不进行无效轮换
        error!(
            "OpenAI Upstream non-retryable error {} on account {}: {}",
            status_code, email, error_text
        );
        return Ok((status, [("X-Account-Email", email.as_str())], error_text).into_response());
    }

    // 所有尝试均失败
    if let Some(email) = last_email {
        Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [("X-Account-Email", email)],
            format!("All accounts exhausted. Last error: {}", last_error),
        )
            .into_response())
    } else {
        Ok((
            StatusCode::TOO_MANY_REQUESTS,
            format!("All accounts exhausted. Last error: {}", last_error),
        )
            .into_response())
    }
}
