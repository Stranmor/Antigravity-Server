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

    let Some(host_str) = parsed.host_str() else {
        return Err("Proxy URL has no host".to_string());
    };

    if host_str.is_empty() {
        return Err("Proxy URL has empty host".to_string());
    }

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
            let addrs = tokio::task::spawn_blocking(move || addr_str.to_socket_addrs())
                .await
                .map_err(|e| format!("DNS resolution task failed: {e}"))?
                .map_err(|e| format!("DNS resolution failed for {host_display}: {e}"))?;
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
                || v4.is_multicast()            // 224.0.0.0/4
                || v4.is_broadcast()            // 255.255.255.255
                || (v4.octets()[0] == 100 && v4.octets()[1] >= 64 && v4.octets()[1] <= 127)
                || (v4.octets()[0] == 198 && (v4.octets()[1] == 18 || v4.octets()[1] == 19))
                || v4.octets()[0] >= 240 // 240.0.0.0/4 reserved (Class E)
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
            v6.is_multicast()                   // ff00::/8
                || (seg0 & 0xFE00) == 0xFC00    // fc00::/7 unique local
                || (seg0 & 0xFFC0) == 0xFE80    // fe80::/10 link-local
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
#[path = "proxy_health_tests.rs"]
mod tests;
