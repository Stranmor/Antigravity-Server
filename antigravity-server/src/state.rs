//! Application State
//!
//! Holds shared state for the server including account manager and proxy config.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    // TODO: Add account manager from antigravity-core
    // TODO: Add proxy server handle
    // TODO: Add config
}

impl AppState {
    pub async fn new() -> Result<Self> {
        // TODO: Initialize account manager
        // TODO: Load config
        // TODO: Start proxy server in background
        
        Ok(Self {
            inner: Arc::new(AppStateInner {}),
        })
    }
}
