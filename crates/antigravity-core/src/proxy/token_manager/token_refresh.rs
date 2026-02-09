use super::proxy_token::ProxyToken;
use super::TokenManager;
use crate::modules::oauth;
use std::sync::Arc;

impl TokenManager {
    pub(super) async fn try_refresh_token(&self, token: &mut ProxyToken) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();

        if now < token.timestamp - 300 {
            return Ok(());
        }

        // Acquire per-account refresh lock to prevent thundering herd
        let lock = self
            .refresh_locks
            .entry(token.account_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        let _guard = lock.lock().await;

        // After acquiring lock: check if another thread already refreshed
        if let Some(entry) = self.tokens.get(&token.account_id) {
            if entry.timestamp > now + 300 {
                token.access_token = entry.access_token.clone();
                token.expires_in = entry.expires_in;
                token.timestamp = entry.timestamp;
                return Ok(());
            }
        }

        tracing::debug!("Account {} token expiring, refreshing...", token.email);

        match oauth::refresh_access_token(&token.refresh_token).await {
            Ok(token_response) => {
                let new_timestamp = now + token_response.expires_in;

                token.access_token = token_response.access_token.clone();
                token.expires_in = token_response.expires_in;
                token.timestamp = new_timestamp;

                if let Some(ref new_refresh) = token_response.refresh_token {
                    token.refresh_token = new_refresh.clone();
                }

                if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                    entry.access_token = token.access_token.clone();
                    entry.expires_in = token.expires_in;
                    entry.timestamp = token.timestamp;
                    if let Some(ref new_refresh) = token_response.refresh_token {
                        entry.refresh_token = new_refresh.clone();
                    }
                }

                if let Err(e) = self.save_refreshed_token(&token.account_id, &token_response).await
                {
                    tracing::warn!("Failed to save refreshed token ({}): {}", token.email, e);
                }
                Ok(())
            },
            Err(e) => {
                tracing::error!("Token refresh failed ({}): {}", token.email, e);
                if e.contains("\"invalid_grant\"") || e.contains("invalid_grant") {
                    tracing::error!("Disabling account due to invalid_grant ({})", token.email);
                    self.tokens.remove(&token.account_id);
                    if let Err(disable_err) = self
                        .disable_account(&token.account_id, &format!("invalid_grant: {}", e))
                        .await
                    {
                        tracing::warn!(
                            "Failed to persist disable for {}: {}",
                            token.email,
                            disable_err
                        );
                    }
                }
                Err(format!("Token refresh failed: {}", e))
            },
        }
    }

    pub(super) async fn ensure_project_id(&self, token: &mut ProxyToken) -> Result<String, String> {
        if let Some(pid) = &token.project_id {
            return Ok(pid.clone());
        }

        tracing::debug!("Account {} missing project_id, fetching...", token.email);
        match crate::proxy::project_resolver::fetch_project_id(&token.access_token).await {
            Ok(pid) => {
                if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                    entry.project_id = Some(pid.clone());
                }
                if let Err(e) = self.save_project_id(&token.account_id, &pid).await {
                    tracing::warn!("Failed to save project_id for {}: {}", token.email, e);
                }
                token.project_id = Some(pid.clone());
                Ok(pid)
            },
            Err(e) => {
                tracing::error!("Failed to fetch project_id for {}: {}", token.email, e);
                Err(format!("Failed to fetch project_id for {}: {}", token.email, e))
            },
        }
    }
}
