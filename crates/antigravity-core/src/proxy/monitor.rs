//! Proxy monitoring and logging.
//!
//! This module provides abstractions for monitoring proxy requests
//! without any GUI-specific dependencies.
#![allow(clippy::arithmetic_side_effects, reason = "counter increments and token accumulation")]

// Re-export ProxyRequestLog for upstream middleware compatibility
use crate::modules::repository::{AccountRepository, RequestLog};
pub use antigravity_types::models::{ProxyRequestLog, ProxyStats};
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for event bus implementations to emit proxy events.
/// Different frontends (Tauri, WebSocket, etc.) can implement this.
pub trait ProxyEventBus: Send + Sync {
    fn emit_request_log(&self, log: &ProxyRequestLog);
}

/// A no-op event bus for headless mode
pub struct NoopEventBus;

impl ProxyEventBus for NoopEventBus {
    fn emit_request_log(&self, _log: &ProxyRequestLog) {
        // No-op
    }
}

/// Proxy monitor for tracking requests and statistics
pub struct ProxyMonitor {
    enabled: AtomicBool,
    stats: RwLock<ProxyStats>,
    event_bus: Arc<dyn ProxyEventBus>,
    logs: RwLock<VecDeque<ProxyRequestLog>>,
    max_logs: usize,
    repository: Option<Arc<dyn AccountRepository>>,
    tokens: Option<Arc<DashMap<String, crate::proxy::token_manager::ProxyToken>>>,
}

impl ProxyMonitor {
    pub fn new() -> Self {
        Self::with_event_bus(Arc::new(NoopEventBus))
    }

    pub fn with_event_bus(event_bus: Arc<dyn ProxyEventBus>) -> Self {
        Self {
            enabled: AtomicBool::new(true),
            stats: RwLock::new(ProxyStats::default()),
            event_bus,
            logs: RwLock::new(VecDeque::with_capacity(1024)),
            max_logs: 1000,
            repository: None,
            tokens: None,
        }
    }

    pub fn with_db(
        event_bus: Arc<dyn ProxyEventBus>,
        repository: Arc<dyn AccountRepository>,
        tokens: Arc<DashMap<String, crate::proxy::token_manager::ProxyToken>>,
    ) -> Self {
        Self {
            enabled: AtomicBool::new(true),
            stats: RwLock::new(ProxyStats::default()),
            event_bus,
            logs: RwLock::new(VecDeque::with_capacity(1024)),
            max_logs: 1000,
            repository: Some(repository),
            tokens: Some(tokens),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub async fn log_request(&self, log: ProxyRequestLog) {
        // Extract DB fields before log is moved into the in-memory buffer
        let db_context = self.extract_db_context(&log);

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
            if log.status >= 400 {
                stats.error_count += 1;
            } else {
                stats.success_count += 1;
            }
            if let Some(tokens) = log.input_tokens {
                stats.total_input_tokens += u64::from(tokens);
            }
            if let Some(tokens) = log.output_tokens {
                stats.total_output_tokens += u64::from(tokens);
            }
        }

        // Emit to event bus
        self.event_bus.emit_request_log(&log);

        // Record to PostgreSQL (background, non-blocking)
        if let Some((repo, request_log)) = db_context {
            tokio::spawn(async move {
                if let Err(e) = repo.log_request(request_log).await {
                    tracing::warn!("Failed to record request to DB: {}", e);
                }
            });
        }

        // Store in logs buffer (VecDeque: O(1) pop_front)
        {
            let mut logs = self.logs.write().await;
            if logs.len() >= self.max_logs {
                let excess = logs.len() - self.max_logs + 1;
                logs.drain(..excess);
            }
            logs.push_back(log);
        }
    }

    fn extract_db_context(
        &self,
        log: &ProxyRequestLog,
    ) -> Option<(Arc<dyn AccountRepository>, RequestLog)> {
        let (repo, tokens) = match (&self.repository, &self.tokens) {
            (Some(r), Some(t)) => (r, t),
            _ => return None,
        };
        let email = log.account_email.as_ref()?;
        let account_id = tokens
            .iter()
            .find(|entry| &entry.value().email == email)
            .map(|entry| entry.key().clone())?;

        let model = log.model.clone().or_else(|| log.mapped_model.clone()).unwrap_or_default();
        let request_log = RequestLog {
            account_id,
            model,
            tokens_in: log.input_tokens.map(|v| i32::try_from(v).unwrap_or(i32::MAX)),
            tokens_out: log.output_tokens.map(|v| i32::try_from(v).unwrap_or(i32::MAX)),
            cached_tokens: log.cached_tokens.map(|v| i32::try_from(v).unwrap_or(i32::MAX)),
            latency_ms: Some(i32::try_from(log.duration).unwrap_or(i32::MAX)),
            status_code: i32::from(log.status),
            error_type: log.error.clone(),
        };
        Some((Arc::clone(repo), request_log))
    }

    pub async fn get_stats(&self) -> ProxyStats {
        *self.stats.read().await
    }

    pub async fn get_logs(&self, limit: Option<usize>) -> Vec<ProxyRequestLog> {
        let logs = self.logs.read().await;
        let limit = limit.unwrap_or(logs.len());
        logs.iter().rev().take(limit).cloned().collect()
    }

    pub async fn clear_logs(&self) {
        let mut logs = self.logs.write().await;
        logs.clear();
    }

    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = ProxyStats::default();
    }
}

impl Default for ProxyMonitor {
    fn default() -> Self {
        Self::new()
    }
}
