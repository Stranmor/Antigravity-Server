//! Load accounts from PostgreSQL via AccountRepository.
//!
//! Converts `Account` (domain model) → `ProxyToken` (proxy runtime representation).
//! Used when DATABASE_URL is configured; falls back to filesystem otherwise.

use super::proxy_token::ProxyToken;
use super::TokenManager;
use crate::modules::repository::AccountRepository;
use std::collections::HashSet;
use std::sync::Arc;

impl TokenManager {
    /// Load accounts from the repository (PostgreSQL).
    /// Returns the number of accounts loaded, or an error.
    pub(crate) async fn load_accounts_from_repository(
        &self,
        repo: &Arc<dyn AccountRepository>,
    ) -> Result<usize, String> {
        let accounts =
            repo.list_accounts().await.map_err(|e| format!("Repository error: {}", e))?;

        let mut new_tokens: Vec<(String, ProxyToken)> = Vec::new();

        for account in &accounts {
            if account.disabled || account.proxy_disabled {
                tracing::debug!(
                    "Skipping disabled account from DB: {} (email={})",
                    account.id,
                    account.email
                );
                continue;
            }

            match self.convert_account_to_token(account) {
                Ok(token) => {
                    new_tokens.push((account.id.clone(), token));
                },
                Err(e) => {
                    tracing::debug!("Failed to convert DB account {}: {}", account.id, e);
                },
            }
        }

        self.merge_tokens(new_tokens)
    }

    /// Convert a domain Account into a ProxyToken for proxy runtime use.
    fn convert_account_to_token(
        &self,
        account: &crate::models::Account,
    ) -> Result<ProxyToken, String> {
        let token = &account.token;

        let subscription_tier = account.quota.as_ref().and_then(|q| q.subscription_tier.clone());

        let remaining_quota =
            account.quota.as_ref().and_then(|q| q.models.iter().map(|m| m.percentage).max());

        let mut protected_models: HashSet<String> = account.protected_models.clone();
        let mut available_models: HashSet<String> = HashSet::new();

        if let Some(quota) = &account.quota {
            for model in &quota.models {
                available_models.insert(model.name.clone());
                if model.percentage == 0 && !protected_models.contains(&model.name) {
                    protected_models.insert(model.name.clone());
                    tracing::debug!("Auto-protected model {} for account (quota=0%)", model.name);
                }
            }
        }

        let account_path = self.data_dir.join("accounts").join(format!("{}.json", account.id));

        let health_score = self.get_health_score(&account.id);

        Ok(ProxyToken::new(
            account.id.clone(),
            token.access_token.clone(),
            token.refresh_token.clone(),
            token.expires_in,
            token.expiry_timestamp,
            account.email.clone(),
            account_path,
            token.project_id.clone(),
            subscription_tier,
            remaining_quota,
            protected_models,
            health_score,
            available_models,
        ))
    }

    /// Merge new tokens into the token map, removing stale entries.
    /// Shared by both filesystem and repository loading paths.
    pub(crate) fn merge_tokens(
        &self,
        new_tokens: Vec<(String, ProxyToken)>,
    ) -> Result<usize, String> {
        let existing_count = self.tokens.len();
        if new_tokens.is_empty() && existing_count > 0 {
            tracing::warn!(
                "Source returned 0 accounts but {} are loaded — keeping existing to prevent outage",
                existing_count
            );
            return Ok(existing_count);
        }

        let old_keys: Vec<String> = self.tokens.iter().map(|e| e.key().clone()).collect();
        let new_keys: HashSet<String> = new_tokens.iter().map(|(k, _)| k.clone()).collect();

        for old_key in &old_keys {
            if !new_keys.contains(old_key) {
                self.tokens.remove(old_key);
            }
        }

        let count = new_tokens.len();
        for (account_id, disk_token) in new_tokens {
            self.tokens
                .entry(account_id)
                .and_modify(|existing| {
                    if disk_token.timestamp > existing.timestamp {
                        *existing = disk_token.clone();
                    }
                })
                .or_insert(disk_token);
        }

        Ok(count)
    }
}
