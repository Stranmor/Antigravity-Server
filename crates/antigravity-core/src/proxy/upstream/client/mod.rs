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
    proxied_client: Arc<tokio::sync::RwLock<Option<(String, Client)>>>,
    warp_client: Arc<tokio::sync::RwLock<Option<(String, Client)>>>,
    base_urls: Vec<String>,
    proxy_config: Arc<tokio::sync::RwLock<crate::proxy::config::UpstreamProxyConfig>>,
}

impl UpstreamClient {
    /// Create a new UpstreamClient with the given HTTP client.
    ///
    /// Accepts a pre-built `reqwest::Client` to avoid blocking TLS initialization
    /// inside an async runtime (which causes a panic on native-tls).
    pub fn new(
        http_client: Client,
        proxy_config: Arc<tokio::sync::RwLock<crate::proxy::config::UpstreamProxyConfig>>,
        base_urls: Option<Vec<String>>,
    ) -> Self {
        let base_urls = resolve_upstream_urls(base_urls);

        Self {
            http_client,
            proxied_client: Arc::new(tokio::sync::RwLock::new(None)),
            warp_client: Arc::new(tokio::sync::RwLock::new(None)),
            base_urls,
            proxy_config,
        }
    }

    async fn get_client(&self) -> Client {
        let config = self.proxy_config.read().await;
        if config.enabled && !config.url.is_empty() {
            {
                let client_guard = self.proxied_client.read().await;
                if let Some((cached_url, client)) = client_guard.as_ref() {
                    if cached_url == &config.url {
                        return client.clone();
                    }
                }
            }

            let mut client_guard = self.proxied_client.write().await;
            if let Some((cached_url, client)) = client_guard.as_ref() {
                if cached_url == &config.url {
                    return client.clone();
                }
            }

            let proxy = match reqwest::Proxy::all(&config.url) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        "Invalid proxy URL '{}': {}. Falling back to direct.",
                        config.url,
                        e
                    );
                    return self.http_client.clone();
                },
            };

            let fallback = self.http_client.clone();
            let new_client = tokio::task::spawn_blocking(move || {
                Client::builder()
                    .connect_timeout(Duration::from_secs(20))
                    .pool_max_idle_per_host(16)
                    .pool_idle_timeout(Duration::from_secs(90))
                    .tcp_keepalive(Duration::from_secs(60))
                    .timeout(Duration::from_secs(600))
                    .user_agent(DEFAULT_USER_AGENT)
                    .proxy(proxy)
                    .build()
                    .unwrap_or_else(|e| {
                        tracing::error!(
                            "Failed to build proxied client: {}. Falling back to direct.",
                            e
                        );
                        fallback
                    })
            })
            .await
            .unwrap_or_else(|e| {
                tracing::error!("spawn_blocking panicked building proxied client: {}", e);
                self.http_client.clone()
            });

            *client_guard = Some((config.url.clone(), new_client.clone()));
            new_client
        } else {
            self.http_client.clone()
        }
    }

    async fn get_warp_client(&self, proxy_url: &str) -> Result<Client, String> {
        {
            let client_guard = self.warp_client.read().await;
            if let Some((cached_url, client)) = client_guard.as_ref() {
                if cached_url == proxy_url {
                    return Ok(client.clone());
                }
            }
        }

        let proxy_url_owned = proxy_url.to_string();
        let mut client_guard = self.warp_client.write().await;
        if let Some((cached_url, client)) = client_guard.as_ref() {
            if cached_url == proxy_url {
                return Ok(client.clone());
            }
        }

        let proxy = reqwest::Proxy::all(&proxy_url_owned)
            .map_err(|e| format!("Invalid WARP proxy URL '{}': {}", proxy_url_owned, e))?;

        let new_client = tokio::task::spawn_blocking(move || {
            Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .pool_max_idle_per_host(4)
                .pool_idle_timeout(Duration::from_secs(30))
                .tcp_keepalive(Duration::from_secs(60))
                .timeout(Duration::from_secs(600))
                .user_agent(DEFAULT_USER_AGENT)
                .proxy(proxy)
                .build()
                .map_err(|e| format!("Failed to create WARP client: {}", e))
        })
        .await
        .map_err(|e| format!("spawn_blocking panicked building WARP client: {}", e))??;

        *client_guard = Some((proxy_url_owned, new_client.clone()));
        Ok(new_client)
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
            self.get_warp_client(proxy_url).await?
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
