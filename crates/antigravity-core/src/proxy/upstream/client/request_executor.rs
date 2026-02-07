use reqwest::{header, Client, Response, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use tokio::time::Duration;

use super::super::endpoint_health::{
    ENDPOINT_HEALTH, MAX_TRANSPORT_RETRIES_PER_ENDPOINT, TRANSPORT_RETRY_DELAY_MS,
};
use super::super::user_agent::DEFAULT_USER_AGENT;
use super::get_upstream_base_urls;

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
    headers.insert(header::USER_AGENT, header::HeaderValue::from_static(DEFAULT_USER_AGENT));

    for (k, v) in extra_headers {
        if let Ok(hk) = header::HeaderName::from_bytes(k.as_bytes()) {
            if let Ok(hv) = header::HeaderValue::from_str(&v) {
                headers.insert(hk, hv);
            }
        }
    }

    Ok(headers)
}

pub async fn execute_with_fallback(
    client: &Client,
    method: &str,
    headers: header::HeaderMap,
    body: &Value,
    query_string: Option<&str>,
    warp_proxy_url: Option<&str>,
) -> Result<Response, String> {
    let mut last_err: Option<String> = None;

    let base_urls = get_upstream_base_urls();

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
                    tracing::debug!("{}", msg);
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

pub async fn execute_fetch_models(
    client: &Client,
    headers: header::HeaderMap,
) -> Result<Value, String> {
    let mut last_err: Option<String> = None;

    let base_urls = get_upstream_base_urls();

    for (idx, base_url) in base_urls.iter().enumerate() {
        if ENDPOINT_HEALTH.get(base_url.as_str()).is_some_and(|h| h.should_skip()) {
            tracing::debug!("Skipping unhealthy endpoint: {}", base_url);
            continue;
        }

        let url = build_url(base_url, "fetchAvailableModels", None);
        let has_next = idx + 1 < base_urls.len();
        let mut transport_retries: u32 = 0;

        loop {
            let response = client
                .post(&url)
                .headers(headers.clone())
                .json(&serde_json::json!({}))
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    if status.is_success() {
                        ENDPOINT_HEALTH.entry(base_url.clone()).or_default().record_success();

                        if idx > 0 {
                            tracing::info!(
                                "Upstream fallback succeeded for fetchAvailableModels | Endpoint: {} | Status: {}",
                                base_url,
                                status
                            );
                        } else {
                            tracing::debug!(
                                "fetchAvailableModels succeeded | Endpoint: {}",
                                base_url
                            );
                        }
                        let json: Value =
                            resp.json().await.map_err(|e| format!("Parse json failed: {}", e))?;
                        return Ok(json);
                    }

                    if has_next && should_try_next_endpoint(status) {
                        tracing::warn!(
                            "fetchAvailableModels returned {} at {}, trying next endpoint",
                            status,
                            base_url
                        );
                        last_err = Some(format!("Upstream error: {}", status));
                        break;
                    }

                    return Err(format!("Upstream error: {}", status));
                },
                Err(e) => {
                    let msg = format!("Request failed at {}: {}", base_url, e);
                    tracing::debug!("{}", msg);
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
                        break;
                    }
                    break;
                },
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "All endpoints failed".to_string()))
}
