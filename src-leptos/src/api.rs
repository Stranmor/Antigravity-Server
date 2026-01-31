//! HTTP API bindings for Leptos
//!
//! This module provides type-safe wrappers for calling the antigravity-server REST API.
//! Replaces Tauri IPC for the headless server architecture.

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

const API_BASE: &str = "/api";

/// Make a GET request to the API
pub async fn api_get<R: DeserializeOwned>(endpoint: &str) -> Result<R, String> {
    let url = format!("{}{}", API_BASE, endpoint);

    let opts = RequestInit::new();
    opts.set_method("GET");

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set headers: {:?}", e))?;

    let window = web_sys::window().ok_or("No window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Response is not a Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("JSON parse failed: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("JSON future failed: {:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Deserialize failed: {}", e))
}

/// Make a POST request to the API
pub async fn api_post<A: Serialize, R: DeserializeOwned>(
    endpoint: &str,
    body: &A,
) -> Result<R, String> {
    let url = format!("{}{}", API_BASE, endpoint);

    let body_str =
        serde_json::to_string(body).map_err(|e| format!("Failed to serialize body: {}", e))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set headers: {:?}", e))?;

    let window = web_sys::window().ok_or("No window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "Response is not a Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let json = JsFuture::from(
        resp.json()
            .map_err(|e| format!("JSON parse failed: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("JSON future failed: {:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Deserialize failed: {}", e))
}

// Re-export common command wrappers
pub mod commands {
    use super::*;
    use crate::api_models::*;

    // ========== Status ==========

    #[derive(serde::Deserialize)]
    pub struct StatusResponse {
        pub version: String,
        pub proxy_running: bool,
        pub accounts_count: usize,
        pub current_account: Option<String>,
    }

    pub async fn get_status() -> Result<StatusResponse, String> {
        api_get("/status").await
    }

    // ========== Config ==========

    pub async fn load_config() -> Result<AppConfig, String> {
        api_get("/config").await
    }

    pub async fn save_config(config: &AppConfig) -> Result<(), String> {
        let _: bool = api_post("/config", config).await?;
        Ok(())
    }

    // ========== Accounts ==========

    /// API response struct (simplified, no token)
    #[derive(serde::Deserialize, Clone, Debug)]
    pub struct ApiAccount {
        pub id: String,
        pub email: String,
        pub name: Option<String>,
        pub disabled: bool,
        #[serde(default)]
        pub proxy_disabled: bool,
        pub is_current: bool,
        pub gemini_quota: Option<i32>,
        pub claude_quota: Option<i32>,
        pub image_quota: Option<i32>,
        pub subscription_tier: Option<String>,
        pub quota: Option<antigravity_types::models::QuotaData>,
    }

    impl ApiAccount {
        /// Convert to full Account type for UI compatibility
        pub fn into_account(self) -> Account {
            use antigravity_types::models::TokenData;
            use std::collections::HashSet;

            Account {
                id: self.id,
                email: self.email,
                name: self.name,
                token: TokenData::new(String::new(), String::new(), 0, None, None, None),
                quota: self.quota,
                disabled: self.disabled,
                disabled_reason: None,
                disabled_at: None,
                proxy_disabled: self.proxy_disabled,
                proxy_disabled_reason: None,
                proxy_disabled_at: None,
                protected_models: HashSet::new(),
                created_at: 0,
                last_used: 0,
            }
        }
    }

    pub async fn list_accounts() -> Result<Vec<Account>, String> {
        let api_accounts: Vec<ApiAccount> = api_get("/accounts").await?;
        Ok(api_accounts.into_iter().map(|a| a.into_account()).collect())
    }

    pub async fn get_current_account() -> Result<Option<Account>, String> {
        let api_account: Option<ApiAccount> = api_get("/accounts/current").await?;
        Ok(api_account.map(|a| a.into_account()))
    }

    pub async fn switch_account(account_id: &str) -> Result<(), String> {
        #[derive(serde::Serialize)]
        struct Req {
            account_id: String,
        }

        let _: bool = api_post(
            "/accounts/switch",
            &Req {
                account_id: account_id.to_string(),
            },
        )
        .await?;
        Ok(())
    }

    pub async fn delete_account(account_id: &str) -> Result<(), String> {
        let _: bool = api_post(
            "/accounts/delete",
            &serde_json::json!({
                "account_id": account_id
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn delete_accounts(account_ids: &[String]) -> Result<(), String> {
        let _: bool = api_post(
            "/accounts/delete-batch",
            &serde_json::json!({
                "account_ids": account_ids
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn toggle_proxy_status(
        account_id: &str,
        enable: bool,
        reason: Option<&str>,
    ) -> Result<(), String> {
        #[derive(serde::Deserialize)]
        struct ToggleProxyResponse {
            #[allow(dead_code)]
            success: bool,
            #[allow(dead_code)]
            account_id: String,
            #[allow(dead_code)]
            proxy_disabled: bool,
        }

        let _: ToggleProxyResponse = api_post(
            "/accounts/toggle-proxy",
            &serde_json::json!({
                "account_id": account_id,
                "enable": enable,
                "reason": reason
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn warmup_account(account_id: &str) -> Result<String, String> {
        #[derive(serde::Deserialize)]
        struct WarmupResponse {
            message: String,
        }
        let response: WarmupResponse = api_post(
            "/accounts/warmup",
            &serde_json::json!({
                "account_id": account_id
            }),
        )
        .await?;
        Ok(response.message)
    }

    pub async fn warmup_all_accounts() -> Result<String, String> {
        #[derive(serde::Deserialize)]
        struct WarmupResponse {
            message: String,
        }
        let response: WarmupResponse =
            api_post("/accounts/warmup-all", &serde_json::json!({})).await?;
        Ok(response.message)
    }

    // ========== OAuth ==========

    #[derive(serde::Deserialize)]
    struct OAuthLoginResponse {
        url: String,
        #[allow(dead_code)]
        message: String,
    }

    pub async fn start_oauth_login() -> Result<Account, String> {
        // Get OAuth URL from backend
        let response: OAuthLoginResponse = api_post("/oauth/login", &serde_json::json!({})).await?;

        // Open URL in browser (user will be redirected back to /api/oauth/callback)
        let window = web_sys::window().ok_or("No window")?;
        window
            .open_with_url_and_target(&response.url, "_blank")
            .map_err(|e| format!("Failed to open browser: {:?}", e))?;

        // Return placeholder - the OAuth flow completes via browser redirect
        // After redirect, user should refresh the accounts list
        Err("OAuth flow started - check browser and refresh accounts list after login".to_string())
    }

    #[derive(serde::Deserialize)]
    struct OAuthUrlResponse {
        url: String,
        #[allow(dead_code)]
        redirect_uri: String,
    }

    pub async fn prepare_oauth_url() -> Result<String, String> {
        let response: OAuthUrlResponse = api_get("/oauth/url").await?;
        Ok(response.url)
    }

    pub async fn cancel_oauth_login() -> Result<(), String> {
        Ok(())
    }

    // ========== Quota ==========

    #[derive(serde::Deserialize)]
    struct QuotaResponse {
        #[allow(dead_code)]
        account_id: String,
        quota: Option<QuotaData>,
    }

    pub async fn fetch_account_quota(account_id: &str) -> Result<QuotaData, String> {
        let response: QuotaResponse = api_post(
            "/accounts/refresh-quota",
            &serde_json::json!({
                "account_id": account_id
            }),
        )
        .await?;
        response
            .quota
            .ok_or_else(|| "No quota data returned".to_string())
    }

    pub async fn refresh_all_quotas() -> Result<RefreshStats, String> {
        api_post("/accounts/refresh-all-quotas", &serde_json::json!({})).await
    }

    // ========== Token-based Add ==========

    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct AddByTokenResponse {
        success_count: usize,
        fail_count: usize,
        accounts: Vec<Account>,
    }

    pub async fn add_accounts_by_token(
        refresh_tokens: Vec<String>,
    ) -> Result<(usize, usize), String> {
        let response: AddByTokenResponse = api_post(
            "/accounts/add-by-token",
            &serde_json::json!({
                "refresh_tokens": refresh_tokens
            }),
        )
        .await?;
        Ok((response.success_count, response.fail_count))
    }

    // ========== Import ==========

    pub async fn sync_account_from_db() -> Result<Option<Account>, String> {
        // Import from local DB is not available in browser mode
        // (requires filesystem access)
        Ok(None)
    }

    pub async fn import_custom_db(_path: &str) -> Result<Account, String> {
        // Import from custom DB is not available in browser mode
        Err("Import from file not available in browser mode".to_string())
    }

    // ========== Proxy ==========

    pub async fn get_proxy_status() -> Result<ProxyStatus, String> {
        api_get("/proxy/status").await
    }

    pub async fn start_proxy_service() -> Result<ProxyStatus, String> {
        api_post("/proxy/start", &serde_json::json!({})).await
    }

    pub async fn stop_proxy_service() -> Result<(), String> {
        let _: bool = api_post("/proxy/stop", &serde_json::json!({})).await?;
        Ok(())
    }

    pub async fn generate_api_key() -> Result<String, String> {
        #[derive(serde::Deserialize)]
        struct Response {
            api_key: String,
        }
        let response: Response = api_post("/proxy/generate-key", &serde_json::json!({})).await?;
        Ok(response.api_key)
    }

    pub async fn get_proxy_stats() -> Result<ProxyStats, String> {
        api_get("/monitor/stats").await
    }

    pub async fn get_proxy_logs(limit: Option<usize>) -> Result<Vec<ProxyRequestLog>, String> {
        let endpoint = match limit {
            Some(l) => format!("/monitor/requests?limit={}", l),
            None => "/monitor/requests".to_string(),
        };
        api_get(&endpoint).await
    }

    pub async fn set_proxy_monitor_enabled(_enabled: bool) -> Result<(), String> {
        // Monitor is always enabled; this is a no-op placeholder for UI compatibility
        Ok(())
    }

    pub async fn clear_proxy_session_bindings() -> Result<(), String> {
        let _: bool = api_post("/proxy/clear-bindings", &serde_json::json!({})).await?;
        Ok(())
    }

    pub async fn clear_proxy_logs() -> Result<(), String> {
        let _: bool = api_post("/monitor/clear", &serde_json::json!({})).await?;
        Ok(())
    }

    pub async fn reload_proxy_accounts() -> Result<usize, String> {
        #[derive(serde::Deserialize)]
        struct Response {
            count: usize,
        }
        let response: Response = api_post("/accounts/reload", &serde_json::json!({})).await?;
        Ok(response.count)
    }

    // ========== Models ==========

    #[derive(serde::Serialize)]
    pub struct ModelDetectRequest {
        pub model: String,
    }

    #[derive(serde::Deserialize, Clone)]
    pub struct ModelDetectResponse {
        pub original_model: String,
        pub mapped_model: String,
        pub mapping_reason: String,
    }

    pub async fn detect_model(model: &str) -> Result<ModelDetectResponse, String> {
        api_post(
            "/models/detect",
            &ModelDetectRequest {
                model: model.to_string(),
            },
        )
        .await
    }

    // ========== System ==========

    pub async fn check_for_updates() -> Result<UpdateInfo, String> {
        Ok(UpdateInfo {
            available: false,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            latest_version: env!("CARGO_PKG_VERSION").to_string(),
            release_url: None,
            release_notes: None,
        })
    }

    pub async fn open_data_folder() -> Result<(), String> {
        Err("Not available in browser".to_string())
    }

    pub async fn get_data_dir_path() -> Result<String, String> {
        Err("Not available in browser".to_string())
    }

    pub async fn clear_log_cache() -> Result<(), String> {
        Ok(())
    }
}
