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

const MAX_REQUEST_LOG_SIZE: usize = 100 * 1024 * 1024;
const MAX_RESPONSE_LOG_SIZE: usize = 100 * 1024 * 1024;

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
    let request = if method == "POST" {
        let (parts, body) = request.into_parts();
        match axum::body::to_bytes(body, MAX_REQUEST_LOG_SIZE).await {
            Ok(bytes) => {
                if model.is_none() {
                    model = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| {
                        v.get("model").and_then(|m| m.as_str()).map(|s| s.to_string())
                    });
                }
                Request::from_parts(parts, Body::from(bytes))
            },
            Err(_) => {
                let duration = start.elapsed().as_millis() as u64;
                let log = ProxyRequestLog {
                    id: uuid::Uuid::new_v4().to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    method,
                    url: uri,
                    status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
                    duration,
                    model,
                    mapped_model: None,
                    mapping_reason: None,
                    account_email: None,
                    error: Some("Request body too large to inspect".to_string()),
                    request_body: None,
                    response_body: None,
                    input_tokens: None,
                    output_tokens: None,
                    cached_tokens: None,
                };
                monitor.log_request(log).await;
                return Response::builder()
                    .status(StatusCode::PAYLOAD_TOO_LARGE)
                    .body(Body::from("Request body too large"))
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
    } else if content_type.contains("application/json") || content_type.contains("text/") {
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
                        last_few_bytes.drain(0..last_few_bytes.len() - 8192);
                    }
                }
                let _ = tx.send(Ok::<_, axum::Error>(chunk)).await;
            } else if let Err(e) = chunk_res {
                let _ = tx.send(Err(axum::Error::new(e))).await;
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
    let (parts, body) = response.into_parts();
    match axum::body::to_bytes(body, MAX_RESPONSE_LOG_SIZE).await {
        Ok(bytes) => {
            if let Ok(s) = std::str::from_utf8(&bytes) {
                if let Ok(json) = serde_json::from_str::<Value>(s) {
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
            Response::from_parts(parts, Body::from(bytes))
        },
        Err(_) => {
            log.response_body = None;
            log.error = Some("Response too large to log".to_string());
            monitor.log_request(log).await;
            Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from("Upstream response too large"))
                .unwrap_or_else(|_| Response::new(Body::empty()))
        },
    }
}
