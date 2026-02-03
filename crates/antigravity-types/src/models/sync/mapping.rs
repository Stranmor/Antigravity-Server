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
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            updated_at: current_timestamp_ms(),
            deleted: false,
        }
    }

    pub fn with_timestamp(target: impl Into<String>, updated_at: i64) -> Self {
        Self {
            target: target.into(),
            updated_at,
            deleted: false,
        }
    }

    pub fn tombstone_at(updated_at: i64) -> Self {
        Self {
            target: String::new(),
            updated_at,
            deleted: true,
        }
    }

    pub fn tombstone() -> Self {
        Self::tombstone_at(current_timestamp_ms())
    }

    pub fn is_tombstone(&self) -> bool {
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SyncableMapping {
    pub entries: HashMap<String, MappingEntry>,
    #[serde(default)]
    pub instance_id: Option<String>,
}

impl SyncableMapping {
    pub fn new() -> Self {
        Self::default()
    }

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

    pub fn to_simple_map(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .filter(|(_, v)| !v.deleted)
            .map(|(k, v)| (k.clone(), v.target.clone()))
            .collect()
    }

    pub fn set(&mut self, source: impl Into<String>, target: impl Into<String>) {
        let source = source.into();
        let now = current_timestamp_ms();
        let ts = self
            .entries
            .get(&source)
            .map_or(now, |e| now.max(e.updated_at + 1));
        self.entries.insert(
            source,
            MappingEntry {
                target: target.into(),
                updated_at: ts,
                deleted: false,
            },
        );
    }

    pub fn remove(&mut self, source: &str) {
        let now = current_timestamp_ms();
        let ts = self
            .entries
            .get(source)
            .map_or(now, |e| now.max(e.updated_at + 1));
        self.entries.insert(
            source.to_string(),
            MappingEntry {
                target: String::new(),
                updated_at: ts,
                deleted: true,
            },
        );
    }

    pub fn get(&self, source: &str) -> Option<&str> {
        self.entries
            .get(source)
            .filter(|e| !e.deleted)
            .map(|e| e.target.as_str())
    }

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

    pub fn len(&self) -> usize {
        self.entries.values().filter(|e| !e.deleted).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn total_entries(&self) -> usize {
        self.entries.len()
    }
}

pub(crate) fn current_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as i64
}
