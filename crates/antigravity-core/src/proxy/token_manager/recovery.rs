use super::proxy_token::ProxyToken;
use super::TokenManager;
use crate::proxy::active_request_guard::ActiveRequestGuard;
use crate::proxy::routing_config::SmartRoutingConfig;
use std::collections::HashSet;
use std::sync::Arc;

impl TokenManager {
    pub(super) async fn try_recovery_selection(
        &self,
        tokens_snapshot: &[ProxyToken],
        attempted: &HashSet<String>,
        normalized_target: &str,
        quota_protection_enabled: bool,
        routing: &SmartRoutingConfig,
    ) -> Result<Option<ProxyToken>, String> {
        let min_wait = tokens_snapshot
            .iter()
            .filter_map(|t| self.rate_limit_tracker.get_reset_seconds(&t.email))
            .min();

        if let Some(wait_sec) = min_wait {
            if wait_sec <= 2 {
                tracing::warn!(
                    "All accounts rate-limited but shortest wait is {}s. Applying 500ms buffer...",
                    wait_sec
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                for t in tokens_snapshot {
                    if !self.is_candidate_eligible(
                        t,
                        normalized_target,
                        attempted,
                        false,
                        &None,
                        false,
                        false,
                    ) {
                        continue;
                    }
                    if ActiveRequestGuard::try_new(
                        Arc::clone(&self.active_requests),
                        t.email.clone(),
                        routing.max_concurrent_per_account,
                    )
                    .is_some()
                    {
                        tracing::info!(
                            "Buffer delay successful! Found available account: {}",
                            t.email
                        );
                        return Ok(Some(t.clone()));
                    }
                }

                tracing::warn!(
                    "Buffer delay failed. Executing optimistic reset for all {} accounts...",
                    tokens_snapshot.len()
                );
                self.rate_limit_tracker.clear_all();

                for t in tokens_snapshot {
                    if attempted.contains(&t.email) {
                        continue;
                    }
                    if ActiveRequestGuard::try_new(
                        Arc::clone(&self.active_requests),
                        t.email.clone(),
                        routing.max_concurrent_per_account,
                    )
                    .is_some()
                    {
                        tracing::info!("Optimistic reset successful! Using account: {}", t.email);
                        return Ok(Some(t.clone()));
                    }
                }

                return Err(
                    "All accounts failed after optimistic reset. Please check account health."
                        .to_string(),
                );
            } else {
                return Err(format!(
                    "All accounts are currently limited. Please wait {}s.",
                    wait_sec
                ));
            }
        }

        tracing::warn!(
            "All {} accounts at max concurrency. Waiting 500ms for availability...",
            tokens_snapshot.len()
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        for t in tokens_snapshot.iter() {
            if attempted.contains(&t.email) {
                continue;
            }
            if self.is_rate_limited_for_model(&t.email, normalized_target) {
                continue;
            }
            if quota_protection_enabled && self.is_model_protected(&t.account_id, normalized_target)
            {
                continue;
            }
            if ActiveRequestGuard::try_new(
                Arc::clone(&self.active_requests),
                t.email.clone(),
                routing.max_concurrent_per_account,
            )
            .is_some()
            {
                tracing::info!("Found available account after wait: {}", t.email);
                return Ok(Some(t.clone()));
            }
        }

        Err("All accounts at maximum capacity. Please retry later.".to_string())
    }
}
