use crate::proxy::config::UpstreamProxyConfig;
use std::time::Duration;

/// Build HTTP client with optional upstream proxy and timeout.
pub fn build_http_client(
    upstream_proxy: Option<&UpstreamProxyConfig>,
    timeout_secs: u64,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(5)))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(Duration::from_secs(25))
        .http2_keep_alive_timeout(Duration::from_secs(10))
        .http2_keep_alive_while_idle(true);

    if let Some(config) = upstream_proxy {
        if config.enabled && !config.url.is_empty() {
            let proxy = reqwest::Proxy::all(&config.url)
                .map_err(|e| format!("Invalid upstream proxy url: {}", e))?;
            builder = builder.proxy(proxy);
        }
    }

    builder.build().map_err(|e| format!("Failed to build HTTP client: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::config::UpstreamProxyConfig;

    #[test]
    fn test_no_proxy() {
        let result = build_http_client(None, 30);
        assert!(result.is_ok());
    }

    #[test]
    fn test_proxy_disabled() {
        let cfg =
            UpstreamProxyConfig { enabled: false, url: "http://x".into(), ..Default::default() };
        let result = build_http_client(Some(&cfg), 30);
        assert!(result.is_ok());
    }

    #[test]
    fn test_proxy_empty_url() {
        let cfg = UpstreamProxyConfig { enabled: true, url: String::new(), ..Default::default() };
        let result = build_http_client(Some(&cfg), 30);
        assert!(result.is_ok());
    }

    #[test]
    fn test_timeout_clamped() {
        let result = build_http_client(None, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_socks5_proxy() {
        let cfg = UpstreamProxyConfig {
            enabled: true,
            url: "socks5://127.0.0.1:1080".into(),
            ..Default::default()
        };
        let result = build_http_client(Some(&cfg), 30);
        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_http_proxy() {
        let cfg = UpstreamProxyConfig {
            enabled: true,
            url: "http://proxy:8080".into(),
            ..Default::default()
        };
        let result = build_http_client(Some(&cfg), 30);
        assert!(result.is_ok());
    }
}
