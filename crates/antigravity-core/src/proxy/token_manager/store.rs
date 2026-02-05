use super::{file_utils::calculate_max_quota_percentage, proxy_token::ProxyToken, TokenManager};
use crate::modules::oauth;
use std::{collections::HashSet, path::PathBuf};

impl TokenManager {
    pub async fn load_accounts(&self) -> Result<usize, String> {
        let accounts_dir = self.data_dir.join("accounts");
        if !accounts_dir.exists() {
            return Err(format!("Accounts directory not found: {}", accounts_dir.display()));
        }

        let mut new_tokens: Vec<(String, ProxyToken)> = Vec::new();
        let mut entries = tokio::fs::read_dir(&accounts_dir)
            .await
            .map_err(|e| format!("Failed to read accounts directory: {}", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read directory entry: {}", e))?
        {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            match self.load_single_account(&path).await {
                Ok(Some(token)) => {
                    let account_id = token.account_id.clone();
                    new_tokens.push((account_id, token));
                },
                Ok(None) => {},
                Err(e) => {
                    tracing::debug!("Failed to load account {:?}: {}", path, e);
                },
            }
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

    pub async fn reload_account(&self, account_id: &str) -> Result<(), String> {
        let path = self.data_dir.join("accounts").join(format!("{}.json", account_id));
        if !path.exists() {
            return Err(format!("Account file not found: {}", path.display()));
        }

        match self.load_single_account(&path).await {
            Ok(Some(token)) => {
                self.tokens.insert(account_id.to_string(), token);
                Ok(())
            },
            Ok(None) => Err("Account load failed".to_string()),
            Err(e) => Err(format!("Failed to sync account: {}", e)),
        }
    }

    pub async fn reload_all_accounts(&self) -> Result<usize, String> {
        self.load_accounts().await
    }

    async fn load_single_account(&self, path: &PathBuf) -> Result<Option<ProxyToken>, String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let account: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {}", e))?;

        if account.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            tracing::debug!(
                "Skipping disabled account file: {:?} (email={})",
                path,
                account.get("email").and_then(|v| v.as_str()).unwrap_or("<unknown>")
            );
            return Ok(None);
        }

        if account.get("proxy_disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            tracing::debug!(
                "Skipping proxy-disabled account file: {:?} (email={})",
                path,
                account.get("email").and_then(|v| v.as_str()).unwrap_or("<unknown>")
            );
            return Ok(None);
        }

        let account_id = account["id"].as_str().ok_or("Missing id field")?.to_string();
        let email = account["email"].as_str().ok_or("Missing email field")?.to_string();

        let token_obj = account["token"].as_object().ok_or("Missing token field")?;

        let access_token =
            token_obj["access_token"].as_str().ok_or("Missing access_token")?.to_string();
        let refresh_token =
            token_obj["refresh_token"].as_str().ok_or("Missing refresh_token")?.to_string();
        let expires_in = token_obj["expires_in"].as_i64().ok_or("Missing expires_in")?;
        let timestamp = token_obj["expiry_timestamp"].as_i64().ok_or("Missing expiry_timestamp")?;

        let project_id =
            token_obj.get("project_id").and_then(|v| v.as_str()).map(|s| s.to_string());

        let subscription_tier = account
            .get("quota")
            .and_then(|q| q.get("subscription_tier"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let remaining_quota = account.get("quota").and_then(calculate_max_quota_percentage);

        let mut protected_models: HashSet<String> = account
            .get("protected_models")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
            .unwrap_or_default();

        let mut available_models: HashSet<String> = HashSet::new();

        const MAX_MODELS_PER_ACCOUNT: usize = 100;
        const MAX_MODEL_NAME_LENGTH: usize = 256;

        if let Some(quota) = account.get("quota") {
            if let Some(models) = quota.get("models").and_then(|m| m.as_array()) {
                for model in models.iter().take(MAX_MODELS_PER_ACCOUNT) {
                    if let (Some(name), Some(percentage)) = (
                        model.get("name").and_then(|n| n.as_str()),
                        model.get("percentage").and_then(|p| p.as_i64()),
                    ) {
                        if name.len() <= MAX_MODEL_NAME_LENGTH {
                            available_models.insert(name.to_string());
                            if percentage == 0 && !protected_models.contains(name) {
                                protected_models.insert(name.to_string());
                                tracing::debug!(
                                    "Auto-protected model {} for account (quota=0%)",
                                    name
                                );
                            }
                        }
                    }
                }
            }
        }

        if !protected_models.is_empty() {
            tracing::info!(
                "Account has {} protected models: {:?}",
                protected_models.len(),
                protected_models
            );
        }

        let health_score = self.get_health_score(&account_id);

        if subscription_tier.as_ref().is_some_and(|t| t.contains("ultra-business")) {
            tracing::info!(
                "Loaded Business-Ultra account: {} (tier={})",
                email,
                subscription_tier.as_deref().unwrap_or("?")
            );
        }

        Ok(Some(ProxyToken {
            account_id,
            access_token,
            refresh_token,
            expires_in,
            timestamp,
            email,
            account_path: path.clone(),
            project_id,
            subscription_tier,
            remaining_quota,
            protected_models,
            health_score,
            available_models,
        }))
    }

    pub async fn has_available_account(&self, quota_group: &str, _target_model: &str) -> bool {
        let tokens_snapshot: Vec<ProxyToken> =
            self.tokens.iter().map(|e| e.value().clone()).collect();

        if tokens_snapshot.is_empty() {
            return false;
        }

        for token in &tokens_snapshot {
            if !self.is_rate_limited(&token.email) {
                return true;
            }
        }

        tracing::debug!("No available accounts for quota_group={}", quota_group);
        false
    }

    pub async fn get_token_by_email(
        &self,
        email: &str,
    ) -> Result<(String, String, String), String> {
        let token = self
            .tokens
            .iter()
            .find(|entry| entry.value().email == email)
            .map(|entry| entry.value().clone());

        let mut token = match token {
            Some(t) => t,
            None => return Err(format!("Account not found: {}", email)),
        };

        let now = chrono::Utc::now().timestamp();
        if now >= token.timestamp - 300 {
            match oauth::refresh_access_token(&token.refresh_token).await {
                Ok(token_response) => {
                    token.access_token = token_response.access_token.clone();
                    token.expires_in = token_response.expires_in;
                    token.timestamp = now + token_response.expires_in;

                    if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                        entry.access_token = token.access_token.clone();
                        entry.expires_in = token.expires_in;
                        entry.timestamp = token.timestamp;
                    }

                    let _ = self.save_refreshed_token(&token.account_id, &token_response).await;
                },
                Err(e) => {
                    return Err(format!("Token refresh failed for {}: {}", email, e));
                },
            }
        }

        let project_id = token.project_id.clone().unwrap_or_default();
        Ok((token.access_token, project_id, token.email))
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}
