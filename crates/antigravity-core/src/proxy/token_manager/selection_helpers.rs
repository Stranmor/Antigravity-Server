use super::proxy_token::ProxyToken;
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
    b.health_score.partial_cmp(&a.health_score).unwrap_or(Ordering::Equal)
}

impl TokenManager {
    pub(super) async fn try_preferred_account(
        &self,
        tokens_snapshot: &[ProxyToken],
        pref_id: &str,
        normalized_target: &str,
        quota_protection_enabled: bool,
        routing: &SmartRoutingConfig,
    ) -> Option<(ProxyToken, ActiveRequestGuard)> {
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

        tracing::info!("Using preferred account: {} (fixed mode)", preferred_token.email);

        let mut token = preferred_token.clone();
        let now = chrono::Utc::now().timestamp();

        if now >= token.timestamp - 300 {
            tracing::debug!("Preferred account {} token expiring, refreshing...", token.email);
            match crate::modules::oauth::refresh_access_token(&token.refresh_token).await {
                Ok(token_response) => {
                    token.access_token = token_response.access_token.clone();
                    token.expires_in = token_response.expires_in;
                    token.timestamp = now + token_response.expires_in;

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
                    if let Err(e) =
                        self.save_refreshed_token(&token.account_id, &token_response).await
                    {
                        tracing::warn!("Failed to save refreshed token for {}: {}", token.email, e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Preferred account token refresh failed: {}", e);
                    return None;
                },
            }
        }

        if token.project_id.is_none() {
            match crate::proxy::project_resolver::fetch_project_id(&token.access_token).await {
                Ok(pid) => {
                    if let Some(mut entry) = self.tokens.get_mut(&token.account_id) {
                        entry.project_id = Some(pid.clone());
                    }
                    if let Err(e) = self.save_project_id(&token.account_id, &pid).await {
                        tracing::warn!("Failed to save project_id for {}: {}", token.email, e);
                    }
                    token.project_id = Some(pid);
                },
                Err(e) => {
                    tracing::warn!("Failed to fetch project_id for {}: {}", token.account_id, e);
                    return None;
                },
            }
        };

        let guard = ActiveRequestGuard::try_new(
            Arc::clone(&self.active_requests),
            token.email.clone(),
            routing.max_concurrent_per_account,
        )?;

        Some((token, guard))
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
        let mut ultra_candidates: Vec<&ProxyToken> = Vec::new();

        for candidate in tokens_snapshot {
            if !candidate.is_ultra_tier() {
                continue;
            }
            if !self.is_candidate_eligible(
                candidate,
                normalized_target,
                attempted,
                quota_protection_enabled,
                aimd,
                true,
                true,
            ) {
                continue;
            }

            ultra_candidates.push(candidate);
        }

        if ultra_candidates.is_empty() {
            return None;
        }

        ultra_candidates.sort_by(|a, b| {
            compare_tokens_by_priority(a, b).then_with(|| {
                let active_a = self.get_active_requests(&a.email);
                let active_b = self.get_active_requests(&b.email);
                active_a.cmp(&active_b)
            })
        });

        for candidate in ultra_candidates {
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
        aimd: &Option<Arc<AdaptiveLimitManager>>,
        routing: &SmartRoutingConfig,
    ) -> Option<(ProxyToken, ActiveRequestGuard)> {
        let bound_id = self.get_session_account(session_id)?;

        let found = tokens_snapshot.iter().find(|t| t.email == bound_id)?;

        // Check eligibility for sticky session
        if !self.is_candidate_eligible(
            found,
            normalized_target,
            attempted,
            quota_protection_enabled,
            aimd,
            true,
            true,
        ) {
            tracing::warn!(
                "Sticky Session: {} is no longer eligible, unbinding session {}",
                bound_id,
                session_id
            );
            self.session_accounts.remove(session_id);
            return None;
        }

        let guard = ActiveRequestGuard::try_new(
            Arc::clone(&self.active_requests),
            found.email.clone(),
            routing.max_concurrent_per_account,
        )?;

        tracing::debug!("Sticky Session: Reusing {} for session {}", found.email, session_id);
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
        let mut scored_candidates: Vec<&ProxyToken> = Vec::new();

        for candidate in tokens_snapshot {
            if !self.is_candidate_eligible(
                candidate,
                normalized_target,
                attempted,
                quota_protection_enabled,
                aimd,
                true,
                true,
            ) {
                continue;
            }

            scored_candidates.push(candidate);
        }

        scored_candidates.sort_by(|a, b| {
            compare_tokens_by_priority(a, b).then_with(|| {
                let active_a = self.get_active_requests(&a.email);
                let active_b = self.get_active_requests(&b.email);
                active_a.cmp(&active_b)
            })
        });

        for candidate in scored_candidates {
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
