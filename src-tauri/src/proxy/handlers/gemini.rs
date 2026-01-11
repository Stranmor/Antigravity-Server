// Gemini Handler
use axum::{
    extract::State,
    extract::{Json, Path},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::{json, Value};
use tracing::{debug, error, info};

use crate::proxy::mappers::gemini::{unwrap_response, wrap_request};
use crate::proxy::server::AppState;
use crate::proxy::session_manager::SessionManager;

const MAX_RETRY_ATTEMPTS: usize = 10;
const MAX_529_RETRY_ATTEMPTS: usize = 3600; // 529 = 1 hour of retries (1 attempt per second)

/// 处理 generateContent 和 streamGenerateContent
/// 路径参数: model_name, method (e.g. "gemini-pro", "generateContent")
pub async fn handle_generate(
    State(state): State<AppState>,
    Path(model_action): Path<String>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // 解析 model:method
    let (model_name, method) = if let Some((m, action)) = model_action.rsplit_once(':') {
        (m.to_string(), action.to_string())
    } else {
        (model_action, "generateContent".to_string())
    };

    crate::modules::logger::log_info(&format!(
        "Received Gemini request: {}/{}",
        model_name, method
    ));

    // 1. 验证方法
    if method != "generateContent" && method != "streamGenerateContent" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unsupported method: {}", method),
        ));
    }
    let is_stream = method == "streamGenerateContent";

    // 2. 获取 UpstreamClient 和 TokenManager
    let upstream = state.upstream.clone();
    let token_manager = state.token_manager;
    let pool_size = token_manager.len();
    // max_attempts НЕ ограничен pool_size: при 529/503 мы не ротируем аккаунт
    let max_attempts = if pool_size == 0 {
        1
    } else {
        MAX_RETRY_ATTEMPTS
    };

    let mut last_error = String::new();
    let mut skip_rotation = false; // 529/503 时跳过账号轮换

    for attempt in 0..max_attempts {
        // 3. 模型路由解析
        let mapped_model = crate::proxy::common::model_mapping::resolve_model_route(
            &model_name,
            &*state.custom_mapping.read().await,
        );
        // 提取 tools 列表以进行联网探测 (Gemini 风格可能是嵌套的)
        let tools_val: Option<Vec<Value>> =
            body.get("tools").and_then(|t| t.as_array()).map(|arr| {
                let mut flattened = Vec::new();
                for tool_entry in arr {
                    if let Some(decls) = tool_entry
                        .get("functionDeclarations")
                        .and_then(|v| v.as_array())
                    {
                        flattened.extend(decls.iter().cloned());
                    } else {
                        flattened.push(tool_entry.clone());
                    }
                }
                flattened
            });

        let config = crate::proxy::mappers::common_utils::resolve_request_config(
            &model_name,
            &mapped_model,
            &tools_val,
        );

        // 4. 获取 Token (使用准确的 request_type)
        // 提取 SessionId (粘性指纹)
        let session_id = SessionManager::extract_gemini_session_id(&body, &model_name);

        // 关键修复: 529/503 时不轮换账号（服务端问题，轮换无意义）
        let force_rotate = attempt > 0 && !skip_rotation;
        skip_rotation = false; // 重置标志
        let (access_token, project_id, email) = match token_manager
            .get_token(&config.request_type, force_rotate, Some(&session_id))
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

        info!("✓ Using account: {} (type: {})", email, config.request_type);

        // 5. 包装请求 (project injection)
        let wrapped_body = wrap_request(&body, &project_id, &mapped_model);

        // 5. 上游调用
        let query_string = if is_stream { Some("alt=sse") } else { None };
        let upstream_method = if is_stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };

        let response = match upstream
            .call_v1_internal(
                upstream_method,
                &access_token,
                wrapped_body.clone(),
                query_string,
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = e.clone();
                debug!(
                    "Gemini Request failed on attempt {}/{}: {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                continue;
            }
        };

        let status = response.status();
        if status.is_success() {
            // 6. 响应处理
            if is_stream {
                use axum::body::Body;
                use axum::response::Response;
                use bytes::{Bytes, BytesMut};
                use futures::StreamExt;

                let mut response_stream = response.bytes_stream();
                let mut buffer = BytesMut::new();

                let stream = async_stream::stream! {
                    while let Some(item) = response_stream.next().await {
                        match item {
                            Ok(bytes) => {
                                debug!("[Gemini-SSE] Received chunk: {} bytes", bytes.len());
                                buffer.extend_from_slice(&bytes);
                                while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                                    let line_raw = buffer.split_to(pos + 1);
                                    if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                                        let line = line_str.trim();
                                        if line.is_empty() { continue; }

                                        if line.starts_with("data: ") {
                                            let json_part = line.trim_start_matches("data: ").trim();
                                            if json_part == "[DONE]" {
                                                yield Ok::<Bytes, String>(Bytes::from("data: [DONE]\n\n"));
                                                continue;
                                            }

                                            match serde_json::from_str::<Value>(json_part) {
                                                Ok(mut json) => {
                                                    // Unwrap v1internal response wrapper
                                                    if let Some(inner) = json.get_mut("response").map(|v| v.take()) {
                                                        let new_line = format!("data: {}\n\n", serde_json::to_string(&inner).unwrap_or_default());
                                                        yield Ok::<Bytes, String>(Bytes::from(new_line));
                                                    } else {
                                                        yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&json).unwrap_or_default())));
                                                    }
                                                }
                                                Err(e) => {
                                                    debug!("[Gemini-SSE] JSON parse error: {}, passing raw line", e);
                                                    yield Ok::<Bytes, String>(Bytes::from(format!("{}\n\n", line)));
                                                }
                                            }
                                        } else {
                                            // Non-data lines (comments, etc.)
                                            yield Ok::<Bytes, String>(Bytes::from(format!("{}\n\n", line)));
                                        }
                                    } else {
                                        // Non-UTF8 data? Just pass it through or skip
                                        debug!("[Gemini-SSE] Non-UTF8 line encountered");
                                        yield Ok::<Bytes, String>(line_raw.freeze());
                                    }
                                }
                            }
                            Err(e) => {
                                error!("[Gemini-SSE] Connection error: {}", e);
                                yield Err(format!("Stream error: {}", e));
                            }
                        }
                    }
                };

                let body = Body::from_stream(stream);
                return Ok(Response::builder()
                    .header("Content-Type", "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .header("Connection", "keep-alive")
                    .header("X-Account-Email", &email)
                    .header("X-Mapped-Model", &mapped_model)
                    .body(body)
                    .unwrap()
                    .into_response());
            }

            let gemini_resp: Value = response
                .json()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

            let unwrapped = unwrap_response(&gemini_resp);
            return Ok((
                StatusCode::OK,
                [
                    ("X-Account-Email", email.as_str()),
                    ("X-Mapped-Model", mapped_model.as_str()),
                ],
                Json(unwrapped),
            )
                .into_response());
        }

        // 处理错误并重试
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

        // 处理可重试错误
        if status_code == 429
            || status_code == 529
            || status_code == 503
            || status_code == 500
            || status_code == 403
            || status_code == 401
        {
            // 记录限流信息 (全局同步)
            token_manager.mark_rate_limited(
                &email,
                status_code,
                retry_after.as_deref(),
                &error_text,
            );

            // 只有明确包含 "QUOTA_EXHAUSTED" 才停止
            if status_code == 429 && error_text.contains("QUOTA_EXHAUSTED") {
                error!("Gemini Quota exhausted (429) on account {} attempt {}/{}, stopping to protect pool.", email, attempt + 1, max_attempts);
                return Err((status, error_text));
            }

            // 529 服务器过载 - 内部循环，最多重试 MAX_529_RETRY_ATTEMPTS 次
            if status_code == 529 {
                for retry_529 in 0..MAX_529_RETRY_ATTEMPTS {
                    tracing::warn!(
                        "Gemini Upstream 529 (server overload) on {} attempt {}/{}, waiting 1s",
                        email,
                        retry_529 + 1,
                        MAX_529_RETRY_ATTEMPTS
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                    // 重新发送请求
                    let retry_response = match upstream
                        .call_v1_internal(
                            upstream_method,
                            &access_token,
                            wrapped_body.clone(),
                            query_string,
                        )
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::debug!("529 retry {} failed: {}", retry_529 + 1, e);
                            continue;
                        }
                    };

                    let retry_status = retry_response.status();
                    if retry_status.is_success() {
                        // 成功！返回结果
                        if is_stream {
                            use axum::body::Body;
                            use axum::response::Response;
                            use bytes::{Bytes, BytesMut};
                            use futures::StreamExt;

                            let mut response_stream = retry_response.bytes_stream();
                            let mut buffer = BytesMut::new();

                            let stream = async_stream::stream! {
                                while let Some(item) = response_stream.next().await {
                                    match item {
                                        Ok(bytes) => {
                                            buffer.extend_from_slice(&bytes);
                                            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                                                let line_raw = buffer.split_to(pos + 1);
                                                if let Ok(line_str) = std::str::from_utf8(&line_raw) {
                                                    let line = line_str.trim();
                                                    if line.is_empty() { continue; }
                                                    if line.starts_with("data: ") {
                                                        let json_part = line.trim_start_matches("data: ").trim();
                                                        if json_part == "[DONE]" {
                                                            yield Ok::<Bytes, String>(Bytes::from("data: [DONE]\n\n"));
                                                            continue;
                                                        }
                                                        match serde_json::from_str::<Value>(json_part) {
                                                            Ok(mut json) => {
                                                                if let Some(inner) = json.get_mut("response").map(|v| v.take()) {
                                                                    yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&inner).unwrap_or_default())));
                                                                } else {
                                                                    yield Ok::<Bytes, String>(Bytes::from(format!("data: {}\n\n", serde_json::to_string(&json).unwrap_or_default())));
                                                                }
                                                            }
                                                            Err(_) => {
                                                                yield Ok::<Bytes, String>(Bytes::from(format!("{}\n\n", line)));
                                                            }
                                                        }
                                                    } else {
                                                        yield Ok::<Bytes, String>(Bytes::from(format!("{}\n\n", line)));
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            yield Err(format!("Stream error: {}", e));
                                        }
                                    }
                                }
                            };

                            let body = Body::from_stream(stream);
                            return Ok(Response::builder()
                                .header("Content-Type", "text/event-stream")
                                .header("Cache-Control", "no-cache")
                                .header("Connection", "keep-alive")
                                .header("X-Account-Email", &email)
                                .header("X-Mapped-Model", &mapped_model)
                                .body(body)
                                .unwrap()
                                .into_response());
                        }

                        let gemini_resp: Value = retry_response.json().await.map_err(|e| {
                            (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e))
                        })?;
                        let unwrapped = unwrap_response(&gemini_resp);
                        return Ok((
                            StatusCode::OK,
                            [
                                ("X-Account-Email", email.as_str()),
                                ("X-Mapped-Model", mapped_model.as_str()),
                            ],
                            Json(unwrapped),
                        )
                            .into_response());
                    }

                    // 529 继续重试，其他错误退出内部循环
                    if retry_response.status().as_u16() != 529 {
                        break;
                    }
                }
                // 内部循环耗尽，继续外部循环（尝试下一个账号）
                continue;
            }

            // 503 服务不可用 - exponential backoff
            if status_code == 503 {
                let backoff_ms = 1000_u64 * 2_u64.pow(attempt as u32).min(8000);
                tracing::warn!(
                    "Gemini Upstream 503 (service unavailable) on {} attempt {}/{}, waiting {}ms (no rotation)",
                    email, attempt + 1, max_attempts, backoff_ms
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                skip_rotation = true;
                continue;
            }

            tracing::warn!(
                "Gemini Upstream {} on account {} attempt {}/{}, rotating account",
                status_code,
                email,
                attempt + 1,
                max_attempts
            );
            continue;
        }

        // 404 等由于模型配置或路径错误的 HTTP 异常，直接报错，不进行无效轮换
        error!(
            "Gemini Upstream non-retryable error {}: {}",
            status_code, error_text
        );
        return Err((status, error_text));
    }

    Ok((
        StatusCode::TOO_MANY_REQUESTS,
        format!("All accounts exhausted. Last error: {}", last_error),
    )
        .into_response())
}

