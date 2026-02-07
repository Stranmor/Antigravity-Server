//! Test helpers for antigravity-server unit tests.

use std::collections::HashMap;
use std::sync::Arc;

use tempfile::TempDir;

use antigravity_core::proxy::{
    server::{AxumServer, ServerStartConfig},
    AIMDController, AdaptiveLimitManager, CircuitBreakerManager, HealthMonitor, ProxyMonitor,
    ProxySecurityConfig, TokenManager,
};
use antigravity_types::models::{
    ExperimentalConfig, ProxyAuthMode, ProxyConfig, UpstreamProxyConfig, ZaiConfig,
};

use crate::state::AppState;

/// Create a minimal `AppState` for testing.
///
/// Returns `(AppState, TempDir)` — keep `TempDir` alive for the test duration.
pub async fn test_app_state() -> (AppState, TempDir) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let token_manager = Arc::new(TokenManager::new(temp_dir.path().to_path_buf()));
    let monitor = Arc::new(ProxyMonitor::new());
    let proxy_config = ProxyConfig::default();

    let server_config = ServerStartConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        token_manager: token_manager.clone(),
        custom_mapping: HashMap::new(),
        upstream_proxy: UpstreamProxyConfig::default(),
        security_config: ProxySecurityConfig {
            auth_mode: ProxyAuthMode::Off,
            api_key: String::new(),
            allow_lan_access: false,
        },
        zai: ZaiConfig::default(),
        monitor: monitor.clone(),
        experimental: ExperimentalConfig::default(),
        adaptive_limits: Arc::new(AdaptiveLimitManager::new(0.85, AIMDController::default())),
        health_monitor: HealthMonitor::new(),
        circuit_breaker: Arc::new(CircuitBreakerManager::new()),
        warp_isolation: None,
    };
    let axum_server = Arc::new(AxumServer::new(server_config));

    let state = AppState::new_with_components(
        token_manager,
        monitor,
        proxy_config,
        axum_server,
        None,
        None,
    )
    .await
    .expect("failed to create test AppState");

    (state, temp_dir)
}
