//! Adaptive Rate Limit System
//!
//! Implements predictive rate limiting using AIMD (Additive Increase, Multiplicative Decrease)
//! algorithm inspired by TCP congestion control. The goal is to predict and avoid 429 errors
//! BEFORE they happen, eliminating latency from rate limiting entirely.
//!
//! # Architecture
//!
//! 1. **AdaptiveLimitTracker** - Per-account limit tracking with usage counters
//! 2. **AIMDController** - Adjusts limits based on success/failure signals
//! 3. **ProbeStrategy** - Determines when to probe for limit changes
//!
//! # Algorithm
//!
//! - Track requests per minute per account
//! - Maintain a "working threshold" at 85% of confirmed limit
//! - When usage approaches threshold, probe to discover if limit increased
//! - On 429: immediately contract limit (×0.7)
//! - On success above threshold: gradually expand limit (+5%)

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// AIMD Controller - Additive Increase, Multiplicative Decrease
/// Inspired by TCP Vegas congestion control
#[derive(Debug, Clone)]
pub struct AIMDController {
    /// Additive increase factor (e.g., 0.05 = +5%)
    pub additive_increase: f64,
    /// Multiplicative decrease factor (e.g., 0.7 = -30%)
    pub multiplicative_decrease: f64,
    /// Minimum limit floor
    pub min_limit: u64,
    /// Maximum limit ceiling
    pub max_limit: u64,
}

impl Default for AIMDController {
    fn default() -> Self {
        Self {
            additive_increase: 0.05,      // +5% on success
            multiplicative_decrease: 0.7, // ×0.7 on 429
            min_limit: 10,                // Never go below 10 RPM
            max_limit: 1000,              // Cap at 1000 RPM
        }
    }
}

impl AIMDController {
    /// Reward: Success above threshold → limit is higher than expected
    /// Additive increase: +5%
    pub fn reward(&self, current: u64) -> u64 {
        let new = (current as f64 * (1.0 + self.additive_increase)).ceil() as u64;
        new.min(self.max_limit)
    }

    /// Penalize: 429 received → limit confirmed, reduce aggressively
    /// Multiplicative decrease: ×0.7
    pub fn penalize(&self, current: u64) -> u64 {
        let new = (current as f64 * self.multiplicative_decrease).floor() as u64;
        new.max(self.min_limit)
    }
}

/// Probe strategy based on current usage ratio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStrategy {
    /// Safe zone - no probing needed
    None,
    /// Background cheap probe (1 token) to test limits
    CheapProbe,
    /// Delayed hedge - launch secondary after P95 latency
    DelayedHedge,
    /// Critical zone - immediate parallel requests
    ImmediateHedge,
}

impl ProbeStrategy {
    /// Determine probe strategy based on usage ratio
    pub fn from_usage_ratio(ratio: f64) -> Self {
        match ratio {
            r if r < 0.70 => ProbeStrategy::None,
            r if r < 0.85 => ProbeStrategy::CheapProbe,
            r if r < 0.95 => ProbeStrategy::DelayedHedge,
            _ => ProbeStrategy::ImmediateHedge,
        }
    }

    /// Whether this strategy requires a secondary request
    pub fn needs_secondary(&self) -> bool {
        matches!(
            self,
            ProbeStrategy::DelayedHedge | ProbeStrategy::ImmediateHedge
        )
    }

    /// Whether this strategy is fire-and-forget (doesn't block on result)
    pub fn is_fire_and_forget(&self) -> bool {
        matches!(self, ProbeStrategy::CheapProbe)
    }
}

/// Per-account adaptive limit tracker
pub struct AdaptiveLimitTracker {
    /// Last confirmed limit (from 429)
    confirmed_limit: AtomicU64,
    /// Working threshold (safety_margin × confirmed_limit)
    working_threshold: AtomicU64,
    /// Historical maximum observed
    ceiling: AtomicU64,
    /// Requests in current minute
    requests_this_minute: AtomicU64,
    /// When current minute started
    minute_started_at: RwLock<Instant>,
    /// When last calibration occurred
    last_calibration: RwLock<Instant>,
    /// Consecutive successes above threshold (for AIMD reward)
    consecutive_above_threshold: AtomicU64,
    /// Safety margin (e.g., 0.85 = 15% buffer)
    safety_margin: f64,
    /// AIMD controller
    aimd: AIMDController,
}

