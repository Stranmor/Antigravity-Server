// Request/response monitoring: byte counting and size limits.
// All arithmetic is on usize for buffer sizes, bounded by MAX_*_LOG_SIZE constants.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "Monitoring middleware: bounded buffer sizes, safe byte operations"
)]

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
use std::time::Instant;

const MAX_REQUEST_LOG_SIZE: usize = 100 * 1024 * 1024; // 100MB
const MAX_RESPONSE_LOG_SIZE: usize = 100 * 1024 * 1024; // 100MB for image responses

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

    // Extract account email from X-Account-Email header if present
    let account_email = response
        .headers()
        .get("X-Account-Email")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Extract mapped model from X-Mapped-Model header if present
    let mapped_model = response
        .headers()
        .get("X-Mapped-Model")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Extract mapping reason from X-Mapping-Reason header if present
    let mapping_reason = response
        .headers()
        .get("X-Mapping-Reason")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let monitor = state.monitor.clone();
    let mut log = ProxyRequestLog {
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
        let (parts, body) = response.into_parts();
        let mut stream = body.into_data_stream();
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        tokio::spawn(async move {
            let mut collected_text = String::new();
            let mut last_few_bytes = Vec::new();
            const MAX_COLLECTED_TEXT: usize = 512 * 1024; // 512KB limit for collected text

            while let Some(chunk_res) = stream.next().await {
                if let Ok(chunk) = chunk_res {
                    // Parse SSE events to extract text content
                    if let Ok(chunk_str) = std::str::from_utf8(&chunk) {
                        for line in chunk_str.lines() {
                            if let Some(json_str) = line.strip_prefix("data: ") {
                                let json_str = json_str.trim();
                                if json_str == "[DONE]" {
                                    continue;
                                }
                                if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                                    // Claude/Anthropic: content_block_delta with delta.text
                                    if let Some(delta) = json.get("delta") {
                                        if let Some(text) =
                                            delta.get("text").and_then(|v| v.as_str())
                                        {
                                            if collected_text.len() + text.len()
                                                <= MAX_COLLECTED_TEXT
                                            {
                                                collected_text.push_str(text);
                                            }
                                        }
                                    }
                                    // OpenAI: choices[0].delta.content
                                    if let Some(choices) =
                                        json.get("choices").and_then(|v| v.as_array())
                                    {
                                        if let Some(choice) = choices.first() {
                                            if let Some(content) = choice
                                                .get("delta")
                                                .and_then(|d| d.get("content"))
                                                .and_then(|v| v.as_str())
                                            {
                                                if collected_text.len() + content.len()
                                                    <= MAX_COLLECTED_TEXT
                                                {
                                                    collected_text.push_str(content);
                                                }
                                            }
                                        }
                                    }
                                    // Gemini: candidates[0].content.parts[0].text
                                    if let Some(candidates) =
                                        json.get("candidates").and_then(|v| v.as_array())
                                    {
                                        if let Some(candidate) = candidates.first() {
                                            if let Some(parts) = candidate
                                                .get("content")
                                                .and_then(|c| c.get("parts"))
                                                .and_then(|p| p.as_array())
                                            {
                                                for part in parts {
                                                    if let Some(text) =
                                                        part.get("text").and_then(|v| v.as_str())
                                                    {
                                                        if collected_text.len() + text.len()
                                                            <= MAX_COLLECTED_TEXT
                                                        {
                                                            collected_text.push_str(text);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Keep last bytes for usage extraction
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

            // Set collected text as response body
            if collected_text.is_empty() {
                log.response_body = Some("[Stream Data - No text extracted]".to_string());
            } else {
                log.response_body = Some(collected_text);
            }

            // Extract usage info from last bytes
            if let Ok(full_tail) = std::str::from_utf8(&last_few_bytes) {
                for line in full_tail.lines().rev() {
                    if line.starts_with("data: ")
                        && (line.contains("\"usage\"") || line.contains("\"usageMetadata\""))
                    {
                        let json_str = line.trim_start_matches("data: ").trim();
                        if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                            // support OpenAI "usage" or Gemini "usageMetadata"
                            if let Some(usage) = json.get("usage").or(json.get("usageMetadata")) {
                                log.input_tokens = usage
                                    .get("prompt_tokens")
                                    .or(usage.get("input_tokens"))
                                    .or(usage.get("promptTokenCount"))
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32);
                                log.output_tokens = usage
                                    .get("completion_tokens")
                                    .or(usage.get("output_tokens"))
                                    .or(usage.get("candidatesTokenCount"))
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32);
                                log.cached_tokens = usage
                                    .get("cachedContentTokenCount")
                                    .or(usage.get("cache_read_input_tokens"))
                                    .and_then(|v| v.as_u64())
                                    .or_else(|| {
                                        usage
                                            .get("prompt_tokens_details")
                                            .and_then(|d| d.get("cached_tokens"))
                                            .and_then(|v| v.as_u64())
                                    })
                                    .map(|v| v as u32);

                                if log.input_tokens.is_none() && log.output_tokens.is_none() {
                                    log.output_tokens = usage
                                        .get("total_tokens")
                                        .or(usage.get("totalTokenCount"))
                                        .and_then(|v| v.as_u64())
                                        .map(|v| v as u32);
                                }
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

        Response::from_parts(
            parts,
            Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)),
        )
    } else if content_type.contains("application/json") || content_type.contains("text/") {
        let (parts, body) = response.into_parts();
        match axum::body::to_bytes(body, MAX_RESPONSE_LOG_SIZE).await {
            Ok(bytes) => {
                if let Ok(s) = std::str::from_utf8(&bytes) {
                    if let Ok(json) = serde_json::from_str::<Value>(s) {
                        // support OpenAI "usage" or Gemini "usageMetadata"
                        if let Some(usage) = json.get("usage").or(json.get("usageMetadata")) {
                            log.input_tokens = usage
                                .get("prompt_tokens")
                                .or(usage.get("input_tokens"))
                                .or(usage.get("promptTokenCount"))
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32);
                            log.output_tokens = usage
                                .get("completion_tokens")
                                .or(usage.get("output_tokens"))
                                .or(usage.get("candidatesTokenCount"))
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32);
                            log.cached_tokens = usage
                                .get("cachedContentTokenCount")
                                .or(usage.get("cache_read_input_tokens"))
                                .and_then(|v| v.as_u64())
                                .or_else(|| {
                                    usage
                                        .get("prompt_tokens_details")
                                        .and_then(|d| d.get("cached_tokens"))
                                        .and_then(|v| v.as_u64())
                                })
                                .map(|v| v as u32);

                            if log.input_tokens.is_none() && log.output_tokens.is_none() {
                                log.output_tokens = usage
                                    .get("total_tokens")
                                    .or(usage.get("totalTokenCount"))
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32);
                            }
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
    } else {
        log.response_body = Some(format!("[{}]", content_type));
        monitor.log_request(log).await;
        response
    }
}
