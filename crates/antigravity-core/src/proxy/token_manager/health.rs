use super::TokenManager;

impl TokenManager {
    pub fn record_success(&self, account_id: &str) {
        self.health_scores
            .entry(account_id.to_string())
            .and_modify(|s| *s = (*s + 0.05).min(1.0))
            .or_insert(1.0);
        tracing::debug!("Health score increased for account {}", account_id);
    }

    pub fn record_failure(&self, account_id: &str) {
        self.health_scores
            .entry(account_id.to_string())
            .and_modify(|s| *s = (*s - 0.2).max(0.0))
            .or_insert(0.8);
        tracing::warn!("Health score decreased for account {}", account_id);
    }

    pub fn mark_account_success(&self, account_id: &str) {
        self.rate_limit_tracker.mark_success(account_id);
    }

    pub(crate) fn get_health_score(&self, account_id: &str) -> f32 {
        self.health_scores.get(account_id).map(|v| *v).unwrap_or(1.0)
    }
}
