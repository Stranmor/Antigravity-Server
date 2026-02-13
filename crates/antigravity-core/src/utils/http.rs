//! HTTP client utilities with proxy support.

use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};

/// Upstream proxy configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpstreamProxyConfig {
    pub enabled: bool,
    pub url: String,
}

/// Create HTTP client with default timeout and optional global proxy.
///
/// Note: In core library, we don't have access to config files directly.
/// The caller should provide proxy config if needed.
pub fn create_client(timeout_secs: u64) -> Client {
    create_client_with_proxy(timeout_secs, None)
}

/// Create HTTP client with specific proxy configuration.
pub fn create_client_with_proxy(
    timeout_secs: u64,
    proxy_config: Option<UpstreamProxyConfig>,
) -> Client {
    let mut builder = Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(std::time::Duration::from_secs(25))
        .http2_keep_alive_timeout(std::time::Duration::from_secs(10))
        .http2_keep_alive_while_idle(true);

    if let Some(config) = proxy_config {
        if config.enabled && !config.url.is_empty() {
            match Proxy::all(&config.url) {
                Ok(proxy) => {
                    builder = builder.proxy(proxy);
                    tracing::info!(url = %config.url, "HTTP client: upstream proxy enabled");
                },
                Err(e) => {
                    tracing::error!(url = %config.url, error = %e, "HTTP client: invalid proxy address");
                },
            }
        }
    }

    match builder.build() {
        Ok(client) => client,
        Err(e) => {
            tracing::error!(error = %e, "HTTP client builder failed, using default");
            Client::new()
        },
    }
}

/// Create an HTTP client that routes through the given account proxy URL.
///
/// This is the **single entry-point** that every service function should use
/// when it needs to make an HTTP request on behalf of a specific account.
/// If `proxy_url` is `None`, a regular (direct) client is returned.
pub fn create_client_for_account(timeout_secs: u64, proxy_url: Option<&str>) -> Client {
    match proxy_url {
        Some(url) => create_client_with_proxy(
            timeout_secs,
            Some(UpstreamProxyConfig { enabled: true, url: url.to_string() }),
        ),
        None => create_client(timeout_secs),
    }
}
