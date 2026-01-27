//! Distributed Sync Types for Model Routing
//!
//! Implements Last-Write-Wins (LWW) merge strategy for bidirectional
//! synchronization of model mappings between instances.
//!
//! # Architecture
//!
//! Uses the same strategy as AWS DynamoDB and Apache Cassandra:
//! - Each entry has a timestamp (unix millis)
//! - On merge, newer timestamp wins per key
//! - No conflicts, eventual consistency guaranteed

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Single model mapping entry with timestamp for LWW merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingEntry {
    /// Target model name (e.g., "gemini-3-pro-high")
    pub target: String,
    /// Unix timestamp in milliseconds when this entry was last updated
    pub updated_at: i64,
    /// Tombstone flag: true = entry is deleted (for sync propagation)
    #[serde(default)]
    pub deleted: bool,
}

impl MappingEntry {
    /// Create new entry with current timestamp.
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            updated_at: current_timestamp_ms(),
            deleted: false,
        }
    }

    /// Create entry with specific timestamp (for testing or import).
    pub fn with_timestamp(target: impl Into<String>, updated_at: i64) -> Self {
        Self {
            target: target.into(),
            updated_at,
            deleted: false,
        }
    }

    /// Create tombstone with specific timestamp (for testing).
    pub fn tombstone_at(updated_at: i64) -> Self {
        Self {
            target: String::new(),
            updated_at,
            deleted: true,
        }
    }

    /// Create tombstone entry (marks key as deleted for sync propagation).
    pub fn tombstone() -> Self {
        Self::tombstone_at(current_timestamp_ms())
    }

    /// Check if entry is a tombstone (deleted).
    pub fn is_tombstone(&self) -> bool {
        self.deleted
    }

    /// LWW comparison: returns true if self should win over other.
    /// Order: higher timestamp wins, on tie: tombstone wins, on tie: higher target wins.
    fn wins_over(&self, other: &Self) -> bool {
        if self.updated_at != other.updated_at {
            return self.updated_at > other.updated_at;
        }
        if self.deleted != other.deleted {
            return self.deleted;
        }
        self.target > other.target
    }
}

/// Syncable model mapping with per-entry timestamps.
///
/// Supports bidirectional sync via LWW (Last-Write-Wins) merge.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SyncableMapping {
    /// Model mappings: source model -> MappingEntry (target + timestamp)
    pub entries: HashMap<String, MappingEntry>,
    /// Instance identifier (for debugging/logging)
    #[serde(default)]
    pub instance_id: Option<String>,
}

impl SyncableMapping {
    /// Create empty syncable mapping.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from simple HashMap (all entries get current timestamp).
    pub fn from_simple_map(map: HashMap<String, String>) -> Self {
        let entries = map
            .into_iter()
            .map(|(k, v)| (k, MappingEntry::new(v)))
            .collect();
        Self {
            entries,
            instance_id: None,
        }
    }

    /// Convert to simple HashMap (for runtime use, excludes tombstones).
    pub fn to_simple_map(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .filter(|(_, v)| !v.deleted)
            .map(|(k, v)| (k.clone(), v.target.clone()))
            .collect()
    }

    /// Set or update a mapping entry (updates timestamp).
    pub fn set(&mut self, source: impl Into<String>, target: impl Into<String>) {
        self.entries
            .insert(source.into(), MappingEntry::new(target));
    }

    /// Mark a mapping entry as deleted (tombstone for sync propagation).
    pub fn remove(&mut self, source: &str) {
        self.entries
            .insert(source.to_string(), MappingEntry::tombstone());
    }

    /// Get target for a source model (returns None for tombstones).
    pub fn get(&self, source: &str) -> Option<&str> {
        self.entries
            .get(source)
            .filter(|e| !e.deleted)
            .map(|e| e.target.as_str())
    }

    /// Merge remote mappings using LWW strategy.
    ///
    /// For each key:
    /// - If only in local -> keep local
    /// - If only in remote -> add from remote
    /// - If in both -> use LWW: higher timestamp, tombstone, then target as tie-breakers
    ///
    /// Returns number of entries updated from remote.
    pub fn merge_lww(&mut self, remote: &SyncableMapping) -> usize {
        let mut updated = 0;

        for (key, remote_entry) in &remote.entries {
            let should_update = match self.entries.get(key) {
                Some(local_entry) => remote_entry.wins_over(local_entry),
                None => true,
            };

            if should_update {
                self.entries.insert(key.clone(), remote_entry.clone());
                updated += 1;
            }
        }

        updated
    }

