//! Gemini streaming response handling.
//!
//! Contains SSE stream processing, peek logic, and response building.

use axum::{body::Body, http::StatusCode, response::Response};
use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use serde_json::Value;
use tracing::{debug, error, warn};

use crate::proxy::mappers::gemini::unwrap_response;

/// Peek first SSE chunk with retry logic (Issue #859)
/// Returns the first meaningful data chunk, skipping heartbeats.
/// On timeout/empty/error, returns Err for account rotation.
pub async fn peek_first_chunk<S>(stream: &mut S) -> Result<Bytes, String>
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

        match tokio::time::timeout(std::time::Duration::from_secs(PEEK_TIMEOUT_SECS), stream.next())
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
            },
            Ok(Some(Err(e))) => {
                return Err(format!("Stream error during peek: {}", e));
            },
            Ok(None) => {
                return Err("Stream ended immediately (empty response)".to_string());
            },
            Err(_) => {
                return Err(format!("Timeout ({}s) waiting for first chunk", PEEK_TIMEOUT_SECS));
            },
        }
    }
}

/// Extract thought signature from response and cache it.
pub fn extract_signature(resp: &Value, session_id: &str) {
    let inner = resp.get("response").unwrap_or(resp);
    if let Some(candidates) = inner.get("candidates").and_then(|c| c.as_array()) {
        for cand in candidates {
            if let Some(parts) =
                cand.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array())
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

/// Build SSE streaming response with buffer overflow protection.
pub async fn build_stream_response<S>(
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
        const MAX_BUFFER_SIZE: usize = 50 * 1024 * 1024; // 50MB — supports 2K+ image generation
        let mut buffer = BytesMut::new();
        let mut first_data = Some(first_chunk);

        loop {
            let item = match first_data.take() {
                Some(fd) => Some(Ok(fd)),
                None => response_stream.next().await,
            };

            let bytes = match item {
                Some(Ok(b)) => b,
                Some(Err(e)) => { error!("[Gemini-SSE] {}", e); yield Err("Stream error".to_string()); break; }
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Response build error: {}", e)))?;

    Ok(resp)
}
