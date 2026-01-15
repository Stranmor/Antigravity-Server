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
    /// Create AppState with pre-initialized components (for headless mode)
    pub async fn new_with_components(
        token_manager: Arc<TokenManager>,
        monitor: Arc<ProxyMonitor>,
        proxy_config: ProxyConfig,
        axum_server: Arc<AxumServer>,
    ) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(AppStateInner {
                token_manager,
                monitor,
                proxy_config: Arc::new(RwLock::new(proxy_config)),
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