impl AdaptiveLimitTracker {
    /// Create new tracker with conservative defaults
    pub fn new(safety_margin: f64, aimd: AIMDController) -> Self {
        let default_limit = 15; // Conservative default for unknown accounts
        Self {
            confirmed_limit: AtomicU64::new(default_limit),
            working_threshold: AtomicU64::new((default_limit as f64 * safety_margin) as u64),
            ceiling: AtomicU64::new(default_limit),
            requests_this_minute: AtomicU64::new(0),
            minute_started_at: RwLock::new(Instant::now()),
            last_calibration: RwLock::new(
                Instant::now()
                    .checked_sub(Duration::from_secs(3600))
                    .unwrap_or_else(Instant::now),
            ),
            consecutive_above_threshold: AtomicU64::new(0),
            safety_margin,
            aimd,
        }
    }

    /// Create tracker from persisted data with age-based decay
    pub fn from_persisted(
        confirmed_limit: u64,
        ceiling: u64,
        age_seconds: u64,
        safety_margin: f64,
        aimd: AIMDController,
    ) -> Self {
        let age_hours = age_seconds / 3600;

        // Decay confidence based on age
        let confidence = match age_hours {
            0..=1 => 1.0,  // Fresh
            2..=6 => 0.9,  // Few hours
            7..=24 => 0.7, // Day old
            _ => 0.5,      // Stale
        };

        let effective_limit = (confirmed_limit as f64 * confidence) as u64;
        let effective_limit = effective_limit.max(aimd.min_limit);

        let tracker = Self::new(safety_margin, aimd);
        tracker
            .confirmed_limit
            .store(effective_limit, Ordering::Relaxed);
        tracker.working_threshold.store(
            (effective_limit as f64 * safety_margin) as u64,
            Ordering::Relaxed,
        );
        tracker.ceiling.store(ceiling, Ordering::Relaxed);

        tracing::debug!(
            "Loaded persisted limit: {} (original: {}, age: {}h, confidence: {:.0}%)",
            effective_limit,
            confirmed_limit,
            age_hours,
            confidence * 100.0
        );

        tracker
    }

    /// Reset minute counter if minute has elapsed
    fn maybe_reset_minute(&self) {
        let now = Instant::now();
        let should_reset = {
            let started = self.minute_started_at.read().unwrap();
            now.duration_since(*started) >= Duration::from_secs(60)
        };

        if should_reset {
            self.requests_this_minute.store(0, Ordering::Relaxed);
            *self.minute_started_at.write().unwrap() = now;
        }
    }

    /// Get current usage ratio (0.0 - 1.0+)
    pub fn usage_ratio(&self) -> f64 {
        self.maybe_reset_minute();
        let usage = self.requests_this_minute.load(Ordering::Relaxed);
        let threshold = self.working_threshold.load(Ordering::Relaxed);
        if threshold == 0 {
            return 0.0;
        }
        usage as f64 / threshold as f64
    }

    /// Get current requests this minute
    pub fn requests_this_minute(&self) -> u64 {
        self.maybe_reset_minute();
        self.requests_this_minute.load(Ordering::Relaxed)
    }

    /// Get working threshold
    pub fn working_threshold(&self) -> u64 {
        self.working_threshold.load(Ordering::Relaxed)
    }

    /// Get confirmed limit
    pub fn confirmed_limit(&self) -> u64 {
        self.confirmed_limit.load(Ordering::Relaxed)
    }

    /// Get ceiling (historical max)
    pub fn ceiling(&self) -> u64 {
        self.ceiling.load(Ordering::Relaxed)
    }

    /// Check if we should allow a request (below threshold)
    pub fn should_allow(&self) -> bool {
        self.usage_ratio() < 1.0
    }

    /// Determine probe strategy based on current state
    pub fn probe_strategy(&self) -> ProbeStrategy {
        let ratio = self.usage_ratio();
        let time_since_calibration = {
            let last = self.last_calibration.read().unwrap();
            Instant::now().duration_since(*last)
        };

        // If recently calibrated, be more conservative
        if time_since_calibration < Duration::from_secs(300) {
            // Within 5 minutes of calibration, don't probe
            if ratio < 0.90 {
                return ProbeStrategy::None;
            }
        }

        ProbeStrategy::from_usage_ratio(ratio)
    }

