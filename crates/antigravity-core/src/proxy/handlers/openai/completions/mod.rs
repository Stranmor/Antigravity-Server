// OpenAI legacy completions handler
mod codex_parser;
mod response_mapper;
mod streaming_handler;

use super::*;

/// 处理 Legacy Completions API (/v1/completions)
/// 将 Prompt 转换为 Chat Message 格式，复用 handle_chat_completions
pub async fn handle_completions(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        "Received /v1/completions or /v1/responses payload: {:?}",
        body
    );

    let is_codex_style = body.get("input").is_some() || body.get("instructions").is_some();

    // Convert Codex-style or legacy prompt to messages format
    if is_codex_style {
        let instructions = body
            .get("instructions")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let input_items = body.get("input").and_then(|v| v.as_array());
        let messages = codex_parser::parse_codex_input_to_messages(instructions, input_items);

        if let Some(obj) = body.as_object_mut() {
            obj.insert("messages".to_string(), json!(messages));
        }
    } else if let Some(prompt_val) = body.get("prompt") {
        // Legacy OpenAI Style: prompt -> Chat
        let prompt_str = match prompt_val {
            Value::String(s) => s.clone(),
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => prompt_val.to_string(),
        };
        let messages = json!([ { "role": "user", "content": prompt_str } ]);
        if let Some(obj) = body.as_object_mut() {
            obj.remove("prompt");
            obj.insert("messages".to_string(), messages);
        }
    }

    let mut openai_req: OpenAIRequest = serde_json::from_value(body.clone())
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;

    // Safety: Inject empty message if needed
    if openai_req.messages.is_empty() {
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

    let upstream = state.upstream.clone();
    let token_manager = state.token_manager;
    let pool_size = token_manager.len();
    let max_attempts = MAX_RETRY_ATTEMPTS.min(pool_size).max(1);

    let mut last_error = String::new();

    let mut last_email: Option<String> = None;
    let trace_id = format!("req_{}", chrono::Utc::now().timestamp_subsec_millis());
    let mut grace_retry_used = false;

    for attempt in 0..max_attempts {
        // 1. 模型路由解析
        let (mapped_model, _reason) = crate::proxy::common::resolve_model_route(
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

        // 3. 提取 SessionId (复用)
        // [New] 使用 TokenManager 内部逻辑提取 session_id，支持粘性调度
        let session_id_str = SessionManager::extract_openai_session_id(&openai_req);
        let session_id = Some(session_id_str.as_str());

        // 重试时强制轮换，除非只是简单的网络抖动但 Claude 逻辑里 attempt > 0 总是 force_rotate
        let force_rotate = attempt > 0;

        let (access_token, project_id, email, _guard) = match token_manager
            .get_token(
                &config.request_type,
                force_rotate,
                session_id,
                &config.final_model,
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
        };

        last_email = Some(email.clone());
        info!("✓ Using account: {} (type: {})", email, config.request_type);

        let gemini_body = transform_openai_request(&openai_req, &project_id, &mapped_model);

        // [New] 打印转换后的报文 (Gemini Body) 供调试 (Codex 路径) ———— 缩减为 simple debug
        debug!(
            "[Codex-Request] Transformed Gemini Body ({} parts)",
            gemini_body
                .get("contents")
                .and_then(|c| c.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
        );

        let list_response = openai_req.stream;
        let method = if list_response {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let query_string = if list_response { Some("alt=sse") } else { None };

        // Get per-account WARP proxy for IP isolation
        let warp_proxy = state.warp_isolation.get_proxy_for_email(&email).await;

        let response = match upstream
            .call_v1_internal_with_warp(
                method,
                &access_token,
                gemini_body,
                query_string,
                std::collections::HashMap::new(),
                warp_proxy.as_deref(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = e.clone();
                debug!(
                    "Codex Request failed on attempt {}/{}: {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                continue;
            }
        };

        let status = response.status();
        if status.is_success() {
            // [智能限流] 请求成功，重置该账号的连续失败计数
            token_manager.mark_account_success(&email);
            token_manager.clear_session_failures(&session_id_str);

            // [AIMD] 记录成功，用于预测性限流调整
            state.adaptive_limits.record_success(&email);

            if list_response {
                let gemini_stream = response.bytes_stream();
                let sse_stream: std::pin::Pin<
                    Box<dyn futures::Stream<Item = Result<Bytes, String>> + Send>,
                > = if is_codex_style {
                    use crate::proxy::mappers::openai::streaming::create_openai_sse_stream;
                    Box::pin(create_openai_sse_stream(
                        Box::pin(gemini_stream),
                        openai_req.model.clone(),
                        None,
                    ))
                } else {
                    use crate::proxy::mappers::openai::streaming::create_legacy_sse_stream;
                    Box::pin(create_legacy_sse_stream(
                        Box::pin(gemini_stream),
                        openai_req.model.clone(),
                    ))
                };

                let peek_config = PeekConfig::openai();
                #[allow(clippy::type_complexity)]
                let (first_data_chunk, sse_stream): (
                    Option<Bytes>,
                    std::pin::Pin<Box<dyn futures::Stream<Item = Result<Bytes, String>> + Send>>,
                ) = match peek_first_data_chunk(sse_stream, &peek_config, &trace_id).await {
                    PeekResult::Data(bytes, stream) => (Some(bytes), stream),
                    PeekResult::Retry(err) => {
                        last_error = err;
                        continue;
                    }
                };

                match first_data_chunk {
                    Some(bytes) => {
                        return Ok(streaming_handler::build_streaming_response(
                            bytes,
                            sse_stream,
                            &email,
                            &mapped_model,
                        )
                        .into_response());
                    }
                    None => {
                        tracing::warn!(
                            "[{}] Stream ended immediately (Empty Response), retrying...",
                            trace_id
                        );
                        last_error = "Empty response stream (None)".to_string();
                        continue;
                    }
                }
            }

            let gemini_resp: Value = response
                .json()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

            let chat_resp = transform_openai_response(&gemini_resp);
            let legacy_resp = response_mapper::map_chat_to_legacy_response(&chat_resp);

            return Ok(axum::Json(legacy_resp).into_response());
        }

        // Handle errors and retry
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

        tracing::error!(
            "[Codex-Upstream] Error Response {}: {}",
            status_code,
            error_text
        );

        // [Grace Retry] For transient 429 (RATE_LIMIT_EXCEEDED), retry once on same account
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

        // 3. 标记限流状态(用于 UI 显示)
        if status_code == 429 || status_code == 529 || status_code == 503 || status_code == 500 {
            // 记录限流信息 (全局同步)
            token_manager
                .mark_rate_limited_async(
                    &email,
                    status_code,
                    retry_after.as_deref(),
                    &error_text,
                    Some(&mapped_model),
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

        // 确定重试策略
        let strategy = determine_retry_strategy(status_code, &error_text, false);

        if apply_retry_strategy(
            strategy,
            attempt,
            MAX_RETRY_ATTEMPTS,
            status_code,
            &trace_id,
        )
        .await
        {
            // 继续重试 (loop 会增加 attempt, 导致 force_rotate=true)
            continue;
        } else {
            // 不可重试
            return Ok((status, [("X-Account-Email", email.as_str())], error_text).into_response());
        }
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
