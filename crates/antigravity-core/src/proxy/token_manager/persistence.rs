// Token persistence: write-back to PostgreSQL (primary) or filesystem (fallback).
// Timestamps are i64/u64 from system time, arithmetic is for age calculations.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "Token persistence: timestamp arithmetic, JSON field access"
)]

use super::file_utils::atomic_write_json;
use super::TokenManager;
use crate::modules::oauth;
use crate::modules::repository::AccountRepository;
use std::sync::Arc;

impl TokenManager {
    pub(crate) fn get_file_lock(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.file_locks
            .entry(account_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Clone the repository Arc without holding the RwLock during DB operations.
    async fn get_repo(&self) -> Option<Arc<dyn AccountRepository>> {
        let guard = self.repository.read().await;
        guard.as_ref().cloned()
    }

    pub(crate) async fn save_project_id(
        &self,
        account_id: &str,
        project_id: &str,
    ) -> Result<(), String> {
        if let Some(repo) = self.get_repo().await {
            return self.save_project_id_to_db(&repo, account_id, project_id).await;
        }
        self.save_project_id_to_file(account_id, project_id).await
    }

    async fn save_project_id_to_db(
        &self,
        repo: &Arc<dyn AccountRepository>,
        account_id: &str,
        project_id: &str,
    ) -> Result<(), String> {
        repo.update_project_id(account_id, project_id)
            .await
            .map_err(|e| format!("DB write: {}", e))?;
        tracing::debug!("Saved project_id to DB for account {}", account_id);
        Ok(())
    }

    async fn save_project_id_to_file(
        &self,
        account_id: &str,
        project_id: &str,
    ) -> Result<(), String> {
        let entry = self.tokens.get(account_id).ok_or("Account not found")?;
        let path = entry.account_path.clone();
        drop(entry);

        let lock = self.get_file_lock(account_id);
        let _guard = lock.lock().await;

        let content_str = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;
        let mut content: serde_json::Value = serde_json::from_str(&content_str)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        content["token"]["project_id"] = serde_json::Value::String(project_id.to_string());
        atomic_write_json(&path, &content).await?;
        tracing::debug!("Saved project_id to file for account {}", account_id);
        Ok(())
    }

    pub(crate) async fn save_refreshed_token(
        &self,
        account_id: &str,
        token_response: &oauth::TokenResponse,
    ) -> Result<(), String> {
        if let Some(repo) = self.get_repo().await {
            return self.save_refreshed_token_to_db(&repo, account_id, token_response).await;
        }
        self.save_refreshed_token_to_file(account_id, token_response).await
    }

    async fn save_refreshed_token_to_db(
        &self,
        repo: &Arc<dyn AccountRepository>,
        account_id: &str,
        token_response: &oauth::TokenResponse,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();
        let expiry = now + token_response.expires_in;
        repo.update_token_credentials(
            account_id,
            &token_response.access_token,
            token_response.expires_in,
            expiry,
        )
        .await
        .map_err(|e| format!("DB write: {}", e))?;
        tracing::debug!("Saved refreshed token to DB for account {}", account_id);
        Ok(())
    }

    async fn save_refreshed_token_to_file(
        &self,
        account_id: &str,
        token_response: &oauth::TokenResponse,
    ) -> Result<(), String> {
        let entry = self.tokens.get(account_id).ok_or("Account not found")?;
        let path = entry.account_path.clone();
        drop(entry);

        let lock = self.get_file_lock(account_id);
        let _guard = lock.lock().await;

        let content_str = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;
        let mut content: serde_json::Value = serde_json::from_str(&content_str)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let now = chrono::Utc::now().timestamp();
        content["token"]["access_token"] =
            serde_json::Value::String(token_response.access_token.clone());
        content["token"]["expires_in"] =
            serde_json::Value::Number(token_response.expires_in.into());
        content["token"]["expiry_timestamp"] =
            serde_json::Value::Number((now + token_response.expires_in).into());

        atomic_write_json(&path, &content).await?;
        tracing::debug!("Saved refreshed token to file for account {}", account_id);
        Ok(())
    }

    pub(crate) async fn disable_account(
        &self,
        account_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        if let Some(repo) = self.get_repo().await {
            return self.disable_account_in_db(&repo, account_id, reason).await;
        }
        self.disable_account_in_file(account_id, reason).await
    }

    async fn disable_account_in_db(
        &self,
        repo: &Arc<dyn AccountRepository>,
        account_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        use super::file_utils::truncate_reason;

        let now = chrono::Utc::now().timestamp();
        let truncated = truncate_reason(reason, 800);
        repo.set_account_disabled(account_id, &truncated, now)
            .await
            .map_err(|e| format!("DB write: {}", e))?;
        tracing::warn!("Account disabled in DB: {}", account_id);
        Ok(())
    }

    async fn disable_account_in_file(&self, account_id: &str, reason: &str) -> Result<(), String> {
        use super::file_utils::truncate_reason;

        let path = if let Some(entry) = self.tokens.get(account_id) {
            entry.account_path.clone()
        } else {
            self.data_dir.join("accounts").join(format!("{}.json", account_id))
        };

        let lock = self.get_file_lock(account_id);
        let _guard = lock.lock().await;

        let content_str = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;
        let mut content: serde_json::Value = serde_json::from_str(&content_str)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let now = chrono::Utc::now().timestamp();
        content["disabled"] = serde_json::Value::Bool(true);
        content["disabled_at"] = serde_json::Value::Number(now.into());
        content["disabled_reason"] = serde_json::Value::String(truncate_reason(reason, 800));

        atomic_write_json(&path, &content).await?;
        tracing::warn!("Account disabled in file: {} ({:?})", account_id, path);
        Ok(())
    }
}
