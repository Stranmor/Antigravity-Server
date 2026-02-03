//! Application State
//!
//! Holds shared state for the server including account manager and proxy config.

use anyhow::Result;
use axum::Router;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use antigravity_core::models::Account;
use antigravity_core::modules::account;
use antigravity_core::modules::repository::{AccountRepository, RepoResult};
use antigravity_core::proxy::{
    build_proxy_router_with_shared_state, server::AxumServer, warp_isolation::WarpIsolationManager,
    AdaptiveLimitManager, CircuitBreakerManager, HealthMonitor, ProxyMonitor, ProxySecurityConfig,
    TokenManager,
};
use antigravity_types::models::ProxyConfig;

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
    pub custom_mapping: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pub mapping_timestamps: Arc<RwLock<std::collections::HashMap<String, i64>>>,
    pub security_config: Arc<RwLock<ProxySecurityConfig>>,
    pub zai_config: Arc<RwLock<antigravity_types::models::ZaiConfig>>,
    pub experimental_config: Arc<RwLock<antigravity_types::models::ExperimentalConfig>>,
    pub adaptive_limits: Arc<AdaptiveLimitManager>,
    pub health_monitor: Arc<HealthMonitor>,
    pub circuit_breaker: Arc<CircuitBreakerManager>,
    pub warp_isolation: Option<Arc<WarpIsolationManager>>,
    pub oauth_states: Arc<DashMap<String, Instant>>,
    pub bound_port: AtomicU16,
    #[allow(dead_code)] // WIP: PostgreSQL migration, will be used in phase 7
    pub repository: Option<Arc<dyn AccountRepository>>,
}

