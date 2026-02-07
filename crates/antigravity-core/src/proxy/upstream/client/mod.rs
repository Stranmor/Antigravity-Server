mod request_executor;

#[cfg(test)]
mod tests;

use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::time::Duration;

use super::user_agent::DEFAULT_USER_AGENT;

const V1_INTERNAL_BASE_URL_PROD: &str = "https://cloudcode-pa.googleapis.com/v1internal";
const V1_INTERNAL_BASE_URL_DAILY: &str =
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal";

static UPSTREAM_BASE_URLS: OnceLock<Vec<String>> = OnceLock::new();

/// Configurable via `ANTIGRAVITY_UPSTREAM_URL` env var; defaults to prod + daily endpoints.
pub(crate) fn get_upstream_base_urls() -> &'static [String] {
    UPSTREAM_BASE_URLS.get_or_init(|| {
        if let Ok(raw) = std::env::var("ANTIGRAVITY_UPSTREAM_URL") {
            let url = raw.trim().trim_end_matches('/').to_string();
            if url.is_empty() {
                tracing::warn!("ANTIGRAVITY_UPSTREAM_URL is empty, using defaults");
                return default_upstream_urls();
            }
            if url::Url::parse(&url).is_err() {
                tracing::warn!("ANTIGRAVITY_UPSTREAM_URL is not a valid URL, using defaults");
                return default_upstream_urls();
            }
            tracing::info!("Using custom upstream URL");
            vec![url]
        } else {
            default_upstream_urls()
        }
    })
}

fn default_upstream_urls() -> Vec<String> {
    vec![V1_INTERNAL_BASE_URL_PROD.to_string(), V1_INTERNAL_BASE_URL_DAILY.to_string()]
}

pub struct UpstreamClient {
    http_client: Client,
}

impl UpstreamClient {
    #[allow(clippy::expect_used, reason = "HTTP client is required for server to function")]
    pub fn new(proxy_config: Option<crate::proxy::config::UpstreamProxyConfig>) -> Self {
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .pool_max_idle_per_host(16)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(600))
            .user_agent(DEFAULT_USER_AGENT);

        if let Some(config) = proxy_config {
            if config.enabled && !config.url.is_empty() {
                if let Ok(proxy) = reqwest::Proxy::all(&config.url) {
                    builder = builder.proxy(proxy);
                    tracing::info!("UpstreamClient enabled proxy: {}", config.url);
                }
            }
        }

        let http_client = builder.build().expect("Failed to create HTTP client");
        Self { http_client }
    }

    pub async fn call_v1_internal(
        &self,
        method: &str,
        access_token: &str,
        body: Value,
        query_string: Option<&str>,
    ) -> Result<reqwest::Response, String> {
        self.call_v1_internal_with_headers(method, access_token, body, query_string, HashMap::new())
            .await
    }

    pub async fn call_v1_internal_with_warp(
        &self,
        method: &str,
        access_token: &str,
        body: Value,
        query_string: Option<&str>,
        extra_headers: HashMap<String, String>,
        warp_proxy_url: Option<&str>,
    ) -> Result<reqwest::Response, String> {
        let client = if let Some(proxy_url) = warp_proxy_url {
            let proxy = reqwest::Proxy::all(proxy_url)
                .map_err(|e| format!("Invalid WARP proxy URL '{}': {}", proxy_url, e))?;

            Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .pool_max_idle_per_host(4)
                .pool_idle_timeout(Duration::from_secs(30))
                .tcp_keepalive(Duration::from_secs(60))
                .timeout(Duration::from_secs(600))
                .user_agent(DEFAULT_USER_AGENT)
                .proxy(proxy)
                .build()
                .map_err(|e| format!("Failed to create WARP client: {}", e))?
        } else {
            self.http_client.clone()
        };

        let headers = request_executor::build_headers(access_token, extra_headers)?;
        request_executor::execute_with_fallback(
            &client,
            method,
            headers,
            &body,
            query_string,
            warp_proxy_url,
        )
        .await
    }

    pub async fn call_v1_internal_with_headers(
        &self,
        method: &str,
        access_token: &str,
        body: Value,
        query_string: Option<&str>,
        extra_headers: HashMap<String, String>,
    ) -> Result<reqwest::Response, String> {
        let headers = request_executor::build_headers(access_token, extra_headers)?;
        request_executor::execute_with_fallback(
            &self.http_client,
            method,
            headers,
            &body,
            query_string,
            None,
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn fetch_available_models(&self, access_token: &str) -> Result<Value, String> {
        let headers = request_executor::build_headers(access_token, HashMap::new())?;
        request_executor::execute_fetch_models(&self.http_client, headers).await
    }
}
