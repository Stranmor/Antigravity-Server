//! Application State
//!
//! Holds shared state for the server including account manager and proxy config.

use anyhow::Result;
use axum::Router;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use antigravity_core::models::Account;
use antigravity_core::modules::account;
use antigravity_core::proxy::{
    build_proxy_router_with_shared_state, server::AxumServer, warp_isolation::WarpIsolationManager,
    AdaptiveLimitManager, CircuitBreakerManager, HealthMonitor, ProxyMonitor, ProxySecurityConfig,
    TokenManager,
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
    #[allow(dead_code)] // Reserved for future hot-reload (listener restart)
    pub axum_server: Arc<AxumServer>,
    // Shared state for hot-reload
    pub custom_mapping: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pub security_config: Arc<RwLock<ProxySecurityConfig>>,
    pub zai_config: Arc<RwLock<antigravity_shared::proxy::config::ZaiConfig>>,
    pub experimental_config: Arc<RwLock<antigravity_shared::proxy::config::ExperimentalConfig>>,
    // AIMD Predictive Rate Limiting System
    // Wired into proxy router via build_proxy_router_with_shared_state
    pub adaptive_limits: Arc<AdaptiveLimitManager>,
    pub health_monitor: Arc<HealthMonitor>,
    pub circuit_breaker: Arc<CircuitBreakerManager>,
    pub warp_isolation: Option<Arc<WarpIsolationManager>>,
    /// OAuth state tokens for CSRF protection (state -> created_at)
    pub oauth_states: Arc<DashMap<String, Instant>>,
}

impl AppState {
    /// Create AppState with pre-initialized components (for headless mode)
    pub async fn new_with_components(
        token_manager: Arc<TokenManager>,
        monitor: Arc<ProxyMonitor>,
        proxy_config: ProxyConfig,
        axum_server: Arc<AxumServer>,
        warp_isolation: Option<Arc<WarpIsolationManager>>,
    ) -> Result<Self> {
        // Create shared Arc references for hot-reload
        let custom_mapping = Arc::new(RwLock::new(proxy_config.custom_mapping.clone()));
        let security_config = Arc::new(RwLock::new(ProxySecurityConfig::from_proxy_config(
            &proxy_config,
        )));
        let zai_config = Arc::new(RwLock::new(proxy_config.zai.clone()));
        let experimental_config = Arc::new(RwLock::new(proxy_config.experimental.clone()));

        // Initialize AIMD Predictive Rate Limiting System
        let adaptive_limits = Arc::new(AdaptiveLimitManager::new(
            0.85, // safety_margin: 85% of confirmed limit is working threshold
            antigravity_core::proxy::AIMDController::default(),
        ));
        let health_monitor = HealthMonitor::new();
        let circuit_breaker = Arc::new(CircuitBreakerManager::new());

        // Start health monitor recovery task
        health_monitor.start_recovery_task();

        // Inject AIMD into TokenManager
        token_manager
            .set_adaptive_limits(adaptive_limits.clone())
            .await;

        tracing::info!("🎯 AIMD rate limiting system initialized");

        Ok(Self {
            inner: Arc::new(AppStateInner {
                token_manager,
                monitor,
                proxy_config: Arc::new(RwLock::new(proxy_config)),
                axum_server,
                custom_mapping,
                security_config,
                zai_config,
                experimental_config,
                adaptive_limits,
                health_monitor,
                circuit_breaker,
                warp_isolation,
                oauth_states: Arc::new(DashMap::new()),
            }),
        })
    }

    pub fn build_proxy_router(&self) -> Router {
        build_proxy_router_with_shared_state(
            self.inner.token_manager.clone(),
            self.inner.custom_mapping.clone(),
            // We need to get upstream_proxy, but it's in proxy_config - for now use default
            antigravity_shared::utils::http::UpstreamProxyConfig::default(),
            self.inner.security_config.clone(),
            self.inner.zai_config.clone(),
            self.inner.monitor.clone(),
            self.inner.experimental_config.clone(),
            self.inner.adaptive_limits.clone(),
            self.inner.health_monitor.clone(),
            self.inner.circuit_breaker.clone(),
            self.inner.warp_isolation.clone(),
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

    pub async fn get_proxy_bind_address(&self) -> String {
        self.inner.proxy_config.read().await.get_bind_address()
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

                // Update shared state directly (these are used by build_proxy_router_with_shared_state)
                {
                    let mut mapping = self.inner.custom_mapping.write().await;
                    *mapping = proxy_config.custom_mapping.clone();
                    tracing::debug!("📝 Updated custom_mapping with {} entries", mapping.len());
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

                // Also update the full proxy_config reference
                let mut inner_proxy_config = self.inner.proxy_config.write().await;
                *inner_proxy_config = proxy_config;

                tracing::info!("✅ Proxy configuration hot reloaded successfully.");
            }
            Err(e) => {
                tracing::error!("❌ Failed to hot reload proxy configuration: {}", e);
            }
        }
    }

    /// Reload accounts into token manager (after OAuth or import)
    pub async fn reload_accounts(&self) -> Result<usize, String> {
        match self.inner.token_manager.load_accounts().await {
            Ok(count) => {
                tracing::info!("🔄 Reloaded {} accounts into token manager", count);
                Ok(count)
            }
            Err(e) => {
                tracing::error!("❌ Failed to reload accounts: {}", e);
                Err(e)
            }
        }
    }

    pub fn clear_session_bindings(&self) {
        self.inner.token_manager.clear_all_sessions();
        tracing::info!("🔄 Cleared all session bindings");
    }

    // AIMD accessors (used by /api/resilience/* endpoints)
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
        let state: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        self.inner
            .oauth_states
            .insert(state.clone(), Instant::now());
        self.cleanup_expired_oauth_states();
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
}

pub fn get_model_quota(account: &Account, model_prefix: &str) -> Option<i32> {
    account.quota.as_ref().and_then(|q| {
        q.models
            .iter()
            .find(|m| m.name.to_lowercase().contains(&model_prefix.to_lowercase()))
            .map(|m| m.percentage)
    })
}
