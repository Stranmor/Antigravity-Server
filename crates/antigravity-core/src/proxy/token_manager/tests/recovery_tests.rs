use super::super::proxy_token::ProxyToken;
use super::super::TokenManager;
use crate::proxy::rate_limit::RateLimitReason;
use crate::proxy::routing_config::SmartRoutingConfig;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::time::{Duration, SystemTime};

fn create_test_manager() -> TokenManager {
    let unique_id = uuid::Uuid::new_v4();
    TokenManager::new(std::env::temp_dir().join(format!("antigravity_test_{unique_id}")))
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

fn make_protected_token(email: &str, protected_model: &str) -> ProxyToken {
    let mut protected = HashSet::new();
    protected.insert(protected_model.to_string());
    ProxyToken::new(
        email.to_string(),
        format!("token_{email}"),
        "refresh".to_string(),
        3600,
        chrono::Utc::now().timestamp() + 3600,
        email.to_string(),
        PathBuf::from("/tmp"),
        Some("test-project".to_string()),
        Some("g1-pro-tier".to_string()),
        Some(100),
        protected,
        1.0,
        HashSet::new(),
    )
}

/// Insert token into manager's DashMap (required for `is_candidate_eligible`).
fn register_token(manager: &TokenManager, token: &ProxyToken) {
    manager.tokens.insert(token.account_id.clone(), token.clone());
}

/// Saturate concurrency for an account so `ActiveRequestGuard::try_new` returns `None`.
fn saturate_concurrency(manager: &TokenManager, email: &str, routing: &SmartRoutingConfig) {
    manager
        .active_requests
        .insert(email.to_string(), AtomicU32::new(routing.max_concurrent_per_account));
}

#[tokio::test]
async fn test_long_rate_limit_returns_immediate_error() {
    let manager = create_test_manager();
    let routing = SmartRoutingConfig::default();
    let token = make_token("a@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    register_token(&manager, &token);

    let tracker = manager.rate_limit_tracker();
    tracker.set_lockout_until(
        "a@test.com",
        SystemTime::now() + Duration::from_secs(300),
        RateLimitReason::QuotaExhausted,
        None,
    );

    let result = manager
        .try_recovery_selection(&[token], &HashSet::new(), "gemini-3-pro", false, &routing)
        .await;

    match result {
        Err(e) => assert!(e.contains("wait"), "Expected 'wait' in error: {e}"),
        Ok(_) => panic!("Expected Err for long rate limit"),
    }
}

#[tokio::test]
async fn test_short_rate_limit_buffer_delay_finds_token() {
    let manager = create_test_manager();
    let routing = SmartRoutingConfig::default();
    let token = make_token("a@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    register_token(&manager, &token);

    let tracker = manager.rate_limit_tracker();
    tracker.set_lockout_until(
        "a@test.com",
        SystemTime::now() + Duration::from_secs(1),
        RateLimitReason::RateLimitExceeded,
        None,
    );

    let result = manager
        .try_recovery_selection(&[token], &HashSet::new(), "gemini-3-pro", false, &routing)
        .await;

    match result {
        Ok(Some(t)) => assert_eq!(t.email, "a@test.com"),
        Ok(None) => panic!("Expected Some token after buffer delay"),
        Err(e) => panic!("Expected Ok, got Err: {e}"),
    }
}

#[tokio::test]
async fn test_short_rate_limit_optimistic_reset_finds_token() {
    let manager = create_test_manager();
    let routing = SmartRoutingConfig::default();
    let token_a = make_token("a@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    let token_b = make_token("b@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    register_token(&manager, &token_a);
    register_token(&manager, &token_b);

    let tracker = manager.rate_limit_tracker();
    tracker.set_lockout_until(
        "a@test.com",
        SystemTime::now() + Duration::from_secs(1),
        RateLimitReason::RateLimitExceeded,
        None,
    );
    tracker.set_lockout_until(
        "b@test.com",
        SystemTime::now() + Duration::from_secs(1),
        RateLimitReason::RateLimitExceeded,
        None,
    );
    saturate_concurrency(&manager, "a@test.com", &routing);

    let snapshot = vec![token_a, token_b];
    let result = manager
        .try_recovery_selection(&snapshot, &HashSet::new(), "gemini-3-pro", false, &routing)
        .await;

    match result {
        Ok(Some(t)) => assert_eq!(t.email, "b@test.com"),
        Ok(None) => panic!("Expected Some token after optimistic reset"),
        Err(e) => panic!("Expected Ok, got Err: {e}"),
    }
}

#[tokio::test]
async fn test_attempted_accounts_skipped_even_after_reset() {
    let manager = create_test_manager();
    let routing = SmartRoutingConfig::default();
    let token = make_token("a@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    register_token(&manager, &token);

    let tracker = manager.rate_limit_tracker();
    tracker.set_lockout_until(
        "a@test.com",
        SystemTime::now() + Duration::from_secs(1),
        RateLimitReason::RateLimitExceeded,
        None,
    );

    let mut attempted = HashSet::new();
    attempted.insert("a@test.com".to_string());

    let result =
        manager.try_recovery_selection(&[token], &attempted, "gemini-3-pro", false, &routing).await;

    match result {
        Err(e) => assert!(
            e.contains("failed") || e.contains("reset"),
            "Expected failure after reset, got: {e}"
        ),
        Ok(Some(t)) => panic!("Should not return attempted account: {}", t.email),
        Ok(None) => panic!("Expected Err, got Ok(None)"),
    }
}

#[tokio::test]
async fn test_protected_model_filtered_during_buffer_delay() {
    let manager = create_test_manager();
    let routing = SmartRoutingConfig::default();
    let token = make_protected_token("a@test.com", "gemini-3-pro");
    register_token(&manager, &token);

    let tracker = manager.rate_limit_tracker();
    tracker.set_lockout_until(
        "a@test.com",
        SystemTime::now() + Duration::from_secs(1),
        RateLimitReason::RateLimitExceeded,
        None,
    );

    let result = manager
        .try_recovery_selection(&[token], &HashSet::new(), "gemini-3-pro", true, &routing)
        .await;

    match result {
        Err(e) => assert!(
            e.contains("failed") || e.contains("reset"),
            "Expected failure due to protected model, got: {e}"
        ),
        Ok(Some(t)) => panic!("Protected model should be filtered: {}", t.email),
        Ok(None) => panic!("Expected Err, got Ok(None)"),
    }
}

#[tokio::test]
async fn test_all_at_max_concurrency_no_rate_limits_returns_error() {
    let manager = create_test_manager();
    let routing = SmartRoutingConfig::default();
    let token_a = make_token("a@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    let token_b = make_token("b@test.com", Some("g1-pro-tier"), Some(100), 1.0);
    register_token(&manager, &token_a);
    register_token(&manager, &token_b);
    saturate_concurrency(&manager, "a@test.com", &routing);
    saturate_concurrency(&manager, "b@test.com", &routing);

    let snapshot = vec![token_a, token_b];
    let result = manager
        .try_recovery_selection(&snapshot, &HashSet::new(), "gemini-3-pro", false, &routing)
        .await;

    match result {
        Err(e) => assert!(
            e.contains("capacity") || e.contains("retry"),
            "Expected capacity error, got: {e}"
        ),
        Ok(Some(t)) => panic!("Should not find token at max concurrency: {}", t.email),
        Ok(None) => panic!("Expected Err, got Ok(None)"),
    }
}

#[tokio::test]
async fn test_optimistic_reset_still_filters_protected_model() {
    let manager = create_test_manager();
    let routing = SmartRoutingConfig::default();
    let token = make_protected_token("a@test.com", "gemini-3-pro");
    register_token(&manager, &token);
    saturate_concurrency(&manager, "a@test.com", &routing);

    let tracker = manager.rate_limit_tracker();
    tracker.set_lockout_until(
        "a@test.com",
        SystemTime::now() + Duration::from_secs(1),
        RateLimitReason::RateLimitExceeded,
        None,
    );

    let result = manager
        .try_recovery_selection(&[token], &HashSet::new(), "gemini-3-pro", true, &routing)
        .await;

    match result {
        Err(e) => assert!(
            e.contains("failed") || e.contains("reset"),
            "Expected failure: protected model after reset, got: {e}"
        ),
        Ok(Some(t)) => panic!("Protected model must be filtered post-reset: {}", t.email),
        Ok(None) => panic!("Expected Err, got Ok(None)"),
    }
}
