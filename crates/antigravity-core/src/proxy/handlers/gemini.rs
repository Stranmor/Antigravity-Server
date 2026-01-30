use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

use crate::proxy::mappers::gemini::{unwrap_response, wrap_request};
use crate::proxy::server::AppState;
use crate::proxy::session_manager::SessionManager;

const MAX_RETRY_ATTEMPTS: usize = 3;

pub async fn handle_generate(
    State(state): State<AppState>,
    Path(model_action): Path<String>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (model_name, method) = match model_action.rsplit_once(':') {
        Some((m, action)) => (m.to_string(), action.to_string()),
        None => (model_action, "generateContent".to_string()),
    };

    info!("[Gemini] Request: {}/{}", model_name, method);

    if method != "generateContent" && method != "streamGenerateContent" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unsupported method: {}", method),
        ));
    }
    let is_stream = method == "streamGenerateContent";

    let token_manager = state.token_manager.clone();
    let max_attempts = MAX_RETRY_ATTEMPTS.min(token_manager.len()).max(1);

    let mut last_error = String::new();
    let mut last_email: Option<String> = None;

    for attempt in 0..max_attempts {
        let (mapped_model, _reason) = crate::proxy::common::resolve_model_route(
            &model_name,
            &*state.custom_mapping.read().await,
        );

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

        let config = crate::proxy::mappers::common_utils::resolve_request_config(
            &model_name,
            &mapped_model,
            &tools_val,
            None,
            None,
        );
        let session_id = SessionManager::extract_gemini_session_id(&body, &model_name);

        let (access_token, project_id, email, _guard) = match token_manager
            .get_token(
                &config.request_type,
                attempt > 0,
                Some(&session_id),
                &config.final_model,
            )
            .await
        {
            Ok(t) => t,
            Err(e) => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("Token error: {}", e),
                ))
            }
        };

        last_email = Some(email.clone());
        info!(
            "[Gemini] Account: {} (type: {})",
            email, config.request_type
        );

        let wrapped_body = wrap_request(&body, &project_id, &mapped_model, Some(&session_id));
        let query_string = if is_stream { Some("alt=sse") } else { None };
        let upstream_method = if is_stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };

        let response = match state
            .upstream
            .call_v1_internal(upstream_method, &access_token, wrapped_body, query_string)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = e.clone();
                debug!(
                    "[Gemini] Attempt {}/{} failed: {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                continue;
            }
        };

        let status = response.status();
        if status.is_success() {
            if is_stream {
                let mut response_stream = response.bytes_stream();

                let first_chunk = match peek_first_chunk(&mut response_stream).await {
                    Ok(chunk) => chunk,
                    Err(peek_err) => {
                        warn!("[Gemini] Peek failed: {}, rotating account", peek_err);
                        last_error = peek_err;
                        continue;
                    }
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
                [
                    ("X-Account-Email", email.as_str()),
                    ("X-Mapped-Model", mapped_model.as_str()),
                ],
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
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| format!("HTTP {}", code));
        last_error = format!("HTTP {}: {}", code, error_text);

        if matches!(code, 429 | 529 | 503 | 500 | 403 | 401) {
            token_manager.mark_rate_limited(&email, code, retry_after.as_deref(), &error_text);
            if code == 429 && error_text.contains("QUOTA_EXHAUSTED") {
                error!("[Gemini] Quota exhausted on {}", email);
                return Ok(
                    (status, [("X-Account-Email", email.as_str())], error_text).into_response()
                );
            }
            warn!("[Gemini] {} on {}, rotating", code, email);
            continue;
        }

        error!("[Gemini] Non-retryable {}: {}", code, error_text);
        return Ok((status, [("X-Account-Email", email.as_str())], error_text).into_response());
    }

    let msg = format!("All accounts exhausted. Last: {}", last_error);
    match last_email {
        Some(email) => Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [("X-Account-Email", email)],
            msg,
        )
            .into_response()),
        None => Ok((StatusCode::TOO_MANY_REQUESTS, msg).into_response()),
    }
}

pub async fn handle_list_models(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use crate::proxy::common::model_mapping::get_all_dynamic_models;
    let model_ids = get_all_dynamic_models(&state.custom_mapping).await;
    let models: Vec<_> = model_ids
        .into_iter()
        .map(|id| {
            json!({
                "name": format!("models/{}", id),
                "version": "001",
                "displayName": id.clone(),
                "inputTokenLimit": 1048576,
                "outputTokenLimit": 65536,
                "supportedGenerationMethods": ["generateContent", "countTokens"]
            })
        })
        .collect();
    Ok(Json(json!({ "models": models })))
}

pub async fn handle_get_model(Path(model_name): Path<String>) -> impl IntoResponse {
    Json(json!({ "name": format!("models/{}", model_name), "displayName": model_name }))
}

/// Token counting stub - returns 0 tokens.
/// Note: Actual token counting requires upstream API support which is not implemented.
/// Clients should use their own tokenization if precise counts are needed.
pub async fn handle_count_tokens(
    State(state): State<AppState>,
    Path(_model_name): Path<String>,
    Json(_body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (_access_token, _project_id, _email, _guard) = state
        .token_manager
        .get_token("gemini", false, None, "gemini")
        .await
        .map_err(|e| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Token error: {}", e),
            )
        })?;
    Ok(Json(json!({"totalTokens": 0})))
}

