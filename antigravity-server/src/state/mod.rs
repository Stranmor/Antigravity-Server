//! Application State
//!
//! Holds shared state for the server including account manager and proxy config.

mod accessors;
mod config_sync;
mod proxy_sync;

use anyhow::Result;
use axum::Router;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU16, AtomicUsize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use antigravity_core::modules::repository::AccountRepository;
use antigravity_core::proxy::{
    build_proxy_router_with_shared_state, AdaptiveLimitManager, CircuitBreakerManager,
    HealthMonitor, ProxyMonitor, ProxyRouterConfig, ProxySecurityConfig, TokenManager,
};
use antigravity_types::models::ProxyConfig;

// Re-export helper function

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub(crate) inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    pub token_manager: Arc<TokenManager>,
    pub monitor: Arc<ProxyMonitor>,
    pub proxy_config: Arc<RwLock<ProxyConfig>>,
    pub custom_mapping: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pub mapping_timestamps: Arc<RwLock<std::collections::HashMap<String, i64>>>,
    pub security_config: Arc<RwLock<ProxySecurityConfig>>,
    pub upstream_proxy: Arc<RwLock<antigravity_types::models::UpstreamProxyConfig>>,
    pub zai_config: Arc<RwLock<antigravity_types::models::ZaiConfig>>,
    pub experimental_config: Arc<RwLock<antigravity_types::models::ExperimentalConfig>>,
    pub adaptive_limits: Arc<AdaptiveLimitManager>,
    pub health_monitor: Arc<HealthMonitor>,
    pub circuit_breaker: Arc<CircuitBreakerManager>,
    pub oauth_states: Arc<DashMap<String, (Instant, Option<String>)>>,
    pub bound_port: AtomicU16,
    pub http_client: wreq::Client,
    pub repository: Option<Arc<dyn AccountRepository>>,
    pub provider_rr: Arc<AtomicUsize>,
    pub zai_vision_mcp: Arc<antigravity_core::proxy::zai_vision_mcp::ZaiVisionMcpState>,
    pub upstream_client: Arc<antigravity_core::proxy::upstream::client::UpstreamClient>,
    pub proxy_assignments: Arc<RwLock<antigravity_types::SyncableProxyAssignments>>,
}

impl AppState {
    /// Create AppState with pre-initialized components (for headless mode)
    pub async fn new_with_components(
        token_manager: Arc<TokenManager>,
        monitor: Arc<ProxyMonitor>,
        proxy_config: ProxyConfig,
        repository: Option<Arc<dyn AccountRepository>>,
    ) -> Result<Self> {
        let custom_mapping = Arc::new(RwLock::new(proxy_config.custom_mapping.clone()));
        let upstream_proxy = Arc::new(RwLock::new(proxy_config.upstream_proxy.clone()));
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

        let http_client = antigravity_core::proxy::common::client_builder::build_http_client(
            Some(&proxy_config.upstream_proxy),
            300,
        )
        .unwrap_or_else(|_| {
            wreq::Client::builder()
                .emulation(antigravity_core::proxy::upstream::emulation::default_emulation())
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to build fallback HTTP client")
        });

        health_monitor.start_recovery_task();

        token_manager.set_adaptive_limits(adaptive_limits.clone()).await;
        token_manager.set_health_monitor(health_monitor.clone()).await;
        token_manager.set_enforce_proxy(proxy_config.upstream_proxy.enforce_proxy);

        tracing::info!("AIMD rate limiting system initialized");

        let provider_rr = Arc::new(AtomicUsize::new(0));
        let zai_vision_mcp =
            Arc::new(antigravity_core::proxy::zai_vision_mcp::ZaiVisionMcpState::new());
        let upstream_client =
            Arc::new(antigravity_core::proxy::upstream::client::UpstreamClient::new(
                http_client.clone(),
                Arc::clone(&upstream_proxy),
                None,
            ));

        Ok(Self {
            inner: Arc::new(AppStateInner {
                token_manager,
                monitor,
                proxy_config: Arc::new(RwLock::new(proxy_config)),
                custom_mapping,
                upstream_proxy,
                mapping_timestamps: Arc::new(RwLock::new(std::collections::HashMap::new())),
                security_config,
                zai_config,
                experimental_config,
                adaptive_limits,
                health_monitor,
                circuit_breaker,
                oauth_states: Arc::new(DashMap::new()),
                bound_port: AtomicU16::new(0),
                http_client,
                repository,
                provider_rr,
                zai_vision_mcp,
                upstream_client,
                proxy_assignments: Arc::new(RwLock::new(
                    antigravity_types::SyncableProxyAssignments::new(),
                )),
            }),
        })
    }

    pub async fn build_proxy_router(&self) -> Router {
        build_proxy_router_with_shared_state(ProxyRouterConfig {
            token_manager: self.inner.token_manager.clone(),
            custom_mapping: self.inner.custom_mapping.clone(),
            upstream_proxy: Arc::clone(&self.inner.upstream_proxy),
            security_config: self.inner.security_config.clone(),
            zai: self.inner.zai_config.clone(),
            monitor: self.inner.monitor.clone(),
            experimental: self.inner.experimental_config.clone(),
            adaptive_limits: self.inner.adaptive_limits.clone(),
            health_monitor: self.inner.health_monitor.clone(),
            circuit_breaker: self.inner.circuit_breaker.clone(),
            http_client: self.inner.http_client.clone(),
            provider_rr: self.inner.provider_rr.clone(),
            zai_vision_mcp: self.inner.zai_vision_mcp.clone(),
            upstream_client: self.inner.upstream_client.clone(),
        })
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
