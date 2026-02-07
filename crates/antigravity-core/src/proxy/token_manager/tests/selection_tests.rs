use super::super::proxy_token::ProxyToken;
use super::super::selection_helpers::compare_tokens_by_priority;
use super::super::TokenManager;
use crate::proxy::rate_limit::RateLimitReason;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn create_test_manager() -> TokenManager {
    let unique_id = uuid::Uuid::new_v4();
    TokenManager::new(PathBuf::from(format!("/tmp/antigravity_test_{unique_id}")))
}

fn make_token(email: &str, tier: Option<&str>, quota: Option<i32>, health: f32) -> ProxyToken {
    ProxyToken::new(
        email.to_string(),
        format!("token_{email}"),
        "refresh".to_string(),
        3600,
        chrono::Utc::now().timestamp() + 3600,
        email.to_string(),
        PathBuf::from("/tmp"),
        Some("test-project".to_string()),
        tier.map(String::from),
        quota,
        HashSet::new(),
        health,
        HashSet::new(),
    )
}

#[test]
fn test_compare_tokens_ultra_before_pro() {
    let ultra = make_token("ultra@test.com", Some("g1-ultra-tier"), Some(100), 1.0);
    let pro = make_token("pro@test.com", Some("g1-pro-tier"), Some(100), 1.0);

    let result = compare_tokens_by_priority(&ultra, &pro);
    assert_eq!(result, std::cmp::Ordering::Less);
}