/// Peek first SSE chunk with retry logic (Issue #859)
/// Returns the first meaningful data chunk, skipping heartbeats.
/// On timeout/empty/error, returns Err for account rotation.
async fn peek_first_chunk<S>(stream: &mut S) -> Result<Bytes, String>
where
    S: futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    const PEEK_TIMEOUT_SECS: u64 = 30;
    const MAX_HEARTBEATS: usize = 20;
    const MAX_PEEK_DURATION_SECS: u64 = 90;

    let peek_start = std::time::Instant::now();
    let mut heartbeat_count = 0;

    loop {
        // Total peek phase limit
        if peek_start.elapsed().as_secs() > MAX_PEEK_DURATION_SECS {
            return Err(format!(
                "Peek phase exceeded {}s limit ({} heartbeats seen)",
                MAX_PEEK_DURATION_SECS, heartbeat_count
            ));
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(PEEK_TIMEOUT_SECS),
            stream.next(),
        )
        .await
        {
            Ok(Some(Ok(bytes))) => {
                if bytes.is_empty() {
                    warn!("[Gemini] Empty chunk received, retrying peek...");
                    heartbeat_count += 1;
                    if heartbeat_count > MAX_HEARTBEATS {
                        return Err(format!(
                            "Too many empty chunks ({}), rotating account",
                            heartbeat_count
                        ));
                    }
                    continue;
                }

                // Check for SSE heartbeat (lines starting with ':')
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    let trimmed = text.trim();
                    if trimmed.starts_with(':') || trimmed.is_empty() {
                        debug!("[Gemini] Skipping SSE heartbeat: {:?}", trimmed);
                        heartbeat_count += 1;
                        if heartbeat_count > MAX_HEARTBEATS {
                            return Err(format!(
                                "Too many heartbeats ({}), rotating account",
                                heartbeat_count
                            ));
                        }
                        continue;
                    }
                }

                // Valid data chunk
                return Ok(bytes);
            }
            Ok(Some(Err(e))) => {
                return Err(format!("Stream error during peek: {}", e));
            }
            Ok(None) => {
                return Err("Stream ended immediately (empty response)".to_string());
            }
            Err(_) => {
                return Err(format!(
                    "Timeout ({}s) waiting for first chunk",
                    PEEK_TIMEOUT_SECS
                ));
            }
        }
    }
}

fn extract_signature(resp: &Value, session_id: &str) {
    let inner = resp.get("response").unwrap_or(resp);
    if let Some(candidates) = inner.get("candidates").and_then(|c| c.as_array()) {
        for cand in candidates {
            if let Some(parts) = cand
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array())
            {
                for part in parts {
                    if let Some(sig) = part.get("thoughtSignature").and_then(|s| s.as_str()) {
                        crate::proxy::SignatureCache::global()
                            .cache_session_signature(session_id, sig.to_string());
                        debug!("[Gemini] Cached signature for {}", session_id);
                    }
                }
            }
        }
    }
}

async fn build_stream_response<S>(
    mut response_stream: S,
    first_chunk: Bytes,
    session_id: String,
    email: String,
    mapped_model: String,
) -> Result<Response<Body>, (StatusCode, String)>
where
    S: futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin + Send + 'static,
{
    let s_id = session_id;

    let stream = async_stream::stream! {
        const MAX_BUFFER_SIZE: usize = 10 * 1024 * 1024; // 10MB limit
        let mut buffer = BytesMut::new();
        let mut first_data = Some(first_chunk);

        loop {
            let item = match first_data.take() {
                Some(fd) => Some(Ok(fd)),
                None => response_stream.next().await,
            };

            let bytes = match item {
                Some(Ok(b)) => b,
                Some(Err(e)) => { error!("[Gemini-SSE] {}", e); yield Err(format!("Stream error: {}", e)); break; }
                None => break,
            };

            buffer.extend_from_slice(&bytes);

            if buffer.len() > MAX_BUFFER_SIZE {
                error!("[Gemini-SSE] Buffer overflow, dropping connection");
                yield Err("Buffer overflow".to_string());
                break;
            }

            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                let line_raw = buffer.split_to(pos + 1);
                let Ok(line_str) = std::str::from_utf8(&line_raw) else {
                    yield Ok::<Bytes, String>(line_raw.freeze());
                    continue;
                };

                let line = line_str.trim();
                if line.is_empty() { continue; }

                if let Some(json_part) = line.strip_prefix("data: ") {
                    let json_part = json_part.trim();
                    if json_part.is_empty() { continue; }

                    if let Ok(parsed) = serde_json::from_str::<Value>(json_part) {
                        extract_signature(&parsed, &s_id);
                        let unwrapped = unwrap_response(&parsed);
                        let out = format!("data: {}\n\n", serde_json::to_string(&unwrapped).unwrap_or_default());
                        yield Ok::<Bytes, String>(Bytes::from(out));
                    } else {
                        yield Ok::<Bytes, String>(Bytes::from(format!("{}\n", line_str)));
                    }
                } else {
                    yield Ok::<Bytes, String>(Bytes::from(format!("{}\n", line_str)));
                }
            }
        }
    };

    let body = Body::from_stream(stream);
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Account-Email", email)
        .header("X-Mapped-Model", mapped_model)
        .body(body)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Response build error: {}", e),
            )
        })?;

    Ok(resp)
}
