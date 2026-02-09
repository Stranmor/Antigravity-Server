//! Session management for TokenManager.
//!
//! Handles session-to-account bindings and failure tracking for sticky sessions.

use super::TokenManager;
use std::sync::atomic::Ordering;

// Session failure counters are heuristic — Relaxed ordering is sufficient.

const SESSION_FAILURE_THRESHOLD: u32 = 3;

impl TokenManager {
    /// Record a failure for a session, returns new failure count.
    pub fn record_session_failure(&self, session_id: &str) -> u32 {
        self.session_failures
            .entry(session_id.to_string())
            .or_insert_with(|| std::sync::atomic::AtomicU32::new(0))
            .fetch_add(1, Ordering::Relaxed)
            + 1
    }

    /// Clear failure count for a session.
    pub fn clear_session_failures(&self, session_id: &str) {
        self.session_failures.remove(session_id);
    }

    /// Get current failure count for a session.
    pub fn get_session_failures(&self, session_id: &str) -> u32 {
        self.session_failures.get(session_id).map(|c| c.load(Ordering::Relaxed)).unwrap_or(0)
    }

    /// Clear binding for a specific session.
    #[allow(dead_code, reason = "public API used in tests")]
    pub fn clear_session_binding(&self, session_id: &str) {
        self.session_accounts.remove(session_id);
    }

    /// Clear all session bindings.
    pub fn clear_all_sessions(&self) {
        self.session_accounts.clear();
    }

    /// Check if session has exceeded failure threshold and should be unbound.
    pub(crate) fn should_unbind_session(&self, session_id: &str) -> bool {
        self.get_session_failures(session_id) >= SESSION_FAILURE_THRESHOLD
    }

    /// Unbind session from its current account due to failures.
    pub(crate) fn unbind_session_on_failures(&self, session_id: &str) -> Option<String> {
        if let Some(bound_id) = self.session_accounts.get(session_id).map(|v| v.clone()) {
            let failures = self.get_session_failures(session_id);
            self.session_accounts.remove(session_id);
            self.clear_session_failures(session_id);
            tracing::warn!(
                "Session {} unbound from {} after {} consecutive failures",
                session_id,
                bound_id,
                failures
            );
            Some(bound_id)
        } else {
            None
        }
    }

    /// Bind session to account if session affinity is enabled.
    pub(crate) fn bind_session_to_account(
        &self,
        session_id: &str,
        email: &str,
        enable_session_affinity: bool,
    ) {
        if enable_session_affinity {
            let current_binding = self.session_accounts.get(session_id).map(|v| v.clone());
            if current_binding.as_ref() != Some(&email.to_string()) {
                self.session_accounts.insert(session_id.to_string(), email.to_string());
                if let Some(ref old) = current_binding {
                    tracing::info!(
                        "Sticky Session: Rebound session {} from {} to {} (cache continuity)",
                        session_id,
                        old,
                        email
                    );
                }
            }
        }
    }

    /// Get account bound to session.
    pub(crate) fn get_session_account(&self, session_id: &str) -> Option<String> {
        self.session_accounts.get(session_id).map(|v| v.clone())
    }
}
