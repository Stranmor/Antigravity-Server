use crate::proxy::server::AppState;
use axum::{extract::Json, extract::State, http::StatusCode, response::IntoResponse};
use bytes::Bytes;
use futures::Stream;
use serde_json::{json, Value};
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub enum RetryStrategy {
    NoRetry,
    FixedDelay(Duration),
    LinearBackoff { base_ms: u64 },
    ExponentialBackoff { base_ms: u64, max_ms: u64 },
}

pub fn determine_retry_strategy(
    status_code: u16,
    error_text: &str,
    retried_without_thinking: bool,
) -> RetryStrategy {
    match status_code {
        400 if !retried_without_thinking
            && (error_text.contains("Invalid `signature`")
                || error_text.contains("thinking.signature")
                || error_text.contains("thinking.thinking")
                || error_text.contains("Corrupted thought signature")) =>
        {
            RetryStrategy::FixedDelay(Duration::from_millis(200))
        }
        429 => {
            if let Some(delay_ms) = crate::proxy::upstream::retry::parse_retry_delay(error_text) {
                let actual_delay = delay_ms.saturating_add(200).min(30_000);
                RetryStrategy::FixedDelay(Duration::from_millis(actual_delay))
            } else {
                RetryStrategy::LinearBackoff { base_ms: 5000 }
            }
        }
        503 | 529 => RetryStrategy::ExponentialBackoff {
            base_ms: 10000,
            max_ms: 60000,
        },
        500 => RetryStrategy::LinearBackoff { base_ms: 3000 },
        401 | 403 => RetryStrategy::FixedDelay(Duration::from_millis(200)),
        _ => RetryStrategy::NoRetry,
    }
}

pub async fn apply_retry_strategy(
    strategy: RetryStrategy,
    attempt: usize,
    max_attempts: usize,
    status_code: u16,
    trace_id: &str,
) -> bool {
    match strategy {
        RetryStrategy::NoRetry => {
            debug!(
                "[{}] Non-retryable error {}, stopping",
                trace_id, status_code
            );
            false
        }
        RetryStrategy::FixedDelay(duration) => {
            let base_ms = duration.as_millis() as u64;
            info!(
                "[{}] ⏱️ Retry with fixed delay: status={}, attempt={}/{}, delay={}ms",
                trace_id,
                status_code,
                attempt + 1,
                max_attempts,
                base_ms
            );
            sleep(duration).await;
            true
        }
        RetryStrategy::LinearBackoff { base_ms } => {
            let calculated_ms = base_ms * (attempt as u64 + 1);
            info!(
                "[{}] ⏱️ Retry with linear backoff: status={}, attempt={}/{}, delay={}ms",
                trace_id,
                status_code,
                attempt + 1,
                max_attempts,
                calculated_ms
            );
            sleep(Duration::from_millis(calculated_ms)).await;
            true
        }
        RetryStrategy::ExponentialBackoff { base_ms, max_ms } => {
            let calculated_ms = (base_ms * 2_u64.pow(attempt as u32)).min(max_ms);
            info!(
                "[{}] ⏱️ Retry with exponential backoff: status={}, attempt={}/{}, delay={}ms",
                trace_id,
                status_code,
                attempt + 1,
                max_attempts,
                calculated_ms
            );
            sleep(Duration::from_millis(calculated_ms)).await;
            true
        }
    }
}

pub fn should_rotate_account(status_code: u16) -> bool {
    matches!(status_code, 429 | 401 | 403 | 500)
}

pub struct PeekConfig {
    pub max_heartbeats: u32,
    pub max_peek_duration: Duration,
    pub single_chunk_timeout: Duration,
}

impl Default for PeekConfig {
    fn default() -> Self {
        Self {
            max_heartbeats: 20,
            max_peek_duration: Duration::from_secs(120),
            single_chunk_timeout: Duration::from_secs(60),
        }
    }
}

impl PeekConfig {
    pub fn openai() -> Self {
        Self {
            max_heartbeats: 20,
            max_peek_duration: Duration::from_secs(90),
            single_chunk_timeout: Duration::from_secs(30),
        }
    }
}

