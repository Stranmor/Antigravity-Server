mod codex_parser;
pub mod request_parser;
mod response_mapper;
mod streaming_handler;

use super::*;
use crate::proxy::common::{sanitize_exhaustion_error, sanitize_upstream_error};
use crate::proxy::SignatureCache;
use request_parser::{ensure_non_empty_messages, normalize_request_body};

pub async fn handle_completions(
    State(state): State<AppState>,
    Json(mut body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("Received /v1/completions or /v1/responses payload: {:?}", body);
    let is_codex_style = normalize_request_body(&mut body);
    let mut openai_req: OpenAIRequest = serde_json::from_value(body.clone())
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;
    ensure_non_empty_messages(&mut openai_req);

    let upstream = state.upstream.clone();
    let token_manager = state.token_manager;
    let pool_size = token_manager.len();
    let max_attempts = MAX_RETRY_ATTEMPTS.min(pool_size).max(1);

    let mut last_error = String::new();

    let mut last_email: Option<String> = None;
    let trace_id = format!("req_{}", chrono::Utc::now().timestamp_micros());
    let mut grace_retry_used = false;

    for attempt in 0..max_attempts {
        let (mapped_model, _reason) = match crate::proxy::common::resolve_model_route(
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

        let session_id_str = SessionManager::extract_openai_session_id(&openai_req);
        let session_id = Some(session_id_str.as_str());
        let force_rotate = attempt > 0;

        let (access_token, project_id, email, _guard) = match token_manager
            .get_token(&config.request_type, force_rotate, session_id, &config.final_model)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                return Err((StatusCode::SERVICE_UNAVAILABLE, format!("Token error: {}", e)));
            },
        };

        last_email = Some(email.clone());
        info!("✓ Using account: {} (type: {})", email, config.request_type);

        // Preload thinking signatures from PostgreSQL for multi-turn conversations
        let content_hashes: Vec<String> = openai_req
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .filter_map(|m| m.reasoning_content.as_ref())
            .filter(|rc| !rc.is_empty())
            .map(|rc| SignatureCache::compute_content_hash(rc))
            .collect();

        if !content_hashes.is_empty() {
            SignatureCache::global().preload_signatures_from_db(&content_hashes).await;
        }

        let gemini_body = transform_openai_request(&openai_req, &project_id, &mapped_model);
        debug!(
            "[Codex-Request] Transformed Gemini Body ({} parts)",
            gemini_body.get("contents").and_then(|c| c.as_array()).map(|a| a.len()).unwrap_or(0)
        );

        let list_response = openai_req.stream;
        let method = if list_response { "streamGenerateContent" } else { "generateContent" };
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
                debug!("Codex Request failed on attempt {}/{}: {}", attempt + 1, max_attempts, e);
                continue;
            },
        };

        let status = response.status();
        if status.is_success() {
            token_manager.mark_account_success(&email);
            token_manager.clear_session_failures(&session_id_str);
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
                        Some(session_id_str.clone()),
                    ))
                } else {
                    use crate::proxy::mappers::openai::streaming::create_legacy_sse_stream;
                    Box::pin(create_legacy_sse_stream(
                        Box::pin(gemini_stream),
                        openai_req.model.clone(),
                    ))
                };

                let peek_config = PeekConfig::openai();
                #[allow(
                    clippy::type_complexity,
                    reason = "decomposing Pin<Box<dyn Stream>> tuple would reduce readability"
                )]
                let (first_data_chunk, sse_stream): (
                    Option<Bytes>,
                    std::pin::Pin<Box<dyn futures::Stream<Item = Result<Bytes, String>> + Send>>,
                ) = match peek_first_data_chunk(sse_stream, &peek_config, &trace_id).await {
                    PeekResult::Data(bytes, stream) => (Some(bytes), stream),
                    PeekResult::Retry(err) => {
                        last_error = err;
                        continue;
                    },
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
                    },
                    None => {
                        tracing::warn!(
                            "[{}] Stream ended immediately (Empty Response), retrying...",
                            trace_id
                        );
                        last_error = "Empty response stream (None)".to_string();
                        continue;
                    },
                }
            }

            let gemini_resp: Value = response
                .json()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

            let chat_resp = transform_openai_response(&gemini_resp);
            let legacy_resp = response_mapper::map_chat_to_legacy_response(&chat_resp);

            return Ok(Json(legacy_resp).into_response());
        }

        // Handle errors and retry
        let status_code = status.as_u16();
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        let error_text = response.text().await.unwrap_or_else(|_| format!("HTTP {}", status_code));
        last_error = format!("HTTP {}: {}", status_code, error_text);

        tracing::error!("[Codex-Upstream] Error Response {}: {}", status_code, error_text);

        // Grace retry for transient 429
        if status_code == 429 && !grace_retry_used {
            let reason = token_manager.rate_limit_tracker().parse_rate_limit_reason(&error_text);
            if reason == crate::proxy::rate_limit::RateLimitReason::RateLimitExceeded {
                grace_retry_used = true;
                tracing::info!("[{}] 🔄 Grace retry on {}, waiting 1s", trace_id, email);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        }

        if status_code == 429 || status_code == 529 || status_code == 503 || status_code == 500 {
            token_manager
                .mark_rate_limited_async(
                    &email,
                    status_code,
                    retry_after.as_deref(),
                    &error_text,
                    Some(&mapped_model),
                )
                .await;

            if status_code == 429 {
                token_manager.record_session_failure(&session_id_str);
                state.adaptive_limits.record_429(&email);
            } else {
                state.adaptive_limits.record_error(&email, status_code);
            }
        }

        if status_code == 404 {
            tracing::warn!(
                "[Completions] 404 on {} (model not available on this tier), rotating",
                email
            );
            continue;
        }

        let profile = RetryProfile::openai();
        let strategy = determine_retry_strategy(status_code, &error_text, false, &profile);
        if apply_retry_strategy(strategy, attempt, status_code, &trace_id).await {
            continue;
        } else {
            return Ok((
                status,
                [("X-Account-Email", email.as_str())],
                sanitize_upstream_error(status_code, &error_text),
            )
                .into_response());
        }
    }

    if let Some(email) = last_email {
        Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [("X-Account-Email", email)],
            format!(
                "All accounts exhausted. Last error: {}",
                sanitize_exhaustion_error(&last_error)
            ),
        )
            .into_response())
    } else {
        Ok((
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "All accounts exhausted. Last error: {}",
                sanitize_exhaustion_error(&last_error)
            ),
        )
            .into_response())
    }
}
