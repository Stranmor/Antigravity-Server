//! Backend bridge for Slint UI.
//!
//! This module provides the interface between the Slint UI and the core business logic.

use antigravity_core::models::Account;
use antigravity_core::modules;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Backend state shared with the UI.
pub struct BackendState {
    /// Currently loaded accounts.
    accounts: Vec<Account>,
    /// Current account index.
    current_account_id: Option<String>,
}

impl BackendState {
    /// Create a new backend state.
    pub fn new() -> Self {
        Self {
            accounts: Vec::new(),
            current_account_id: None,
        }
    }

    /// Load all accounts from storage.
    pub fn load_accounts(&mut self) -> Result<(), String> {
        self.accounts = modules::list_accounts()?;
        self.current_account_id = modules::get_current_account_id()?;
        tracing::info!("Loaded {} accounts", self.accounts.len());
        Ok(())
    }

    /// Get all accounts as a slice.
    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    /// Get current account.
    pub fn get_current_account(&self) -> Option<&Account> {
        self.current_account_id.as_ref().and_then(|id| {
            self.accounts.iter().find(|a| &a.id == id)
        })
    }

    /// Get current account ID.
    pub fn current_account_id(&self) -> Option<&str> {
        self.current_account_id.as_deref()
    }

    /// Get account count.
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }

    /// Calculate average quota for a model type (e.g., "gemini", "claude").
    fn avg_quota_for_model(&self, model_prefix: &str) -> f32 {
        let quotas: Vec<f32> = self.accounts.iter()
            .filter_map(|a| a.quota.as_ref())
            .flat_map(|q| {
                q.models.iter()
                    .filter(|m| m.name.to_lowercase().contains(model_prefix))
                    .map(|m| m.percentage as f32)
            })
            .collect();
        
        if quotas.is_empty() { 0.0 } else { quotas.iter().sum::<f32>() / quotas.len() as f32 }
    }

    /// Calculate average Gemini quota.
    pub fn avg_gemini_quota(&self) -> f32 {
        self.avg_quota_for_model("gemini")
    }

    /// Calculate average Gemini Image quota.
    pub fn avg_gemini_image_quota(&self) -> f32 {
        self.avg_quota_for_model("image")
    }

    /// Calculate average Claude quota.
    pub fn avg_claude_quota(&self) -> f32 {
        self.avg_quota_for_model("claude")
    }

    /// Count accounts with low quota (< 20%).
    pub fn low_quota_count(&self) -> usize {
        self.accounts.iter()
            .filter(|a| {
                if let Some(q) = &a.quota {
                    q.models.iter().any(|m| m.percentage < 20)
                } else {
                    false
                }
            })
            .count()
    }

    /// Count accounts by subscription tier.
    fn count_by_tier(&self, tier: &str) -> usize {
        self.accounts.iter()
            .filter(|a| {
                if let Some(q) = &a.quota {
                    if let Some(t) = &q.subscription_tier {
                        return t.to_lowercase().contains(tier);
                    }
                }
                false
            })
            .count()
    }

    /// Count PRO accounts (PRO but not ULTRA).
    pub fn pro_count(&self) -> usize {
        self.count_by_tier("pro").saturating_sub(self.ultra_count())
    }

    /// Count ULTRA accounts.
    pub fn ultra_count(&self) -> usize {
        self.count_by_tier("ultra")
    }

    /// Count FREE accounts.
    pub fn free_count(&self) -> usize {
        self.accounts.iter()
            .filter(|a| {
                if let Some(q) = &a.quota {
                    if let Some(t) = &q.subscription_tier {
                        let t = t.to_lowercase();
                        return !t.contains("pro") && !t.contains("ultra");
                    }
                }
                true // No tier = free
            })
            .count()
    }

    /// Get quota for a specific model from an account.
    pub fn get_model_quota(account: &Account, model_name: &str) -> i32 {
        account.quota.as_ref()
            .and_then(|q| q.models.iter().find(|m| m.name.to_lowercase().contains(model_name)))
            .map(|m| m.percentage)
            .unwrap_or(0)
    }

    /// Get subscription tier from account.
    pub fn get_tier(account: &Account) -> String {
        account.quota.as_ref()
            .and_then(|q| q.subscription_tier.as_ref())
            .map(|t| {
                let t = t.to_lowercase();
                if t.contains("ultra") { "ULTRA".to_string() }
                else if t.contains("pro") { "PRO".to_string() }
                else { "FREE".to_string() }
            })
            .unwrap_or_else(|| "FREE".to_string())
    }
}

impl Default for BackendState {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe backend handle.
pub type Backend = Arc<Mutex<BackendState>>;

/// Create a new backend handle.
pub fn create_backend() -> Backend {
    Arc::new(Mutex::new(BackendState::new()))
}
