use crate::proxy::config::UpstreamProxyConfig;
use std::time::Duration;

/// Build HTTP client with optional upstream proxy and timeout.
pub fn build_http_client(
    upstream_proxy: Option<&UpstreamProxyConfig>,
    timeout_secs: u64,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(5)))
        .tcp_nodelay(true);

    if let Some(config) = upstream_proxy {
        if config.enabled && !config.url.is_empty() {
            let proxy = reqwest::Proxy::all(&config.url)
                .map_err(|e| format!("Invalid upstream proxy url: {}", e))?;
            builder = builder.proxy(proxy);
        }
    }

    builder.build().map_err(|e| format!("Failed to build HTTP client: {}", e))
}
