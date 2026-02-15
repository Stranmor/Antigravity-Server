//! Proxy URL validation and health checking for per-account proxies.

use futures::StreamExt as _;
use std::net::{IpAddr, ToSocketAddrs as _};
use std::time::Duration;

const PROXY_HEALTH_TIMEOUT_SECS: u64 = 15;
const MAX_HEALTH_RESPONSE_BYTES: usize = 8192;
const DEFAULT_HEALTH_CHECK_URL: &str = "https://ifconfig.co";
const FALLBACK_HEALTH_CHECK_URL: &str = "https://api.ipify.org";

fn health_check_urls() -> Vec<&'static str> {
    static URLS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let urls = URLS.get_or_init(|| {
        if let Ok(custom) = std::env::var("ANTIGRAVITY_HEALTH_CHECK_URL") {
            vec![custom]
        } else {
            vec![DEFAULT_HEALTH_CHECK_URL.to_owned(), FALLBACK_HEALTH_CHECK_URL.to_owned()]
        }
    });
    urls.iter().map(String::as_str).collect()
}

pub fn validate_proxy_url(raw: &str) -> Result<(), String> {
    const VALID_SCHEMES: &[&str] = &["socks5", "socks5h", "http", "https"];

    let parsed = reqwest::Url::parse(raw).map_err(|e| format!("Malformed proxy URL: {e}"))?;

    if !VALID_SCHEMES.contains(&parsed.scheme()) {
        return Err(format!(
            "Invalid proxy URL scheme '{}'. Must be one of: {}",
            parsed.scheme(),
            VALID_SCHEMES.join(", ")
        ));
    }

    if let Some(host_str) = parsed.host_str() {
        let bare_host =
            host_str.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(host_str);
        if let Ok(ip) = bare_host.parse::<IpAddr>() {
            if is_private_ip(ip) {
                return Err(format!("Proxy URL points to private/reserved IP: {ip}"));
            }
        }
        let lower = host_str.to_ascii_lowercase();
        if lower == "localhost"
            || lower.ends_with(".local")
            || lower.ends_with(".internal")
            || lower == "metadata.google.internal"
        {
            return Err(format!("Proxy URL points to reserved hostname: {host_str}"));
        }
        // IP-based hostnames already checked above; domain hostnames checked via async DNS in
        // resolve_and_check_dns() called by check_proxy_health() and validate_proxy_url_async().
    }

    Ok(())
}

