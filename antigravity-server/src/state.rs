//! Application State
//!
//! Holds shared state for the server including account manager and proxy config.

use anyhow::Result;
use std::sync::Arc;

use antigravity_core::modules::account;
use antigravity_core::models::Account;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    // Runtime state - accounts are loaded from disk on each request
    // (antigravity-core uses file-based storage with index)
}

impl AppState {
    pub async fn new() -> Result<Self> {
        // Verify data directory exists and is accessible
        let data_dir = account::get_data_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get data directory: {}", e))?;
        
        tracing::info!("📁 Data directory: {:?}", data_dir);
        
        // Try to load account index to verify it works
        match account::load_account_index() {
            Ok(index) => {
                tracing::info!("📊 Loaded {} accounts from index", index.accounts.len());
            }
            Err(e) => {
                tracing::warn!("⚠️ Could not load account index: {}", e);
            }
        }
        
        Ok(Self {
            inner: Arc::new(AppStateInner {}),
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
    
    /// Switch to a different account (sync wrapper for async function)
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
}

/// Helper to extract quota percentage by model name
pub fn get_model_quota(account: &Account, model_prefix: &str) -> Option<i32> {
    account.quota.as_ref().and_then(|q| {
        q.models.iter()
            .find(|m| m.name.to_lowercase().contains(&model_prefix.to_lowercase()))
            .map(|m| m.percentage)
    })
}