impl AppState {
    /// Create AppState with pre-initialized components (for headless mode)
    pub async fn new_with_components(
        token_manager: Arc<TokenManager>,
        monitor: Arc<ProxyMonitor>,
        proxy_config: ProxyConfig,
        axum_server: Arc<AxumServer>,
        warp_isolation: Option<Arc<WarpIsolationManager>>,
        repository: Option<Arc<dyn AccountRepository>>,
    ) -> Result<Self> {
        let custom_mapping = Arc::new(RwLock::new(proxy_config.custom_mapping.clone()));
        let security_config = Arc::new(RwLock::new(ProxySecurityConfig::from_proxy_config(
            &proxy_config,
        )));
        let zai_config = Arc::new(RwLock::new(proxy_config.zai.clone()));
        let experimental_config = Arc::new(RwLock::new(proxy_config.experimental.clone()));

        let adaptive_limits = Arc::new(AdaptiveLimitManager::new(
            0.85,
            antigravity_core::proxy::AIMDController::default(),
        ));
        let health_monitor = HealthMonitor::new();
        let circuit_breaker = Arc::new(CircuitBreakerManager::new());

        health_monitor.start_recovery_task();

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
                mapping_timestamps: Arc::new(RwLock::new(std::collections::HashMap::new())),
                security_config,
                zai_config,
                experimental_config,
                adaptive_limits,
                health_monitor,
                circuit_breaker,
                warp_isolation,
                oauth_states: Arc::new(DashMap::new()),
                bound_port: AtomicU16::new(0),
                repository,
            }),
        })
    }

    /// Set the actual port the server is listening on (called after listener bind)
    pub fn set_bound_port(&self, port: u16) {
        self.inner.bound_port.store(port, Ordering::Relaxed);
    }

    /// Get the actual bound port (returns 0 if not yet bound)
    pub fn get_bound_port(&self) -> u16 {
        self.inner.bound_port.load(Ordering::Relaxed)
    }

    pub fn build_proxy_router(&self) -> Router {
        build_proxy_router_with_shared_state(
            self.inner.token_manager.clone(),
            self.inner.custom_mapping.clone(),
            // We need to get upstream_proxy, but it's in proxy_config - for now use default
            antigravity_types::models::UpstreamProxyConfig::default(),
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

    pub fn clear_all_rate_limits(&self) {
        self.inner.token_manager.clear_all_rate_limits();
    }

    pub fn clear_rate_limit(&self, account_id: &str) -> bool {
        self.inner.token_manager.clear_rate_limit(account_id)
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

    pub async fn get_syncable_mapping(&self) -> antigravity_types::SyncableMapping {
        use antigravity_types::MappingEntry;

        let mapping = self.inner.custom_mapping.read().await;
        let timestamps = self.inner.mapping_timestamps.read().await;

        let entries = mapping
            .iter()
            .map(|(k, v)| {
                let ts = timestamps
                    .get(k)
                    .copied()
                    .unwrap_or_else(current_timestamp_ms);
                (k.clone(), MappingEntry::with_timestamp(v.clone(), ts))
            })
            .collect();

        antigravity_types::SyncableMapping {
            entries,
            instance_id: Some(get_instance_id()),
        }
    }

    pub async fn sync_with_remote(
        &self,
        remote: &antigravity_types::SyncableMapping,
    ) -> (usize, antigravity_types::SyncableMapping) {
        use antigravity_types::MappingEntry;

        let (mapping_to_persist, inbound, diff) = {
            let mut mapping = self.inner.custom_mapping.write().await;
            let mut timestamps = self.inner.mapping_timestamps.write().await;

            let local_entries: std::collections::HashMap<_, _> = mapping
                .iter()
                .map(|(k, v)| {
                    let ts = timestamps.get(k).copied().unwrap_or(0);
                    (k.clone(), MappingEntry::with_timestamp(v.clone(), ts))
                })
                .collect();
            let local_mapping = antigravity_types::SyncableMapping {
                entries: local_entries,
                instance_id: Some(get_instance_id()),
            };

            let diff = local_mapping.diff_newer_than(remote);

            let mut updated = 0;
            for (key, remote_entry) in &remote.entries {
                let local_ts = timestamps.get(key).copied().unwrap_or(0);
                if remote_entry.updated_at > local_ts {
                    mapping.insert(key.clone(), remote_entry.target.clone());
                    timestamps.insert(key.clone(), remote_entry.updated_at);
                    updated += 1;
                }
            }

            let persist = if updated > 0 {
                Some(mapping.clone())
            } else {
                None
            };

            (persist, updated, diff)
        };

        if let Some(ref map) = mapping_to_persist {
            tracing::info!(
                "🔄 Merged {} mapping entries from remote (instance: {:?})",
                inbound,
                remote.instance_id
            );
            if let Err(e) = self.persist_mapping_to_config(map).await {
                tracing::error!("Failed to persist mapping to config: {}", e);
            }
        }

        (inbound, diff)
    }

    pub async fn merge_remote_mapping(&self, remote: &antigravity_types::SyncableMapping) -> usize {
        let mapping_to_persist = {
            let mut mapping = self.inner.custom_mapping.write().await;
            let mut timestamps = self.inner.mapping_timestamps.write().await;

            let mut updated = 0;

            for (key, remote_entry) in &remote.entries {
                let local_ts = timestamps.get(key).copied().unwrap_or(0);

                if remote_entry.updated_at > local_ts {
                    mapping.insert(key.clone(), remote_entry.target.clone());
                    timestamps.insert(key.clone(), remote_entry.updated_at);
                    updated += 1;
                }
            }

            if updated > 0 {
                tracing::info!(
                    "🔄 Merged {} mapping entries from remote (instance: {:?})",
                    updated,
                    remote.instance_id
                );
                Some((mapping.clone(), updated))
            } else {
                None
            }
        };

        if let Some((mapping, updated)) = mapping_to_persist {
            if let Err(e) = self.persist_mapping_to_config(&mapping).await {
                tracing::error!("Failed to persist mapping to config: {}", e);
            }
            updated
        } else {
            0
        }
    }

    async fn persist_mapping_to_config(
        &self,
        mapping: &std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        use antigravity_core::modules::config as core_config;

        let mapping_clone = mapping.clone();
        tokio::task::spawn_blocking(move || {
            core_config::update_config(|config| {
                config.proxy.custom_mapping = mapping_clone;
            })
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {}", e))??;

        let mut proxy_config = self.inner.proxy_config.write().await;
        proxy_config.custom_mapping = mapping.clone();

        Ok(())
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
            None => Err(
                antigravity_core::modules::repository::RepositoryError::Database(
                    "No database configured".to_string(),
                ),
            ),
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

fn current_timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as i64
}

fn get_instance_id() -> String {
    use std::sync::OnceLock;
    static INSTANCE_ID: OnceLock<String> = OnceLock::new();

    INSTANCE_ID
        .get_or_init(|| {
            let hostname = hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".to_string());
            let pid = std::process::id();
            format!("{}-{}", hostname, pid)
        })
        .clone()
}
