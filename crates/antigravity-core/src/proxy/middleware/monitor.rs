// Request/response monitoring: byte counting and size limits.
// All arithmetic is on usize for buffer sizes, bounded by MAX_*_LOG_SIZE constants.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "Monitoring middleware: bounded buffer sizes, safe byte operations"
)]

use super::monitor_usage::{extract_text_from_sse_line, extract_usage_from_json};
use crate::proxy::monitor::ProxyRequestLog;
use crate::proxy::server::AppState;
use axum::{
    body::Body,
    extract::{Request, State},
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

    let request_body_str;
    let request = if method == "POST" {
        let (parts, body) = request.into_parts();
        match axum::body::to_bytes(body, MAX_REQUEST_LOG_SIZE).await {
            Ok(bytes) => {
                if model.is_none() {
                    model = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| {
                        v.get("model").and_then(|m| m.as_str()).map(|s| s.to_string())
                    });
                }
                request_body_str = if let Ok(s) = std::str::from_utf8(&bytes) {
                    Some(s.to_string())
                } else {
                    Some("[Binary Request Data]".to_string())
                };
                Request::from_parts(parts, Body::from(bytes))
            },
            Err(_) => {
                request_body_str = None;
                Request::from_parts(parts, Body::empty())
            },
        }
    } else {
        request_body_str = None;
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

    let monitor = state.monitor.clone();
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
        request_body: request_body_str,
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
        let mut log = log;
        log.response_body = Some(format!("[{}]", content_type));
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
        let mut collected_text = String::new();
        let mut last_few_bytes = Vec::new();
        let mut line_buffer = String::new();
        const MAX_COLLECTED_TEXT: usize = 512 * 1024;

        while let Some(chunk_res) = stream.next().await {
            if let Ok(chunk) = chunk_res {
                if let Ok(chunk_str) = std::str::from_utf8(&chunk) {
                    line_buffer.push_str(chunk_str);

                    while let Some(newline_pos) = line_buffer.find('\n') {
                        let line = line_buffer[..newline_pos].trim();
                        if let Some(json_str) = line.strip_prefix("data: ") {
                            let json_str = json_str.trim();
                            if json_str != "[DONE]" {
                                if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                                    extract_text_from_sse_line(
                                        &json,
                                        &mut collected_text,
                                        MAX_COLLECTED_TEXT,
                                    );
                                }
                            }
                        }
                        line_buffer = line_buffer[newline_pos + 1..].to_string();
                    }
                }

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

        if collected_text.is_empty() {
            log.response_body = Some("[Stream Data - No text extracted]".to_string());
        } else {
            log.response_body = Some(collected_text);
        }

        if let Ok(full_tail) = std::str::from_utf8(&last_few_bytes) {
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
                log.response_body = Some(s.to_string());
            } else {
                log.response_body = Some("[Binary Response Data]".to_string());
            }
            if log.status >= 400 {
                log.error = log.response_body.clone();
            }
            monitor.log_request(log).await;
            Response::from_parts(parts, Body::from(bytes))
        },
        Err(_) => {
            log.response_body = Some("[Response too large (>100MB)]".to_string());
            monitor.log_request(log).await;
            Response::from_parts(parts, Body::empty())
        },
    }
}
