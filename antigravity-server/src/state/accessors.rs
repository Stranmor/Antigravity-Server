//! Accessor methods for AppState

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use antigravity_core::models::Account;
use antigravity_core::modules::account;
use antigravity_core::modules::repository::AccountRepository;
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

    pub async fn list_accounts(&self) -> Result<Vec<Account>, String> {
        if let Some(repo) = self.repository() {
            repo.list_accounts().await.map_err(|e| e.to_string())
        } else {
            tokio::task::spawn_blocking(account::list_accounts)
                .await
                .map_err(|e| format!("spawn_blocking panicked: {e}"))?
        }
    }

    pub async fn get_current_account(&self) -> Result<Option<Account>, String> {
        if let Some(repo) = self.repository() {
            let id = repo.get_current_account_id().await.map_err(|e| e.to_string())?;
            match id {
                Some(account_id) => {
                    repo.get_account(&account_id).await.map(Some).map_err(|e| e.to_string())
                },
                None => Ok(None),
            }
        } else {
            tokio::task::spawn_blocking(account::get_current_account)
                .await
                .map_err(|e| format!("spawn_blocking panicked: {e}"))?
        }
    }

    pub async fn switch_account(&self, account_id: &str) -> Result<(), String> {
        // Write DB first — if this fails, file is untouched (no split-brain)
        if let Some(repo) = self.repository() {
            repo.set_current_account_id(account_id)
                .await
                .map_err(|e| format!("Failed to set current account in DB: {e}"))?;
        }
        // Then update file
        account::switch_account(account_id).await?;
        Ok(())
    }

    pub async fn get_account_count(&self) -> Result<usize, String> {
        let accounts = if let Some(repo) = self.repository() {
            repo.list_accounts().await.map_err(|e| e.to_string())?
        } else {
            tokio::task::spawn_blocking(account::list_accounts)
                .await
                .map_err(|e| format!("spawn_blocking panicked: {e}"))??
        };
        Ok(accounts.iter().filter(|a| !a.disabled).count())
    }

    pub async fn get_proxy_bind_address(&self) -> String {
        self.inner.proxy_config.read().await.get_bind_address()
    }

    pub async fn get_proxy_stats(&self) -> antigravity_types::models::ProxyStats {
        self.inner.monitor.get_stats().await
    }

    pub async fn get_token_usage_stats(&self) -> antigravity_types::models::TokenUsageStats {
        antigravity_core::modules::proxy_db::get_token_usage_stats().await.unwrap_or_default()
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

    #[allow(
        clippy::significant_drop_tightening,
        reason = "All config guards acquired atomically to prevent torn reads"
    )]
    pub async fn hot_reload_proxy_config(&self) {
        let app_config =
            match tokio::task::spawn_blocking(antigravity_core::modules::config::load_config).await
            {
                Ok(Ok(config)) => config,
                Ok(Err(e)) => {
                    tracing::error!("Failed to hot reload proxy configuration: {}", e);
                    return;
                },
                Err(e) => {
                    tracing::error!("spawn_blocking panicked during config load: {}", e);
                    return;
                },
            };

        let proxy_config = app_config.proxy;
        tracing::info!("Hot reloading proxy configuration...");

        antigravity_core::proxy::common::thinking_config::update_thinking_budget_config(
            proxy_config.thinking_budget.clone(),
        );

        // Acquire ALL write guards atomically (alphabetical lock order to prevent deadlocks)
        let mut mapping = self.inner.custom_mapping.write().await;
        let mut experimental = self.inner.experimental_config.write().await;
        let mut inner_proxy_config = self.inner.proxy_config.write().await;
        let mut security = self.inner.security_config.write().await;
        let mut upstream = self.inner.upstream_proxy.write().await;
        let mut zai = self.inner.zai_config.write().await;

        *mapping = proxy_config.custom_mapping.clone();
        *experimental = proxy_config.experimental;
        *security = ProxySecurityConfig::from_proxy_config(&proxy_config);
        *upstream = proxy_config.upstream_proxy.clone();
        *zai = proxy_config.zai.clone();
        *inner_proxy_config = proxy_config;

        // Sync enforce_proxy to TokenManager for side-channel leak prevention
        self.inner.token_manager.set_enforce_proxy(upstream.enforce_proxy);

        // Update proxy pool with new configuration (rotation strategy, proxy URLs, etc.)
        self.inner.upstream_client.update_proxy_config().await;

        tracing::info!("Proxy configuration hot reloaded successfully.");
    }

    pub async fn reload_accounts(&self) -> Result<usize, String> {
        match self.inner.token_manager.load_accounts().await {
            Ok(count) => {
                tracing::info!("Reloaded {} accounts into token manager", count);
                Ok(count)
            },
            Err(e) => {
                tracing::error!("Failed to reload accounts: {}", e);
                Err(e)
            },
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

    pub fn generate_oauth_state(&self, proxy_url: Option<String>) -> String {
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
        self.inner.oauth_states.insert(state.clone(), (Instant::now(), proxy_url));
        state
    }

    /// Validate oauth state and return the associated proxy_url if present.
    pub fn validate_oauth_state(&self, state: &str) -> Option<Option<String>> {
        if let Some((_, (created_at, proxy_url))) = self.inner.oauth_states.remove(state) {
            if created_at.elapsed().as_secs() < 600 {
                Some(proxy_url)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn cleanup_expired_oauth_states(&self) {
        self.inner.oauth_states.retain(|_, (created_at, _)| created_at.elapsed().as_secs() < 600);
    }

    pub fn repository(&self) -> Option<&Arc<dyn AccountRepository>> {
        self.inner.repository.as_ref()
    }
}
