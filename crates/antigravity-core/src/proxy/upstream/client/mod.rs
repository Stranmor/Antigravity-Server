mod request_executor;

#[cfg(test)]
mod tests;

use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::proxy::proxy_pool::ProxyPool;

// Cloud Code v1internal endpoints (fallback order: Sandbox → Daily → Prod)
// Ref upstream Issue #1176: prefer Sandbox/Daily to avoid Prod 429 errors
const V1_INTERNAL_BASE_URL_PROD: &str = "https://cloudcode-pa.googleapis.com/v1internal";
const V1_INTERNAL_BASE_URL_DAILY: &str = "https://daily-cloudcode-pa.googleapis.com/v1internal";
const V1_INTERNAL_BASE_URL_SANDBOX: &str =
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal";

fn default_upstream_urls() -> Vec<String> {
    vec![
        V1_INTERNAL_BASE_URL_SANDBOX.to_string(), // Priority 1: Sandbox (stable)
        V1_INTERNAL_BASE_URL_DAILY.to_string(),   // Priority 2: Daily (fallback)
        V1_INTERNAL_BASE_URL_PROD.to_string(),    // Priority 3: Prod (last resort)
    ]
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
    proxy_pool: Arc<ProxyPool>,
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
        let pool = {
            let config = proxy_config.try_read().ok();
            match config {
                Some(cfg) => ProxyPool::new(http_client.clone(), &cfg),
                None => ProxyPool::new(
                    http_client.clone(),
                    &crate::proxy::config::UpstreamProxyConfig::default(),
                ),
            }
        };

        let base_urls = resolve_upstream_urls(base_urls);

        Self { http_client, proxy_pool: Arc::new(pool), base_urls, proxy_config }
    }

    /// Get the proxy pool reference for external use.
    pub fn proxy_pool(&self) -> &Arc<ProxyPool> {
        &self.proxy_pool
    }

    /// Update proxy pool configuration (called on hot-reload).
    pub async fn update_proxy_config(&self) {
        let config = self.proxy_config.read().await;
        self.proxy_pool.update_config(&config).await;
    }

    /// Get a client for the given account, respecting per-account proxy and pool rotation.
    ///
    /// Priority: per-account proxy_url > pool/custom mode > direct.
    pub(crate) async fn get_client_for_account(
        &self,
        account_email: Option<&str>,
        account_proxy_url: Option<&str>,
    ) -> Result<Client, String> {
        // Per-account proxy takes absolute priority
        if let Some(proxy_url) = account_proxy_url {
            return self.proxy_pool.get_or_create_warp_client(proxy_url).await;
        }

        let config = self.proxy_config.read().await;
        let mode = config.mode;
        let enforce_proxy = config.enforce_proxy;
        drop(config);

        match mode {
            antigravity_types::models::UpstreamProxyMode::Pool => {
                self.proxy_pool.get_client(account_email).await
            },
            antigravity_types::models::UpstreamProxyMode::Custom => {
                self.proxy_pool.get_client(account_email).await
            },
            _ => {
                if enforce_proxy {
                    Err("enforce_proxy is enabled but no proxy available (mode is Direct/System, no per-account proxy_url) — blocking request to prevent IP leak".to_string())
                } else {
                    Ok(self.http_client.clone())
                }
            },
        }
    }

    /// Get the effective proxy URL for an account (for logging/headers).
    ///
    /// Per-account proxy_url takes priority over pool selection.
    pub(crate) async fn get_effective_proxy_url(
        &self,
        account_email: Option<&str>,
        account_proxy_url: Option<&str>,
    ) -> Result<Option<String>, String> {
        if let Some(url) = account_proxy_url {
            return Ok(Some(url.to_string()));
        }
        self.proxy_pool.select_proxy_url(account_email).await
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

    /// Call v1internal with per-account fingerprinting.
    ///
    /// This is the preferred method for account-specific requests. It injects:
    /// - Per-account User-Agent from the UA pool
    /// - `x-goog-api-client` header mimicking Google Cloud SDK
    /// - Device fingerprint identifiers
    ///
    /// Each account appears as a unique Antigravity IDE instance.
    /// Uses proxy pool rotation when Pool mode is enabled.
    pub async fn call_v1_internal_fingerprinted(
        &self,
        method: &str,
        access_token: &str,
        body: Value,
        query_string: Option<&str>,
        account_email: &str,
        account_proxy_url: Option<&str>,
    ) -> Result<reqwest::Response, String> {
        let headers = request_executor::build_headers_with_fingerprint(
            access_token,
            account_email,
            HashMap::new(),
        )?;
        let client = self.get_client_for_account(Some(account_email), account_proxy_url).await?;
        let proxy_url =
            self.get_effective_proxy_url(Some(account_email), account_proxy_url).await?;
        request_executor::execute_with_fallback(
            &client,
            method,
            headers,
            &body,
            query_string,
            proxy_url.as_deref(),
            &self.base_urls,
        )
        .await
    }

    /// Call v1internal with per-account fingerprinting via WARP proxy or proxy pool.
    ///
    /// Priority: explicit warp_proxy_url > per-account proxy_url > pool rotation.
    pub async fn call_v1_internal_fingerprinted_warp(
        &self,
        method: &str,
        access_token: &str,
        body: Value,
        query_string: Option<&str>,
        account_email: &str,
        extra_headers: HashMap<String, String>,
        warp_proxy_url: Option<&str>,
        account_proxy_url: Option<&str>,
    ) -> Result<reqwest::Response, String> {
        // Priority: explicit WARP > per-account proxy > pool rotation
        let (client, effective_proxy) = if let Some(proxy_url) = warp_proxy_url {
            let client = self.proxy_pool.get_or_create_warp_client(proxy_url).await?;
            (client, Some(proxy_url.to_string()))
        } else {
            let client =
                self.get_client_for_account(Some(account_email), account_proxy_url).await?;
            let proxy =
                self.get_effective_proxy_url(Some(account_email), account_proxy_url).await?;
            (client, proxy)
        };

        let headers = request_executor::build_headers_with_fingerprint(
            access_token,
            account_email,
            extra_headers,
        )?;
        request_executor::execute_with_fallback(
            &client,
            method,
            headers,
            &body,
            query_string,
            effective_proxy.as_deref(),
            &self.base_urls,
        )
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
        let (client, effective_proxy) = if let Some(proxy_url) = warp_proxy_url {
            let client = self.proxy_pool.get_or_create_warp_client(proxy_url).await?;
            (client, Some(proxy_url.to_string()))
        } else {
            let client = self.get_client_for_account(None, None).await?;
            let proxy = self.proxy_pool.select_proxy_url(None).await?;
            (client, proxy)
        };

        let headers = request_executor::build_headers(access_token, extra_headers)?;
        request_executor::execute_with_fallback(
            &client,
            method,
            headers,
            &body,
            query_string,
            effective_proxy.as_deref(),
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
        let client = self.get_client_for_account(None, None).await?;
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
