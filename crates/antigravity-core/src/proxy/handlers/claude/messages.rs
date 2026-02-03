use crate::proxy::mappers::claude::{
    clean_cache_control_from_messages, close_tool_loop_for_thinking,
    filter_invalid_thinking_blocks_with_family, merge_consecutive_messages,
    transform_claude_request_in, transform_response, ClaudeRequest,
};
use crate::proxy::server::AppState;
use axum::http::HeaderMap;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};
use tokio::time::Duration;
use tracing::{debug, error, info};

use super::background_detection::{detect_background_task_type, select_background_model};
use super::dispatch::{decide_dispatch_mode, forward_to_zai};
use super::error_recovery::{
    apply_background_task_cleanup, apply_user_request_cleanup, handle_thinking_signature_error,
};
use super::preprocessing::{extract_meaningful_message, log_request_debug, log_request_info};
use super::request_validation::{
    all_retries_exhausted_error, generate_trace_id, parse_request, prompt_too_long_error,
};
use super::response_handler::{handle_nonstreaming_success, ResponseContext};
use super::retry_logic::{
    apply_retry_strategy, determine_retry_strategy, is_signature_error, should_rotate_account,
    RetryStrategy, MAX_RETRY_ATTEMPTS,
};
use super::streaming::{handle_streaming_response, StreamingContext, StreamResult};
use super::token_selection::{acquire_token, extract_session_id};
use super::warmup::{create_warmup_response, is_warmup_request};

