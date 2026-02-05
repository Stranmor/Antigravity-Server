use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

/// Single model mapping entry with timestamp for LWW merge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// Creates a new mapping entry with current timestamp.
    pub fn new(target: impl Into<String>) -> Self {
        Self { target: target.into(), updated_at: current_timestamp_ms(), deleted: false }
    }

    /// Creates a mapping entry with a specific timestamp.
    pub fn with_timestamp(target: impl Into<String>, updated_at: i64) -> Self {
        Self { target: target.into(), updated_at, deleted: false }
    }

    /// Creates a tombstone entry at a specific timestamp.
    pub const fn tombstone_at(updated_at: i64) -> Self {
        Self { target: String::new(), updated_at, deleted: true }
    }

    /// Creates a tombstone entry with current timestamp.
    pub fn tombstone() -> Self {
        Self::tombstone_at(current_timestamp_ms())
    }

    /// Returns true if this entry is a tombstone (deleted).
    pub const fn is_tombstone(&self) -> bool {
        self.deleted
    }

    /// LWW comparison: returns true if self should win over other.
    /// Order: higher timestamp wins, on tie: tombstone wins, on tie: higher target wins.
    pub(crate) fn wins_over(&self, other: &Self) -> bool {
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SyncableMapping {
    /// Map of source model names to their mapping entries.
    pub entries: BTreeMap<String, MappingEntry>,
    /// Optional instance identifier for sync tracking.
    #[serde(default)]
    pub instance_id: Option<String>,
}

impl SyncableMapping {
    /// Creates an empty syncable mapping.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a syncable mapping from a simple key-value map.
    pub fn from_simple_map(map: HashMap<String, String>) -> Self {
        let entries = map.into_iter().map(|(k, v)| (k, MappingEntry::new(v))).collect();
        Self { entries, instance_id: None }
    }

    /// Converts to a simple key-value map, excluding tombstones.
    pub fn to_simple_map(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .filter(|&(_, v)| !v.deleted)
            .map(|(k, v)| (k.clone(), v.target.clone()))
            .collect()
    }

    /// Sets a mapping from source to target model.
    pub fn set(&mut self, source: impl Into<String>, target: impl Into<String>) {
        let source = source.into();
        let now = current_timestamp_ms();
        let ts = self.entries.get(&source).map_or(now, |e| now.max(e.updated_at.saturating_add(1)));
        drop(self.entries.insert(
            source,
            MappingEntry { target: target.into(), updated_at: ts, deleted: false },
        ));
    }

    /// Removes a mapping by inserting a tombstone.
    pub fn remove(&mut self, source: &str) {
        let now = current_timestamp_ms();
        let ts = self.entries.get(source).map_or(now, |e| now.max(e.updated_at.saturating_add(1)));
        drop(self.entries.insert(
            source.to_string(),
            MappingEntry { target: String::new(), updated_at: ts, deleted: true },
        ));
    }

    /// Gets the target model for a source, if not deleted.
    pub fn get(&self, source: &str) -> Option<&str> {
        self.entries.get(source).filter(|e| !e.deleted).map(|e| e.target.as_str())
    }

    /// Merges remote entries using LWW. Returns count of updated entries.
    pub fn merge_lww(&mut self, remote: &Self) -> usize {
        let mut updated = 0_usize;

        for (key, remote_entry) in &remote.entries {
            let should_update =
                self.entries.get(key).is_none_or(|local_entry| remote_entry.wins_over(local_entry));

            if should_update {
                drop(self.entries.insert(key.clone(), remote_entry.clone()));
                updated = updated.saturating_add(1);
            }
        }

        updated
    }

    /// Returns entries that are newer than the remote's version.
    #[must_use]
    pub fn diff_newer_than(&self, remote: &Self) -> Self {
        let entries: BTreeMap<String, MappingEntry> = self
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

    /// Returns count of active (non-deleted) mappings.
    pub fn len(&self) -> usize {
        self.entries.values().filter(|e| !e.deleted).count()
    }

    /// Returns true if no active mappings exist.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns total entry count including tombstones.
    pub fn total_entries(&self) -> usize {
        self.entries.len()
    }
}

/// Returns current timestamp in milliseconds since UNIX epoch.
/// Returns 0 if system clock is before UNIX epoch (should never happen).
#[allow(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "u128 millis won't exceed i64::MAX until year 292 million"
)]
pub fn current_timestamp_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}
