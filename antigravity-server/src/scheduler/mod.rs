//! Background Schedulers
//!
//! ## Smart Warmup Scheduler
//! Background task that periodically warms up accounts to maintain active sessions.
//!
//! Modes:
//! - `only_low_quota: false` — Warms up accounts with 100% quota to prevent staleness
//! - `only_low_quota: true` — Warms up accounts with <50% quota to refresh them
//!
//! Features:
//! - Configurable interval (default 60 minutes)
//! - 4-hour cooldown per account to prevent re-warming
//! - Whitelisted models only (from SmartWarmupConfig)
//! - Persistent history across restarts (async I/O)
//! - Groups warmup by account to avoid N+1 API calls
//!
//! ## Auto Quota Refresh Scheduler
//! Background task that periodically refreshes account quotas from Google API.
//!
//! Features:
//! - Enabled via `config.auto_refresh` flag
//! - Configurable interval via `config.refresh_interval` (minutes, default 15)
//! - Required for quota protection and smart warmup to have fresh data

mod quota_refresh;
mod state;
mod warmup;

pub use quota_refresh::start_quota_refresh;
pub use warmup::start;

use crate::state::AppState;
use std::time::Duration;
use tokio::time::interval;

pub fn start_oauth_cleanup(state: AppState) {
    tokio::spawn(async move {
        let mut cleanup_interval = interval(Duration::from_secs(60));
        loop {
            cleanup_interval.tick().await;
            let before = state.inner.oauth_states.len();
            state
                .inner
                .oauth_states
                .retain(|_, (created_at, _)| created_at.elapsed().as_secs() < 600);
            let after = state.inner.oauth_states.len();
            let removed = before.saturating_sub(after);
            if removed > 0 {
                tracing::debug!("[Scheduler] Cleaned up {} expired OAuth states", removed);
            }
        }
    });
}
