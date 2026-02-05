use super::super::file_utils::truncate_reason;
use super::super::proxy_token::ProxyToken;
use super::super::TokenManager;
use crate::proxy::active_request_guard::ActiveRequestGuard;
use crate::proxy::adaptive_limit::AdaptiveLimitManager;
use crate::proxy::SmartRoutingConfig;
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

fn create_test_manager() -> TokenManager {
    let unique_id = uuid::Uuid::new_v4();
    TokenManager::new(PathBuf::from(format!("/tmp/antigravity_test_{unique_id}")))
}

fn create_test_token(tier: Option<&str>) -> ProxyToken {
    ProxyToken {
        account_id: "test".to_string(),
        access_token: "token".to_string(),
        refresh_token: "refresh".to_string(),
        expires_in: 3600,
        timestamp: 0,
        email: "test@example.com".to_string(),
        account_path: PathBuf::from("/tmp"),
        project_id: None,
        subscription_tier: tier.map(String::from),
        remaining_quota: Some(100),
        protected_models: HashSet::new(),
        health_score: 1.0,
    }
}

#[test]
fn test_new_manager_is_empty() {
    let manager = create_test_manager();
    assert!(manager.is_empty());
    assert_eq!(manager.len(), 0);
}

#[test]
fn test_rate_limit_integration() {
    let manager = create_test_manager();
    let account_id = "test_account_123";

    assert!(!manager.is_rate_limited(account_id));

    manager.mark_rate_limited(account_id, 429, Some("60"), "");
    assert!(manager.is_rate_limited(account_id));

    manager.mark_account_success(account_id);
    assert!(!manager.is_rate_limited(account_id));
}

#[test]
fn test_rate_limit_with_model() {
    let manager = create_test_manager();
    let account_id = "test_account_456";

    manager.mark_rate_limited_with_model(
        account_id,
        429,
        Some("30"),
        "",
        Some("gemini-pro".to_string()),
    );

    // Model-specific rate limit only blocks that model, not the whole account
    assert!(manager.is_rate_limited_for_model(account_id, "gemini-pro"));
    let info = manager.rate_limit_tracker().get_for_model(account_id, "gemini-pro");
    assert!(info.is_some());
    assert_eq!(info.unwrap().model, Some("gemini-pro".to_string()));
}

#[tokio::test]
async fn test_preferred_account_mode() {
    let manager = create_test_manager();

    assert!(manager.get_preferred_account().await.is_none());

    manager.set_preferred_account(Some("fixed_account".to_string())).await;
    assert_eq!(manager.get_preferred_account().await, Some("fixed_account".to_string()));

    manager.set_preferred_account(None).await;
    assert!(manager.get_preferred_account().await.is_none());
}

#[tokio::test]
async fn test_routing_config_update() {
    let manager = create_test_manager();

    let initial = manager.get_routing_config().await;
    assert!(initial.enable_session_affinity);
    assert_eq!(initial.max_concurrent_per_account, 5);

    let new_config = SmartRoutingConfig {
        enable_session_affinity: false,
        max_concurrent_per_account: 5,
        ..Default::default()
    };
    manager.update_routing_config(new_config).await;

    let updated = manager.get_routing_config().await;
    assert!(!updated.enable_session_affinity);
    assert_eq!(updated.max_concurrent_per_account, 5);
}

#[test]
fn test_active_requests_increment_decrement() {
    let manager = create_test_manager();

    assert_eq!(manager.get_active_requests("account_a"), 0);

    let count = manager.increment_active_requests("account_a");
    assert_eq!(count, 1);
    assert_eq!(manager.get_active_requests("account_a"), 1);

    let count = manager.increment_active_requests("account_a");
    assert_eq!(count, 2);

    manager.decrement_active_requests("account_a");
    assert_eq!(manager.get_active_requests("account_a"), 1);

    manager.decrement_active_requests("account_a");
    assert_eq!(manager.get_active_requests("account_a"), 0);
}

