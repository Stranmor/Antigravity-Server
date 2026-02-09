mod request_executor;

#[cfg(test)]
mod tests;

use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::Duration;

use super::user_agent::DEFAULT_USER_AGENT;

const V1_INTERNAL_BASE_URL_PROD: &str = "https://cloudcode-pa.googleapis.com/v1internal";
const V1_INTERNAL_BASE_URL_DAILY: &str =
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal";

fn default_upstream_urls() -> Vec<String> {
    vec![V1_INTERNAL_BASE_URL_PROD.to_string(), V1_INTERNAL_BASE_URL_DAILY.to_string()]
}

fn resolve_upstream_urls(explicit: Option<Vec<String>>) -> Vec<String> {
    if let Some(urls) = explicit {
        return urls;
    }
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
}

pub struct UpstreamClient {
    http_client: Client,
    base_urls: Vec<String>,
    proxy_config: Arc<tokio::sync::RwLock<crate::proxy::config::UpstreamProxyConfig>>,
}

impl UpstreamClient {
    #[allow(clippy::expect_used, reason = "HTTP client is required for server to function")]
    pub fn new(
        proxy_config: crate::proxy::config::UpstreamProxyConfig,
        base_urls: Option<Vec<String>>,
    ) -> Self {
        let base_urls = resolve_upstream_urls(base_urls);
        let proxy_config = Arc::new(tokio::sync::RwLock::new(proxy_config));

        let http_client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .pool_max_idle_per_host(16)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(600))
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .expect("Failed to create HTTP client");

        Self { http_client, base_urls, proxy_config }
    }

    pub async fn update_proxy_config(&self, config: crate::proxy::config::UpstreamProxyConfig) {
        let mut guard = self.proxy_config.write().await;
        *guard = config;
    }

    async fn get_client(&self) -> Client {
        let config = self.proxy_config.read().await;
        if config.enabled && !config.url.is_empty() {
            // If proxy is enabled, we need a client with that proxy.
            // For performance, we could cache the client, but for now we build it.
            // In a real high-load scenario, we'd use a client pool or cache.
            Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .pool_max_idle_per_host(16)
                .pool_idle_timeout(Duration::from_secs(90))
                .tcp_keepalive(Duration::from_secs(60))
                .timeout(Duration::from_secs(600))
                .user_agent(DEFAULT_USER_AGENT)
                .proxy(reqwest::Proxy::all(&config.url).expect("Invalid proxy URL"))
                .build()
                .expect("Failed to build proxied client")
        } else {
            self.http_client.clone()
        }
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
            self.get_client().await
        };

        let headers = request_executor::build_headers(access_token, extra_headers)?;
        request_executor::execute_with_fallback(
            &client,
            method,
            headers,
            &body,
            query_string,
            warp_proxy_url,
            &self.base_urls,
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
        let client = self.get_client().await;
        request_executor::execute_with_fallback(
            &client,
            method,
            headers,
            &body,
            query_string,
            None,
            &self.base_urls,
        )
        .await
    }
}
