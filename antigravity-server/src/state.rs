//! Application State
//!
//! Holds shared state for the server including account manager and proxy server.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

use antigravity_core::modules::account;
use antigravity_core::models::Account;
use antigravity_core::proxy::{AxumServer, ProxyMonitor, TokenManager};
use antigravity_shared::proxy::config::ProxyConfig;
use antigravity_shared::utils::http::UpstreamProxyConfig;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    /// Token manager for account rotation
    token_manager: Arc<TokenManager>,
    /// Proxy monitor for logging
    monitor: Arc<ProxyMonitor>,
    /// Running proxy server (if any)
    proxy_server: RwLock<Option<ProxyServerHandle>>,
    /// Proxy configuration
    proxy_config: RwLock<ProxyConfig>,
}

/// Handle to a running proxy server
struct ProxyServerHandle {
    server: AxumServer,
    _handle: tokio::task::JoinHandle<()>,
}

impl AppState {
    pub async fn new() -> Result<Self> {
        // Get data directory
        let data_dir = account::get_data_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get data directory: {}", e))?;
        
        tracing::info!("📁 Data directory: {:?}", data_dir);
        
        // Initialize token manager
        let token_manager = Arc::new(TokenManager::new(data_dir.clone()));
        
        // Load accounts into token manager
        match token_manager.load_accounts().await {
            Ok(count) => {
                tracing::info!("📊 Loaded {} accounts into token manager", count);
            }
            Err(e) => {
                tracing::warn!("⚠️ Could not load accounts: {}", e);
            }
        }
        
        // Initialize proxy monitor
        let monitor = Arc::new(ProxyMonitor::new());
        
        // Load proxy config (or use defaults with different port)
        let mut proxy_config = load_proxy_config(&data_dir).unwrap_or_default();
        
        // Ensure proxy port is different from WebUI port (8045)
        if proxy_config.port == 8045 {
            proxy_config.port = 8046;
            tracing::info!("📡 Using port 8046 for proxy (8045 is WebUI)");
        }
        
        Ok(Self {
            inner: Arc::new(AppStateInner {
                token_manager,
                monitor,
                proxy_server: RwLock::new(None),
                proxy_config: RwLock::new(proxy_config),
            }),
        })
    }
    
    /// List all accounts
    pub fn list_accounts(&self) -> Result<Vec<Account>, String> {
        account::list_accounts()
    }
    
    /// Get current active account
    pub fn get_current_account(&self) -> Result<Option<Account>, String> {
        account::get_current_account()
    }
    
    /// Switch to a different account
    pub async fn switch_account(&self, account_id: &str) -> Result<(), String> {
        account::switch_account(account_id).await
    }
    
    /// Get enabled account count
    pub fn get_account_count(&self) -> usize {
        match account::list_accounts() {
            Ok(accounts) => accounts.iter().filter(|a| !a.disabled).count(),
            Err(_) => 0,
        }
    }
    
    /// Check if proxy is running
    pub async fn is_proxy_running(&self) -> bool {
        self.inner.proxy_server.read().await.is_some()
    }
    
    /// Get proxy port from config
    pub async fn get_proxy_port(&self) -> u16 {
        self.inner.proxy_config.read().await.port
    }
    
    /// Start the proxy server
    pub async fn start_proxy(&self) -> Result<(), String> {
        // Check if already running
        if self.is_proxy_running().await {
            return Err("Proxy is already running".to_string());
        }
        
        // Reload accounts
        self.inner.token_manager.load_accounts().await?;
        
        if self.inner.token_manager.len() == 0 {
            return Err("No accounts available for proxy".to_string());
        }
        
        // Get config
        let config = self.inner.proxy_config.read().await.clone();
        
        // Start proxy server on configured port
        let host = if config.allow_lan_access { "0.0.0.0" } else { "127.0.0.1" };
        let port = config.port;
        
        // Build security config
        let security_config = antigravity_core::proxy::ProxySecurityConfig::from_proxy_config(&config);
        
        // Start the server
        let (server, handle) = AxumServer::start(
            host.to_string(),
            port,
            self.inner.token_manager.clone(),
            config.custom_mapping.clone(),
            config.request_timeout,
            UpstreamProxyConfig::default(),
            security_config,
            config.zai.clone(),
            self.inner.monitor.clone(),
            config.experimental.clone(),
        ).await?;
        
        // Store the handle
        {
            let mut proxy_server = self.inner.proxy_server.write().await;
            *proxy_server = Some(ProxyServerHandle {
                server,
                _handle: handle,
            });
        }
        
        tracing::info!("🚀 Proxy server started on {}:{}", host, port);
        Ok(())
    }
    
    /// Stop the proxy server
    pub async fn stop_proxy(&self) -> Result<(), String> {
        let mut proxy_server = self.inner.proxy_server.write().await;
        
        if let Some(handle) = proxy_server.take() {
            handle.server.stop();
            tracing::info!("🛑 Proxy server stopped");
            Ok(())
        } else {
            Err("Proxy is not running".to_string())
        }
    }
    
    /// Get proxy stats
    pub async fn get_proxy_stats(&self) -> antigravity_shared::models::ProxyStats {
        self.inner.monitor.get_stats().await
    }
    
    /// Get proxy logs
    pub async fn get_proxy_logs(&self, limit: Option<usize>) -> Vec<antigravity_shared::models::ProxyRequestLog> {
        self.inner.monitor.get_logs(limit).await
    }
    
    /// Clear proxy logs
    pub async fn clear_proxy_logs(&self) {
        self.inner.monitor.clear_logs().await;
    }
    
    /// Get monitor reference
    pub fn get_monitor(&self) -> Arc<ProxyMonitor> {
        self.inner.monitor.clone()
    }
}

/// Helper to extract quota percentage by model name
pub fn get_model_quota(account: &Account, model_prefix: &str) -> Option<i32> {
    account.quota.as_ref().and_then(|q| {
        q.models.iter()
            .find(|m| m.name.to_lowercase().contains(&model_prefix.to_lowercase()))
            .map(|m| m.percentage)
    })
}

/// Load proxy config from disk
fn load_proxy_config(data_dir: &std::path::Path) -> Option<ProxyConfig> {
    let config_path = data_dir.join("config.json");
    
    if !config_path.exists() {
        return None;
    }
    
    let content = std::fs::read_to_string(&config_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    
    // Extract proxy config from the main config
    if let Some(proxy) = value.get("proxy") {
        serde_json::from_value(proxy.clone()).ok()
    } else {
        None
    }
}
