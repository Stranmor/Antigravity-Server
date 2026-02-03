use super::proxy_token::ProxyToken;
use super::session::STICKY_UNBIND_RATE_LIMIT_SECONDS;
use super::TokenManager;
use crate::proxy::active_request_guard::ActiveRequestGuard;
use crate::proxy::routing_config::SmartRoutingConfig;
use crate::proxy::AdaptiveLimitManager;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Arc;

pub fn compare_tokens_by_priority(a: &ProxyToken, b: &ProxyToken) -> Ordering {
    let tier_cmp = a.tier_priority().cmp(&b.tier_priority());
    if tier_cmp != Ordering::Equal {
        return tier_cmp;
    }
    let quota_a = a.remaining_quota.unwrap_or(0);
    let quota_b = b.remaining_quota.unwrap_or(0);
    let quota_cmp = quota_b.cmp(&quota_a);
    if quota_cmp != Ordering::Equal {
        return quota_cmp;
    }
    b.health_score
        .partial_cmp(&a.health_score)
        .unwrap_or(Ordering::Equal)
}

impl TokenManager {
    pub(super) async fn try_preferred_account(
        &self,
        tokens_snapshot: &[ProxyToken],
        pref_id: &str,
        normalized_target: &str,
        quota_protection_enabled: bool,
        routing: &SmartRoutingConfig,
    ) -> Option<(String, String, String, ActiveRequestGuard)> {
        let preferred_token = tokens_snapshot.iter().find(|t| t.account_id == pref_id)?;

        let is_rate_limited =
            self.is_rate_limited_for_model(&preferred_token.email, normalized_target);
        let is_quota_protected = quota_protection_enabled
            && self.is_model_protected(&preferred_token.account_id, normalized_target);

        if is_rate_limited {
            tracing::warn!(
                "Preferred account {} is rate-limited, falling back to round-robin",
                preferred_token.email
            );
            return None;
        }

        if is_quota_protected {
            tracing::warn!(
                "Preferred account {} is quota-protected for model {}, falling back to round-robin",
                preferred_token.email,
                normalized_target
            );
            return None;
        }

        tracing::info!(
            "Using preferred account: {} (fixed mode)",
            preferred_token.email
        );

        let mut token = preferred_token.clone();
        let now = chrono::Utc::now().timestamp();

        if now >= token.timestamp - 300 {
            tracing::debug!(
                "Preferred account {} token expiring, refreshing...",
                token.email
            );
            match crate::modules::oauth::refresh_access_token(&token.refresh_token).await {
                Ok(token_response) => {
                    token.access_token = token_response.access_token.clone();
                    token.expires_in = token_response.expires_in;
                    token.timestamp = now + token_response.expires_in;

                    if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                        entry.access_token = token.access_token.clone();
                        entry.expires_in = token.expires_in;
                        entry.timestamp = token.timestamp;
                    }
                    let _ = self
                        .save_refreshed_token(&token.account_id, &token_response)
                        .await;
                }
                Err(e) => {
                    tracing::warn!("Preferred account token refresh failed: {}", e);
                }
            }
        }

