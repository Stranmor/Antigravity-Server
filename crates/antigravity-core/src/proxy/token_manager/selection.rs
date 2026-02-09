use super::proxy_token::ProxyToken;
use super::TokenManager;
use crate::modules::config;
use crate::proxy::active_request_guard::ActiveRequestGuard;
use std::collections::HashSet;
use std::sync::Arc;

impl TokenManager {
    pub async fn get_token(
        &self,
        quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str,
    ) -> Result<(String, String, String, ActiveRequestGuard), String> {
        self.get_token_with_exclusions(quota_group, force_rotate, session_id, target_model, None)
            .await
    }

    pub async fn get_token_with_exclusions(
        &self,
        quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str,
        exclude_accounts: Option<&HashSet<String>>,
    ) -> Result<(String, String, String, ActiveRequestGuard), String> {
        let timeout_duration = std::time::Duration::from_secs(5);
        match tokio::time::timeout(
            timeout_duration,
            self.get_token_internal(
                quota_group,
                force_rotate,
                session_id,
                target_model,
                exclude_accounts,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                Err("Token acquisition timeout (5s) - system too busy or deadlock detected"
                    .to_string())
            },
        }
    }

    /// Force selection of a specific account by email.
    /// Bypasses smart routing and uses the specified account directly.
    /// Returns error if account not found or unavailable.
    pub async fn get_token_forced(
        &self,
        forced_email: &str,
        _target_model: &str,
    ) -> Result<(String, String, String, ActiveRequestGuard), String> {
        let forced_email_lower = forced_email.to_lowercase();
        let mut token = self
            .tokens
            .iter()
            .find(|entry| entry.value().email.to_lowercase() == forced_email_lower)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| format!("Forced account not found: {}", forced_email))?;

        if self.is_rate_limited(&token.account_id) {
            return Err(format!("Forced account {} is rate limited", forced_email));
        }

        let routing = self.routing_config.read().await.clone();
        let guard = ActiveRequestGuard::try_new(
            Arc::clone(&self.active_requests),
            token.email.clone(),
            routing.max_concurrent_per_account,
        )
        .ok_or_else(|| {
            format!(
                "Forced account {} at max concurrency ({})",
                forced_email, routing.max_concurrent_per_account
            )
        })?;

        let now = chrono::Utc::now().timestamp();
        if now >= token.timestamp - 300 {
            self.try_refresh_token(&mut token).await?;
            // Persist refreshed token back to shared cache
            self.tokens.insert(token.account_id.clone(), token.clone());
        }