    /// Compute diff: entries that would win if merged into remote.
    ///
    /// Used to send only entries that would update remote.
    pub fn diff_newer_than(&self, remote: &SyncableMapping) -> SyncableMapping {
        let entries: HashMap<_, _> = self
            .entries
            .iter()
            .filter(|(key, local_entry)| {
                remote
                    .entries
                    .get(*key)
                    .is_none_or(|remote_entry| local_entry.wins_over(remote_entry))
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        SyncableMapping {
            entries,
            instance_id: self.instance_id.clone(),
        }
    }

    /// Number of live entries (excludes tombstones).
    pub fn len(&self) -> usize {
        self.entries.values().filter(|e| !e.deleted).count()
    }

    /// Check if empty (no live entries).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total entries including tombstones (for sync/debug).
    pub fn total_entries(&self) -> usize {
        self.entries.len()
    }
}

/// Get current timestamp in milliseconds.
fn current_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lww_merge_remote_newer() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 1000),
        );

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-flash", 2000), // newer
        );

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 1);
        assert_eq!(local.get("gpt-4o"), Some("gemini-3-flash"));
    }

    #[test]
    fn test_lww_merge_local_newer() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 3000), // newer
        );

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-flash", 2000),
        );

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 0);
        assert_eq!(local.get("gpt-4o"), Some("gemini-3-pro")); // local kept
    }

    #[test]
    fn test_lww_merge_new_keys() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 1000),
        );

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "claude-opus".to_string(),
            MappingEntry::with_timestamp("claude-opus-4-5", 2000),
        );

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 1);
        assert_eq!(local.len(), 2);
        assert_eq!(local.get("gpt-4o"), Some("gemini-3-pro"));
        assert_eq!(local.get("claude-opus"), Some("claude-opus-4-5"));
    }

    #[test]
    fn test_from_simple_map() {
        let mut simple: HashMap<String, String> = HashMap::new();
        simple.insert("gpt-4o".to_string(), "gemini-3-pro".to_string());

        let syncable = SyncableMapping::from_simple_map(simple);

        assert_eq!(syncable.len(), 1);
        assert_eq!(syncable.get("gpt-4o"), Some("gemini-3-pro"));
        assert!(syncable.entries["gpt-4o"].updated_at > 0);
    }

    #[test]
    fn test_to_simple_map() {
        let mut syncable = SyncableMapping::new();
        syncable.set("gpt-4o", "gemini-3-pro");
        syncable.set("claude", "claude-opus");

        let simple = syncable.to_simple_map();

        assert_eq!(simple.len(), 2);
        assert_eq!(simple.get("gpt-4o"), Some(&"gemini-3-pro".to_string()));
        assert_eq!(simple.get("claude"), Some(&"claude-opus".to_string()));
    }

    #[test]
    fn test_diff_newer_than() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 3000), // newer
        );
        local.entries.insert(
            "claude".to_string(),
            MappingEntry::with_timestamp("claude-opus", 1000), // older
        );
        local.entries.insert(
            "new-model".to_string(),
            MappingEntry::with_timestamp("target", 5000), // only in local
        );

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("old-target", 2000),
        );
        remote.entries.insert(
            "claude".to_string(),
            MappingEntry::with_timestamp("newer-target", 2000),
        );

        let diff = local.diff_newer_than(&remote);

        assert_eq!(diff.len(), 2); // gpt-4o (newer) and new-model (only local)
        assert!(diff.entries.contains_key("gpt-4o"));
        assert!(diff.entries.contains_key("new-model"));
        assert!(!diff.entries.contains_key("claude")); // remote is newer
    }

    #[test]
    fn test_tombstone_creation() {
        let tombstone = MappingEntry::tombstone();
        assert!(tombstone.is_tombstone());
        assert!(tombstone.deleted);
        assert!(tombstone.target.is_empty());
    }

    #[test]
    fn test_remove_creates_tombstone() {
        let mut mapping = SyncableMapping::new();
        mapping.set("gpt-4o", "gemini-3-pro");
        assert_eq!(mapping.len(), 1);

        mapping.remove("gpt-4o");

        assert_eq!(mapping.len(), 0);
        assert_eq!(mapping.total_entries(), 1);
        assert!(mapping.entries.get("gpt-4o").unwrap().is_tombstone());
        assert_eq!(mapping.get("gpt-4o"), None);
    }

    #[test]
    fn test_tombstone_excluded_from_simple_map() {
        let mut mapping = SyncableMapping::new();
        mapping.set("gpt-4o", "gemini-3-pro");
        mapping.set("claude", "claude-opus");
        mapping.remove("claude");

        let simple = mapping.to_simple_map();

        assert_eq!(simple.len(), 1);
        assert!(simple.contains_key("gpt-4o"));
        assert!(!simple.contains_key("claude"));
    }

    #[test]
    fn test_tombstone_propagates_via_merge() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 1000),
        );

        let mut remote = SyncableMapping::new();
        let mut tombstone = MappingEntry::tombstone();
        tombstone.updated_at = 2000;
        remote.entries.insert("gpt-4o".to_string(), tombstone);

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 1);
        assert!(local.entries.get("gpt-4o").unwrap().is_tombstone());
        assert_eq!(local.get("gpt-4o"), None);
    }

    #[test]
    fn test_lww_tie_breaker_lexicographic() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("aaa-model", 1000),
        );

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("zzz-model", 1000),
        );

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 1);
        assert_eq!(local.get("gpt-4o"), Some("zzz-model"));
    }

    #[test]
    fn test_lww_tie_breaker_local_wins_if_greater() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("zzz-model", 1000),
        );

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("aaa-model", 1000),
        );

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 0);
        assert_eq!(local.get("gpt-4o"), Some("zzz-model"));
    }

    #[test]
    fn test_tombstone_wins_over_live_on_timestamp_tie() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 1000),
        );

        let mut remote = SyncableMapping::new();
        remote
            .entries
            .insert("gpt-4o".to_string(), MappingEntry::tombstone_at(1000));

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 1);
        assert!(local.entries.get("gpt-4o").unwrap().is_tombstone());
    }

    #[test]
    fn test_live_does_not_override_tombstone_on_timestamp_tie() {
        let mut local = SyncableMapping::new();
        local
            .entries
            .insert("gpt-4o".to_string(), MappingEntry::tombstone_at(1000));

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 1000),
        );

        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 0);
        assert!(local.entries.get("gpt-4o").unwrap().is_tombstone());
    }

    #[test]
    fn test_set_overwrites_tombstone() {
        let mut mapping = SyncableMapping::new();
        mapping.set("gpt-4o", "old-target");
        mapping.remove("gpt-4o");
        assert!(mapping.entries.get("gpt-4o").unwrap().is_tombstone());

        mapping.set("gpt-4o", "new-target");

        assert!(!mapping.entries.get("gpt-4o").unwrap().is_tombstone());
        assert_eq!(mapping.get("gpt-4o"), Some("new-target"));
        assert_eq!(mapping.len(), 1);
    }

    #[test]
    fn test_diff_includes_tombstones() {
        let mut local = SyncableMapping::new();
        local
            .entries
            .insert("gpt-4o".to_string(), MappingEntry::tombstone_at(3000));

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 2000),
        );

        let diff = local.diff_newer_than(&remote);

        assert_eq!(diff.total_entries(), 1);
        assert!(diff.entries.get("gpt-4o").unwrap().is_tombstone());
    }

    #[test]
    fn test_diff_uses_same_tiebreaker_as_merge() {
        let mut local = SyncableMapping::new();
        local
            .entries
            .insert("gpt-4o".to_string(), MappingEntry::tombstone_at(1000));

        let mut remote = SyncableMapping::new();
        remote.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 1000),
        );

        let diff = local.diff_newer_than(&remote);

        assert_eq!(diff.total_entries(), 1);
    }

    #[test]
    fn test_identical_entries_no_update() {
        let mut local = SyncableMapping::new();
        local.entries.insert(
            "gpt-4o".to_string(),
            MappingEntry::with_timestamp("gemini-3-pro", 1000),
        );

        let remote = local.clone();
        let updated = local.merge_lww(&remote);

        assert_eq!(updated, 0);
    }
}