        let project_id = if let Some(pid) = &token.project_id {
            pid.clone()
        } else {
            match crate::proxy::project_resolver::fetch_project_id(&token.access_token).await {
                Ok(pid) => {
                    if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                        entry.project_id = Some(pid.clone());
                    }
                    let _ = self.save_project_id(&token.account_id, &pid).await;
                    pid
                }
                Err(_) => "bamboo-precept-lgxtn".to_string(),
            }
        };

        let guard = ActiveRequestGuard::try_new(
            Arc::clone(&self.active_requests),
            token.email.clone(),
            routing.max_concurrent_per_account,
        )?;

        Some((token.access_token, project_id, token.email, guard))
    }

    pub(super) async fn try_ultra_tier_selection(
        &self,
        tokens_snapshot: &[ProxyToken],
        attempted: &HashSet<String>,
        normalized_target: &str,
        quota_protection_enabled: bool,
        aimd: &Option<Arc<AdaptiveLimitManager>>,
        routing: &SmartRoutingConfig,
    ) -> Option<(ProxyToken, ActiveRequestGuard)> {
        let mut ultra_candidates: Vec<(&ProxyToken, u8, u32)> = Vec::new();

        for candidate in tokens_snapshot {
            if !candidate.is_ultra_tier() {
                continue;
            }
            if attempted.contains(&candidate.email) {
                continue;
            }
            if self.is_rate_limited_for_model(&candidate.email, normalized_target) {
                continue;
            }
            if quota_protection_enabled
                && self.is_model_protected(&candidate.account_id, normalized_target)
            {
                continue;
            }
            if let Some(aimd) = aimd {
                if aimd.usage_ratio(&candidate.email) > 1.2 {
                    continue;
                }
            }

            let active = self.get_active_requests(&candidate.email);
            let tier = candidate.tier_priority();
            ultra_candidates.push((candidate, tier, active));
        }

        if ultra_candidates.is_empty() {
            return None;
        }

        ultra_candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

        for (candidate, _tier, _active) in ultra_candidates {
            if let Some(guard) = ActiveRequestGuard::try_new(
                Arc::clone(&self.active_requests),
                candidate.email.clone(),
                routing.max_concurrent_per_account,
            ) {
                tracing::debug!(
                    "Ultra Priority: Selected {} ({:?}) over sticky session",
                    candidate.email,
                    candidate.account_tier()
                );
                return Some((candidate.clone(), guard));
            }
        }

        None
    }

    pub(super) async fn try_sticky_session(
        &self,
        session_id: &str,
        tokens_snapshot: &[ProxyToken],
        attempted: &HashSet<String>,
        normalized_target: &str,
        quota_protection_enabled: bool,
        routing: &SmartRoutingConfig,
    ) -> Option<(ProxyToken, ActiveRequestGuard)> {
        let bound_id = self.get_session_account(session_id)?;

        let reset_sec = self
            .rate_limit_tracker
            .get_remaining_wait_for_model(&bound_id, normalized_target);

        if reset_sec > 0 {
            if reset_sec > STICKY_UNBIND_RATE_LIMIT_SECONDS {
                self.session_accounts.remove(session_id);
                tracing::warn!(
                    "Sticky Session: {} rate-limited ({}s), unbinding session {}",
                    bound_id,
                    reset_sec,
                    session_id
                );
            } else {
                tracing::debug!(
                    "Sticky Session: {} rate-limited ({}s), migrating this request only",
                    bound_id,
                    reset_sec
                );
            }
            return None;
        }

        if attempted.contains(&bound_id) {
            return None;
        }

        let is_quota_protected = quota_protection_enabled
            && tokens_snapshot
                .iter()
                .find(|t| t.email == bound_id)
                .is_some_and(|t| self.is_model_protected(&t.account_id, normalized_target));

        if is_quota_protected {
            tracing::debug!(
                "Sticky Session: {} is quota-protected for {}, unbinding",
                bound_id,
                normalized_target
            );
            self.session_accounts.remove(session_id);
            return None;
        }

        let found = tokens_snapshot.iter().find(|t| t.email == bound_id)?;

        let guard = ActiveRequestGuard::try_new(
            Arc::clone(&self.active_requests),
            found.email.clone(),
            routing.max_concurrent_per_account,
        )?;

        tracing::debug!(
            "Sticky Session: Reusing {} for session {}",
            found.email,
            session_id
        );
        Some((found.clone(), guard))
    }

    pub(super) async fn try_scored_selection(
        &self,
        tokens_snapshot: &[ProxyToken],
        attempted: &HashSet<String>,
        normalized_target: &str,
        quota_protection_enabled: bool,
        aimd: &Option<Arc<AdaptiveLimitManager>>,
        routing: &SmartRoutingConfig,
    ) -> Option<(ProxyToken, ActiveRequestGuard)> {
        let mut scored_candidates: Vec<(&ProxyToken, u8, u32)> = Vec::new();

        for candidate in tokens_snapshot {
            if attempted.contains(&candidate.email) {
                continue;
            }
            if self.is_rate_limited_for_model(&candidate.email, normalized_target) {
                continue;
            }
            if quota_protection_enabled
                && self.is_model_protected(&candidate.account_id, normalized_target)
            {
                continue;
            }
            if let Some(aimd) = aimd {
                if aimd.usage_ratio(&candidate.email) > 1.2 {
                    continue;
                }
            }

            let active = self.get_active_requests(&candidate.email);
            let tier = candidate.tier_priority();
            scored_candidates.push((candidate, tier, active));
        }

        scored_candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

        for (candidate, _tier, _active) in scored_candidates {
            if let Some(guard) = ActiveRequestGuard::try_new(
                Arc::clone(&self.active_requests),
                candidate.email.clone(),
                routing.max_concurrent_per_account,
            ) {
                return Some((candidate.clone(), guard));
            }
        }

        None
    }
}
