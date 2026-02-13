//! HTTP client utilities with proxy support.

use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};

fn redact_proxy_url(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| {
            if u.username().is_empty() && u.password().is_none() {
                format!(
                    "{}://{}:{}",
                    u.scheme(),
                    u.host_str().unwrap_or("?"),
                    u.port().unwrap_or(0)
                )
            } else {
                format!(
                    "{}://***@{}:{}",
                    u.scheme(),
                    u.host_str().unwrap_or("?"),
                    u.port().unwrap_or(0)
                )
            }
        })
        .unwrap_or_else(|_| "<invalid-url>".to_string())
}

/// Upstream proxy configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpstreamProxyConfig {
    pub enabled: bool,
    pub url: String,
}

/// Create HTTP client with default timeout and no proxy.
///
/// Note: In core library, we don't have access to config files directly.
/// The caller should provide proxy config if needed.
pub fn create_client(timeout_secs: u64) -> Result<Client, String> {
    base_builder(timeout_secs).build().map_err(|e| format!("HTTP client builder failed: {e}"))
}

/// Shared builder with keepalive settings.
fn base_builder(timeout_secs: u64) -> reqwest::ClientBuilder {
    Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(std::time::Duration::from_secs(25))
        .http2_keep_alive_timeout(std::time::Duration::from_secs(10))
        .http2_keep_alive_while_idle(true)
}

/// Create HTTP client with specific proxy configuration.
///
/// Returns `Err` if proxy is configured but the URL is invalid or the
/// client cannot be built — **never** silently falls back to a direct
/// connection when a proxy was requested.
pub fn create_client_with_proxy(
    timeout_secs: u64,
    proxy_config: Option<UpstreamProxyConfig>,
) -> Result<Client, String> {
    let mut builder = base_builder(timeout_secs);

    if let Some(config) = proxy_config {
        if config.enabled {
            if config.url.is_empty() {
                return Err(
                    "proxy enabled but URL is empty — refusing to fall back to direct connection"
                        .to_string(),
                );
            }
            let proxy = Proxy::all(&config.url).map_err(|e| format!("invalid proxy URL: {e}"))?;
            builder = builder.proxy(proxy);
            tracing::info!(host = %redact_proxy_url(&config.url), "HTTP client: upstream proxy enabled");
        }
    }

    builder.build().map_err(|e| format!("HTTP client builder failed: {e}"))
}

/// Create an HTTP client that routes through the given account proxy URL.
///
/// This is the **single entry-point** that every service function should use
/// when it needs to make an HTTP request on behalf of a specific account.
///
/// When `enforce_proxy` is `true` and `proxy_url` is `None`, returns an error
/// instead of silently falling back to a direct (no-proxy) connection — this
/// prevents IP leaks when the caller requires all traffic to be proxied.
///
/// When `enforce_proxy` is `false` and `proxy_url` is `None`, a regular
/// (direct) client is returned (legacy behaviour).
pub fn create_client_for_account(
    timeout_secs: u64,
    proxy_url: Option<&str>,
    enforce_proxy: bool,
) -> Result<Client, String> {
    match proxy_url {
        Some(url) if !url.is_empty() => create_client_with_proxy(
            timeout_secs,
            Some(UpstreamProxyConfig { enabled: true, url: url.to_string() }),
        ),
        _ if enforce_proxy => {
            Err("enforce_proxy is enabled but account has no proxy_url configured".to_string())
        },
        _ => create_client(timeout_secs),
    }
}
