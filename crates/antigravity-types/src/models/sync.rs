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
}

impl MappingEntry {
    /// Create new entry with current timestamp.
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            updated_at: current_timestamp_ms(),
        }
    }

    /// Create entry with specific timestamp (for testing or import).
    pub fn with_timestamp(target: impl Into<String>, updated_at: i64) -> Self {
        Self {
            target: target.into(),
            updated_at,
        }
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

    /// Convert to simple HashMap (for runtime use).
    pub fn to_simple_map(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .map(|(k, v)| (k.clone(), v.target.clone()))
            .collect()
    }

    /// Set or update a mapping entry (updates timestamp).
    pub fn set(&mut self, source: impl Into<String>, target: impl Into<String>) {
        self.entries
            .insert(source.into(), MappingEntry::new(target));
    }

    /// Remove a mapping entry.
    pub fn remove(&mut self, source: &str) -> Option<MappingEntry> {
        self.entries.remove(source)
    }

    /// Get target for a source model.
    pub fn get(&self, source: &str) -> Option<&str> {
        self.entries.get(source).map(|e| e.target.as_str())
    }

    /// Merge remote mappings using LWW strategy.
    ///
    /// For each key:
    /// - If only in local -> keep local
    /// - If only in remote -> add from remote
    /// - If in both -> keep the one with higher timestamp
    ///
    /// Returns number of entries updated from remote.
    pub fn merge_lww(&mut self, remote: &SyncableMapping) -> usize {
        let mut updated = 0;

        for (key, remote_entry) in &remote.entries {
            let should_update = match self.entries.get(key) {
                Some(local_entry) => remote_entry.updated_at > local_entry.updated_at,
                None => true,
            };

            if should_update {
                self.entries.insert(key.clone(), remote_entry.clone());
                updated += 1;
            }
        }

        updated
    }

    /// Compute diff: entries that are newer locally than in remote.
    ///
    /// Used to send only changed entries to remote.
    pub fn diff_newer_than(&self, remote: &SyncableMapping) -> SyncableMapping {
        let entries: HashMap<_, _> = self
            .entries
            .iter()
            .filter(|(key, local_entry)| {
                remote
                    .entries
                    .get(*key)
                    .is_none_or(|remote_entry| local_entry.updated_at > remote_entry.updated_at)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        SyncableMapping {
            entries,
            instance_id: self.instance_id.clone(),
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
}
