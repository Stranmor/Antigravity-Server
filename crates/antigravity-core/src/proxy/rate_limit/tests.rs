use super::*;
use std::time::SystemTime;

#[test]
fn test_parse_retry_time_minutes_seconds() {
    let body = "Rate limit exceeded. Try again in 2m 30s";
    let time = parser::parse_retry_time_from_body(body);
    assert_eq!(time, Some(150));
}

#[test]
fn test_parse_google_json_delay() {
    let body = r#"{
        "error": {
            "details": [
                {
                    "metadata": {
                        "quotaResetDelay": "42s"
                    }
                }
            ]
        }
    }"#;
    let time = parser::parse_retry_time_from_body(body);
    assert_eq!(time, Some(42));
}

#[test]
fn test_parse_retry_after_ignore_case() {
    let body = "Quota limit hit. Retry After 99 Seconds";
    let time = parser::parse_retry_time_from_body(body);
    assert_eq!(time, Some(99));
}

#[test]
fn test_get_remaining_wait() {
    let tracker = RateLimitTracker::new();
    tracker.parse_from_error("acc1", 429, Some("30"), "", None);
    let wait = tracker.get_remaining_wait("acc1");
    assert!(wait > 25 && wait <= 30);
}

#[test]
fn test_safety_buffer() {
    let tracker = RateLimitTracker::new();
    // if API return 1s，weforce设as 2s
    tracker.parse_from_error("acc1", 429, Some("1"), "", None);
    let wait = tracker.get_remaining_wait("acc1");
    // Due to time passing, it might be 1 or 2
    assert!((1..=2).contains(&wait));
}

#[test]
fn test_tpm_exhausted_is_rate_limit_exceeded() {
    let tracker = RateLimitTracker::new();
    // simulatetrue实世界  TPM error，同whencontaining "Resource exhausted"  and  "per minute"
    let body =
        "Resource has been exhausted (e.g. check quota). Quota limit 'Tokens per minute' exceeded.";
    let reason = tracker.parse_rate_limit_reason(body);
    // shouldbe识别as RateLimitExceeded，而notis QuotaExhausted
    assert_eq!(reason, RateLimitReason::RateLimitExceeded);
}

#[test]
fn test_mark_success_clears_rate_limit() {
    let tracker = RateLimitTracker::new();
    tracker.parse_from_error("acc1", 429, Some("60"), "", None);
    assert!(tracker.is_rate_limited("acc1"));
    tracker.mark_success("acc1");
    assert!(!tracker.is_rate_limited("acc1"));
}

#[test]
fn test_set_lockout_until_iso() {
    let tracker = RateLimitTracker::new();
    let future = chrono::Utc::now() + chrono::Duration::seconds(120);
    let iso_str = future.to_rfc3339();
    let result =
        tracker.set_lockout_until_iso("acc1", &iso_str, RateLimitReason::QuotaExhausted, None);
    assert!(result);
    assert!(tracker.is_rate_limited("acc1"));
    let remaining = tracker.get_remaining_wait("acc1");
    assert!((115..=125).contains(&remaining));
}

#[test]
fn test_parse_duration_string_variants() {
    assert_eq!(parser::parse_duration_string("1h30m"), Some(5400));
    assert_eq!(parser::parse_duration_string("2h1m1s"), Some(7261));
    assert_eq!(parser::parse_duration_string("5m"), Some(300));
    assert_eq!(parser::parse_duration_string("30s"), Some(30));
    assert_eq!(parser::parse_duration_string("1h"), Some(3600));
}

#[test]
fn test_cleanup_expired_removes_old_records() {
    let tracker = RateLimitTracker::new();
    let past = SystemTime::now() - Duration::from_secs(10);
    tracker.limits.insert(
        RateLimitKey::Account("expired".to_string()),
        RateLimitInfo {
            reset_time: past,
            retry_after_sec: 60,
            detected_at: past,
            reason: RateLimitReason::Unknown,
            model: None,
        },
    );
    let future = SystemTime::now() + Duration::from_secs(60);
    tracker.limits.insert(
        RateLimitKey::Account("active".to_string()),
        RateLimitInfo {
            reset_time: future,
            retry_after_sec: 60,
            detected_at: SystemTime::now(),
            reason: RateLimitReason::Unknown,
            model: None,
        },
    );
    let cleaned = tracker.cleanup_expired();
    assert_eq!(cleaned, 1);
    assert!(!tracker.limits.contains_key(&RateLimitKey::account("expired")));
    assert!(tracker.limits.contains_key(&RateLimitKey::account("active")));
}

#[test]
fn test_clear_all_removes_everything() {
    let tracker = RateLimitTracker::new();
    tracker.parse_from_error("acc1", 429, Some("60"), "", None);
    tracker.parse_from_error("acc2", 429, Some("60"), "", None);
    assert!(tracker.is_rate_limited("acc1"));
    assert!(tracker.is_rate_limited("acc2"));
    tracker.clear_all();
    assert!(!tracker.is_rate_limited("acc1"));
    assert!(!tracker.is_rate_limited("acc2"));
}

#[test]
fn test_model_level_rate_limit() {
    let tracker = RateLimitTracker::new();
    tracker.parse_from_error("acc1", 429, Some("60"), "", Some("gemini-pro".to_string()));
    assert!(tracker.is_rate_limited_for_model("acc1", "gemini-pro"));
    let info = tracker.get_for_model("acc1", "gemini-pro").expect("should have rate limit");
    assert_eq!(info.model, Some("gemini-pro".to_string()));
}
