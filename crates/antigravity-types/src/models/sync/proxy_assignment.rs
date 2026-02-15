//! Per-account proxy assignment sync types with LWW merge.
//!
//! Keyed by account email (stable across instances, unlike UUID).
//! Follows the same LWW merge strategy as `SyncableMapping`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::mapping::current_timestamp_ms;

const MAX_PROXY_ENTRIES: usize = 10_000;

/// Single proxy assignment entry with timestamp for LWW merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyAssignment {
    /// Proxy URL (socks5://, socks5h://, http://, https://), or None if removed.
    pub proxy_url: Option<String>,
    /// Unix timestamp in milliseconds when this entry was last updated.
    pub updated_at: i64,
}

impl ProxyAssignment {
    /// Creates a new proxy assignment with current timestamp.
    pub fn new(proxy_url: Option<String>) -> Self {
        Self { proxy_url, updated_at: current_timestamp_ms() }
    }

    /// Creates a proxy assignment with a specific timestamp.
    pub fn with_timestamp(proxy_url: Option<String>, updated_at: i64) -> Self {
        Self { proxy_url, updated_at }
    }

    /// LWW comparison: returns true if self should win over other.
    /// Higher timestamp wins. On tie: None (removal) wins over Some (assignment).
    /// On tie with same kind: lexicographic comparison of URL.
    pub(crate) fn wins_over(&self, other: &Self) -> bool {
        if self.updated_at != other.updated_at {
            return self.updated_at > other.updated_at;
        }
        // On timestamp tie: removal wins (conservative — don't assign dead proxy)
        match (&self.proxy_url, &other.proxy_url) {
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (Some(a), Some(b)) => a > b,
            (None, None) => false,
        }
    }
}

/// Syncable proxy assignments with per-entry timestamps.
///
/// Keyed by account email (stable across instances).
/// Supports bidirectional sync via LWW (Last-Write-Wins) merge.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SyncableProxyAssignments {
    /// Map of account email to their proxy assignment.
    pub entries: BTreeMap<String, ProxyAssignment>,
    /// Optional instance identifier for sync tracking.
    #[serde(default)]
    pub instance_id: Option<String>,
}

impl SyncableProxyAssignments {
    /// Creates an empty syncable proxy assignments map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a proxy assignment for an account email.
    pub fn set(&mut self, email: impl Into<String>, proxy_url: Option<String>) {
        let email = email.into();
        let now = current_timestamp_ms();
        let ts = self.entries.get(&email).map_or(now, |e| now.max(e.updated_at.saturating_add(1)));
        drop(self.entries.insert(email, ProxyAssignment { proxy_url, updated_at: ts }));
    }

    /// Gets the proxy URL for an email, if assigned.
    pub fn get(&self, email: &str) -> Option<Option<&str>> {
        self.entries.get(email).map(|e| e.proxy_url.as_deref())
    }

    /// Merges remote entries using LWW. Returns count of updated entries.
    /// Rejects entries with far-future timestamps (>24h ahead) to prevent pinning attacks.
    pub fn merge_lww(&mut self, remote: &Self) -> usize {
        let mut updated = 0_usize;
        let max_allowed = current_timestamp_ms().saturating_add(86_400_000); // 24h clock skew

        for (key, remote_entry) in &remote.entries {
            if remote_entry.updated_at > max_allowed {
                continue;
            }

            let is_update = self.entries.contains_key(key);
            if !is_update && self.entries.len() >= MAX_PROXY_ENTRIES {
                self.evict_oldest();
            }

            let should_update =
                self.entries.get(key).is_none_or(|local_entry| remote_entry.wins_over(local_entry));

            if should_update {
                drop(self.entries.insert(key.clone(), remote_entry.clone()));
                updated = updated.saturating_add(1);
            }
        }

        updated
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest_key) =
            self.entries.iter().min_by_key(|(_, v)| v.updated_at).map(|(k, _)| k.clone())
        {
            drop(self.entries.remove(&oldest_key));
        }
    }

    /// Returns entries that are newer than the remote's version.
    #[must_use]
    pub fn diff_newer_than(&self, remote: &Self) -> Self {
        let entries: BTreeMap<String, ProxyAssignment> = self
            .entries
            .iter()
            .filter(|&(key, local_entry)| {
                remote
                    .entries
                    .get(key)
                    .is_none_or(|remote_entry| local_entry.wins_over(remote_entry))
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Self { entries, instance_id: self.instance_id.clone() }
    }

    /// Returns count of entries with active proxy assignments.
    pub fn active_count(&self) -> usize {
        self.entries.values().filter(|e| e.proxy_url.is_some()).count()
    }

    /// Returns total entry count (including removals).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no entries exist.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