pub async fn handle_list_models(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use crate::proxy::common::model_mapping::get_all_dynamic_models;

    // 获取所有动态模型列表（与 /v1/models 一致）
    let model_ids = get_all_dynamic_models(&state.custom_mapping).await;

    // 转换为 Gemini API 格式
    let models: Vec<_> = model_ids
        .into_iter()
        .map(|id| {
            json!({
                "name": format!("models/{}", id),
                "version": "001",
                "displayName": id.clone(),
                "description": "",
                "inputTokenLimit": 128000,
                "outputTokenLimit": 8192,
                "supportedGenerationMethods": ["generateContent", "countTokens"],
                "temperature": 1.0,
                "topP": 0.95,
                "topK": 64
            })
        })
        .collect();

    Ok(Json(json!({ "models": models })))
}

pub async fn handle_get_model(Path(model_name): Path<String>) -> impl IntoResponse {
    Json(json!({
        "name": format!("models/{}", model_name),
        "displayName": model_name
    }))
}

pub async fn handle_count_tokens(
    State(state): State<AppState>,
    Path(_model_name): Path<String>,
    Json(_body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let model_group = "gemini";
    let (_access_token, _project_id, _) = state
        .token_manager
        .get_token(model_group, false, None)
        .await
        .map_err(|e| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Token error: {}", e),
            )
        })?;

    Ok(Json(json!({"totalTokens": 0})))
}
