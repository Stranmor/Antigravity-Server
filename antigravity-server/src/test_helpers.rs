//! Test helpers for antigravity-server unit tests.

use std::sync::Arc;

use tempfile::TempDir;

use antigravity_core::proxy::{ProxyMonitor, TokenManager};
use antigravity_types::models::ProxyConfig;

use crate::state::AppState;

/// Create a minimal `AppState` for testing.
///
/// Returns `(AppState, TempDir)` — keep `TempDir` alive for the test duration.
pub async fn test_app_state() -> (AppState, TempDir) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let token_manager = Arc::new(TokenManager::new(temp_dir.path().to_path_buf()));
    let monitor = Arc::new(ProxyMonitor::new());
    let proxy_config = ProxyConfig::default();

    let state = AppState::new_with_components(token_manager, monitor, proxy_config, None, None)
        .await
        .expect("failed to create test AppState");

    (state, temp_dir)
}
