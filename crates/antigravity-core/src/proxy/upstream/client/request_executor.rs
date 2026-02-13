use reqwest::{header, Client, Response, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use tokio::time::Duration;

use super::super::device_fingerprint;
use super::super::endpoint_health::{
    ENDPOINT_HEALTH, MAX_TRANSPORT_RETRIES_PER_ENDPOINT, TRANSPORT_RETRY_DELAY_MS,
};
use super::super::user_agent::{get_user_agent_for_account, DEFAULT_USER_AGENT};

pub fn build_url(base_url: &str, method: &str, query_string: Option<&str>) -> String {
    if let Some(qs) = query_string {
        format!("{}:{}?{}", base_url, method, qs)
    } else {
        format!("{}:{}", base_url, method)
    }
}

fn should_try_next_endpoint(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::NOT_FOUND
        || status.is_server_error()
}

/// Build hardened request headers with per-account fingerprinting.
///
/// When `account_email` is provided, headers include:
/// - Per-account User-Agent from the UA pool
/// - `x-goog-api-client` header mimicking real Google Cloud SDK
/// - Google API User-Agent (`google-api-nodejs-client/9.15.1`)
///
/// These headers make each account's requests look like they come from
/// a unique, legitimate Antigravity IDE instance.
pub fn build_headers(
    access_token: &str,
    extra_headers: HashMap<String, String>,
) -> Result<header::HeaderMap, String> {
    let mut headers = header::HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", access_token))
            .map_err(|e| e.to_string())?,
    );

    // Determine User-Agent: prefer per-account UA from extra_headers,
    // otherwise use default
    let has_custom_ua =
        extra_headers.contains_key("user-agent") || extra_headers.contains_key("User-Agent");

    if !has_custom_ua {
        headers.insert(header::USER_AGENT, header::HeaderValue::from_static(DEFAULT_USER_AGENT));
    }

    for (k, v) in extra_headers {
        if let Ok(hk) = header::HeaderName::from_bytes(k.as_bytes()) {
            if let Ok(hv) = header::HeaderValue::from_str(&v) {
                headers.insert(hk, hv);
            }
        }
    }

    Ok(headers)
}

/// Build headers with per-account fingerprinting.
///
/// This is the preferred method when account email is known. It:
/// 1. Selects a deterministic User-Agent for this account
/// 2. Adds `x-goog-api-client` header matching Google SDK patterns
/// 3. Adds Google API client User-Agent
/// 4. Merges any additional extra_headers
///
/// This makes each account appear as a unique Antigravity IDE instance.
pub fn build_headers_with_fingerprint(
    access_token: &str,
    account_email: &str,
    extra_headers: HashMap<String, String>,
) -> Result<header::HeaderMap, String> {
    let fp = device_fingerprint::get_fingerprint(account_email);
    let ua = get_user_agent_for_account(account_email);

    let mut merged_headers = fp.to_extra_headers();

    // Add per-account User-Agent
    merged_headers.insert("User-Agent".to_string(), ua.to_string());

    // Merge caller's extra headers (they take precedence)
    for (k, v) in extra_headers {
        merged_headers.insert(k, v);
    }

    build_headers(access_token, merged_headers)
}

pub async fn execute_with_fallback(
    client: &Client,
    method: &str,
    headers: header::HeaderMap,
    body: &Value,
    query_string: Option<&str>,
    warp_proxy_url: Option<&str>,
    base_urls: &[String],
) -> Result<Response, String> {
    let mut last_err: Option<String> = None;

    for (idx, base_url) in base_urls.iter().enumerate() {
        if ENDPOINT_HEALTH.get(base_url.as_str()).is_some_and(|h| h.should_skip()) {
            tracing::debug!("Skipping unhealthy endpoint: {}", base_url);
            continue;
        }

        let url = build_url(base_url, method, query_string);
        let has_next = idx + 1 < base_urls.len();
        let mut transport_retries: u32 = 0;

        loop {
            let response = client.post(&url).headers(headers.clone()).json(body).send().await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    if status.is_success() {
                        ENDPOINT_HEALTH.entry(base_url.clone()).or_default().record_success();

                        if warp_proxy_url.is_some() {
                            tracing::debug!(
                                "WARP request succeeded | Endpoint: {} | Status: {}",
                                base_url,
                                status
                            );
                        } else if idx > 0 {
                            tracing::info!(
                                "Upstream fallback succeeded | Endpoint: {} | Status: {} | Attempt: {}/{}",
                                base_url,
                                status,
                                idx + 1,
                                base_urls.len()
                            );
                        } else {
                            tracing::debug!(
                                "Upstream request succeeded | Endpoint: {} | Status: {}",
                                base_url,
                                status
                            );
                        }
                        return Ok(resp);
                    }

                    if has_next && should_try_next_endpoint(status) {
                        tracing::warn!(
                            "Upstream endpoint returned {} at {} (method={}), trying next",
                            status,
                            base_url,
                            method
                        );
                        last_err = Some(format!("Upstream {} returned {}", base_url, status));
                        break;
                    }

                    return Ok(resp);
                },
                Err(e) => {
                    let msg = format!("HTTP request failed at {}: {}", base_url, e);
                    tracing::error!("{}", msg);
                    last_err = Some(msg);

                    if transport_retries < MAX_TRANSPORT_RETRIES_PER_ENDPOINT {
                        transport_retries += 1;
                        tracing::warn!(
                            "Transport error at {}, retry {}/{} after {}ms",
                            base_url,
                            transport_retries,
                            MAX_TRANSPORT_RETRIES_PER_ENDPOINT,
                            TRANSPORT_RETRY_DELAY_MS
                        );
                        tokio::time::sleep(Duration::from_millis(TRANSPORT_RETRY_DELAY_MS)).await;
                        continue;
                    }

                    ENDPOINT_HEALTH.entry(base_url.clone()).or_default().record_failure();

                    if !has_next {
                        return Err(last_err.unwrap_or_else(|| "All endpoints failed".to_string()));
                    }
                    break;
                },
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "All endpoints failed".to_string()))
}
