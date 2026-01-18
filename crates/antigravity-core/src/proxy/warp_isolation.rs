//! WARP IP Isolation - Per-account SOCKS5 proxy management.
//!
//! This module provides IP isolation for Google accounts by mapping each account
//! to a dedicated WARP SOCKS5 proxy endpoint.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌─────────────┐
//! │  Account 1  │ ──► │ SOCKS5:10800 │ ──► │  WARP IP 1  │
//! ├─────────────┤     ├──────────────┤     ├─────────────┤
//! │  Account 2  │ ──► │ SOCKS5:10801 │ ──► │  WARP IP 2  │
//! └─────────────┘     └──────────────┘     └─────────────┘
//! ```
//!
//! # Configuration
//!
//! The mapping file (`/etc/antigravity/warp/ip_mapping.json`) contains:
//! ```json
//! {
//!   "accounts": [
//!     {"id": "uuid-123", "email": "user@example.com", "socks5_endpoint": "socks5://127.0.0.1:10800"}
//!   ]
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Default path to WARP IP mapping file.
pub const DEFAULT_WARP_MAPPING_PATH: &str = "/etc/antigravity/warp/ip_mapping.json";

/// WARP account mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarpAccountMapping {
    /// Account ID (UUID).
    pub id: String,
    /// Account email.
    pub email: String,
    /// WARP SOCKS5 port.
    pub warp_port: u16,
    /// WARP container name.
    pub warp_container: String,
    /// Full SOCKS5 endpoint URL (e.g., "socks5://127.0.0.1:10800").
    pub socks5_endpoint: String,
}

/// WARP IP mapping file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarpMappingFile {
    /// File format version.
    pub version: String,
    /// Generation timestamp.
    pub generated_at: String,
    /// Account mappings.
    pub accounts: Vec<WarpAccountMapping>,
}

/// WARP Isolation Manager - manages per-account proxy routing.
#[derive(Debug, Clone)]
pub struct WarpIsolationManager {
    /// Account ID -> SOCKS5 endpoint mapping.
    account_to_proxy: Arc<RwLock<HashMap<String, String>>>,
    /// Email -> Account ID mapping (for lookup by email).
    email_to_account: Arc<RwLock<HashMap<String, String>>>,
    /// Whether WARP isolation is enabled.
    enabled: Arc<RwLock<bool>>,
    /// Path to mapping file for hot-reload.
    mapping_path: String,
}

impl WarpIsolationManager {
    /// Create a new WARP isolation manager.
    pub fn new() -> Self {
        Self {
            account_to_proxy: Arc::new(RwLock::new(HashMap::new())),
            email_to_account: Arc::new(RwLock::new(HashMap::new())),
            enabled: Arc::new(RwLock::new(false)),
            mapping_path: DEFAULT_WARP_MAPPING_PATH.to_string(),
        }
    }

    /// Create manager with custom mapping path.
    pub fn with_path(path: impl Into<String>) -> Self {
        let mut manager = Self::new();
        manager.mapping_path = path.into();
        manager
    }

    /// Load mappings from file.
    pub async fn load_mappings(&self) -> Result<usize, String> {
        let path = Path::new(&self.mapping_path);

        if !path.exists() {
            warn!(
                "WARP mapping file not found: {}. IP isolation disabled.",
                self.mapping_path
            );
            *self.enabled.write().await = false;
            return Ok(0);
        }

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read WARP mapping file: {}", e))?;

        let mapping: WarpMappingFile = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse WARP mapping file: {}", e))?;

        let mut account_map = self.account_to_proxy.write().await;
        let mut email_map = self.email_to_account.write().await;

        account_map.clear();
        email_map.clear();

        for entry in &mapping.accounts {
            account_map.insert(entry.id.clone(), entry.socks5_endpoint.clone());
            email_map.insert(entry.email.clone(), entry.id.clone());

            debug!(
                "WARP mapping: {} ({}) -> {}",
                entry.email, entry.id, entry.socks5_endpoint
            );
        }

        let count = account_map.len();
        *self.enabled.write().await = count > 0;

        info!(
            "Loaded {} WARP account mappings from {}",
            count, self.mapping_path
        );

        Ok(count)
    }

    /// Check if WARP isolation is enabled.
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    /// Get SOCKS5 proxy URL for an account by ID.
    pub async fn get_proxy_for_account(&self, account_id: &str) -> Option<String> {
        if !self.is_enabled().await {
            return None;
        }

        self.account_to_proxy.read().await.get(account_id).cloned()
    }

    /// Get SOCKS5 proxy URL for an account by email.
    ///
    /// [DISABLED 2026-01-17] WARP proxy causes artificial 429 errors from Google.
    /// Google detects Cloudflare WARP IPs and applies stricter rate limits.
    /// Requests should go directly like upstream does.
    /// TODO: Re-enable when proper IP rotation strategy is implemented.
    pub async fn get_proxy_for_email(&self, _email: &str) -> Option<String> {
        // WARP disabled - always return None to use direct connection
        None
    }

    /// Get all account IDs with WARP proxies.
    pub async fn get_warp_accounts(&self) -> Vec<String> {
        self.account_to_proxy.read().await.keys().cloned().collect()
    }

    /// Reload mappings from file (for hot-reload).
    pub async fn reload(&self) -> Result<usize, String> {
        info!("Reloading WARP mappings from {}", self.mapping_path);
        self.load_mappings().await
    }

    /// Create a reqwest client configured with the account's WARP proxy.
    pub async fn create_client_for_account(
        &self,
        account_id: &str,
        timeout_secs: u64,
    ) -> Result<reqwest::Client, String> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs.max(30)));

        if let Some(proxy_url) = self.get_proxy_for_account(account_id).await {
            let proxy = reqwest::Proxy::all(&proxy_url)
                .map_err(|e| format!("Invalid WARP proxy URL '{}': {}", proxy_url, e))?;
            builder = builder.proxy(proxy);
            debug!("Using WARP proxy {} for account {}", proxy_url, account_id);
        } else {
            debug!(
                "No WARP proxy for account {}, using direct connection",
                account_id
            );
        }

        builder
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {}", e))
    }
}

impl Default for WarpIsolationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_disabled_when_no_file() {
        let manager = WarpIsolationManager::with_path("/nonexistent/path.json");
        let result = manager.load_mappings().await;
        assert!(result.is_ok());
        assert!(!manager.is_enabled().await);
    }

    #[tokio::test]
    async fn test_get_proxy_returns_none_when_disabled() {
        let manager = WarpIsolationManager::new();
        assert!(manager.get_proxy_for_account("test-id").await.is_none());
    }
}
