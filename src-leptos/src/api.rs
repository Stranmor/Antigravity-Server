//! HTTP API bindings for Leptos
//!
//! This module provides type-safe wrappers for calling the antigravity-server REST API.
//! Replaces Tauri IPC for the headless server architecture.

use serde::{Serialize, de::DeserializeOwned};
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
    
    request.headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set headers: {:?}", e))?;
    
    let window = web_sys::window().ok_or("No window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;
    
    let resp: Response = resp_value.dyn_into()
        .map_err(|_| "Response is not a Response")?;
    
    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }
    
    let json = JsFuture::from(resp.json().map_err(|e| format!("JSON parse failed: {:?}", e))?)
        .await
        .map_err(|e| format!("JSON future failed: {:?}", e))?;
    
    serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Deserialize failed: {}", e))
}

/// Make a POST request to the API
pub async fn api_post<A: Serialize, R: DeserializeOwned>(endpoint: &str, body: &A) -> Result<R, String> {
    let url = format!("{}{}", API_BASE, endpoint);
    
    let body_str = serde_json::to_string(body)
        .map_err(|e| format!("Failed to serialize body: {}", e))?;
    
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&body_str));
    
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;
    
    request.headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set headers: {:?}", e))?;
    
    let window = web_sys::window().ok_or("No window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;
    
    let resp: Response = resp_value.dyn_into()
        .map_err(|_| "Response is not a Response")?;
    
    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }
    
    let json = JsFuture::from(resp.json().map_err(|e| format!("JSON parse failed: {:?}", e))?)
        .await
        .map_err(|e| format!("JSON future failed: {:?}", e))?;
    
    serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Deserialize failed: {}", e))
}

// Re-export common command wrappers
pub mod commands {
    use super::*;
    use crate::types::*;

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
        // TODO: When config endpoint is implemented
        Ok(AppConfig::default())
    }

    pub async fn save_config(_config: &AppConfig) -> Result<(), String> {
        // TODO: When config endpoint is implemented
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
        pub is_current: bool,
        pub gemini_quota: Option<i32>,
        pub claude_quota: Option<i32>,
        pub image_quota: Option<i32>,
        pub subscription_tier: Option<String>,
    }

    impl ApiAccount {
        /// Convert to full Account type for UI compatibility
        pub fn into_account(self) -> Account {
            use antigravity_shared::models::{TokenData, QuotaData, ModelQuota};
            
            let mut quota = QuotaData::default();
            if let Some(g) = self.gemini_quota {
                quota.models.push(ModelQuota {
                    name: "gemini-pro".to_string(),
                    percentage: g,
                    reset_time: "".to_string(),
                });
            }
            if let Some(c) = self.claude_quota {
                quota.models.push(ModelQuota {
                    name: "claude".to_string(),
                    percentage: c,
                    reset_time: "".to_string(),
                });
            }
            if let Some(i) = self.image_quota {
                quota.models.push(ModelQuota {
                    name: "image".to_string(),
                    percentage: i,
                    reset_time: "".to_string(),
                });
            }
            quota.subscription_tier = self.subscription_tier;
            
            Account {
                id: self.id,
                email: self.email,
                name: self.name,
                token: TokenData::new(
                    String::new(),
                    String::new(),
                    0,
                    None,
                    None,
                    None,
                ),
                quota: Some(quota),
                disabled: self.disabled,
                disabled_reason: None,
                disabled_at: None,
                proxy_disabled: false,
                proxy_disabled_reason: None,
                proxy_disabled_at: None,
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
        struct Req { account_id: String }
        
        let _: bool = api_post("/accounts/switch", &Req { account_id: account_id.to_string() }).await?;
        Ok(())
    }

    pub async fn delete_account(_account_id: &str) -> Result<(), String> {
        // TODO: Implement delete endpoint
        Err("Not implemented".to_string())
    }

    pub async fn delete_accounts(_account_ids: &[String]) -> Result<(), String> {
        // TODO: Implement batch delete endpoint
        Err("Not implemented".to_string())
    }

    // ========== OAuth ==========

    pub async fn start_oauth_login() -> Result<Account, String> {
        // TODO: Implement OAuth endpoint
        Err("OAuth not implemented in headless mode".to_string())
    }

    pub async fn prepare_oauth_url() -> Result<String, String> {
        // TODO: Implement OAuth URL endpoint
        Err("OAuth not implemented in headless mode".to_string())
    }

    pub async fn cancel_oauth_login() -> Result<(), String> {
        Ok(())
    }

    // ========== Quota ==========

    pub async fn fetch_account_quota(_account_id: &str) -> Result<QuotaData, String> {
        // TODO: Implement quota refresh endpoint
        Err("Not implemented".to_string())
    }

    pub async fn refresh_all_quotas() -> Result<RefreshStats, String> {
        // TODO: Implement refresh endpoint
        Err("Not implemented".to_string())
    }

    // ========== Import ==========

    pub async fn sync_account_from_db() -> Result<Option<Account>, String> {
        // TODO: Implement import endpoint
        Ok(None)
    }

    pub async fn import_custom_db(_path: &str) -> Result<Account, String> {
        Err("Not implemented".to_string())
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
        Err("Not implemented".to_string())
    }

    pub async fn get_proxy_stats() -> Result<ProxyStats, String> {
        Err("Not implemented".to_string())
    }

    pub async fn get_proxy_logs(_limit: Option<usize>) -> Result<Vec<ProxyRequestLog>, String> {
        Ok(vec![])
    }

    pub async fn set_proxy_monitor_enabled(_enabled: bool) -> Result<(), String> {
        Ok(())
    }

    pub async fn clear_proxy_session_bindings() -> Result<(), String> {
        Ok(())
    }

    pub async fn clear_proxy_logs() -> Result<(), String> {
        Ok(())
    }

    pub async fn reload_proxy_accounts() -> Result<usize, String> {
        Ok(0)
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
