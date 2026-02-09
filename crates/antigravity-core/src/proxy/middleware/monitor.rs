// Request/response monitoring: byte counting and size limits.
// All arithmetic is on usize for buffer sizes, bounded by MAX_*_LOG_SIZE constants.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "Monitoring middleware: bounded buffer sizes, safe byte operations"
)]

use super::monitor_usage::extract_usage_from_json;
use crate::proxy::monitor::ProxyRequestLog;
use crate::proxy::server::AppState;
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

pub async fn monitor_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !state.monitor.is_enabled() {
        return next.run(request).await;
    }

    let start = Instant::now();
    let method = request.method().to_string();
    let uri = request.uri().to_string();

    if uri.contains("event_logging") {
        return next.run(request).await;
    }

    let mut model = if uri.contains("/v1beta/models/") {
        uri.split("/v1beta/models/").nth(1).and_then(|s| s.split(':').next()).map(|s| s.to_string())
    } else {
        None
    };

    let monitor = state.monitor.clone();
    let content_length = request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());

    let request = if method == "POST" && content_length.is_some_and(|l| l <= 2 * 1024 * 1024) {
        let (parts, body) = request.into_parts();
        // Limit request body inspection to 2MB to avoid DoS
        match axum::body::to_bytes(body, 2 * 1024 * 1024).await {
            Ok(bytes) => {
                if model.is_none() {
                    model = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| {
                        v.get("model").and_then(|m| m.as_str()).map(|s| s.to_string())
                    });
                }
                Request::from_parts(parts, Body::from(bytes))
            },
            Err(e) => {
                tracing::error!("Failed to buffer request body: {}", e);
                return Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(Body::from("Failed to buffer request body"))
                    .unwrap_or_else(|_| Response::new(Body::empty()));
            },
        }
    } else {
        request
    };

    let response = next.run(request).await;
    let duration = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let account_email = response
        .headers()
        .get("X-Account-Email")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let mapped_model = response
        .headers()
        .get("X-Mapped-Model")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let mapping_reason = response
        .headers()
        .get("X-Mapping-Reason")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let log = ProxyRequestLog {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        method,
        url: uri,
        status,
        duration,
        model,
        mapped_model,
        mapping_reason,
        account_email,
        error: None,
        request_body: None,
        response_body: None,
        input_tokens: None,
        output_tokens: None,
        cached_tokens: None,
    };

    if content_type.contains("text/event-stream") {
        handle_sse_response(response, log, monitor).await
    } else if content_type.contains("application/json") {
        handle_json_response(response, log, monitor).await
    } else {
        monitor.log_request(log).await;
        response
    }
}

async fn handle_sse_response(
    response: Response,
    mut log: ProxyRequestLog,
    monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
) -> Response {
    let (parts, body) = response.into_parts();
    let mut stream = body.into_data_stream();
    let (tx, rx) = tokio::sync::mpsc::channel(64);

    tokio::spawn(async move {
        let mut last_few_bytes = Vec::new();

        while let Some(chunk_res) = stream.next().await {
            if let Ok(chunk) = chunk_res {
                if chunk.len() > 8192 {
                    last_few_bytes = chunk.slice(chunk.len() - 8192..).to_vec();
                } else {
                    last_few_bytes.extend_from_slice(&chunk);
                    if last_few_bytes.len() > 8192 {
                        let keep_from = last_few_bytes.len() - 8192;
                        last_few_bytes.copy_within(keep_from.., 0);
                        last_few_bytes.truncate(8192);
                    }
                }
                if tx.send(Ok::<_, axum::Error>(chunk)).await.is_err() {
                    break;
                }
            } else if let Err(e) = chunk_res {
                if tx.send(Err(axum::Error::new(e))).await.is_err() {
                    break;
                }
            }
        }

        log.response_body = None;

        let full_tail = String::from_utf8_lossy(&last_few_bytes);
        for line in full_tail.lines().rev() {
            if line.starts_with("data: ")
                && (line.contains("\"usage\"") || line.contains("\"usageMetadata\""))
            {
                let json_str = line.trim_start_matches("data: ").trim();
                if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                    if let Some(usage) = json.get("usage").or(json.get("usageMetadata")) {
                        extract_usage_from_json(usage, &mut log);
                        break;
                    }
                }
            }
        }

        if log.status >= 400 {
            log.error = Some("Stream Error or Failed".to_string());
        }
        monitor.log_request(log).await;
    });

    Response::from_parts(parts, Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)))
}

async fn handle_json_response(
    response: Response,
    mut log: ProxyRequestLog,
    monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
) -> Response {
    let content_length = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());

    // If response is too large, don't even try to buffer it in background
    if content_length.is_some_and(|l| l > 2 * 1024 * 1024) {
        if log.status >= 400 {
            log.error = Some("Large upstream error response".to_string());
        }
        monitor.log_request(log).await;
        return response;
    }

    let (parts, body) = response.into_parts();
    let mut stream = body.into_data_stream();
    let (tx, rx) = tokio::sync::mpsc::channel(64);

    tokio::spawn(async move {
        let mut buffer = Vec::new();
        let mut failed_to_buffer = false;

        while let Some(chunk_res) = stream.next().await {
            match chunk_res {
                Ok(chunk) => {
                    if !failed_to_buffer {
                        if buffer.len() + chunk.len() <= 2 * 1024 * 1024 {
                            buffer.extend_from_slice(&chunk);
                        } else {
                            failed_to_buffer = true;
                            buffer.clear();
                            buffer.shrink_to_fit();
                        }
                    }
                    if tx.send(Ok::<_, axum::Error>(chunk)).await.is_err() {
                        break;
                    }
                },
                Err(e) => {
                    let _ = tx.send(Err(axum::Error::new(e))).await;
                    break;
                },
            }
        }

        if !failed_to_buffer && !buffer.is_empty() {
            if let Ok(json) = serde_json::from_slice::<Value>(&buffer) {
                if let Some(usage) = json.get("usage").or(json.get("usageMetadata")) {
                    extract_usage_from_json(usage, &mut log);
                }
            }
        }

        if log.status >= 400 {
            log.error = Some("Upstream error response received".to_string());
        }
        log.response_body = None;
        monitor.log_request(log).await;
    });

    Response::from_parts(parts, Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)))
}
