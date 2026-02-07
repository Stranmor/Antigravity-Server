use super::retry_strategy::*;
use std::time::Duration;

// ── determine_retry_strategy ──────────────────────────────────────

#[test]
fn strategy_400_signature_error_returns_fixed_delay() {
    let strategy = determine_retry_strategy(400, "Invalid `signature` in request", false);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_400_thinking_signature_returns_fixed_delay() {
    let strategy = determine_retry_strategy(400, "thinking.signature is malformed", false);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_400_corrupted_thought_returns_fixed_delay() {
    let strategy = determine_retry_strategy(400, "Corrupted thought signature detected", false);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_400_signature_already_retried_returns_no_retry() {
    let strategy = determine_retry_strategy(400, "Invalid `signature` in request", true);
    assert!(matches!(strategy, RetryStrategy::NoRetry));
}

#[test]
fn strategy_400_unrelated_error_returns_no_retry() {
    let strategy = determine_retry_strategy(400, "Bad request body", false);
    assert!(matches!(strategy, RetryStrategy::NoRetry));
}

#[test]
fn strategy_429_with_retry_delay_returns_fixed() {
    // JSON body that parse_retry_delay can parse
    let body = r#"{"error":{"details":[{"@type":"RetryInfo","retryDelay":"5s"}]}}"#;
    let strategy = determine_retry_strategy(429, body, false);
    // parse_retry_delay returns 5000ms, then +200 = 5200, capped at 30000
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(5200)));
}

#[test]
fn strategy_429_without_parseable_delay_returns_linear() {
    let strategy = determine_retry_strategy(429, "Rate limit exceeded", false);
    assert!(matches!(strategy, RetryStrategy::LinearBackoff { base_ms: 5000 }));
}

#[test]
fn strategy_503_returns_exponential_backoff() {
    let strategy = determine_retry_strategy(503, "Service unavailable", false);
    assert!(matches!(
        strategy,
        RetryStrategy::ExponentialBackoff { base_ms: 10000, max_ms: 60000 }
    ));
}

#[test]
fn strategy_529_returns_exponential_backoff() {
    let strategy = determine_retry_strategy(529, "Overloaded", false);
    assert!(matches!(
        strategy,
        RetryStrategy::ExponentialBackoff { base_ms: 10000, max_ms: 60000 }
    ));
}

#[test]
fn strategy_500_returns_linear_backoff() {
    let strategy = determine_retry_strategy(500, "Internal error", false);
    assert!(matches!(strategy, RetryStrategy::LinearBackoff { base_ms: 3000 }));
}

#[test]
fn strategy_401_returns_fixed_delay() {
    let strategy = determine_retry_strategy(401, "Unauthorized", false);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_403_returns_fixed_delay() {
    let strategy = determine_retry_strategy(403, "Forbidden", false);
    assert!(matches!(strategy, RetryStrategy::FixedDelay(d) if d == Duration::from_millis(200)));
}

#[test]
fn strategy_unknown_code_returns_no_retry() {
    let strategy = determine_retry_strategy(418, "I'm a teapot", false);
    assert!(matches!(strategy, RetryStrategy::NoRetry));
}

// ── should_rotate_account ─────────────────────────────────────────

#[test]
fn rotate_on_429_500_401_403() {
    assert!(should_rotate_account(429));
    assert!(should_rotate_account(500));
    assert!(should_rotate_account(401));
    assert!(should_rotate_account(403));
    assert!(should_rotate_account(503));
    assert!(should_rotate_account(529));
}

#[test]
fn no_rotate_on_other_codes() {
    assert!(!should_rotate_account(200));
    assert!(!should_rotate_account(400));
    assert!(!should_rotate_account(418));
}

// ── PeekConfig ────────────────────────────────────────────────────

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

// ── apply_retry_strategy ──────────────────────────────────────────

#[tokio::test]
async fn apply_no_retry_returns_false() {
    let result = apply_retry_strategy(RetryStrategy::NoRetry, 0, 3, 418, "test").await;
    assert!(!result);
}

#[tokio::test]
async fn apply_fixed_delay_returns_true() {
    let result = apply_retry_strategy(
        RetryStrategy::FixedDelay(Duration::from_millis(1)),
        0,
        3,
        401,
        "test",
    )
    .await;
    assert!(result);
}

#[tokio::test]
async fn apply_linear_backoff_returns_true() {
    let result =
        apply_retry_strategy(RetryStrategy::LinearBackoff { base_ms: 1 }, 0, 3, 500, "test").await;
    assert!(result);
}

#[tokio::test]
async fn apply_exponential_backoff_returns_true() {
    let result = apply_retry_strategy(
        RetryStrategy::ExponentialBackoff { base_ms: 1, max_ms: 10 },
        0,
        3,
        503,
        "test",
    )
    .await;
    assert!(result);
}
