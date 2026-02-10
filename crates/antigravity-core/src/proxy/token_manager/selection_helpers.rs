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

        if token.needs_refresh() {
            self.try_refresh_token(&mut token).await.ok()?;
        }

        self.ensure_project_id(&mut token).await.ok()?;

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

        // Pre-calculate active requests for stable O(N log N) sorting with O(N) lookups
        let mut candidates_with_load: Vec<(&ProxyToken, u32)> =
            ultra_candidates.into_iter().map(|t| (t, self.get_active_requests(&t.email))).collect();

        candidates_with_load.sort_by(|(a, load_a), (b, load_b)| {
            compare_tokens_by_priority(a, b).then_with(|| load_a.cmp(load_b))
        });

        for (candidate, _) in candidates_with_load {
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

        // Pre-calculate active requests for stable O(N log N) sorting with O(N) lookups
        let mut candidates_with_load: Vec<(&ProxyToken, u32)> = scored_candidates
            .into_iter()
            .map(|t| (t, self.get_active_requests(&t.email)))
            .collect();

        candidates_with_load.sort_by(|(a, load_a), (b, load_b)| {
            compare_tokens_by_priority(a, b).then_with(|| load_a.cmp(load_b))
        });

        for (candidate, _) in candidates_with_load {
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
