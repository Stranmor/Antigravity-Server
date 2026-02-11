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
async fn test_warp_client_cache_reuses_url() {
    let proxy_config =
        Arc::new(RwLock::new(antigravity_types::models::config::UpstreamProxyConfig::default()));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    // First call — builds and caches client for this URL
    if let Err(error) = client.get_warp_client("http://127.0.0.1:12345").await {
        panic!("warp client build failed: {}", error);
    }
    {
        let guard = client.warp_client.read().await;
        let (cached_url, _) = guard.as_ref().expect("warp cache empty after first build");
        assert_eq!(cached_url, "http://127.0.0.1:12345");
    }

    // Second call with same URL — should return cached client (no rebuild)
    if let Err(error) = client.get_warp_client("http://127.0.0.1:12345").await {
        panic!("warp client build failed: {}", error);
    }
    {
        let guard = client.warp_client.read().await;
        let (cached_url, _) = guard.as_ref().expect("warp cache empty after second call");
        assert_eq!(
            cached_url, "http://127.0.0.1:12345",
            "cache URL should not change for same URL"
        );
    }

    // Third call with different URL — cache should update
    if let Err(error) = client.get_warp_client("http://127.0.0.1:23456").await {
        panic!("warp client build failed: {}", error);
    }
    {
        let guard = client.warp_client.read().await;
        let (cached_url, _) = guard.as_ref().expect("warp cache empty after URL change");
        assert_eq!(cached_url, "http://127.0.0.1:23456", "cache should update for new URL");
    }
}

#[tokio::test]
async fn test_get_client_disabled_proxy_returns_direct() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        enabled: false,
        url: String::new(),
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_client_enabled_empty_url_returns_error() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        enabled: true,
        url: String::new(),
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn test_get_client_enabled_valid_url_returns_ok() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        enabled: true,
        url: "http://127.0.0.1:8080".to_string(),
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    let result = client.get_client().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_client_caches_proxied_client() {
    let config = antigravity_types::models::config::UpstreamProxyConfig {
        enabled: true,
        url: "http://127.0.0.1:9999".to_string(),
        ..Default::default()
    };
    let proxy_config = Arc::new(RwLock::new(config));
    let client = super::UpstreamClient::new(reqwest::Client::new(), proxy_config, None);

    assert!(client.get_client().await.is_ok());
    {
        let guard = client.proxied_client.read().await;
        let (cached_url, _) = guard.as_ref().expect("proxied cache should be populated");
        assert_eq!(cached_url, "http://127.0.0.1:9999");
    }

    assert!(client.get_client().await.is_ok());
}
