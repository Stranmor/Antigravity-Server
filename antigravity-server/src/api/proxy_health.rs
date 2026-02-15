//! Proxy URL validation and health checking for per-account proxies.

use std::time::Duration;

const PROXY_HEALTH_TIMEOUT_SECS: u64 = 15;
const HEALTH_CHECK_URL: &str = "https://ifconfig.co";

/// Validates that a proxy URL has an accepted scheme prefix.
pub fn validate_proxy_url(url: &str) -> Result<(), String> {
    const VALID_PREFIXES: &[&str] = &["socks5://", "socks5h://", "http://", "https://"];
    if VALID_PREFIXES.iter().any(|p| url.starts_with(p)) {
        Ok(())
    } else {
        Err(format!(
            "Invalid proxy URL scheme. Must start with one of: {}",
            VALID_PREFIXES.join(", ")
        ))
    }
}

/// Tests proxy connectivity by making a request through it.
/// Returns the exit IP on success, or an error description on failure.
pub async fn check_proxy_health(proxy_url: &str) -> Result<String, String> {
    validate_proxy_url(proxy_url)?;

    let proxy = wreq::Proxy::all(proxy_url).map_err(|e| format!("Invalid proxy URL: {e}"))?;

    let client = wreq::Client::builder()
        .emulation(antigravity_core::proxy::upstream::emulation::default_emulation())
        .proxy(proxy)
        .timeout(Duration::from_secs(PROXY_HEALTH_TIMEOUT_SECS))
        .tcp_nodelay(true)
        .build()
        .map_err(|e| format!("Failed to build health check client: {e}"))?;

    let response = client
        .get(HEALTH_CHECK_URL)
        .send()
        .await
        .map_err(|e| format!("Health check request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Health check returned HTTP {}", response.status()));
    }

    let exit_ip = response
        .text()
        .await
        .map_err(|e| format!("Failed to read health check response: {e}"))?
        .trim()
        .to_string();

    if exit_ip.is_empty() {
        return Err("Health check returned empty response".to_string());
    }

    Ok(exit_ip)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "test assertions")]
mod tests {
    use super::*;

    #[test]
    fn test_validate_proxy_url_socks5() {
        assert!(validate_proxy_url("socks5://127.0.0.1:1080").is_ok());
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
}
