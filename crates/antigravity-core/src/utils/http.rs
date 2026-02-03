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
    let mut builder = Client::builder().timeout(std::time::Duration::from_secs(timeout_secs));

    if let Some(config) = proxy_config {
        if config.enabled && !config.url.is_empty() {
            match Proxy::all(&config.url) {
                Ok(proxy) => {
                    builder = builder.proxy(proxy);
                    tracing::info!("HTTP clientalreadyenableupstreamproxy: {}", config.url);
                }
                Err(e) => {
                    tracing::error!("invalid proxyaddress: {}, error: {}", config.url, e);
                }
            }
        }
    }

    builder.build().unwrap_or_else(|_| Client::new())
}
