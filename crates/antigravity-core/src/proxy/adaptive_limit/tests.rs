//! Tests for adaptive rate limiting

use super::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

#[test]
fn test_aimd_reward() {
    let aimd = AIMDController::default();
    assert_eq!(aimd.reward(100), 105); // +5%
    assert_eq!(aimd.reward(1000), 1000); // Capped at max
}

#[test]
fn test_aimd_penalize() {
    let aimd = AIMDController::default();
    assert_eq!(aimd.penalize(100), 70); // ×0.7
    assert_eq!(aimd.penalize(10), 10); // Min floor
}

#[test]
fn test_probe_strategy() {
    assert_eq!(ProbeStrategy::from_usage_ratio(0.5), ProbeStrategy::None);
    assert_eq!(
        ProbeStrategy::from_usage_ratio(0.75),
        ProbeStrategy::CheapProbe
    );
    assert_eq!(
        ProbeStrategy::from_usage_ratio(0.90),
        ProbeStrategy::DelayedHedge
    );
    assert_eq!(
        ProbeStrategy::from_usage_ratio(0.99),
        ProbeStrategy::ImmediateHedge
    );
}

#[test]
fn test_tracker_usage_ratio() {
    let tracker = AdaptiveLimitTracker::new(0.85, AIMDController::default());
    assert_eq!(tracker.usage_ratio(), 0.0);

    // Simulate requests
    for _ in 0..6 {
        tracker.record_success();
    }
    // Default limit = 15, threshold = 12.75 ≈ 12
    // 6 / 12 = 0.5
    let ratio = tracker.usage_ratio();
    assert!(ratio > 0.4 && ratio < 0.6, "ratio was {}", ratio);
}

#[test]
fn test_tracker_429_contracts_limit() {
    let tracker = AdaptiveLimitTracker::new(0.85, AIMDController::default());
    let initial = tracker.confirmed_limit();

    tracker.record_429();

    assert!(tracker.confirmed_limit() < initial);
}

#[test]
fn test_tracker_expansion_after_successes() {
    let tracker = AdaptiveLimitTracker::new(0.85, AIMDController::default());

    // Set threshold low so we can exceed it
    tracker.working_threshold.store(5, Ordering::Relaxed);
    let initial = tracker.confirmed_limit();

    // Record successes above threshold
    for _ in 0..10 {
        tracker.record_success();
    }

    // Should have expanded
    assert!(tracker.confirmed_limit() > initial);
}

#[test]
fn test_persisted_with_decay() {
    let fresh = AdaptiveLimitTracker::from_persisted(100, 100, 0, 0.85, AIMDController::default());
    assert_eq!(fresh.confirmed_limit(), 100);

    let stale = AdaptiveLimitTracker::from_persisted(
        100,
        100,
        86400 * 2,
        0.85,
        AIMDController::default(), // 2 days old
    );
    assert!(stale.confirmed_limit() < 100); // Should be decayed
}

#[test]
fn test_manager_get_or_create() {
    let manager = AdaptiveLimitManager::default();
    assert!(manager.is_empty());

    let _ = manager.get_or_create("account1");
    assert_eq!(manager.len(), 1);

    let _ = manager.get_or_create("account1");
    assert_eq!(manager.len(), 1); // No duplicate

    let _ = manager.get_or_create("account2");
    assert_eq!(manager.len(), 2);
}

#[test]
fn test_probe_strategy_needs_secondary() {
    assert!(!ProbeStrategy::None.needs_secondary());
    assert!(!ProbeStrategy::CheapProbe.needs_secondary());
    assert!(ProbeStrategy::DelayedHedge.needs_secondary());
    assert!(ProbeStrategy::ImmediateHedge.needs_secondary());
}

#[test]
fn test_probe_strategy_is_fire_and_forget() {
    assert!(!ProbeStrategy::None.is_fire_and_forget());
    assert!(ProbeStrategy::CheapProbe.is_fire_and_forget());
    assert!(!ProbeStrategy::DelayedHedge.is_fire_and_forget());
    assert!(!ProbeStrategy::ImmediateHedge.is_fire_and_forget());
}

#[test]
fn test_manager_record_success_creates_tracker() {
    let manager = AdaptiveLimitManager::default();
    assert!(manager.is_empty());

    manager.record_success("new_account");
    assert_eq!(manager.len(), 1);
}

#[test]
fn test_manager_record_429_contracts() {
    let manager = AdaptiveLimitManager::default();
    let _ = manager.get_or_create("account");
    let initial = manager.get("account").unwrap().confirmed_limit();

    manager.record_429("account");
    let after = manager.get("account").unwrap().confirmed_limit();

    assert!(after < initial);
}

#[test]
fn test_manager_should_allow() {
    let manager = AdaptiveLimitManager::default();
    assert!(manager.should_allow("account"));
}

#[test]
fn test_aimd_custom_params() {
    let aimd = AIMDController {
        additive_increase: 0.10,
        multiplicative_decrease: 0.5,
        min_limit: 5,
        max_limit: 500,
    };
    assert_eq!(aimd.reward(100), 111); // 100 * 1.10 = 110.0, ceil = 111
    assert_eq!(aimd.penalize(100), 50);
    assert_eq!(aimd.penalize(8), 5);
    assert_eq!(aimd.reward(500), 500);
}

#[test]
fn test_concurrent_expansion_no_double() {
    let tracker = Arc::new(AdaptiveLimitTracker::new(0.85, AIMDController::default()));
    tracker.working_threshold.store(1, Ordering::Relaxed);
    let initial = tracker.confirmed_limit();

    let mut handles = vec![];
    for _ in 0..10 {
        let t = Arc::clone(&tracker);
        handles.push(thread::spawn(move || {
            for _ in 0..5 {
                t.record_success();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let final_limit = tracker.confirmed_limit();
    let expansion_count =
        ((final_limit as f64 / initial as f64).ln() / (1.05_f64).ln()).round() as u32;

    assert!(
        expansion_count <= 20,
        "Too many expansions: {} (limit went {} -> {})",
        expansion_count,
        initial,
        final_limit
    );
}

#[test]
fn test_concurrent_get_or_create_no_overwrite() {
    let manager = Arc::new(AdaptiveLimitManager::default());

    let mut handles = vec![];
    for _ in 0..10 {
        let m = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            let _ = m.get_or_create("shared_account");
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(manager.len(), 1);
}
