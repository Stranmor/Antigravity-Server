use super::proxy_token::ProxyToken;
use super::TokenManager;
use crate::modules::oauth;

impl TokenManager {
    pub(super) async fn try_refresh_token(&self, token: &mut ProxyToken) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();

        if now < token.timestamp - 300 {
            return Ok(());
        }

        tracing::debug!("Account {} token expiring, refreshing...", token.email);

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

                if let Err(e) = self.save_refreshed_token(&token.account_id, &token_response).await
                {
                    tracing::debug!("Failed to save refreshed token ({}): {}", token.email, e);
                }
                Ok(())
            },
            Err(e) => {
                tracing::error!("Token refresh failed ({}): {}", token.email, e);
                if e.contains("\"invalid_grant\"") || e.contains("invalid_grant") {
                    tracing::error!(
                        "Disabling account due to invalid_grant ({}): refresh_token likely revoked/expired",
                        token.email
                    );
                    match self
                        .disable_account(&token.account_id, &format!("invalid_grant: {}", e))
                        .await
                    {
                        Ok(()) => {
                            self.tokens.remove(&token.account_id);
                        },
                        Err(disable_err) => {
                            tracing::warn!(
                                "Failed to disable account {}: {} — keeping in memory to prevent reload loop",
                                token.email,
                                disable_err
                            );
                        },
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
