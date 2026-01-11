//! Tauri IPC bindings for Leptos
//!
//! This module provides type-safe wrappers around Tauri's invoke() function.

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

/// Call a Tauri command with typed arguments and return value.
pub async fn tauri_invoke<A, R>(cmd: &str, args: A) -> Result<R, String>
where
    A: Serialize,
    R: DeserializeOwned,
{
    let args_js = serde_wasm_bindgen::to_value(&args)
        .map_err(|e| format!("Failed to serialize args: {}", e))?;
    
    let result = invoke(cmd, args_js).await;
    
    // Check if result is an error
    if result.is_undefined() || result.is_null() {
        return Err("Command returned null/undefined".to_string());
    }
    
    serde_wasm_bindgen::from_value(result)
        .map_err(|e| format!("Failed to deserialize result: {}", e))
}

/// Call a Tauri command with no arguments.
pub async fn tauri_invoke_no_args<R>(cmd: &str) -> Result<R, String>
where
    R: DeserializeOwned,
{
    tauri_invoke(cmd, serde_json::json!({})).await
}

// Re-export common command wrappers
pub mod commands {
    use super::*;
    use crate::types::*;
    
    // ========== Config ==========
    
    /// Load application configuration
    pub async fn load_config() -> Result<AppConfig, String> {
        tauri_invoke_no_args("load_config").await
    }
    
    /// Save application configuration
    pub async fn save_config(config: &AppConfig) -> Result<(), String> {
        tauri_invoke("save_config", serde_json::json!({ "config": config })).await
    }
    
    // ========== Accounts ==========
    
    /// List all accounts
    pub async fn list_accounts() -> Result<Vec<Account>, String> {
        tauri_invoke_no_args("list_accounts").await
    }
    
    /// Get current account
    pub async fn get_current_account() -> Result<Option<Account>, String> {
        tauri_invoke_no_args("get_current_account").await
    }
    
    /// Switch to account
    pub async fn switch_account(account_id: &str) -> Result<(), String> {
        tauri_invoke("switch_account", serde_json::json!({ "accountId": account_id })).await
    }
    
    /// Delete account
    pub async fn delete_account(account_id: &str) -> Result<(), String> {
        tauri_invoke("delete_account", serde_json::json!({ "accountId": account_id })).await
    }
    
    /// Delete multiple accounts
    pub async fn delete_accounts(account_ids: &[String]) -> Result<(), String> {
        tauri_invoke("delete_accounts", serde_json::json!({ "accountIds": account_ids })).await
    }
    
    // ========== OAuth ==========
    
    /// Start OAuth login flow (opens browser)
    pub async fn start_oauth_login() -> Result<Account, String> {
        tauri_invoke_no_args("start_oauth_login").await
    }
    
    /// Prepare OAuth URL without opening browser
    pub async fn prepare_oauth_url() -> Result<String, String> {
        tauri_invoke_no_args("prepare_oauth_url").await
    }
    
    /// Cancel ongoing OAuth login
    pub async fn cancel_oauth_login() -> Result<(), String> {
        tauri_invoke_no_args("cancel_oauth_login").await
    }
    
    // ========== Quota ==========
    
    /// Fetch quota for single account
    pub async fn fetch_account_quota(account_id: &str) -> Result<AccountQuota, String> {
        tauri_invoke("fetch_account_quota", serde_json::json!({ "accountId": account_id })).await
    }
    
    /// Refresh all account quotas
    pub async fn refresh_all_quotas() -> Result<RefreshStats, String> {
        tauri_invoke_no_args("refresh_all_quotas").await
    }
    
    // ========== Import ==========
    
    /// Import accounts from local Antigravity DB
    pub async fn sync_account_from_db() -> Result<Option<Account>, String> {
        tauri_invoke_no_args("sync_account_from_db").await
    }
    
    /// Import from custom DB path
    pub async fn import_custom_db(path: &str) -> Result<Account, String> {
        tauri_invoke("import_custom_db", serde_json::json!({ "path": path })).await
    }
    
    // ========== Proxy ==========
    
    /// Get proxy status
    pub async fn get_proxy_status() -> Result<ProxyStatus, String> {
        tauri_invoke_no_args("get_proxy_status").await
    }
    
    /// Start proxy service (uses saved config)
    pub async fn start_proxy_service() -> Result<ProxyStatus, String> {
        // Load current config and pass it
        let config = load_config().await?;
        tauri_invoke("start_proxy_service", serde_json::json!({ "config": config.proxy })).await
    }
    
    /// Stop proxy service
    pub async fn stop_proxy_service() -> Result<(), String> {
        tauri_invoke_no_args("stop_proxy_service").await
    }
    
    /// Generate API key
    pub async fn generate_api_key() -> Result<String, String> {
        tauri_invoke_no_args("generate_api_key").await
    }
    
    /// Get proxy statistics
    pub async fn get_proxy_stats() -> Result<ProxyStats, String> {
        tauri_invoke_no_args("get_proxy_stats").await
    }
    
    /// Get proxy request logs
    pub async fn get_proxy_logs(limit: Option<usize>) -> Result<Vec<ProxyRequestLog>, String> {
        tauri_invoke("get_proxy_logs", serde_json::json!({ "limit": limit })).await
    }
    
    /// Enable/disable proxy monitoring
    pub async fn set_proxy_monitor_enabled(enabled: bool) -> Result<(), String> {
        tauri_invoke("set_proxy_monitor_enabled", serde_json::json!({ "enabled": enabled })).await
    }
    
    /// Clear proxy session bindings
    pub async fn clear_proxy_session_bindings() -> Result<(), String> {
        tauri_invoke_no_args("clear_proxy_session_bindings").await
    }
    
    /// Clear proxy logs
    pub async fn clear_proxy_logs() -> Result<(), String> {
        tauri_invoke_no_args("clear_proxy_logs").await
    }
    
    /// Reload proxy accounts
    pub async fn reload_proxy_accounts() -> Result<usize, String> {
        tauri_invoke_no_args("reload_proxy_accounts").await
    }
    
    // ========== System ==========
    
    /// Check for updates
    pub async fn check_for_updates() -> Result<UpdateInfo, String> {
        tauri_invoke_no_args("check_for_updates").await
    }
    
    /// Open data folder
    pub async fn open_data_folder() -> Result<(), String> {
        tauri_invoke_no_args("open_data_folder").await
    }
    
    /// Get data directory path
    pub async fn get_data_dir_path() -> Result<String, String> {
        tauri_invoke_no_args("get_data_dir_path").await
    }
    
    /// Clear log cache
    pub async fn clear_log_cache() -> Result<(), String> {
        tauri_invoke_no_args("clear_log_cache").await
    }
}
