use crate::proxy::retry::*;
use std::time::Duration;

fn openai_profile() -> RetryProfile {
    RetryProfile::openai()
}

#[test]
fn strategy_400_signature_error_returns_fixed_delay() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(400, "Invalid `signature` in request", false, &p);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_400_thinking_signature_returns_fixed_delay() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(400, "thinking.signature is malformed", false, &p);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_400_corrupted_thought_returns_fixed_delay() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(400, "Corrupted thought signature detected", false, &p);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_400_signature_already_retried_returns_no_retry() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(400, "Invalid `signature` in request", true, &p);
    assert!(matches!(strategy, RetryStrategy::NoRetry));
}

#[test]
fn strategy_400_unrelated_error_returns_no_retry() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(400, "Bad request body", false, &p);
    assert!(matches!(strategy, RetryStrategy::NoRetry));
}

#[test]
fn strategy_429_with_retry_delay_returns_fixed() {
    let p = openai_profile();
    let body = r#"{"error":{"details":[{"@type":"RetryInfo","retryDelay":"5s"}]}}"#;
    let strategy = determine_retry_strategy(429, body, false, &p);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(5200)));
}

#[test]
fn strategy_429_without_parseable_delay_returns_linear() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(429, "Rate limit exceeded", false, &p);
    assert!(matches!(strategy, RetryStrategy::LinearBackoff { base_ms: 5000 }));
}

#[test]
fn strategy_503_returns_exponential_backoff() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(503, "Service unavailable", false, &p);
    assert!(matches!(
        strategy,
        RetryStrategy::ExponentialBackoff { base_ms: 10000, max_ms: 60000 }
    ));
}

#[test]
fn strategy_529_returns_exponential_backoff() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(529, "Overloaded", false, &p);
    assert!(matches!(
        strategy,
        RetryStrategy::ExponentialBackoff { base_ms: 10000, max_ms: 60000 }
    ));
}

#[test]
fn strategy_500_returns_linear_backoff() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(500, "Internal error", false, &p);
    assert!(matches!(strategy, RetryStrategy::LinearBackoff { base_ms: 3000 }));
}

#[test]
fn strategy_401_returns_fixed_delay() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(401, "Unauthorized", false, &p);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_403_returns_fixed_delay() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(403, "Forbidden", false, &p);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_unknown_code_returns_no_retry() {
    let p = openai_profile();
    let strategy = determine_retry_strategy(418, "I'm a teapot", false, &p);
    assert!(matches!(strategy, RetryStrategy::NoRetry));
}

#[test]
fn rotate_on_retryable_codes() {
    assert!(should_rotate_account(429));
    assert!(should_rotate_account(500));
    assert!(should_rotate_account(401));
    assert!(should_rotate_account(403));
    assert!(should_rotate_account(503));
    assert!(should_rotate_account(529));
    assert!(should_rotate_account(404));
}

#[test]
fn no_rotate_on_other_codes() {
    assert!(!should_rotate_account(200));
    assert!(!should_rotate_account(400));
    assert!(!should_rotate_account(418));
}

#[test]
fn peek_config_default_values() {
    let cfg = PeekConfig::default();
    assert_eq!(cfg.max_heartbeats, 20);
    assert_eq!(cfg.max_peek_duration, Duration::from_secs(120));
    assert_eq!(cfg.single_chunk_timeout, Duration::from_secs(60));
}

#[test]
fn peek_config_openai_values() {
    let cfg = PeekConfig::openai();
    assert_eq!(cfg.max_heartbeats, 20);
    assert_eq!(cfg.max_peek_duration, Duration::from_secs(90));
    assert_eq!(cfg.single_chunk_timeout, Duration::from_secs(30));
}

#[tokio::test]
async fn apply_no_retry_returns_false() {
    let result = apply_retry_strategy(RetryStrategy::NoRetry, 0, 418, "test").await;
    assert!(!result);
}

#[tokio::test]
async fn apply_fixed_delay_returns_true() {
    let result =
        apply_retry_strategy(RetryStrategy::FixedDelay(Duration::from_millis(1)), 0, 401, "test")
            .await;
    assert!(result);
}

#[tokio::test]
async fn apply_linear_backoff_returns_true() {
    let result =
        apply_retry_strategy(RetryStrategy::LinearBackoff { base_ms: 1 }, 0, 500, "test").await;
    assert!(result);
}

#[tokio::test]
async fn apply_exponential_backoff_returns_true() {
    let result = apply_retry_strategy(
        RetryStrategy::ExponentialBackoff { base_ms: 1, max_ms: 10 },
        0,
        503,
        "test",
    )
    .await;
    assert!(result);
}

#[test]
fn claude_profile_has_more_signature_patterns() {
    let claude = RetryProfile::claude();
    let openai = RetryProfile::openai();
    assert!(claude.signature_patterns.len() > openai.signature_patterns.len());
    assert_eq!(claude.signature_patterns.len(), 13);
}

#[test]
fn is_signature_error_claude_patterns() {
    let p = RetryProfile::claude();
    assert!(!is_signature_error(
        "Image does not match the declared MIME type: INVALID_ARGUMENT",
        &p
    ));
    assert!(is_signature_error("Invalid `signature` in thinking block", &p));
    assert!(is_signature_error("failed to deserialise body", &p));
    assert!(is_signature_error("Found `text` instead of thinking", &p));
    assert!(is_signature_error("must be `thinking` type", &p));
    assert!(!is_signature_error("normal error message", &p));
}

#[test]
fn claude_profile_different_backoff_values() {
    let p = RetryProfile::claude();
    let strategy = determine_retry_strategy(429, "Rate limit exceeded", false, &p);
    assert!(matches!(strategy, RetryStrategy::LinearBackoff { base_ms: 1000 }));

    let strategy = determine_retry_strategy(503, "Service unavailable", false, &p);
    assert!(matches!(strategy, RetryStrategy::ExponentialBackoff { base_ms: 1000, max_ms: 8000 }));
}
