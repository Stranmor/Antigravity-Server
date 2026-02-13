use super::proxy_token::ProxyToken;
use super::TokenManager;
use crate::modules::oauth;
use std::sync::atomic::Ordering;
use std::sync::Arc;

impl TokenManager {
    pub(super) async fn try_refresh_token(&self, token: &mut ProxyToken) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();

        if !token.needs_refresh() {
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
                token.refresh_token = entry.refresh_token.clone();
                return Ok(());
            }
        }

        tracing::debug!("Account {} token expiring, refreshing...", token.email);

        if token.proxy_url.is_none() && self.enforce_proxy.load(Ordering::Acquire) {
            return Err(format!(
                "enforce_proxy: account {} has no proxy_url — blocking token refresh to prevent IP leak",
                token.email
            ));
        }

        // Use per-account proxy for token refresh to prevent IP leak
        match oauth::refresh_access_token_with_proxy(
            &token.refresh_token,
            token.proxy_url.as_deref(),
        )
        .await
        {
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
                    if token_response.refresh_token.is_some() {
                        // Provider rotated refresh_token — failing to persist means
                        // account will be locked out on restart (old token invalidated).
                        tracing::error!(
                            "CRITICAL: Failed to persist rotated refresh_token ({}): {}",
                            token.email,
                            e
                        );
                        return Err(format!("Failed to persist rotated refresh token: {}", e));
                    }
                    // Access token only — can be refreshed again, just warn
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
            if !pid.is_empty() {
                return Ok(pid.clone());
            }
        }

        tracing::debug!("Account {} missing project_id, fetching...", token.email);

        if token.proxy_url.is_none() && self.enforce_proxy.load(Ordering::Acquire) {
            return Err(format!(
                "enforce_proxy: account {} has no proxy_url — blocking project_id fetch to prevent IP leak",
                token.email
            ));
        }

        let pid = match crate::proxy::project_resolver::fetch_project_id_with_proxy(
            &token.access_token,
            token.proxy_url.as_deref(),
        )
        .await
        {
            Ok(pid) => pid,
            Err(e) => {
                // Fallback to default project when fetch fails (network error, etc.)
                let default_pid = "bamboo-precept-lgxtn".to_string();
                tracing::warn!(
                    "Failed to fetch project_id for {}: {}. Using default: {}",
                    token.email,
                    e,
                    default_pid
                );
                default_pid
            },
        };

        if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
            entry.project_id = Some(pid.clone());
        }
        if let Err(e) = self.save_project_id(&token.account_id, &pid).await {
            tracing::warn!("Failed to save project_id for {}: {}", token.email, e);
        }
        token.project_id = Some(pid.clone());
        Ok(pid)
    }
}
