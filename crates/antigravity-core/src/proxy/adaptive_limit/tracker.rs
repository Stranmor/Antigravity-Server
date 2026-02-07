// AIMD rate limiting: arithmetic on request counts and limits.
// Values are bounded: limits are u64 but practically < 10000 RPM.
// f64 precision loss is acceptable for rate limiting heuristics.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::arithmetic_side_effects,
    clippy::integer_division,
    reason = "AIMD algorithm: bounded rate limits, f64 math for smooth adjustments"
)]

use super::aimd::{AIMDController, ProbeStrategy};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::{Duration, Instant};

pub struct AdaptiveLimitTracker {
    confirmed_limit: AtomicU64,
    pub(crate) working_threshold: AtomicU64,
    ceiling: AtomicU64,
    requests_this_minute: AtomicU64,
    minute_started_at: RwLock<Instant>,
    last_calibration: RwLock<Instant>,
    consecutive_above_threshold: AtomicU64,
    safety_margin: f64,
    aimd: AIMDController,
    limit_update_lock: Mutex<()>,
}

impl AdaptiveLimitTracker {
    pub fn new(safety_margin: f64, aimd: AIMDController) -> Self {
        let default_limit = aimd.min_limit.max(15).min(aimd.max_limit);
        Self {
            confirmed_limit: AtomicU64::new(default_limit),
            working_threshold: AtomicU64::new((default_limit as f64 * safety_margin) as u64),
            ceiling: AtomicU64::new(default_limit),
            requests_this_minute: AtomicU64::new(0),
            minute_started_at: RwLock::new(Instant::now()),
            last_calibration: RwLock::new(
                Instant::now().checked_sub(Duration::from_secs(3600)).unwrap_or_else(Instant::now),
            ),
            consecutive_above_threshold: AtomicU64::new(0),
            safety_margin,
            aimd,
            limit_update_lock: Mutex::new(()),
        }
    }