pub enum PeekResult<S> {
    Data(Bytes, S),
    Retry(String),
}

pub async fn peek_first_data_chunk<S, E>(
    mut stream: Pin<Box<S>>,
    config: &PeekConfig,
    trace_id: &str,
) -> PeekResult<Pin<Box<S>>>
where
    S: Stream<Item = Result<Bytes, E>> + Send + ?Sized,
    E: std::fmt::Display,
{
    use futures::StreamExt;

    let mut heartbeat_count = 0u32;
    let peek_start = Instant::now();

    loop {
        if peek_start.elapsed() > config.max_peek_duration {
            tracing::warn!(
                "[{}] Peek phase exceeded {}s total timeout, retrying...",
                trace_id,
                config.max_peek_duration.as_secs()
            );
            crate::proxy::prometheus::record_peek_retry("timeout");
            return PeekResult::Retry(format!(
                "Peek phase timeout after {}s",
                config.max_peek_duration.as_secs()
            ));
        }

        match tokio::time::timeout(config.single_chunk_timeout, stream.next()).await {
            Ok(Some(Ok(bytes))) => {
                if bytes.is_empty() {
                    continue;
                }

                let text = String::from_utf8_lossy(&bytes);
                if text.trim().starts_with(':') {
                    heartbeat_count += 1;
                    crate::proxy::prometheus::record_peek_heartbeat();
                    tracing::debug!(
                        "[{}] Skipping peek heartbeat {}/{}: {}",
                        trace_id,
                        heartbeat_count,
                        config.max_heartbeats,
                        text.trim()
                    );

                    if heartbeat_count >= config.max_heartbeats {
                        tracing::warn!(
                            "[{}] Exceeded {} heartbeats without real data, retrying...",
                            trace_id,
                            config.max_heartbeats
                        );
                        crate::proxy::prometheus::record_peek_retry("heartbeats");
                        return PeekResult::Retry(format!(
                            "Too many heartbeats ({}) without data",
                            config.max_heartbeats
                        ));
                    }
                    continue;
                }

                return PeekResult::Data(bytes, stream);
            }
            Ok(Some(Err(e))) => {
                tracing::warn!(
                    "[{}] Stream error during peek: {}, retrying...",
                    trace_id,
                    e
                );
                crate::proxy::prometheus::record_peek_retry("stream_error");
                return PeekResult::Retry(format!("Stream error during peek: {}", e));
            }
            Ok(None) => {
                tracing::warn!(
                    "[{}] Stream ended during peek (Empty Response), retrying...",
                    trace_id
                );
                crate::proxy::prometheus::record_peek_retry("empty_response");
                return PeekResult::Retry("Empty response stream during peek".to_string());
            }
            Err(_) => {
                tracing::warn!(
                    "[{}] Timeout waiting for first data ({}s), retrying...",
                    trace_id,
                    config.single_chunk_timeout.as_secs()
                );
                crate::proxy::prometheus::record_peek_retry("chunk_timeout");
                return PeekResult::Retry("Timeout waiting for first data".to_string());
            }
        }
    }
}

/// Detects model capabilities and configuration
/// POST /v1/models/detect
pub async fn handle_detect_model(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let model_name = body.get("model").and_then(|v| v.as_str()).unwrap_or("");

    if model_name.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing 'model' field").into_response();
    }

    // 1. Resolve mapping
    let (mapped_model, reason) =
        crate::proxy::common::resolve_model_route(model_name, &*state.custom_mapping.read().await);

    // 2. Resolve capabilities
    let config = crate::proxy::mappers::request_config::resolve_request_config(
        model_name,
        &mapped_model,
        &None, // We don't check tools for static capability detection
        None,
        None,
    );

    // 3. Construct response
    let mut response = json!({
        "model": model_name,
        "mapped_model": mapped_model,
        "mapping_reason": reason,
        "type": config.request_type,
        "features": {
            "has_web_search": config.inject_google_search,
            "is_image_gen": config.request_type == "image_gen"
        }
    });

    if let Some(img_conf) = config.image_config {
        if let Some(obj) = response.as_object_mut() {
            obj.insert("config".to_string(), img_conf);
        }
    }

    Json(response).into_response()
}
