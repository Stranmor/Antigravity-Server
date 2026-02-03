use super::mapping::{current_timestamp_ms, MappingEntry, SyncableMapping};
use std::collections::HashMap;

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
        MappingEntry::with_timestamp("gemini-3-flash", 2000),
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
        MappingEntry::with_timestamp("gemini-3-pro", 3000),
    );

    let mut remote = SyncableMapping::new();
    remote.entries.insert(
        "gpt-4o".to_string(),
        MappingEntry::with_timestamp("gemini-3-flash", 2000),
    );

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 0);
    assert_eq!(local.get("gpt-4o"), Some("gemini-3-pro"));
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
        MappingEntry::with_timestamp("gemini-3-pro", 3000),
    );
    local.entries.insert(
        "claude".to_string(),
        MappingEntry::with_timestamp("claude-opus", 1000),
    );
    local.entries.insert(
        "new-model".to_string(),
        MappingEntry::with_timestamp("target", 5000),
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

    assert_eq!(diff.len(), 2);
    assert!(diff.entries.contains_key("gpt-4o"));
    assert!(diff.entries.contains_key("new-model"));
    assert!(!diff.entries.contains_key("claude"));
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
fn test_current_timestamp_ms_returns_positive() {
    let ts = current_timestamp_ms();
    assert!(ts > 0);
    assert!(ts > 1700000000000);
}
