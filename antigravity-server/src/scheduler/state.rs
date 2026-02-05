use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const COOLDOWN_SECONDS: i64 = 14400;
pub const LOW_QUOTA_THRESHOLD: i32 = 50;

pub const DEFAULT_WARMUP_MODELS: &[&str] =
    &["gemini-3-flash", "claude-sonnet-4-5", "gemini-3-pro-high", "gemini-3-pro-image"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WarmupHistory {
    pub entries: HashMap<String, i64>,
}

pub struct SchedulerState {
    pub history: WarmupHistory,
    pub history_path: PathBuf,
}

impl SchedulerState {
    pub async fn new_async(data_dir: PathBuf) -> Self {
        let history_path = data_dir.join("warmup_history.json");
        let history = Self::load_history_async(&history_path).await;
        Self { history, history_path }
    }

    async fn load_history_async(path: &PathBuf) -> WarmupHistory {
        if path.exists() {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("[Scheduler] Failed to read history file: {}", e);
                    WarmupHistory::default()
                },
            }
        } else {
            WarmupHistory::default()
        }
    }

    pub async fn save_history_async(&self) {
        let path = self.history_path.clone();
        let content = match serde_json::to_string_pretty(&self.history) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[Scheduler] Failed to serialize history: {}", e);
                return;
            },
        };
        if let Err(e) = tokio::fs::write(&path, content).await {
            tracing::warn!("[Scheduler] Failed to write history to {:?}: {}", path, e);
        }
    }

    pub fn record_warmup(&mut self, key: &str, timestamp: i64) {
        self.history.entries.insert(key.to_string(), timestamp);
    }

    pub fn is_in_cooldown(&self, key: &str, now: i64) -> bool {
        self.history.entries.get(key).is_some_and(|&ts| now - ts < COOLDOWN_SECONDS)
    }

    pub fn cleanup_stale(&mut self, cutoff: i64) -> usize {
        let before = self.history.entries.len();
        self.history.entries.retain(|_, &mut ts| ts > cutoff);
        before - self.history.entries.len()
    }
}
