use super::super::TokenManager;
use crate::proxy::rate_limit::RateLimitReason;
use std::time::{Duration, SystemTime};

fn create_test_manager() -> TokenManager {
    let unique_id = uuid::Uuid::new_v4();
    TokenManager::new(std::env::temp_dir().join(format!("antigravity_test_{unique_id}")))
}

#[test]
fn test_mark_rate_limited_sets_account_limit() {
    let manager = create_test_manager();
    let account = "acct_1";

    assert!(!manager.is_rate_limited(account));
    manager.mark_rate_limited(account, 429, Some("30"), "");
    assert!(manager.is_rate_limited(account));
}

#[test]
fn test_model_specific_rate_limit_does_not_block_other_models() {
    let manager = create_test_manager();
    let account = "acct_2";

    manager.mark_rate_limited_with_model(
        account,
        429,
        Some("30"),
        "",
        Some("gemini-3-pro".to_string()),
    );

    assert!(manager.is_rate_limited_for_model(account, "gemini-3-pro"));
    // Different model should NOT be blocked by model-specific limit
    assert!(!manager.is_rate_limited_for_model(account, "gemini-3-flash"));
}

#[test]
fn test_account_level_limit_blocks_all_models() {
    let manager = create_test_manager();
    let account = "acct_3";

    // Set account-level rate limit (no model)
    manager.mark_rate_limited(account, 429, Some("30"), "");

    // Account-level limit blocks ALL models
    assert!(manager.is_rate_limited_for_model(account, "gemini-3-pro"));
    assert!(manager.is_rate_limited_for_model(account, "gemini-3-flash"));
}

#[test]
fn test_clear_rate_limit_removes_account_limit() {
    let manager = create_test_manager();
    let account = "acct_4";

    manager.mark_rate_limited(account, 429, Some("60"), "");
    assert!(manager.is_rate_limited(account));

    let cleared = manager.clear_rate_limit(account);
    assert!(cleared);
    assert!(!manager.is_rate_limited(account));
}

#[test]
fn test_clear_nonexistent_rate_limit_returns_false() {
    let manager = create_test_manager();
    let cleared = manager.clear_rate_limit("nonexistent_account");
    assert!(!cleared);
}

#[test]
fn test_clear_all_rate_limits() {
    let manager = create_test_manager();

    manager.mark_rate_limited("acct_a", 429, Some("60"), "");
    manager.mark_rate_limited("acct_b", 429, Some("60"), "");
    assert!(manager.is_rate_limited("acct_a"));
    assert!(manager.is_rate_limited("acct_b"));

    manager.clear_all_rate_limits();
    assert!(!manager.is_rate_limited("acct_a"));
    assert!(!manager.is_rate_limited("acct_b"));
}

#[test]
fn test_mark_success_clears_rate_limit() {
    let manager = create_test_manager();
    let account = "acct_5";

    manager.mark_rate_limited(account, 429, Some("60"), "");
    assert!(manager.is_rate_limited(account));

    manager.mark_account_success(account);
    assert!(!manager.is_rate_limited(account));
}

#[test]
fn test_cleanup_expired_rate_limits() {
    let manager = create_test_manager();
    let tracker = manager.rate_limit_tracker();

    // Insert an already-expired rate limit directly via tracker
    let expired_time = SystemTime::now() - Duration::from_secs(10);
    tracker.set_lockout_until(
        "expired_acct",
        expired_time,
        RateLimitReason::RateLimitExceeded,
        None,
    );

    // Insert a still-active rate limit
    let future_time = SystemTime::now() + Duration::from_secs(300);
    tracker.set_lockout_until("active_acct", future_time, RateLimitReason::RateLimitExceeded, None);

    let cleaned = manager.cleanup_expired_rate_limits();
    assert_eq!(cleaned, 1);

    // Expired one should be gone, active one should remain
    assert!(!manager.is_rate_limited("expired_acct"));
    assert!(manager.is_rate_limited("active_acct"));
}

#[test]
fn test_model_capacity_exhausted_not_tracked() {
    let manager = create_test_manager();
    let account = "acct_6";

    // ModelCapacityExhausted errors should NOT create rate limit entries
    let body = r#"{"error":{"details":[{"reason":"MODEL_CAPACITY_EXHAUSTED"}]}}"#;
    manager.mark_rate_limited(account, 429, None, body);

    // Should NOT be rate limited — handler retries instead
    assert!(!manager.is_rate_limited(account));
}

#[test]
fn test_rate_limit_tracker_get_for_model() {
    let manager = create_test_manager();
    let account = "acct_7";
    let model = "gemini-3-pro";

    manager.mark_rate_limited_with_model(account, 429, Some("45"), "", Some(model.to_string()));

    let info = manager.rate_limit_tracker().get_for_model(account, model);
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.model, Some(model.to_string()));
    assert_eq!(info.retry_after_sec, 45);
}

#[test]
fn test_adaptive_model_lockout_progression() {
    let tracker = crate::proxy::rate_limit::RateLimitTracker::new();
    let account = "acct_adaptive";
    let model = "gemini-3-pro";

    // First failure: 5s
    let secs = tracker.set_adaptive_model_lockout(account, model);
    assert_eq!(secs, 5);

    // Second failure: 15s
    let secs = tracker.set_adaptive_model_lockout(account, model);
    assert_eq!(secs, 15);

    // Third failure: 30s
    let secs = tracker.set_adaptive_model_lockout(account, model);
    assert_eq!(secs, 30);

    // Fourth+ failure: 60s (max)
    let secs = tracker.set_adaptive_model_lockout(account, model);
    assert_eq!(secs, 60);

    // Fifth failure: still 60s
    let secs = tracker.set_adaptive_model_lockout(account, model);
    assert_eq!(secs, 60);
}

#[test]
fn test_model_success_resets_model_lockout() {
    let tracker = crate::proxy::rate_limit::RateLimitTracker::new();
    let account = "acct_reset";
    let model = "gemini-3-pro";

    // Set a model lockout
    let future = SystemTime::now() + Duration::from_secs(60);
    tracker.set_model_lockout(account, model, future, RateLimitReason::RateLimitExceeded);
    assert!(tracker.is_rate_limited_for_model(account, model));

    // Mark model success should clear it
    tracker.mark_model_success(account, model);
    assert!(!tracker.is_rate_limited_for_model(account, model));
}

#[test]
fn test_mark_success_does_not_clear_model_specific_limits() {
    let manager = create_test_manager();
    let account = "acct_model_persist";

    let tracker = manager.rate_limit_tracker();
    let future = SystemTime::now() + Duration::from_secs(60);
    tracker.set_model_lockout(account, "gemini-3-pro", future, RateLimitReason::RateLimitExceeded);

    manager.mark_account_success(account);

    assert!(!manager.is_rate_limited(account));
    assert!(manager.is_rate_limited_for_model(account, "gemini-3-pro"));
}

#[test]
fn test_status_500_creates_rate_limit() {
    let manager = create_test_manager();
    let account = "acct_500";

    manager.mark_rate_limited(account, 500, None, "Internal Server Error");
    assert!(manager.is_rate_limited(account));
}

#[test]
fn test_non_error_status_does_not_create_rate_limit() {
    let manager = create_test_manager();
    let account = "acct_200";

    manager.mark_rate_limited(account, 200, None, "OK");
    assert!(!manager.is_rate_limited(account));

    manager.mark_rate_limited(account, 403, None, "Forbidden");
    assert!(!manager.is_rate_limited(account));
}
