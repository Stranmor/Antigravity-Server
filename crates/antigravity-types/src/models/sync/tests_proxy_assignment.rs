use super::proxy_assignment::{ProxyAssignment, SyncableProxyAssignments};

#[test]
fn test_lww_merge_remote_newer() {
    let mut local = SyncableProxyAssignments::new();
    local.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://old:1080".to_string()), 1000),
    );

    let mut remote = SyncableProxyAssignments::new();
    remote.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://new:1080".to_string()), 2000),
    );

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 1);
    assert_eq!(
        local.entries.get("a@test.com").unwrap().proxy_url.as_deref(),
        Some("socks5://new:1080")
    );
}

#[test]
fn test_lww_merge_local_newer() {
    let mut local = SyncableProxyAssignments::new();
    local.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://local:1080".to_string()), 3000),
    );

    let mut remote = SyncableProxyAssignments::new();
    remote.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://remote:1080".to_string()), 2000),
    );

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 0);
    assert_eq!(
        local.entries.get("a@test.com").unwrap().proxy_url.as_deref(),
        Some("socks5://local:1080")
    );
}

#[test]
fn test_lww_merge_new_keys() {
    let mut local = SyncableProxyAssignments::new();
    local.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://a:1080".to_string()), 1000),
    );

    let mut remote = SyncableProxyAssignments::new();
    remote.entries.insert(
        "b@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://b:1080".to_string()), 2000),
    );

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 1);
    assert_eq!(local.len(), 2);
}

#[test]
fn test_removal_propagates_via_merge() {
    let mut local = SyncableProxyAssignments::new();
    local.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://old:1080".to_string()), 1000),
    );

    let mut remote = SyncableProxyAssignments::new();
    remote.entries.insert("a@test.com".to_string(), ProxyAssignment::with_timestamp(None, 2000));

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 1);
    assert!(local.entries.get("a@test.com").unwrap().proxy_url.is_none());
}

#[test]
fn test_tie_removal_wins() {
    let mut local = SyncableProxyAssignments::new();
    local.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://x:1080".to_string()), 1000),
    );

    let mut remote = SyncableProxyAssignments::new();
    remote.entries.insert("a@test.com".to_string(), ProxyAssignment::with_timestamp(None, 1000));

    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 1);
    assert!(local.entries.get("a@test.com").unwrap().proxy_url.is_none());
}

#[test]
fn test_diff_newer_than() {
    let mut local = SyncableProxyAssignments::new();
    local.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://a:1080".to_string()), 3000),
    );
    local.entries.insert(
        "b@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://b:1080".to_string()), 1000),
    );
    local.entries.insert(
        "c@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://c:1080".to_string()), 5000),
    );

    let mut remote = SyncableProxyAssignments::new();
    remote.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://a-old:1080".to_string()), 2000),
    );
    remote.entries.insert(
        "b@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://b-new:1080".to_string()), 2000),
    );

    let diff = local.diff_newer_than(&remote);

    assert_eq!(diff.len(), 2);
    assert!(diff.entries.contains_key("a@test.com"));
    assert!(diff.entries.contains_key("c@test.com"));
    assert!(!diff.entries.contains_key("b@test.com"));
}

#[test]
fn test_active_count() {
    let mut assignments = SyncableProxyAssignments::new();
    assignments.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://a:1080".to_string()), 1000),
    );
    assignments
        .entries
        .insert("b@test.com".to_string(), ProxyAssignment::with_timestamp(None, 1000));
    assignments.entries.insert(
        "c@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("http://c:8080".to_string()), 1000),
    );

    assert_eq!(assignments.active_count(), 2);
    assert_eq!(assignments.len(), 3);
}

#[test]
fn test_set_monotonic_timestamps() {
    let mut assignments = SyncableProxyAssignments::new();
    assignments.set("a@test.com", Some("socks5://v1:1080".to_string()));
    let ts1 = assignments.entries.get("a@test.com").unwrap().updated_at;

    assignments.set("a@test.com", Some("socks5://v2:1080".to_string()));
    let ts2 = assignments.entries.get("a@test.com").unwrap().updated_at;

    assert!(ts2 > ts1);
}

#[test]
fn test_identical_entries_no_update() {
    let mut local = SyncableProxyAssignments::new();
    local.entries.insert(
        "a@test.com".to_string(),
        ProxyAssignment::with_timestamp(Some("socks5://x:1080".to_string()), 1000),
    );

    let remote = local.clone();
    let updated = local.merge_lww(&remote);

    assert_eq!(updated, 0);
}