    pub fn from_persisted(
        confirmed_limit: u64,
        ceiling: u64,
        age_seconds: u64,
        safety_margin: f64,
        aimd: AIMDController,
    ) -> Self {
        let age_hours = age_seconds / 3600;
        let confidence = match age_hours {
            0..=1 => 1.0,
            2..=6 => 0.9,
            7..=24 => 0.7,
            _ => {
                tracing::debug!(
                    "Persisted AIMD data is {}h old, applying minimum confidence 50%",
                    age_hours
                );
                0.5
            },
        };

        let effective_limit = (confirmed_limit as f64 * confidence) as u64;
        let effective_limit = effective_limit.max(aimd.min_limit).min(aimd.max_limit);

        let tracker = Self::new(safety_margin, aimd);
        tracker.confirmed_limit.store(effective_limit, Ordering::Relaxed);
        tracker
            .working_threshold
            .store((effective_limit as f64 * safety_margin) as u64, Ordering::Relaxed);
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

    fn maybe_reset_minute(&self) {
        let now = Instant::now();
        let should_reset = self
            .minute_started_at
            .read()
            .map(|started| now.duration_since(*started) >= Duration::from_secs(60))
            .unwrap_or(true);

        if should_reset {
            let mut guard = match self.minute_started_at.write() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            if now.duration_since(*guard) >= Duration::from_secs(60) {
                self.requests_this_minute.store(0, Ordering::Relaxed);
                *guard = now;
            }
        }
    }

    pub fn usage_ratio(&self) -> f64 {
        self.maybe_reset_minute();
        let usage = self.requests_this_minute.load(Ordering::Relaxed);
        let threshold = self.working_threshold.load(Ordering::Relaxed);
        if threshold == 0 {
            return 0.0;
        }
        usage as f64 / threshold as f64
    }

    pub fn requests_this_minute(&self) -> u64 {
        self.maybe_reset_minute();
        self.requests_this_minute.load(Ordering::Relaxed)
    }

    pub fn working_threshold(&self) -> u64 {
        self.working_threshold.load(Ordering::Relaxed)
    }

    pub fn confirmed_limit(&self) -> u64 {
        self.confirmed_limit.load(Ordering::Relaxed)
    }

    pub fn ceiling(&self) -> u64 {
        self.ceiling.load(Ordering::Relaxed)
    }

    pub fn should_allow(&self) -> bool {
        self.usage_ratio() < 1.0
    }

    pub fn probe_strategy(&self) -> ProbeStrategy {
        let ratio = self.usage_ratio();
        let time_since_calibration = self
            .last_calibration
            .read()
            .map(|last| Instant::now().duration_since(*last))
            .unwrap_or(Duration::from_secs(3600));

        if time_since_calibration < Duration::from_secs(300) && ratio < 0.90 {
            return ProbeStrategy::None;
        }

        ProbeStrategy::from_usage_ratio(ratio)
    }

    pub fn record_success(&self) {
        self.maybe_reset_minute();
        let current = self.requests_this_minute.fetch_add(1, Ordering::Relaxed) + 1;
        let threshold = self.working_threshold.load(Ordering::Relaxed);

        if current > threshold {
            let consecutive = self.consecutive_above_threshold.fetch_add(1, Ordering::Relaxed) + 1;

            if consecutive >= 3 {
                let _lock = self.limit_update_lock.lock().unwrap_or_else(|e| e.into_inner());

                let current_consecutive = self.consecutive_above_threshold.load(Ordering::Relaxed);
                if current_consecutive >= 3 {
                    self.expand_limit_inner();
                    self.consecutive_above_threshold.store(0, Ordering::Relaxed);
                }
            }
        }
    }

    pub fn record_error(&self, status_code: u16) {
        self.consecutive_above_threshold.store(0, Ordering::Relaxed);

        if status_code == 429 {
            self.record_429();
        } else if (500..600).contains(&status_code) {
            tracing::warn!("Received 5xx error ({}). Resetting AIMD success counter.", status_code);
        }
    }

    pub fn record_429(&self) {
        let _lock = self.limit_update_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.maybe_reset_minute();
        let current_requests = self.requests_this_minute.load(Ordering::Relaxed);
        let old_limit = self.confirmed_limit.load(Ordering::Relaxed);

        let actual_limit = current_requests.max(old_limit);

        let new_limit = self.aimd.penalize(actual_limit);
        let new_threshold = (new_limit as f64 * self.safety_margin) as u64;

        self.confirmed_limit.store(new_limit, Ordering::Relaxed);
        self.working_threshold.store(new_threshold, Ordering::Relaxed);
        self.ceiling.store(actual_limit, Ordering::Relaxed);
        match self.last_calibration.write() {
            Ok(mut guard) => *guard = Instant::now(),
            Err(poisoned) => *poisoned.into_inner() = Instant::now(),
        }
        self.consecutive_above_threshold.store(0, Ordering::Relaxed);

        crate::proxy::prometheus::record_aimd_penalty();

        tracing::warn!(
            "AIMD penalize: limit {} -> {} (threshold: {}), actual hit at: {}",
            old_limit,
            new_limit,
            new_threshold,
            current_requests
        );
    }

    fn expand_limit(&self) {
        let _lock = self.limit_update_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.expand_limit_inner();
    }

    fn expand_limit_inner(&self) {
        let old_limit = self.confirmed_limit.load(Ordering::Relaxed);
        let new_limit = self.aimd.reward(old_limit);
        let new_threshold = (new_limit as f64 * self.safety_margin) as u64;

        self.confirmed_limit.store(new_limit, Ordering::Relaxed);
        self.working_threshold.store(new_threshold, Ordering::Relaxed);
        self.ceiling.fetch_max(new_limit, Ordering::Relaxed);
        match self.last_calibration.write() {
            Ok(mut guard) => *guard = Instant::now(),
            Err(poisoned) => *poisoned.into_inner() = Instant::now(),
        }

        crate::proxy::prometheus::record_aimd_reward();

        tracing::info!(
            "AIMD reward: limit {} -> {} (threshold: {})",
            old_limit,
            new_limit,
            new_threshold
        );
    }

    pub fn force_expand(&self) {
        self.expand_limit();
    }

    pub fn to_persisted(&self) -> (u64, u64, u64) {
        let elapsed = self.last_calibration.read().map(|t| t.elapsed().as_secs()).unwrap_or(3600);
        (
            self.confirmed_limit.load(Ordering::Relaxed),
            self.ceiling.load(Ordering::Relaxed),
            elapsed,
        )
    }

    pub fn time_since_calibration(&self) -> Duration {
        self.last_calibration.read().map(|t| t.elapsed()).unwrap_or(Duration::from_secs(3600))
    }
}
