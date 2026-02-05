//! Proxy and monitor API calls

use super::{api_get, api_post};
use crate::api_models::*;

pub(crate) async fn get_proxy_status() -> Result<ProxyStatus, String> {
    api_get("/proxy/status").await
}

pub(crate) async fn start_proxy_service() -> Result<ProxyStatus, String> {
    api_post("/proxy/start", &serde_json::json!({})).await
}

pub(crate) async fn stop_proxy_service() -> Result<(), String> {
    let _: bool = api_post("/proxy/stop", &serde_json::json!({})).await?;
    Ok(())
}

pub(crate) async fn generate_api_key() -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Response {
        api_key: String,
    }
    let response: Response = api_post("/proxy/generate-key", &serde_json::json!({})).await?;
    Ok(response.api_key)
}

pub(crate) async fn get_proxy_stats() -> Result<ProxyStats, String> {
    api_get("/monitor/stats").await
}

pub(crate) async fn get_proxy_logs(limit: Option<usize>) -> Result<Vec<ProxyRequestLog>, String> {
    let endpoint = match limit {
        Some(l) => format!("/monitor/requests?limit={}", l),
        None => "/monitor/requests".to_string(),
    };
    api_get(&endpoint).await
}

pub(crate) async fn set_proxy_monitor_enabled(_enabled: bool) -> Result<(), String> {
    Ok(())
}

pub(crate) async fn clear_proxy_session_bindings() -> Result<(), String> {
    let _: bool = api_post("/proxy/clear-bindings", &serde_json::json!({})).await?;
    Ok(())
}

pub(crate) async fn clear_proxy_logs() -> Result<(), String> {
    let _: bool = api_post("/monitor/clear", &serde_json::json!({})).await?;
    Ok(())
}