pub async fn handle_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let force_account = headers
        .get("X-Force-Account")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    tracing::debug!(
        "handle_messages called. Body JSON len: {}",
        body.to_string().len()
    );

    let trace_id = generate_trace_id();

    let mut request: ClaudeRequest = match parse_request(body) {
        Ok(r) => r,
        Err(response) => return response,
    };

    // Decide whether to use z.ai (Anthropic passthrough) or Google flow
    let dispatch = decide_dispatch_mode(&state, &request, &trace_id).await;

    // [CRITICAL FIX] 预先清理所有消息中的 cache_control 字段 (Issue #744)
    // 必须在序列化之前处理，以确保 z.ai 和 Google Flow 都不受历史消息缓存标记干扰
    clean_cache_control_from_messages(&mut request.messages);

    // [FIX #813] 合并连续的同角色消息
    merge_consecutive_messages(&mut request.messages);

    // [CRITICAL FIX] 过滤并修复 Thinking 块签名 (with family compatibility check)
    filter_invalid_thinking_blocks_with_family(&mut request.messages, None);

    // [New] Recover from broken tool loops (where signatures were stripped)
    // This prevents "Assistant message must start with thinking" errors by closing the loop with synthetic messages
    if state.experimental.read().await.enable_tool_loop_recovery {
        close_tool_loop_for_thinking(&mut request.messages);
    }

    // ===== [Issue #467 Fix] 拦截 Claude Code Warmup 请求 =====
    // Claude Code 会每 10 秒发送一次 warmup 请求来保持连接热身，
    // 这些请求会消耗大量配额。检测到 warmup 请求后直接返回模拟响应。
    if is_warmup_request(&request) {
        tracing::info!(
            "[{}] 🔥 拦截 Warmup 请求，返回模拟响应（节省配额）",
            trace_id
        );
        return create_warmup_response(&request, request.stream);
    }

    if dispatch.use_zai {
        match forward_to_zai(&state, &headers, &request).await {
            Ok(response) => return response,
            Err(e) => {
                tracing::error!("{}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    // Google Flow continues with request object

    // [NEW] 获取上下文缩放配置
    let scaling_enabled = state.experimental.read().await.enable_usage_scaling;

    let latest_msg = extract_meaningful_message(&request);
    log_request_info(&trace_id, &request);
    log_request_debug(&trace_id, &request, &latest_msg);

    // 1. 获取 会话 ID (已废弃基于内容的哈希，改用 TokenManager 内部的时间窗口锁定)
    let _session_id: Option<&str> = None;

    // 2. 获取 UpstreamClient
    let upstream = state.upstream.clone();

    // 3. 准备闭包
    let mut request_for_body = request.clone();
    let token_manager = state.token_manager;

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
        // 2. 模型路由解析
        let (mut mapped_model, reason) = crate::proxy::common::resolve_model_route(
            &request_for_body.model,
            &*state.custom_mapping.read().await,
        );

        // 将 Claude 工具转为 Value 数组以便探测联网
        let tools_val: Option<Vec<Value>> = request_for_body.tools.as_ref().map(|list| {
            list.iter()
                .map(|t| serde_json::to_value(t).unwrap_or(json!({})))
                .collect()
        });

        let config = crate::proxy::mappers::request_config::resolve_request_config(
            &request_for_body.model,
            &mapped_model,
            &tools_val,
            None,
            None,
        );

        let session_id_str = extract_session_id(&request_for_body);
        let session_id = Some(session_id_str.as_str());

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

        let background_task_type = detect_background_task_type(&request_for_body);

        // 传递映射后的模型名
        let mut request_with_mapped = request_for_body.clone();

        if let Some(task_type) = background_task_type {
            let downgrade_model = select_background_model(task_type);
            apply_background_task_cleanup(
                &mut request_with_mapped,
                downgrade_model,
                &trace_id,
                &mapped_model,
            );
            mapped_model = downgrade_model.to_string();
        } else {
            apply_user_request_cleanup(&mut request_with_mapped, &trace_id, &mapped_model);
        }

        request_with_mapped.model = mapped_model.clone();

        // 生成 Trace ID (简单用时间戳后缀)
        // let _trace_id = format!("req_{}", chrono::Utc::now().timestamp_subsec_millis());

        let gemini_body = match transform_claude_request_in(
            &request_with_mapped,
            &project_id,
            retried_without_thinking,
        ) {
            Ok(b) => {
                debug!(
                    "[{}] Transformed Gemini Body: {}",
                    trace_id,
                    serde_json::to_string_pretty(&b).unwrap_or_default()
                );
                b
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "type": "error",
                        "error": {
                            "type": "api_error",
                            "message": format!("Transform error: {}", e)
                        }
                    })),
                )
                    .into_response();
            }
        };

        // 4. 上游调用 - 自动转换逻辑
        let client_wants_stream = request.stream;
        // [AUTO-CONVERSION] 非 Stream 请求自动转换为 Stream 以享受更宽松的配额
        let force_stream_internally = !client_wants_stream;
        let actual_stream = client_wants_stream || force_stream_internally;

        if force_stream_internally {
            info!(
                "[{}] 🔄 Auto-converting non-stream request to stream for better quota",
                trace_id
            );
        }

        let method = if actual_stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let query = if actual_stream { Some("alt=sse") } else { None };
        // [FIX #765] Prepare Beta Headers for Thinking + Tools
        let mut extra_headers = std::collections::HashMap::new();
        if request_with_mapped.thinking.is_some() && request_with_mapped.tools.is_some() {
            extra_headers.insert(
                "anthropic-beta".to_string(),
                "interleaved-thinking-2025-05-14".to_string(),
            );
            tracing::debug!(
                "[{}] Added Beta Header: interleaved-thinking-2025-05-14",
                trace_id
            );
        }

        // 5. 上游调用 - with WARP IP isolation
        // Get per-account SOCKS5 proxy for IP isolation
        let warp_proxy = state.warp_isolation.get_proxy_for_email(&email).await;
        if warp_proxy.is_some() {
            tracing::debug!("[{}] Using WARP proxy for account {}", trace_id, email);
        }

        let response = match upstream
            .call_v1_internal_with_warp(
                method,
                &access_token,
                gemini_body,
                query,
                extra_headers.clone(),
                warp_proxy.as_deref(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = e.clone();
                debug!(
                    "Request failed on attempt {}/{}: {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                attempt += 1;
                grace_retry_used = false;
                continue;
            }
        };

        let status = response.status();

        // 成功
        if status.is_success() {
            // [智能限流] 请求成功，重置该账号的连续失败计数
            token_manager.mark_account_success(&email);
            token_manager.clear_session_failures(&session_id_str);

            // [AIMD] 记录成功，用于预测性限流调整
            state.adaptive_limits.record_success(&email);

            // Determine context limit based on model
            let context_limit =
                crate::proxy::mappers::claude::token_scaling::get_context_limit_for_model(
                    &request_with_mapped.model,
                );

            let estimated_tokens = {
                use crate::proxy::mappers::context_manager::ContextManager;
                use crate::proxy::mappers::estimation_calibrator::get_calibrator;
                let raw_estimate = ContextManager::estimate_token_usage(&request);
                Some(get_calibrator().calibrate(raw_estimate))
            };

            // 处理流式响应
            if actual_stream {
                let ctx = StreamingContext {
                    trace_id: trace_id.clone(),
                    email: email.clone(),
                    session_id: session_id_str.clone(),
                    mapped_model: mapped_model.clone(),
                    reason: reason.clone(),
                    scaling_enabled,
                    context_limit,
                    estimated_tokens,
                    client_wants_stream,
                };
                match handle_streaming_response(response, &ctx).await {
                    StreamResult::Success(resp) => return resp,
                    StreamResult::Retry(err) => {
                        last_error = err;
                        attempt += 1;
                        grace_retry_used = false;
                        continue;
                    }
                }
            } else {
                // 处理非流式响应
                let bytes = match response.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        return (
                            StatusCode::BAD_GATEWAY,
                            format!("Failed to read body: {}", e),
                        )
                            .into_response();
                    }
                };

                // Debug print
                if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                    debug!("Upstream Response for Claude request: {}", text);
                }

                let gemini_resp: Value = match serde_json::from_slice(&bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        return (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e))
                            .into_response();
                    }
                };

                // 解包 response 字段（v1internal 格式）
                let raw = gemini_resp.get("response").unwrap_or(&gemini_resp);

                // 转换为 Gemini Response 结构
                let gemini_response: crate::proxy::mappers::claude::models::GeminiResponse =
                    match serde_json::from_value(raw.clone()) {
                        Ok(r) => r,
                        Err(e) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Convert error: {}", e),
                            )
                                .into_response();
                        }
                    };

                // Determine context limit based on model
                let context_limit =
                    crate::proxy::mappers::claude::token_scaling::get_context_limit_for_model(
                        &request_with_mapped.model,
                    );

                // 转换
                // [FIX #765] Pass session_id and model_name for signature caching
                let s_id_owned = session_id.map(|s| s.to_string());
                let claude_response = match transform_response(
                    &gemini_response,
                    scaling_enabled,
                    context_limit,
                    s_id_owned,
                    request_with_mapped.model.clone(),
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Transform error: {}", e),
                        )
                            .into_response();
                    }
                };

                // [Optimization] 记录闭环日志：消耗情况
                let cache_info = if let Some(cached) = claude_response.usage.cache_read_input_tokens
                {
                    format!(", Cached: {}", cached)
                } else {
                    String::new()
                };

                tracing::info!(
                    "[{}] Request finished. Model: {}, Tokens: In {}, Out {}{}",
                    trace_id,
                    request_with_mapped.model,
                    claude_response.usage.input_tokens,
                    claude_response.usage.output_tokens,
                    cache_info
                );

                return (
                    StatusCode::OK,
                    [
                        ("X-Account-Email", email.as_str()),
                        ("X-Mapped-Model", mapped_model.as_str()),
                        ("X-Mapping-Reason", reason.as_str()),
                    ],
                    Json(claude_response),
                )
                    .into_response();
            }
        }

        // 1. 立即提取状态码和 headers（防止 response 被 move）
        let status_code = status.as_u16();
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        // 2. 获取错误文本并转移 Response 所有权
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| format!("HTTP {}", status));
        last_error = format!("HTTP {}: {}", status_code, error_text);
        debug!("[{}] Upstream Error Response: {}", trace_id, error_text);

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
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        }

        // 3. 标记限流状态(用于 UI 显示) - 使用异步版本以支持实时配额刷新
        // 🆕 传入实际使用的模型,实现模型级别限流,避免不同模型配额互相影响
        if status_code == 429 || status_code == 529 || status_code == 503 || status_code == 500 {
            token_manager
                .mark_rate_limited_async(
                    &email,
                    status_code,
                    retry_after.as_deref(),
                    &error_text,
                    Some(&request_with_mapped.model),
                )
                .await;

            // Record session failure for consecutive failure tracking
            if status_code == 429 {
                token_manager.record_session_failure(&session_id_str);
                state.adaptive_limits.record_429(&email);
            } else {
                state.adaptive_limits.record_error(&email, status_code);
            }
        }

        // 4. 处理 400 错误 (Thinking 签名失效)
        // 由于已经主动过滤,这个错误应该很少发生
        if status_code == 400 && !retried_without_thinking && is_signature_error(&error_text) {
            handle_thinking_signature_error(&mut request_for_body, session_id, &trace_id);
            retried_without_thinking = true;

            if apply_retry_strategy(
                RetryStrategy::FixedDelay(Duration::from_millis(100)),
                attempt,
                status_code,
                &trace_id,
            )
            .await
            {
                continue;
            }
        }

        // 5. 统一处理所有可重试错误
        // [REMOVED] 不再特殊处理 QUOTA_EXHAUSTED,允许账号轮换
        // 原逻辑会在第一个账号配额耗尽时直接返回,导致"平衡"模式无法切换账号

        // 确定重试策略
        let strategy = determine_retry_strategy(status_code, &error_text, retried_without_thinking);

        // 执行退避
        if apply_retry_strategy(strategy, attempt, status_code, &trace_id).await {
            if should_rotate_account(status_code) {
                attempted_accounts.insert(email.clone());
                attempt += 1;
                grace_retry_used = false;
            }
            continue;
        } else {
            if status_code == 400
                && (error_text.contains("too long")
                    || error_text.contains("exceeds")
                    || error_text.contains("limit"))
            {
                return prompt_too_long_error(&email);
            }

            error!(
                "[{}] Non-retryable error {}: {}",
                trace_id, status_code, error_text
            );
            return (status, [("X-Account-Email", email.as_str())], error_text).into_response();
        }
    }

    all_retries_exhausted_error(max_attempts, &last_error, last_email.as_deref())
}

/// 列出可用模型
pub async fn handle_list_models(State(state): State<AppState>) -> impl IntoResponse {
    use crate::proxy::common::model_mapping::get_all_dynamic_models;

    let model_ids = get_all_dynamic_models(&state.custom_mapping).await;

    let data: Vec<_> = model_ids
        .into_iter()
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": 1706745600,
                "owned_by": "antigravity"
            })
        })
        .collect();

    Json(json!({
        "object": "list",
        "data": data
    }))
}

/// 计算 tokens (占位符)
pub async fn handle_count_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let zai = state.zai.read().await.clone();
    let zai_enabled =
        zai.enabled && !matches!(zai.dispatch_mode, crate::proxy::ZaiDispatchMode::Off);

    if zai_enabled {
        return crate::proxy::providers::zai_anthropic::forward_anthropic_json(
            &state,
            axum::http::Method::POST,
            "/v1/messages/count_tokens",
            &headers,
            body,
        )
        .await;
    }

    Json(json!({
        "input_tokens": 0,
        "output_tokens": 0
    }))
    .into_response()
}
