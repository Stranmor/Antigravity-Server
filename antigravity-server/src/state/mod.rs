//! Application State
//!
//! Holds shared state for the server including account manager and proxy config.

mod accessors;
mod config_sync;

use anyhow::Result;
use axum::Router;
use dashmap::DashMap;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use antigravity_core::modules::repository::AccountRepository;
use antigravity_core::proxy::{
    build_proxy_router_with_shared_state, server::AxumServer, warp_isolation::WarpIsolationManager,
    AdaptiveLimitManager, CircuitBreakerManager, HealthMonitor, ProxyMonitor, ProxySecurityConfig,
    TokenManager,
};
use antigravity_types::models::ProxyConfig;

// Re-export helper function
pub use accessors::get_model_quota;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub(crate) inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub token_manager: Arc<TokenManager>,
    pub monitor: Arc<ProxyMonitor>,
    pub proxy_config: Arc<RwLock<ProxyConfig>>,
    #[allow(dead_code, reason = "Reserved for future hot-reload (listener restart)")]
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
    #[allow(dead_code, reason = "WIP: PostgreSQL migration, will be used in phase 7")]
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
        let security_config =
            Arc::new(RwLock::new(ProxySecurityConfig::from_proxy_config(&proxy_config)));
        let zai_config = Arc::new(RwLock::new(proxy_config.zai.clone()));
        let experimental_config = Arc::new(RwLock::new(proxy_config.experimental));

        let adaptive_limits = Arc::new(AdaptiveLimitManager::new(
            0.85,
            antigravity_core::proxy::AIMDController::default(),
        ));
        let health_monitor = HealthMonitor::new();
        let circuit_breaker = Arc::new(CircuitBreakerManager::new());

        health_monitor.start_recovery_task();

        token_manager.set_adaptive_limits(adaptive_limits.clone()).await;
        token_manager.set_health_monitor(health_monitor.clone()).await;

        tracing::info!("AIMD rate limiting system initialized");

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
}

#[allow(
    clippy::expect_used,
    reason = "System clock before UNIX epoch = fundamentally broken system"
)]
pub(crate) fn current_timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as i64
}

pub(crate) fn get_instance_id() -> String {
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
