use super::request_executor::build_url;
use std::sync::Arc;
use tokio::sync::RwLock;

#[test]
fn test_build_url() {
    let base_url = "https://cloudcode-pa.googleapis.com/v1internal";

    let url1 = build_url(base_url, "generateContent", None);
    assert_eq!(url1, "https://cloudcode-pa.googleapis.com/v1internal:generateContent");

    let url2 = build_url(base_url, "streamGenerateContent", Some("alt=sse"));
    assert_eq!(
        url2,
        "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
    );
}

#[tokio::test]
async fn test_direct_mode_returns_ok() {
    let config = antigravity_types::models::config::UpstreamProxyConfig::default();
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client_for_account(None, None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pool_mode_empty_is_strict_error() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Pool,
        enabled: true,
        proxy_urls: vec![], // Empty pool
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client_for_account(Some("test@test.com"), None).await;
    assert!(result.is_err(), "Empty pool in Pool mode MUST return error, not fallback to direct");
    assert!(result.unwrap_err().contains("EMPTY"));
}

#[tokio::test]
async fn test_per_account_sticky_proxy() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Pool,
        enabled: true,
        url: String::new(),
        proxy_urls: vec![
            "http://127.0.0.1:8081".to_string(),
            "http://127.0.0.1:8082".to_string(),
            "http://127.0.0.1:8083".to_string(),
        ],
        rotation_strategy: antigravity_types::models::ProxyRotationStrategy::PerAccount,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);
    let pool = client.proxy_pool();

    // Same account should always get same proxy
    let email = "test@example.com";
    let url1 = pool.select_proxy_url(Some(email)).await.unwrap();
    let url2 = pool.select_proxy_url(Some(email)).await.unwrap();
    let url3 = pool.select_proxy_url(Some(email)).await.unwrap();
    assert_eq!(url1, url2);
    assert_eq!(url2, url3);
}

#[tokio::test]
async fn test_per_account_no_email_is_error() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Pool,
        enabled: true,
        url: String::new(),
        proxy_urls: vec!["http://127.0.0.1:8081".to_string()],
        rotation_strategy: antigravity_types::models::ProxyRotationStrategy::PerAccount,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);
    let pool = client.proxy_pool();

    let result = pool.select_proxy_url(None).await;
    assert!(result.is_err(), "PerAccount without email MUST error");
}

#[tokio::test]
async fn test_webshare_format_parsing() {
    use crate::proxy::proxy_pool::parse_proxy_url;

    let result = parse_proxy_url("31.59.20.176:6754:gqkywhck:4fhnq5cyq4tk");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "http://gqkywhck:4fhnq5cyq4tk@31.59.20.176:6754");

    let result2 = parse_proxy_url("socks5://127.0.0.1:1080");
    assert!(result2.is_ok());
    assert_eq!(result2.unwrap(), "socks5://127.0.0.1:1080");

    let result3 = parse_proxy_url("");
    assert!(result3.is_err());
}

#[tokio::test]
async fn test_pool_stats() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Pool,
        enabled: true,
        url: String::new(),
        proxy_urls: vec!["http://127.0.0.1:8081".to_string(), "http://127.0.0.1:8082".to_string()],
        rotation_strategy: antigravity_types::models::ProxyRotationStrategy::PerAccount,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);
    let pool = client.proxy_pool();

    let stats = pool.stats().await;
    assert_eq!(stats.pool_size, 2);
    assert_eq!(stats.mode, antigravity_types::models::UpstreamProxyMode::Pool);
}

// -- enforce_proxy tests --

#[tokio::test]
async fn test_enforce_proxy_blocks_direct_mode() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Direct,
        enforce_proxy: true,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client_for_account(None, None).await;
    assert!(result.is_err(), "enforce_proxy=true with Direct mode MUST block");
    assert!(result.unwrap_err().contains("enforce_proxy"));
}

#[tokio::test]
async fn test_enforce_proxy_blocks_system_mode() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::System,
        enforce_proxy: true,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client_for_account(Some("test@example.com"), None).await;
    assert!(result.is_err(), "enforce_proxy=true with System mode MUST block");
    assert!(result.unwrap_err().contains("IP leak"));
}

#[tokio::test]
async fn test_enforce_proxy_allows_per_account_proxy() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Direct,
        enforce_proxy: true,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    // Per-account proxy takes priority — should succeed even with enforce_proxy
    let result = client
        .get_client_for_account(Some("test@example.com"), Some("socks5://127.0.0.1:1080"))
        .await;
    assert!(result.is_ok(), "Per-account proxy should bypass enforce_proxy block");
}

#[tokio::test]
async fn test_enforce_proxy_false_allows_direct() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Direct,
        enforce_proxy: false,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client_for_account(None, None).await;
    assert!(result.is_ok(), "enforce_proxy=false should allow direct connections");
}

#[tokio::test]
async fn test_enforce_proxy_pool_mode_with_urls_allowed() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        mode: antigravity_types::models::UpstreamProxyMode::Pool,
        enforce_proxy: true,
        enabled: true,
        proxy_urls: vec!["http://127.0.0.1:8081".to_string()],
        rotation_strategy: antigravity_types::models::ProxyRotationStrategy::RoundRobin,
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    // Pool mode with actual proxies should work even with enforce_proxy
    let result = client.get_client_for_account(Some("test@example.com"), None).await;
    assert!(result.is_ok(), "Pool mode with proxies should work with enforce_proxy=true");
}
