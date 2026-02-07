use super::TokenManager;

impl TokenManager {
    pub fn record_success(&self, account_id: &str) {
        if let Some(monitor) = self.health_monitor.try_read().ok().and_then(|g| g.clone()) {
            monitor.record_success(account_id);
        }
        tracing::debug!("Health: success recorded for {}", account_id);
    }

    pub fn mark_account_success(&self, account_id: &str) {
        self.rate_limit_tracker.mark_success(account_id);
        self.record_success(account_id);
    }

    pub(crate) fn get_health_score(&self, account_id: &str) -> f32 {
        self.health_monitor
            .try_read()
            .ok()
            .and_then(|g| g.as_ref().map(|m| m.get_score(account_id)))
            .unwrap_or(1.0)
    }
}
