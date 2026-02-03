use super::*;
use std::time::Duration;

#[test]
fn test_circuit_breaker_opens_after_failures() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        open_duration: Duration::from_secs(60),
        success_threshold: 2,
    };
    let manager = CircuitBreakerManager::with_config(config);

    assert!(manager.check("acc1").is_none());
    assert_eq!(manager.get_state("acc1"), CircuitState::Closed);

    manager.record_failure("acc1", "error 1");
    manager.record_failure("acc1", "error 2");
    assert!(manager.check("acc1").is_none());

    manager.record_failure("acc1", "error 3");
    assert!(manager.check("acc1").is_some());
    assert_eq!(manager.get_state("acc1"), CircuitState::Open);
}

#[test]
fn test_circuit_breaker_success_resets_failures() {
    let manager = CircuitBreakerManager::default();

    manager.record_failure("acc1", "error");
    manager.record_failure("acc1", "error");
    manager.record_success("acc1");

    assert!(manager.check("acc1").is_none());
    assert_eq!(manager.get_state("acc1"), CircuitState::Closed);
}

#[test]
fn test_circuit_breaker_half_open_recovery() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        open_duration: Duration::from_millis(10),
        success_threshold: 2,
    };
    let manager = CircuitBreakerManager::with_config(config);

    manager.record_failure("acc1", "error");
    manager.record_failure("acc1", "error");
    assert_eq!(manager.get_state("acc1"), CircuitState::Open);

    std::thread::sleep(Duration::from_millis(15));

    assert!(manager.check("acc1").is_none());
    assert_eq!(manager.get_state("acc1"), CircuitState::HalfOpen);

    manager.record_success("acc1");
    manager.record_success("acc1");
    assert_eq!(manager.get_state("acc1"), CircuitState::Closed);
}
