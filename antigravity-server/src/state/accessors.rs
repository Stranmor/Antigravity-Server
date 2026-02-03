//! Accessor methods for AppState

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use antigravity_core::models::Account;
use antigravity_core::modules::account;
use antigravity_core::modules::repository::{AccountRepository, RepoResult, RepositoryError};
use antigravity_core::proxy::{
    AdaptiveLimitManager, CircuitBreakerManager, HealthMonitor, ProxySecurityConfig,
};

use super::AppState;

impl AppState {
    pub fn set_bound_port(&self, port: u16) {
        self.inner.bound_port.store(port, Ordering::Relaxed);
    }

    pub fn get_bound_port(&self) -> u16 {
        self.inner.bound_port.load(Ordering::Relaxed)
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

    pub async fn get_proxy_bind_address(&self) -> String {
        self.inner.proxy_config.read().await.get_bind_address()
    }

    pub async fn get_proxy_stats(&self) -> antigravity_types::models::ProxyStats {
        self.inner.monitor.get_stats().await
    }

    pub async fn get_proxy_logs(
        &self,
        limit: Option<usize>,
    ) -> Vec<antigravity_types::models::ProxyRequestLog> {
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
                tracing::info!("Hot reloading proxy configuration...");

                {
                    let mut mapping = self.inner.custom_mapping.write().await;
                    *mapping = proxy_config.custom_mapping.clone();
                    tracing::debug!("Updated custom_mapping with {} entries", mapping.len());
                }
                {
                    let mut security = self.inner.security_config.write().await;
                    *security = ProxySecurityConfig::from_proxy_config(&proxy_config);
                }
                {
                    let mut zai = self.inner.zai_config.write().await;
                    *zai = proxy_config.zai.clone();
                }
                {
                    let mut experimental = self.inner.experimental_config.write().await;
                    *experimental = proxy_config.experimental.clone();
                }

                let mut inner_proxy_config = self.inner.proxy_config.write().await;
                *inner_proxy_config = proxy_config;

                tracing::info!("Proxy configuration hot reloaded successfully.");
            }
            Err(e) => {
                tracing::error!("Failed to hot reload proxy configuration: {}", e);
            }
        }
    }

    pub async fn reload_accounts(&self) -> Result<usize, String> {
        match self.inner.token_manager.load_accounts().await {
            Ok(count) => {
                tracing::info!("Reloaded {} accounts into token manager", count);
                Ok(count)
            }
            Err(e) => {
                tracing::error!("Failed to reload accounts: {}", e);
                Err(e)
            }
        }
    }

    pub fn clear_session_bindings(&self) {
        self.inner.token_manager.clear_all_sessions();
        tracing::info!("Cleared all session bindings");
    }

    pub fn clear_all_rate_limits(&self) {
        self.inner.token_manager.clear_all_rate_limits();
    }

    pub fn clear_rate_limit(&self, account_id: &str) -> bool {
        self.inner.token_manager.clear_rate_limit(account_id)
    }

    pub fn adaptive_limits(&self) -> &Arc<AdaptiveLimitManager> {
        &self.inner.adaptive_limits
    }

    pub fn health_monitor(&self) -> &Arc<HealthMonitor> {
        &self.inner.health_monitor
    }

    pub fn circuit_breaker(&self) -> &Arc<CircuitBreakerManager> {
        &self.inner.circuit_breaker
    }

    pub fn generate_oauth_state(&self) -> String {
        use rand::Rng;

        const MAX_OAUTH_STATES: usize = 100;
        if self.inner.oauth_states.len() >= MAX_OAUTH_STATES {
            self.cleanup_expired_oauth_states();
            if self.inner.oauth_states.len() >= MAX_OAUTH_STATES {
                self.inner.oauth_states.clear();
                tracing::warn!("OAuth states limit reached, cleared all pending states");
            }
        }

        let state: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        self.inner
            .oauth_states
            .insert(state.clone(), Instant::now());
        state
    }

    pub fn validate_oauth_state(&self, state: &str) -> bool {
        if let Some((_, created_at)) = self.inner.oauth_states.remove(state) {
            created_at.elapsed().as_secs() < 600
        } else {
            false
        }
    }

    fn cleanup_expired_oauth_states(&self) {
        self.inner
            .oauth_states
            .retain(|_, created_at: &mut Instant| created_at.elapsed().as_secs() < 600);
    }

    #[allow(dead_code)]
    pub fn repository(&self) -> Option<&Arc<dyn AccountRepository>> {
        self.inner.repository.as_ref()
    }

    #[allow(dead_code)]
    pub fn has_database(&self) -> bool {
        self.inner.repository.is_some()
    }

    #[allow(dead_code)]
    pub async fn list_accounts_db(&self) -> RepoResult<Vec<Account>> {
        match &self.inner.repository {
            Some(repo) => repo.list_accounts().await,
            None => Err(RepositoryError::Database(
                "No database configured".to_string(),
            )),
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
