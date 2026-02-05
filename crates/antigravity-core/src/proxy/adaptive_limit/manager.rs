use super::aimd::{AIMDController, ProbeStrategy};
use super::stats::AimdAccountStats;
use super::tracker::AdaptiveLimitTracker;
use dashmap::DashMap;

pub struct AdaptiveLimitManager {
    trackers: DashMap<String, AdaptiveLimitTracker>,
    safety_margin: f64,
    aimd: AIMDController,
}

impl AdaptiveLimitManager {
    pub fn new(safety_margin: f64, aimd: AIMDController) -> Self {
        Self { trackers: DashMap::new(), safety_margin, aimd }
    }

    pub fn get_or_create(
        &self,
        account_id: &str,
    ) -> dashmap::mapref::one::Ref<'_, String, AdaptiveLimitTracker> {
        if let Some(tracker) = self.trackers.get(account_id) {
            return tracker;
        }
        self.trackers
            .entry(account_id.to_string())
            .or_insert_with(|| AdaptiveLimitTracker::new(self.safety_margin, self.aimd.clone()))
            .downgrade()
    }

    pub fn get(
        &self,
        account_id: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, AdaptiveLimitTracker>> {
        self.trackers.get(account_id)
    }

    pub fn load_persisted(
        &self,
        account_id: &str,
        confirmed_limit: u64,
        ceiling: u64,
        age_seconds: u64,
    ) {
        let tracker = AdaptiveLimitTracker::from_persisted(
            confirmed_limit,
            ceiling,
            age_seconds,
            self.safety_margin,
            self.aimd.clone(),
        );
        self.trackers.insert(account_id.to_string(), tracker);
    }

    pub fn usage_ratio(&self, account_id: &str) -> f64 {
        self.get_or_create(account_id).usage_ratio()
    }

    pub fn probe_strategy(&self, account_id: &str) -> ProbeStrategy {
        self.get_or_create(account_id).probe_strategy()
    }

    pub fn record_success(&self, account_id: &str) {
        self.get_or_create(account_id).record_success();
    }

    pub fn record_429(&self, account_id: &str) {
        self.get_or_create(account_id).record_429();
    }

    pub fn record_error(&self, account_id: &str, status_code: u16) {
        self.get_or_create(account_id).record_error(status_code);
    }

    pub fn force_expand(&self, account_id: &str) {
        self.get_or_create(account_id).force_expand();
    }

    pub fn should_allow(&self, account_id: &str) -> bool {
        self.get_or_create(account_id).should_allow()
    }

    pub fn all_for_persistence(&self) -> Vec<(String, u64, u64, u64)> {
        self.trackers
            .iter()
            .map(|entry| {
                let (confirmed, ceiling, age) = entry.value().to_persisted();
                (entry.key().clone(), confirmed, ceiling, age)
            })
            .collect()
    }

    pub fn all_stats(&self) -> Vec<AimdAccountStats> {
        self.trackers
            .iter()
            .map(|entry| AimdAccountStats {
                account_id: entry.key().clone(),
                confirmed_limit: entry.value().confirmed_limit(),
                ceiling: entry.value().ceiling(),
                requests_this_minute: entry.value().requests_this_minute(),
                working_threshold: entry.value().working_threshold(),
                usage_ratio: entry.value().usage_ratio(),
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.trackers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.trackers.is_empty()
    }
}

impl Default for AdaptiveLimitManager {
    fn default() -> Self {
        Self::new(0.85, AIMDController::default())
    }
}
