use crate::proxy::config::UpstreamProxyConfig;
use std::time::Duration;

/// Build HTTP client with optional upstream proxy and timeout.
///
/// When `enforce_proxy` is `true` on the config and no proxy is available
/// (disabled or missing URL), returns `Err` to prevent direct connections.
pub fn build_http_client(
    upstream_proxy: Option<&UpstreamProxyConfig>,
    timeout_secs: u64,
) -> Result<reqwest::Client, String> {
    if timeout_secs < 5 {
        tracing::warn!(
            requested = timeout_secs,
            clamped_to = 5,
            "HTTP client timeout clamped to minimum 5 seconds"
        );
    }

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(5)))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(Duration::from_secs(25))
        .http2_keep_alive_timeout(Duration::from_secs(10))
        .http2_keep_alive_while_idle(true);

    let mut proxy_active = false;

    if let Some(config) = upstream_proxy {
        if config.enabled {
            if config.url.is_empty() {
                return Err("Upstream proxy enabled but URL is empty".to_string());
            }
            let proxy = reqwest::Proxy::all(&config.url)
                .map_err(|e| format!("Invalid upstream proxy url: {}", e))?;
            builder = builder.proxy(proxy);
            proxy_active = true;
        }

        // enforce_proxy: block direct connections when no proxy is active
        if config.enforce_proxy && !proxy_active {
            return Err(
                "enforce_proxy is enabled but no upstream proxy is active — direct connection blocked".to_string()
            );
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
    fn test_proxy_empty_url_returns_error() {
        let cfg = UpstreamProxyConfig { enabled: true, url: String::new(), ..Default::default() };
        let result = build_http_client(Some(&cfg), 30);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_timeout_clamped() {
        let result = build_http_client(None, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_enforce_proxy_blocks_when_disabled() {
        let cfg = UpstreamProxyConfig { enabled: false, enforce_proxy: true, ..Default::default() };
        let result = build_http_client(Some(&cfg), 30);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("enforce_proxy"));
    }

    #[test]
    fn test_enforce_proxy_allows_when_proxy_active() {
        let cfg = UpstreamProxyConfig {
            enabled: true,
            url: "http://proxy:8080".into(),
            enforce_proxy: true,
            ..Default::default()
        };
        let result = build_http_client(Some(&cfg), 30);
        assert!(result.is_ok());
    }

    #[test]
    fn test_enforce_proxy_false_allows_direct() {
        let cfg =
            UpstreamProxyConfig { enabled: false, enforce_proxy: false, ..Default::default() };
        let result = build_http_client(Some(&cfg), 30);
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
