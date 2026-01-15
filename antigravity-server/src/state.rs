//! Application State
//!
//! Holds shared state for the server including account manager and proxy config.

use anyhow::Result;
use axum::Router;
use std::sync::Arc;
use tokio::sync::RwLock;

use antigravity_core::models::Account;
use antigravity_core::modules::account;
use antigravity_core::proxy::{
    build_proxy_router, server::AxumServer, ProxyMonitor, ProxySecurityConfig, TokenManager,
};
use antigravity_shared::proxy::config::ProxyConfig;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub token_manager: Arc<TokenManager>,
    pub monitor: Arc<ProxyMonitor>,
    pub proxy_config: Arc<RwLock<ProxyConfig>>,
    pub axum_server: Arc<AxumServer>,
}

impl AppState {
    pub async fn new(axum_server: Arc<AxumServer>) -> Result<Self> {
        let data_dir = account::get_data_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get data directory: {}", e))?;

        tracing::info!("📁 Data directory: {:?}", data_dir);

        let token_manager = Arc::new(TokenManager::new(data_dir.clone()));

        match token_manager.load_accounts().await {
            Ok(count) => {
                tracing::info!("📊 Loaded {} accounts into token manager", count);
            }
            Err(e) => {
                tracing::warn!("⚠️ Could not load accounts: {}", e);
            }
        }

        let monitor = Arc::new(ProxyMonitor::new());

        let proxy_config = Arc::new(RwLock::new(
            load_proxy_config(&data_dir).unwrap_or_default(),
        ));

        Ok(Self {
            inner: Arc::new(AppStateInner {
                token_manager,
                monitor,
                proxy_config,
                axum_server,
            }),
        })
    }

    pub async fn build_proxy_router(&self) -> Router {
        let config = self.inner.proxy_config.read().await;

        let security_config = ProxySecurityConfig::from_proxy_config(&config);

        build_proxy_router(
            self.inner.token_manager.clone(),
            config.custom_mapping.clone(),
            config.upstream_proxy.clone(),
            security_config,
            config.zai.clone(),
            self.inner.monitor.clone(),
            config.experimental.clone(),
        )
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>, String> {
        account::list_accounts()
    }

    pub fn get_current_account(&self) -> Result<Option<Account>, String> {
        account::get_current_account()
    }

    pub async fn switch_account(&self, account_id: &str) -> Result<(), String> {
        account::switch_account(account_id).await
    }

    pub fn get_account_count(&self) -> usize {
        match account::list_accounts() {
            Ok(accounts) => accounts.iter().filter(|a| !a.disabled).count(),
            Err(_) => 0,
        }
    }

    pub async fn get_proxy_port(&self) -> u16 {
        self.inner.proxy_config.read().await.port
    }

    pub async fn get_proxy_stats(&self) -> antigravity_shared::models::ProxyStats {
        self.inner.monitor.get_stats().await
    }

    pub async fn get_proxy_logs(
        &self,
        limit: Option<usize>,
    ) -> Vec<antigravity_shared::models::ProxyRequestLog> {
        self.inner.monitor.get_logs(limit).await
    }

    pub async fn clear_proxy_logs(&self) {
        self.inner.monitor.clear_logs().await;
    }

    pub fn get_token_manager_count(&self) -> usize {
        self.inner.token_manager.len()
    }

    pub async fn hot_reload_proxy_config(&self) {
        match antigravity_core::modules::config::load_config() {
            Ok(app_config) => {
                let proxy_config = app_config.proxy;
                tracing::info!("🔄 Hot reloading proxy configuration...");
                self.inner.axum_server.update_mapping(&proxy_config).await;
                self.inner.axum_server.update_security(&proxy_config).await;
                self.inner.axum_server.update_zai(&proxy_config).await;

                let mut inner_proxy_config = self.inner.proxy_config.write().await;
                *inner_proxy_config = proxy_config;

                tracing::info!("✅ Proxy configuration hot reloaded successfully.");
            }
            Err(e) => {
                tracing::error!("❌ Failed to hot reload proxy configuration: {}", e);
            }
        }
    }
}

pub fn get_model_quota(account: &Account, model_prefix: &str) -> Option<i32> {
    account.quota.as_ref().and_then(|q| {
        q.models
            .iter()
            .find(|m| m.name.to_lowercase().contains(&model_prefix.to_lowercase()))
            .map(|m| m.percentage)
    })
}

fn load_proxy_config(data_dir: &std::path::Path) -> Option<ProxyConfig> {
    let config_path = data_dir.join("config.json");

    if !config_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&config_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;

    if let Some(proxy) = value.get("proxy") {
        serde_json::from_value(proxy.clone()).ok()
    } else {
        None
    }
}