    /// Record a successful request
    pub fn record_success(&self) {
        self.maybe_reset_minute();
        let current = self.requests_this_minute.fetch_add(1, Ordering::Relaxed) + 1;
        let threshold = self.working_threshold.load(Ordering::Relaxed);

        // If we succeeded above threshold, the limit might be higher
        if current > threshold {
            let consecutive = self
                .consecutive_above_threshold
                .fetch_add(1, Ordering::Relaxed)
                + 1;

            // After 3 consecutive successes above threshold, expand limit
            if consecutive >= 3 {
                self.expand_limit();
                self.consecutive_above_threshold.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Record a 429 error - immediately contract limit
    pub fn record_429(&self) {
        self.maybe_reset_minute();
        let current_requests = self.requests_this_minute.load(Ordering::Relaxed);
        let old_limit = self.confirmed_limit.load(Ordering::Relaxed);

        // The actual limit is what we just hit
        let actual_limit = if current_requests > 0 {
            current_requests
        } else {
            old_limit
        };

        // Apply AIMD penalize
        let new_limit = self.aimd.penalize(actual_limit);
        let new_threshold = (new_limit as f64 * self.safety_margin) as u64;

        self.confirmed_limit.store(new_limit, Ordering::Relaxed);
        self.working_threshold
            .store(new_threshold, Ordering::Relaxed);
        // Ceiling also contracts - limit might have decreased
        self.ceiling.store(actual_limit, Ordering::Relaxed);
        *self.last_calibration.write().unwrap() = Instant::now();
        self.consecutive_above_threshold.store(0, Ordering::Relaxed);

        crate::proxy::prometheus::record_aimd_penalty();

        tracing::warn!(
            "📉 AIMD penalize: limit {} → {} (threshold: {}), actual hit at: {}",
            old_limit,
            new_limit,
            new_threshold,
            current_requests
        );
    }

    /// Expand limit after successful probing
    fn expand_limit(&self) {
        let old_limit = self.confirmed_limit.load(Ordering::Relaxed);
        let new_limit = self.aimd.reward(old_limit);
        let new_threshold = (new_limit as f64 * self.safety_margin) as u64;

        self.confirmed_limit.store(new_limit, Ordering::Relaxed);
        self.working_threshold
            .store(new_threshold, Ordering::Relaxed);
        self.ceiling.fetch_max(new_limit, Ordering::Relaxed);
        *self.last_calibration.write().unwrap() = Instant::now();

        crate::proxy::prometheus::record_aimd_reward();

        tracing::info!(
            "📈 AIMD reward: limit {} → {} (threshold: {})",
            old_limit,
            new_limit,
            new_threshold
        );
    }

    /// Force expansion after successful probe (called by SmartProber)
    pub fn force_expand(&self) {
        self.expand_limit();
    }

    /// Get data for persistence
    pub fn to_persisted(&self) -> (u64, u64, u64) {
        (
            self.confirmed_limit.load(Ordering::Relaxed),
            self.ceiling.load(Ordering::Relaxed),
            self.last_calibration.read().unwrap().elapsed().as_secs(),
        )
    }

    /// Time since last calibration
    pub fn time_since_calibration(&self) -> Duration {
        self.last_calibration.read().unwrap().elapsed()
    }
}

/// Manager for all account limit trackers
pub struct AdaptiveLimitManager {
    trackers: DashMap<String, AdaptiveLimitTracker>,
    safety_margin: f64,
    aimd: AIMDController,
}

impl AdaptiveLimitManager {
    pub fn new(safety_margin: f64, aimd: AIMDController) -> Self {
        Self {
            trackers: DashMap::new(),
            safety_margin,
            aimd,
        }
    }

    /// Get or create tracker for account
    pub fn get_or_create(
        &self,
        account_id: &str,
    ) -> dashmap::mapref::one::Ref<'_, String, AdaptiveLimitTracker> {
        self.trackers
            .entry(account_id.to_string())
            .or_insert_with(|| AdaptiveLimitTracker::new(self.safety_margin, self.aimd.clone()));
        self.trackers.get(account_id).expect("just inserted")
    }

    /// Get tracker for account (if exists)
    pub fn get(
        &self,
        account_id: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, AdaptiveLimitTracker>> {
        self.trackers.get(account_id)
    }

    /// Load tracker from persisted data
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

    /// Get usage ratio for account
    pub fn usage_ratio(&self, account_id: &str) -> f64 {
        self.get_or_create(account_id).usage_ratio()
    }

    /// Get probe strategy for account
    pub fn probe_strategy(&self, account_id: &str) -> ProbeStrategy {
        self.get_or_create(account_id).probe_strategy()
    }

    /// Record success for account
    pub fn record_success(&self, account_id: &str) {
        self.get_or_create(account_id).record_success();
    }

    /// Record 429 for account
    pub fn record_429(&self, account_id: &str) {
        self.get_or_create(account_id).record_429();
    }

    /// Force expand after successful probe
    pub fn force_expand(&self, account_id: &str) {
        self.get_or_create(account_id).force_expand();
    }

    /// Check if request should be allowed
    pub fn should_allow(&self, account_id: &str) -> bool {
        self.get_or_create(account_id).should_allow()
    }

    /// Get all trackers for persistence
    pub fn all_for_persistence(&self) -> Vec<(String, u64, u64, u64)> {
        self.trackers
            .iter()
            .map(|entry| {
                let (confirmed, ceiling, age) = entry.value().to_persisted();
                (entry.key().clone(), confirmed, ceiling, age)
            })
            .collect()
    }

    /// Get count of tracked accounts
    pub fn len(&self) -> usize {
        self.trackers.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.trackers.is_empty()
    }
}

impl Default for AdaptiveLimitManager {
    fn default() -> Self {
        Self::new(0.85, AIMDController::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aimd_reward() {
        let aimd = AIMDController::default();
        assert_eq!(aimd.reward(100), 105); // +5%
        assert_eq!(aimd.reward(1000), 1000); // Capped at max
    }

    #[test]
    fn test_aimd_penalize() {
        let aimd = AIMDController::default();
        assert_eq!(aimd.penalize(100), 70); // ×0.7
        assert_eq!(aimd.penalize(10), 10); // Min floor
    }

    #[test]
    fn test_probe_strategy() {
        assert_eq!(ProbeStrategy::from_usage_ratio(0.5), ProbeStrategy::None);
        assert_eq!(
            ProbeStrategy::from_usage_ratio(0.75),
            ProbeStrategy::CheapProbe
        );
        assert_eq!(
            ProbeStrategy::from_usage_ratio(0.90),
            ProbeStrategy::DelayedHedge
        );
        assert_eq!(
            ProbeStrategy::from_usage_ratio(0.99),
            ProbeStrategy::ImmediateHedge
        );
    }

    #[test]
    fn test_tracker_usage_ratio() {
        let tracker = AdaptiveLimitTracker::new(0.85, AIMDController::default());
        assert_eq!(tracker.usage_ratio(), 0.0);

        // Simulate requests
        for _ in 0..6 {
            tracker.record_success();
        }
        // Default limit = 15, threshold = 12.75 ≈ 12
        // 6 / 12 = 0.5
        let ratio = tracker.usage_ratio();
        assert!(ratio > 0.4 && ratio < 0.6, "ratio was {}", ratio);
    }

    #[test]
    fn test_tracker_429_contracts_limit() {
        let tracker = AdaptiveLimitTracker::new(0.85, AIMDController::default());
        let initial = tracker.confirmed_limit();

        tracker.record_429();

        assert!(tracker.confirmed_limit() < initial);
    }

    #[test]
    fn test_tracker_expansion_after_successes() {
        let tracker = AdaptiveLimitTracker::new(0.85, AIMDController::default());

        // Set threshold low so we can exceed it
        tracker.working_threshold.store(5, Ordering::Relaxed);
        let initial = tracker.confirmed_limit();

        // Record successes above threshold
        for _ in 0..10 {
            tracker.record_success();
        }

        // Should have expanded
        assert!(tracker.confirmed_limit() > initial);
    }

    #[test]
    fn test_persisted_with_decay() {
        let fresh =
            AdaptiveLimitTracker::from_persisted(100, 100, 0, 0.85, AIMDController::default());
        assert_eq!(fresh.confirmed_limit(), 100);

        let stale = AdaptiveLimitTracker::from_persisted(
            100,
            100,
            86400 * 2,
            0.85,
            AIMDController::default(), // 2 days old
        );
        assert!(stale.confirmed_limit() < 100); // Should be decayed
    }

    #[test]
    fn test_manager_get_or_create() {
        let manager = AdaptiveLimitManager::default();
        assert!(manager.is_empty());

        let _ = manager.get_or_create("account1");
        assert_eq!(manager.len(), 1);

        let _ = manager.get_or_create("account1");
        assert_eq!(manager.len(), 1); // No duplicate

        let _ = manager.get_or_create("account2");
        assert_eq!(manager.len(), 2);
    }
}