/// Async DNS defense-in-depth: resolves hostname, rejects if any resolved IP is private.
/// Point-in-time check — DNS can change between validation and connection.
async fn resolve_and_check_dns(raw: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(raw).map_err(|e| format!("Malformed proxy URL: {e}"))?;
    if let Some(host_str) = parsed.host_str() {
        let bare_host =
            host_str.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(host_str);
        if bare_host.parse::<IpAddr>().is_err() {
            let port = parsed.port().unwrap_or(1080);
            let addr_str = format!("{bare_host}:{port}");
            let host_display = host_str.to_string();
            let resolved = tokio::task::spawn_blocking(move || addr_str.to_socket_addrs())
                .await
                .map_err(|e| format!("DNS resolution task failed: {e}"))?;
            if let Ok(addrs) = resolved {
                for sock_addr in addrs {
                    if is_private_ip(sock_addr.ip()) {
                        return Err(format!(
                            "Proxy hostname resolves to private IP: {} -> {}",
                            host_display,
                            sock_addr.ip()
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Async validation: sync URL checks + async DNS resolution.
/// Use in API handlers. For sync-only contexts, use `validate_proxy_url`.
pub async fn validate_proxy_url_async(raw: &str) -> Result<(), String> {
    validate_proxy_url(raw)?;
    resolve_and_check_dns(raw).await
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_unspecified() // 0.0.0.0 → routes to localhost on Linux
                || v4.is_private()
                || v4.is_link_local()
                || (v4.octets()[0] == 100 && v4.octets()[1] >= 64 && v4.octets()[1] <= 127)
                || (v4.octets()[0] == 198 && (v4.octets()[1] == 18 || v4.octets()[1] == 19))
        },
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            // to_ipv4() covers BOTH ::ffff:x.x.x.x (mapped) AND ::x.x.x.x (compatible)
            if let Some(v4) = v6.to_ipv4_mapped().or_else(|| v6.to_ipv4()) {
                return is_private_ip(IpAddr::V4(v4));
            }
            let seg0 = v6.segments()[0];
            (seg0 & 0xFE00) == 0xFC00  // fc00::/7 unique local
                || (seg0 & 0xFFC0) == 0xFE80  // fe80::/10 link-local
                || (seg0 & 0xFFC0) == 0xFEC0 // fec0::/10 site-local (deprecated)
        },
    }
}

pub async fn check_proxy_health(proxy_url: &str) -> Result<String, String> {
    validate_proxy_url_async(proxy_url).await?;

    let proxy = reqwest::Proxy::all(proxy_url).map_err(|e| format!("Invalid proxy URL: {e}"))?;

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(PROXY_HEALTH_TIMEOUT_SECS))
        .tcp_nodelay(true)
        .build()
        .map_err(|e| format!("Failed to build health check client: {e}"))?;

    let mut last_err = String::new();
    for url in health_check_urls() {
        match try_health_check(&client, url).await {
            Ok(ip) => return Ok(ip),
            Err(e) => {
                tracing::debug!("Health check via {url} failed: {e}");
                last_err = e;
            },
        }
    }

    Err(format!("All health check endpoints failed. Last error: {last_err}"))
}

async fn try_health_check(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header("Accept", "text/plain")
        .send()
        .await
        .map_err(|e| format!("Health check request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Health check returned HTTP {}", response.status()));
    }

    let mut body = Vec::with_capacity(1024);
    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        let chunk =
            chunk_result.map_err(|e| format!("Failed to read health check response: {e}"))?;
        if body.len().saturating_add(chunk.len()) > MAX_HEALTH_RESPONSE_BYTES {
            return Err("Health check response too large".to_string());
        }
        body.extend_from_slice(&chunk);
    }

    let exit_ip = String::from_utf8_lossy(&body).trim().to_string();

    if exit_ip.is_empty() {
        return Err("Health check returned empty response".to_string());
    }

    if exit_ip.parse::<IpAddr>().is_err() {
        return Err("Health check returned non-IP response".to_string());
    }

    Ok(exit_ip)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "test assertions")]
mod tests {
    use super::*;

    #[test]
    fn test_validate_proxy_url_socks5() {
        assert!(validate_proxy_url("socks5://1.2.3.4:1080").is_ok());
    }

    #[test]
    fn test_validate_proxy_url_socks5h() {
        assert!(validate_proxy_url("socks5h://user:pass@host:1080").is_ok());
    }

    #[test]
    fn test_validate_proxy_url_http() {
        assert!(validate_proxy_url("http://proxy:8080").is_ok());
    }

    #[test]
    fn test_validate_proxy_url_https() {
        assert!(validate_proxy_url("https://proxy:8443").is_ok());
    }

    #[test]
    fn test_validate_proxy_url_invalid_scheme() {
        let result = validate_proxy_url("ftp://proxy:21");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid proxy URL scheme"));
    }

    #[test]
    fn test_validate_proxy_url_no_scheme() {
        assert!(validate_proxy_url("127.0.0.1:1080").is_err());
    }

    #[test]
    fn test_validate_proxy_url_empty() {
        assert!(validate_proxy_url("").is_err());
    }

    #[test]
    fn test_reject_private_ips() {
        assert!(validate_proxy_url("socks5://127.0.0.1:1080").is_err());
        assert!(validate_proxy_url("socks5://10.0.0.1:1080").is_err());
        assert!(validate_proxy_url("socks5://192.168.1.1:1080").is_err());
        assert!(validate_proxy_url("socks5://172.16.0.1:1080").is_err());
    }

    #[test]
    fn test_reject_localhost() {
        assert!(validate_proxy_url("http://localhost:8080").is_err());
        assert!(validate_proxy_url("http://something.local:8080").is_err());
    }

    #[test]
    fn test_reject_metadata_ip() {
        assert!(validate_proxy_url("http://169.254.169.254:80").is_err());
    }

    #[test]
    fn test_reject_ipv6_loopback() {
        assert!(validate_proxy_url("socks5://[::1]:1080").is_err());
    }

    #[test]
    fn test_reject_ipv6_mapped_ipv4_loopback() {
        assert!(validate_proxy_url("socks5://[::ffff:127.0.0.1]:1080").is_err());
    }

    #[test]
    fn test_reject_ipv6_mapped_ipv4_private() {
        assert!(validate_proxy_url("socks5://[::ffff:10.0.0.1]:1080").is_err());
        assert!(validate_proxy_url("socks5://[::ffff:192.168.1.1]:1080").is_err());
        assert!(validate_proxy_url("socks5://[::ffff:169.254.169.254]:1080").is_err());
    }

    #[test]
    fn test_reject_ipv6_unique_local() {
        assert!(validate_proxy_url("socks5://[fd00::1]:1080").is_err());
        assert!(validate_proxy_url("socks5://[fc00::1]:1080").is_err());
    }

    #[test]
    fn test_reject_ipv6_link_local() {
        assert!(validate_proxy_url("socks5://[fe80::1]:1080").is_err());
    }

    #[test]
    fn test_reject_unspecified_ipv4() {
        assert!(validate_proxy_url("socks5://0.0.0.0:1080").is_err());
    }

    #[test]
    fn test_reject_ipv4_compatible_ipv6_loopback() {
        // ::127.0.0.1 parses as ::7f00:1 — IPv4-compatible address
        assert!(validate_proxy_url("socks5://[::7f00:1]:1080").is_err());
    }

    #[test]
    fn test_reject_ipv4_compatible_ipv6_private() {
        // ::10.0.0.1 parses as ::a00:1 — IPv4-compatible address
        assert!(validate_proxy_url("socks5://[::a00:1]:1080").is_err());
    }

    #[test]
    fn test_accept_domain_proxy_url() {
        // Domain-based proxy URLs are accepted — the hostname IS the proxy server (user-controlled).
        // DNS rebinding is not a concern: socks5h resolves targets on the proxy side, and the
        // health check target is controlled by us (configurable via ANTIGRAVITY_HEALTH_CHECK_URL).
        assert!(validate_proxy_url("socks5h://proxy.example.com:1080").is_ok());
        assert!(validate_proxy_url("socks5h://user:pass@proxy.example.com:1080").is_ok());
        assert!(validate_proxy_url("http://proxy.example.com:8080").is_ok());
    }
}