#[test]
fn test_active_requests_underflow_protection() {
    let manager = create_test_manager();

    manager.decrement_active_requests("nonexistent");
    assert_eq!(manager.get_active_requests("nonexistent"), 0);

    manager.increment_active_requests("account_b");
    manager.decrement_active_requests("account_b");
    manager.decrement_active_requests("account_b");
    assert_eq!(manager.get_active_requests("account_b"), 0);
}

#[test]
fn test_active_request_guard_try_new_respects_limit() {
    let active_requests = Arc::new(DashMap::new());
    let max_concurrent = 3;

    let guard1 = ActiveRequestGuard::try_new(
        Arc::clone(&active_requests),
        "account_a".to_string(),
        max_concurrent,
    );
    assert!(guard1.is_some());

    let guard2 = ActiveRequestGuard::try_new(
        Arc::clone(&active_requests),
        "account_a".to_string(),
        max_concurrent,
    );
    assert!(guard2.is_some());

    let guard3 = ActiveRequestGuard::try_new(
        Arc::clone(&active_requests),
        "account_a".to_string(),
        max_concurrent,
    );
    assert!(guard3.is_some());

    let guard4 = ActiveRequestGuard::try_new(
        Arc::clone(&active_requests),
        "account_a".to_string(),
        max_concurrent,
    );
    assert!(guard4.is_none());

    drop(guard1);

    let guard5 = ActiveRequestGuard::try_new(
        Arc::clone(&active_requests),
        "account_a".to_string(),
        max_concurrent,
    );
    assert!(guard5.is_some());
}

#[test]
fn test_session_bindings() {
    let manager = create_test_manager();

    manager.session_accounts.insert("session_1".to_string(), "account_a".to_string());
    manager.session_accounts.insert("session_2".to_string(), "account_b".to_string());

    assert_eq!(manager.session_accounts.len(), 2);

    manager.clear_session_binding("session_1");
    assert_eq!(manager.session_accounts.len(), 1);

    manager.clear_all_sessions();
    assert_eq!(manager.session_accounts.len(), 0);
}

#[test]
fn test_truncate_reason() {
    assert_eq!(truncate_reason("short", 10), "short");
    assert_eq!(truncate_reason("this is a very long reason", 10), "this is a …");
    assert_eq!(truncate_reason("exact10chr", 10), "exact10chr");
}

#[tokio::test]
async fn test_adaptive_limits_injection() {
    let manager = create_test_manager();

    {
        let guard = manager.adaptive_limits.read().await;
        assert!(guard.is_none());
    }

    let limits = Arc::new(AdaptiveLimitManager::new(0.8, Default::default()));
    manager.set_adaptive_limits(limits).await;

    {
        let guard = manager.adaptive_limits.read().await;
        assert!(guard.is_some());
    }
}

#[test]
fn test_tier_weight_scoring() {
    let tier_weight = |tier: Option<&str>| -> f32 { create_test_token(tier).tier_weight() };

    let score = |tier: Option<&str>, active: u32| -> f32 {
        let w = tier_weight(tier);
        w + (active as f32) * w
    };

    assert!((tier_weight(Some("ws-ai-ultra-business-tier")) - 0.1).abs() < f32::EPSILON);
    assert!((tier_weight(Some("g1-ultra-tier")) - 0.25).abs() < f32::EPSILON);
    assert!((tier_weight(Some("g1-pro-tier")) - 0.8).abs() < f32::EPSILON);
    assert!((tier_weight(Some("free-tier")) - 1.0).abs() < f32::EPSILON);
    assert!((tier_weight(None) - 1.25).abs() < f32::EPSILON);

    assert!(score(Some("ultra-business"), 0) < score(Some("ultra"), 0));
    assert!(score(Some("ultra"), 0) < score(Some("free"), 0));
    assert!(score(Some("ultra"), 0) < score(Some("pro"), 0));
    assert!(score(Some("ultra"), 1) < score(Some("free"), 1));
    assert!(score(Some("ultra"), 4) < score(Some("free"), 1));
}
