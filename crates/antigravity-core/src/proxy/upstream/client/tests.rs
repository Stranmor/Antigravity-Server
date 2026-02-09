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

    if let Err(error) = client.get_warp_client("http://127.0.0.1:12345").await {
        panic!("warp client build failed: {}", error);
    }
    let first_ptr = {
        let guard = client.warp_client.read().await;
        match guard.as_ref() {
            Some((cached_url, cached_client)) => {
                assert_eq!(cached_url, "http://127.0.0.1:12345");
                cached_client as *const reqwest::Client
            },
            None => panic!("warp cache was empty after first build"),
        }
    };

    if let Err(error) = client.get_warp_client("http://127.0.0.1:12345").await {
        panic!("warp client build failed: {}", error);
    }
    let second_ptr = {
        let guard = client.warp_client.read().await;
        match guard.as_ref() {
            Some((cached_url, cached_client)) => {
                assert_eq!(cached_url, "http://127.0.0.1:12345");
                cached_client as *const reqwest::Client
            },
            None => panic!("warp cache was empty after second build"),
        }
    };

    assert_eq!(first_ptr, second_ptr);

    if let Err(error) = client.get_warp_client("http://127.0.0.1:23456").await {
        panic!("warp client build failed: {}", error);
    }
    let guard = client.warp_client.read().await;
    match guard.as_ref() {
        Some((cached_url, _)) => {
            assert_eq!(cached_url, "http://127.0.0.1:23456");
        },
        None => panic!("warp cache was empty after third build"),
    }
}
