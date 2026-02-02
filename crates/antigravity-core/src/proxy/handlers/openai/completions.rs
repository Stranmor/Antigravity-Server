// OpenAI legacy completions handler
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

    // 1. Convert Payload to Messages (Shared Chat Format)
    if is_codex_style {
        let instructions = body
            .get("instructions")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let input_items = body.get("input").and_then(|v| v.as_array());

        let mut messages = Vec::new();

        // System Instructions
        if !instructions.is_empty() {
            messages.push(json!({ "role": "system", "content": instructions }));
        }

        let mut call_id_to_name = std::collections::HashMap::new();

        // Pass 1: Build Call ID to Name Map
        if let Some(items) = input_items {
            for item in items {
                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match item_type {
                    "function_call" | "local_shell_call" | "web_search_call" => {
                        let call_id = item
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .or_else(|| item.get("id").and_then(|v| v.as_str()))
                            .unwrap_or("unknown");

                        let name = if item_type == "local_shell_call" {
                            "shell"
                        } else if item_type == "web_search_call" {
                            "google_search"
                        } else {
                            item.get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        };

                        call_id_to_name.insert(call_id.to_string(), name.to_string());
                        tracing::debug!("Mapped call_id {} to name {}", call_id, name);
                    }
                    _ => {}
                }
            }
        }

        // Pass 2: Map Input Items to Messages
        if let Some(items) = input_items {
            for item in items {
                let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match item_type {
                    "message" => {
                        let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                        let content = item.get("content").and_then(|v| v.as_array());
                        let mut text_parts = Vec::new();
                        let mut image_parts: Vec<Value> = Vec::new();

                        if let Some(parts) = content {
                            for part in parts {
                                // 处理文本块
                                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                    text_parts.push(text.to_string());
                                }
                                // [NEW] 处理图像块 (Codex input_image 格式)
                                else if part.get("type").and_then(|v| v.as_str())
                                    == Some("input_image")
                                {
                                    if let Some(image_url) =
                                        part.get("image_url").and_then(|v| v.as_str())
                                    {
                                        image_parts.push(json!({
                                            "type": "image_url",
                                            "image_url": { "url": image_url }
                                        }));
                                        debug!("[Codex] Found input_image: {}", image_url);
                                    }
                                }
                                // [NEW] 兼容标准 OpenAI image_url 格式
                                else if part.get("type").and_then(|v| v.as_str())
                                    == Some("image_url")
                                {
                                    if let Some(url_obj) = part.get("image_url") {
                                        image_parts.push(json!({
                                            "type": "image_url",
                                            "image_url": url_obj.clone()
                                        }));
                                    }
                                }
                            }
                        }

                        // 构造消息内容：如果有图像则使用数组格式
                        if image_parts.is_empty() {
                            messages.push(json!({
                                "role": role,
                                "content": text_parts.join("\n")
                            }));
                        } else {
                            let mut content_blocks: Vec<Value> = Vec::new();
                            if !text_parts.is_empty() {
                                content_blocks.push(json!({
                                    "type": "text",
                                    "text": text_parts.join("\n")
                                }));
                            }
                            content_blocks.extend(image_parts);
                            messages.push(json!({
                                "role": role,
                                "content": content_blocks
                            }));
                        }
                    }
                    "function_call" | "local_shell_call" | "web_search_call" => {
                        let mut name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let mut args_str = item
                            .get("arguments")
                            .and_then(|v| v.as_str())
                            .unwrap_or("{}")
                            .to_string();
                        let call_id = item
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .or_else(|| item.get("id").and_then(|v| v.as_str()))
                            .unwrap_or("unknown");

                        // Handle native shell calls
                        if item_type == "local_shell_call" {
                            name = "shell";
                            if let Some(action) = item.get("action") {
                                if let Some(exec) = action.get("exec") {
                                    // Map to ShellCommandToolCallParams (string command) or ShellToolCallParams (array command)
                                    // Most LLMs prefer a single string for shell
                                    let mut args_obj = serde_json::Map::new();
                                    if let Some(cmd) = exec.get("command") {
                                        // CRITICAL FIX: The 'shell' tool schema defines 'command' as an ARRAY of strings.
                                        // We MUST pass it as an array, not a joined string, otherwise Gemini rejects with 400 INVALID_ARGUMENT.
                                        let cmd_val = if cmd.is_string() {
                                            json!([cmd]) // Wrap in array
                                        } else {
                                            cmd.clone() // Assume already array
                                        };
                                        args_obj.insert("command".to_string(), cmd_val);
                                    }
                                    if let Some(wd) =
                                        exec.get("working_directory").or(exec.get("workdir"))
                                    {
                                        args_obj.insert("workdir".to_string(), wd.clone());
                                    }
                                    args_str = serde_json::to_string(&args_obj)
                                        .unwrap_or("{}".to_string());
                                }
                            }
                        } else if item_type == "web_search_call" {
                            name = "google_search";
                            if let Some(action) = item.get("action") {
                                let mut args_obj = serde_json::Map::new();
                                if let Some(q) = action.get("query") {
                                    args_obj.insert("query".to_string(), q.clone());
                                }
                                args_str =
                                    serde_json::to_string(&args_obj).unwrap_or("{}".to_string());
                            }
                        }

                        messages.push(json!({
                            "role": "assistant",
                            "tool_calls": [
                                {
                                    "id": call_id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": args_str
                                    }
                                }
                            ]
                        }));
                    }
                    "function_call_output" | "custom_tool_call_output" => {
                        let call_id = item
                            .get("call_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let output = item.get("output");
                        let output_str = if let Some(o) = output {
                            if o.is_string() {
                                o.as_str().unwrap_or("").to_string()
                            } else if let Some(content) = o.get("content").and_then(|v| v.as_str())
                            {
                                content.to_string()
                            } else {
                                o.to_string()
                            }
                        } else {
                            "".to_string()
                        };

                        let name = call_id_to_name.get(call_id).cloned().unwrap_or_else(|| {
                            // Fallback: if unknown and we see function_call_output, it's likely "shell" in this context
                            tracing::warn!(
                                "Unknown tool name for call_id {}, defaulting to 'shell'",
                                call_id
                            );
                            "shell".to_string()
                        });

                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "name": name,
                            "content": output_str
                        }));
                    }
                    _ => {}
                }
            }
        }

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

    // 2. Reuse handle_chat_completions logic (wrapping with custom handler or direct call)
    // Actually, due to SSE handling differences (Codex uses different event format), we replicate the loop here or abstract it.
    // For now, let's replicate the core loop but with Codex specific SSE mapping.

    // [Fix Phase 2] Backport normalization logic from handle_chat_completions
    // Handle "instructions" + "input" (Codex style) -> system + user messages
    // This is critical because `transform_openai_request` expects `messages` to be populated.

    // 1. If we have instructions/input, regardless of messages (which might be empty), force normalization.
    // Logic: if instructions OR input exists, we prefer creating messages from them.
    let has_codex_fields = body.get("instructions").is_some() || body.get("input").is_some();
    if has_codex_fields {
        tracing::debug!("[Codex] Detected Codex-style request (force normalization)");

        let mut messages = Vec::new();

        // instructions -> system message
        if let Some(inst) = body.get("instructions").and_then(|v| v.as_str()) {
            if !inst.is_empty() {
                messages.push(json!({
                    "role": "system",
                    "content": inst
                }));
            }
        }

        // input -> user message
        if let Some(input) = body.get("input") {
            // Handle array or string input
            let content = if let Some(s) = input.as_str() {
                s.to_string()
            } else if let Some(arr) = input.as_array() {
                // Join array parts
                arr.iter()
                    .map(|v| v.as_str().unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                input.to_string()
            };

            if !content.is_empty() {
                messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            }
        }

        if let Some(obj) = body.as_object_mut() {
            tracing::debug!(
                "[Codex] Injecting normalized messages: {} messages",
                messages.len()
            );
            obj.insert("messages".to_string(), json!(messages));
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
                use axum::body::Body;
                use axum::response::Response;
                use futures::StreamExt;

                let gemini_stream = response.bytes_stream();
                let sse_stream: std::pin::Pin<
                    Box<dyn futures::Stream<Item = Result<Bytes, String>> + Send>,
                > = if is_codex_style {
                    use crate::proxy::mappers::openai::streaming::create_codex_sse_stream;
                    Box::pin(create_codex_sse_stream(
                        Box::pin(gemini_stream),
                        openai_req.model.clone(),
                    ))
                } else {
                    use crate::proxy::mappers::openai::streaming::create_legacy_sse_stream;
                    Box::pin(create_legacy_sse_stream(
                        Box::pin(gemini_stream),
                        openai_req.model.clone(),
                    ))
                };

                // [FIX #859] Enhanced Peek logic to handle heartbeats and slow start
                let peek_config = PeekConfig::openai();
                let (first_data_chunk, sse_stream) =
                    match peek_first_data_chunk(sse_stream, &peek_config, &trace_id).await {
                        PeekResult::Data(bytes, stream) => (Some(bytes), stream),
                        PeekResult::Retry(err) => {
                            last_error = err;
                            continue;
                        }
                    };

                match first_data_chunk {
                    Some(bytes) => {
                        let combined_stream =
                            Box::pin(futures::stream::once(async move { Ok(bytes) }).chain(
                                sse_stream.map(|result| -> Result<Bytes, std::io::Error> {
                                    match result {
                                        Ok(b) => Ok(b),
                                        Err(e) => {
                                            let user_message = if e.contains("decoding") || e.contains("hyper") {
                                                "Upstream server closed connection (overload). Please retry your request."
                                            } else {
                                                "Stream interrupted by upstream. Please retry your request."
                                            };
                                            tracing::warn!("Stream error during transmission: {} (user msg: {})", e, user_message);
                                            Ok(Bytes::from(format!(
                                                "data: {{\"error\":{{\"message\":\"{}\",\"type\":\"server_error\",\"code\":\"overloaded\",\"param\":null}}}}\n\ndata: [DONE]\n\n",
                                                user_message
                                            )))
                                        }
                                    }
                                }),
                            ));

                        let body = Body::from_stream(combined_stream);
                        return Ok(Response::builder()
                            .header("Content-Type", "text/event-stream")
                            .header("Cache-Control", "no-cache")
                            .header("Connection", "keep-alive")
                            .header("X-Account-Email", &email)
                            .header("X-Mapped-Model", &mapped_model)
                            .body(body)
                            .expect("valid streaming response")
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

            // Map Chat Response -> Legacy Completions Response
            let choices = chat_resp.choices.iter().map(|c| {
                json!({
                    "text": match &c.message.content {
                        Some(crate::proxy::mappers::openai::OpenAIContent::String(s)) => s.clone(),
                        _ => "".to_string()
                    },
                    "index": c.index,
                    "logprobs": null,
                    "finish_reason": c.finish_reason
                })
            }).collect::<Vec<_>>();

            let legacy_resp = json!({
                "id": chat_resp.id,
                "object": "text_completion",
                "created": chat_resp.created,
                "model": chat_resp.model,
                "choices": choices
            });

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
