use super::mapping::{MappingEntry, SyncableMapping};

#[test]
fn test_lww_tie_breaker_lexicographic() {
    let mut local = SyncableMapping::new();
    local.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("aaa-model", 1000));

    let mut remote = SyncableMapping::new();
    remote.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("zzz-model", 1000));

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 1);
    assert_eq!(local.get("gpt-4o"), Some("zzz-model"));
}

#[test]
fn test_lww_tie_breaker_local_wins_if_greater() {
    let mut local = SyncableMapping::new();
    local.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("zzz-model", 1000));

    let mut remote = SyncableMapping::new();
    remote.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("aaa-model", 1000));

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 0);
    assert_eq!(local.get("gpt-4o"), Some("zzz-model"));
}

#[test]
fn test_tombstone_wins_over_live_on_timestamp_tie() {
    let mut local = SyncableMapping::new();
    local.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("gemini-3-pro", 1000));

    let mut remote = SyncableMapping::new();
    remote.entries.insert("gpt-4o".to_string(), MappingEntry::tombstone_at(1000));

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 1);
    assert!(local.entries.get("gpt-4o").unwrap().is_tombstone());
}

#[test]
fn test_live_does_not_override_tombstone_on_timestamp_tie() {
    let mut local = SyncableMapping::new();
    local.entries.insert("gpt-4o".to_string(), MappingEntry::tombstone_at(1000));

    let mut remote = SyncableMapping::new();
    remote.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("gemini-3-pro", 1000));

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
    local.entries.insert("gpt-4o".to_string(), MappingEntry::tombstone_at(3000));

    let mut remote = SyncableMapping::new();
    remote.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("gemini-3-pro", 2000));

    let diff = local.diff_newer_than(&remote);

    assert_eq!(diff.total_entries(), 1);
    assert!(diff.entries.get("gpt-4o").unwrap().is_tombstone());
}

#[test]
fn test_diff_uses_same_tiebreaker_as_merge() {
    let mut local = SyncableMapping::new();
    local.entries.insert("gpt-4o".to_string(), MappingEntry::tombstone_at(1000));

    let mut remote = SyncableMapping::new();
    remote.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("gemini-3-pro", 1000));

    let diff = local.diff_newer_than(&remote);

    assert_eq!(diff.total_entries(), 1);
}

#[test]
fn test_identical_entries_no_update() {
    let mut local = SyncableMapping::new();
    local.entries.insert("gpt-4o".to_string(), MappingEntry::with_timestamp("gemini-3-pro", 1000));

    let remote = local.clone();
    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 0);
}

#[test]
fn test_monotonic_timestamp_on_rapid_updates() {
    let mut mapping = SyncableMapping::new();
    mapping.set("gpt-4o", "target-1");
    let ts1 = mapping.entries.get("gpt-4o").unwrap().updated_at;

    mapping.set("gpt-4o", "target-2");
    let ts2 = mapping.entries.get("gpt-4o").unwrap().updated_at;

    mapping.set("gpt-4o", "target-3");
    let ts3 = mapping.entries.get("gpt-4o").unwrap().updated_at;

    assert!(ts2 > ts1);
    assert!(ts3 > ts2);
}

#[test]
fn test_monotonic_timestamp_remove_after_set() {
    let mut mapping = SyncableMapping::new();
    mapping.set("gpt-4o", "target");
    let ts_set = mapping.entries.get("gpt-4o").unwrap().updated_at;

    mapping.remove("gpt-4o");
    let ts_remove = mapping.entries.get("gpt-4o").unwrap().updated_at;

    assert!(ts_remove > ts_set);
}