        let project_id = self.ensure_project_id(&mut token).await?;
        // Persist project_id back to shared cache if it was fetched
        if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
            entry.project_id = Some(project_id.clone());
        }

        tracing::info!(
            "[Force-Account] Using forced account: {} (bypassing smart routing)",
            token.email
        );

        Ok((token.access_token, project_id, token.email, guard))
    }

    async fn get_token_internal(
        &self,
        _quota_group: &str,
        force_rotate: bool,
        session_id: Option<&str>,
        target_model: &str,
        exclude_accounts: Option<&HashSet<String>>,
    ) -> Result<(String, String, String, ActiveRequestGuard), String> {
        let mut tokens_snapshot: Vec<ProxyToken> =
            self.tokens.iter().map(|e| e.value().clone()).collect();
        let total = tokens_snapshot.len();

        if total == 0 {
            return Err("Token pool is empty".to_string());
        }

        tokens_snapshot.sort_by(super::selection_helpers::compare_tokens_by_priority);

        let routing = self.routing_config.read().await.clone();

        let quota_protection_enabled =
            config::load_config_cached().map(|cfg| cfg.quota_protection.enabled).unwrap_or(false);

        let normalized_target =
            crate::proxy::common::model_mapping::normalize_to_standard_id(target_model)
                .unwrap_or_else(|| target_model.to_string());

        if quota_protection_enabled {
            let original_count = tokens_snapshot.len();
            tokens_snapshot.retain(|t| !self.is_model_protected(&t.account_id, &normalized_target));
            let filtered_count = original_count - tokens_snapshot.len();
            if filtered_count > 0 {
                tracing::debug!(
                    "Quota protection: filtered out {} accounts with 0% quota for {}",
                    filtered_count,
                    normalized_target
                );
            }
        }

        let preferred_id = self.preferred_account_id.read().await.clone();
        if let Some(ref pref_id) = preferred_id {
            if let Some((token, guard)) = self
                .try_preferred_account(
                    &tokens_snapshot,
                    pref_id,
                    &normalized_target,
                    quota_protection_enabled,
                    &routing,
                )
                .await
            {
                return Ok((
                    token.access_token,
                    token.project_id.unwrap_or_default(),
                    token.email,
                    guard,
                ));
            }
        }

        let mut attempted: HashSet<String> = exclude_accounts.cloned().unwrap_or_default();
        let mut last_error: Option<String> = None;
        let aimd = self.adaptive_limits.read().await.clone();

        for attempt in 0..total {
            let rotate = force_rotate || attempt > 0;

            if let Some(sid) = session_id {
                if self.should_unbind_session(sid) {
                    self.unbind_session_on_failures(sid);
                }
            }

            let mut target_token: Option<ProxyToken> = None;
            let mut active_guard: Option<ActiveRequestGuard> = None;

            if !rotate {
                if let Some((token, guard)) = self
                    .try_ultra_tier_selection(
                        &tokens_snapshot,
                        &attempted,
                        &normalized_target,
                        quota_protection_enabled,
                        &aimd,
                        &routing,
                    )
                    .await
                {
                    target_token = Some(token);
                    active_guard = Some(guard);
                }
            }

            if target_token.is_none() {
                if let Some(sid) = session_id {
                    if !rotate && routing.enable_session_affinity {
                        if let Some((token, guard)) = self
                            .try_sticky_session(
                                sid,
                                &tokens_snapshot,
                                &attempted,
                                &normalized_target,
                                quota_protection_enabled,
                                &aimd,
                                &routing,
                            )
                            .await
                        {
                            target_token = Some(token);
                            active_guard = Some(guard);
                        }
                    }
                }
            }

            if target_token.is_none() {
                if let Some((token, guard)) = self
                    .try_scored_selection(
                        &tokens_snapshot,
                        &attempted,
                        &normalized_target,
                        quota_protection_enabled,
                        &aimd,
                        &routing,
                    )
                    .await
                {
                    target_token = Some(token);
                    active_guard = Some(guard);
                }
            }

            let mut token = match target_token {
                Some(t) => t,
                None => {
                    if let Some(t) = self
                        .try_recovery_selection(
                            &tokens_snapshot,
                            &attempted,
                            &normalized_target,
                            quota_protection_enabled,
                            &routing,
                        )
                        .await?
                    {
                        active_guard = Some(
                            ActiveRequestGuard::try_new(
                                Arc::clone(&self.active_requests),
                                t.email.clone(),
                                routing.max_concurrent_per_account,
                            )
                            .ok_or("Failed to acquire guard after recovery")?,
                        );
                        t
                    } else {
                        return Err(last_error.unwrap_or_else(|| "All accounts failed".to_string()));
                    }
                },
            };

            if let Some(sid) = session_id {
                self.bind_session_to_account(sid, &token.email, routing.enable_session_affinity);
            }

            let now = chrono::Utc::now().timestamp();
            if now >= token.timestamp - 300 {
                if let Err(e) = self.try_refresh_token(&mut token).await {
                    last_error = Some(e);
                    attempted.insert(token.email.clone());
                    continue;
                }
                // Persist refreshed token back to shared cache
                self.tokens.insert(token.account_id.clone(), token.clone());
            }

            let project_id = match self.ensure_project_id(&mut token).await {
                Ok(pid) => {
                    // Persist project_id back to shared cache if it was fetched
                    if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                        entry.project_id = Some(pid.clone());
                    }
                    pid
                },
                Err(e) => {
                    last_error = Some(e);
                    attempted.insert(token.email.clone());
                    continue;
                },
            };

            let guard = match active_guard {
                Some(g) => g,
                None => {
                    match ActiveRequestGuard::try_new(
                        Arc::clone(&self.active_requests),
                        token.email.clone(),
                        routing.max_concurrent_per_account,
                    ) {
                        Some(g) => g,
                        None => {
                            tracing::warn!(
                                "Account {} at capacity after selection. Retrying with next account.",
                                token.email
                            );
                            attempted.insert(token.email.clone());
                            continue;
                        },
                    }
                },
            };

            return Ok((token.access_token, project_id, token.email, guard));
        }

        Err(last_error.unwrap_or_else(|| "All accounts failed".to_string()))
    }
}