#[test]
fn test_compare_tokens_pro_before_free() {
    let pro = make_token("pro@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    let free = make_token("free@test.com", Some("free-tier"), Some(100), 1.0);

    let result = compare_tokens_by_priority(&pro, &free);
    assert_eq!(result, std::cmp::Ordering::Less);
}

#[test]
fn test_compare_tokens_same_tier_higher_quota_first() {
    let high_quota = make_token("high@test.com", Some("g1-pro-tier"), Some(500), 1.0);
    let low_quota = make_token("low@test.com", Some("g1-pro-tier"), Some(50), 1.0);

    let result = compare_tokens_by_priority(&high_quota, &low_quota);
    assert_eq!(result, std::cmp::Ordering::Less);
}

#[test]
fn test_compare_tokens_same_tier_same_quota_higher_health_first() {
    let healthy = make_token("healthy@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    let degraded = make_token("degraded@test.com", Some("g1-pro-tier"), Some(100), 0.5);

    let result = compare_tokens_by_priority(&healthy, &degraded);
    assert_eq!(result, std::cmp::Ordering::Less);
}

#[test]
fn test_compare_tokens_sorting_produces_correct_order() {
    let ultra_biz = make_token("ub@test.com", Some("ws-ai-ultra-business-tier"), Some(100), 1.0);
    let ultra = make_token("u@test.com", Some("g1-ultra-tier"), Some(100), 1.0);
    let pro = make_token("p@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    let free = make_token("f@test.com", Some("free-tier"), Some(100), 1.0);
    let unknown = make_token("x@test.com", None, Some(100), 1.0);

    let mut tokens = [free.clone(), unknown.clone(), ultra.clone(), pro.clone(), ultra_biz.clone()];
    tokens.sort_by(compare_tokens_by_priority);

    assert_eq!(tokens[0].email, "ub@test.com");
    assert_eq!(tokens[1].email, "u@test.com");
    assert_eq!(tokens[2].email, "p@test.com");
    assert_eq!(tokens[3].email, "f@test.com");
    assert_eq!(tokens[4].email, "x@test.com");
}

#[tokio::test]
async fn test_get_token_empty_pool_returns_error() {
    let manager = create_test_manager();
    let result = manager.get_token("default", false, None, "gemini-3-pro").await;
    match result {
        Err(e) => assert!(e.contains("empty"), "Expected 'empty' in error: {e}"),
        Ok(_) => panic!("Expected error for empty pool"),
    }
}

#[tokio::test]
async fn test_get_token_forced_nonexistent_account() {
    let manager = create_test_manager();
    let result = manager.get_token_forced("nobody@test.com", "gemini-3-pro").await;
    match result {
        Err(e) => assert!(e.contains("not found"), "Expected 'not found' in error: {e}"),
        Ok(_) => panic!("Expected error for nonexistent account"),
    }
}

#[tokio::test]
async fn test_get_token_forced_rate_limited_account() {
    let manager = create_test_manager();
    let token = make_token("limited@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    manager.tokens.insert(token.account_id.clone(), token);

    manager.mark_rate_limited("limited@test.com", 429, Some("60"), "");

    let result = manager.get_token_forced("limited@test.com", "gemini-3-pro").await;
    match result {
        Err(e) => assert!(e.contains("rate limited"), "Expected 'rate limited' in error: {e}"),
        Ok(_) => panic!("Expected error for rate-limited account"),
    }
}

#[tokio::test]
async fn test_get_token_with_exclusions_skips_excluded() {
    let manager = create_test_manager();

    let token_a = make_token("a@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    let token_b = make_token("b@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    manager.tokens.insert(token_a.account_id.clone(), token_a);
    manager.tokens.insert(token_b.account_id.clone(), token_b);

    let mut excluded = HashSet::new();
    excluded.insert("a@test.com".to_string());

    let result = manager
        .get_token_with_exclusions("default", false, None, "gemini-3-pro", Some(&excluded))
        .await;

    match result {
        Ok((_token, _project, email, _guard)) => {
            assert_eq!(email, "b@test.com");
        },
        Err(e) => {
            panic!("Expected success but got error: {e}");
        },
    }
}

#[test]
fn test_is_model_protected_with_protected_models() {
    let manager = create_test_manager();

    let mut protected = HashSet::new();
    protected.insert("gemini-3-pro".to_string());

    let token = ProxyToken::new(
        "protected@test.com".to_string(),
        "token".to_string(),
        "refresh".to_string(),
        3600,
        0,
        "protected@test.com".to_string(),
        PathBuf::from("/tmp"),
        None,
        Some("g1-pro-tier".to_string()),
        Some(100),
        protected,
        1.0,
        HashSet::new(),
    );
    manager.tokens.insert(token.account_id.clone(), token);

    assert!(manager.is_model_protected("protected@test.com", "gemini-3-pro"));
    assert!(!manager.is_model_protected("protected@test.com", "gemini-3-flash"));
    assert!(!manager.is_model_protected("nonexistent", "gemini-3-pro"));
}

#[tokio::test]
async fn test_get_token_timeout_on_all_rate_limited() {
    let manager = create_test_manager();

    let token = make_token("only@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    manager.tokens.insert(token.account_id.clone(), token);

    let tracker = manager.rate_limit_tracker();
    let future = SystemTime::now() + Duration::from_secs(300);
    tracker.set_lockout_until("only@test.com", future, RateLimitReason::QuotaExhausted, None);

    let result = manager.get_token("default", false, None, "gemini-3-pro").await;
    match result {
        Err(e) => assert!(
            e.contains("limited") || e.contains("wait"),
            "Expected rate-limit error, got: {e}"
        ),
        Ok(_) => panic!("Expected error when all accounts rate-limited"),
    }
}

#[test]
fn test_compare_tokens_none_quota_sorted_after_some_quota() {
    let with_quota = make_token("has@test.com", Some("g1-pro-tier"), Some(50), 1.0);
    let no_quota = make_token("none@test.com", Some("g1-pro-tier"), None, 1.0);

    let result = compare_tokens_by_priority(&with_quota, &no_quota);
    assert_eq!(result, std::cmp::Ordering::Less);
}

#[tokio::test]
async fn test_get_token_forced_case_insensitive_email() {
    let manager = create_test_manager();
    let token = make_token("User@Example.COM", Some("g1-pro-tier"), Some(100), 1.0);
    manager.tokens.insert(token.account_id.clone(), token);

    let result = manager.get_token_forced("user@example.com", "gemini-3-pro").await;
    match result {
        Ok((_tok, _proj, email, _guard)) => {
            assert_eq!(email, "User@Example.COM");
        },
        Err(e) => panic!("Expected success for case-insensitive match: {e}"),
    }
}
