use crate::proxy::health::monitor::HealthMonitor;
use crate::proxy::health::types::{AccountHealth, ErrorType, HealthConfig};

#[tokio::test]
async fn test_error_threshold() {
    let config = HealthConfig {
        error_threshold: 3,
        cooldown_seconds: 60,
        track_rate_limits: true,
        recovery_check_interval_seconds: 30,
    };
    let monitor = HealthMonitor::with_config(config);

    monitor.register_account("test-1".to_string(), "test@example.com".to_string());

    assert!(!monitor.record_error("test-1", 500, "Error 1").await);
    assert!(!monitor.record_error("test-1", 500, "Error 2").await);
    assert!(monitor.record_error("test-1", 500, "Error 3").await);
    assert!(!monitor.is_available("test-1"));
}

#[tokio::test]
async fn test_success_resets_errors() {
    let _config = HealthConfig { error_threshold: 3, ..Default::default() };
    let monitor = HealthMonitor::new();

    monitor.register_account("test-1".to_string(), "test@example.com".to_string());

    monitor.record_error("test-1", 500, "Error 1").await;
    monitor.record_error("test-1", 500, "Error 2").await;

    monitor.record_success("test-1");

    monitor.record_error("test-1", 500, "Error 1").await;
    monitor.record_error("test-1", 500, "Error 2").await;

    assert!(monitor.is_available("test-1"));
}

#[tokio::test]
async fn test_force_enable() {
    let config = HealthConfig { error_threshold: 1, ..Default::default() };
    let monitor = HealthMonitor::with_config(config);

    monitor.register_account("test-1".to_string(), "test@example.com".to_string());

    monitor.record_error("test-1", 500, "Error").await;
    assert!(!monitor.is_available("test-1"));

    assert!(monitor.force_enable("test-1").await);
    assert!(monitor.is_available("test-1"));
}

#[test]
fn test_error_type_from_status() {
    assert_eq!(ErrorType::from_status_code(401), Some(ErrorType::Unauthorized));
    assert_eq!(ErrorType::from_status_code(403), Some(ErrorType::Forbidden));
    assert_eq!(ErrorType::from_status_code(429), Some(ErrorType::RateLimited));
    assert_eq!(ErrorType::from_status_code(500), Some(ErrorType::ServerError));
    assert_eq!(ErrorType::from_status_code(502), Some(ErrorType::ServerError));
    assert_eq!(ErrorType::from_status_code(200), None);
    assert_eq!(ErrorType::from_status_code(400), None);
}

#[test]
fn test_account_health_counters() {
    let health = AccountHealth::new("acc1".to_string(), "test@example.com".to_string());
    assert_eq!(health.consecutive_errors(), 0);
    assert_eq!(health.total_successes(), 0);
    assert_eq!(health.total_errors(), 0);
    assert!(!health.is_disabled());
}

#[test]
fn test_register_and_unregister() {
    let monitor = HealthMonitor::new();
    monitor.register_account("acc1".to_string(), "email@test.com".to_string());
    assert_eq!(monitor.healthy_count(), 1);

    monitor.unregister_account("acc1");
    assert_eq!(monitor.healthy_count(), 0);
}

#[test]
fn test_disabled_count() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let config = HealthConfig { error_threshold: 1, ..Default::default() };
        let monitor = HealthMonitor::with_config(config);

        monitor.register_account("acc1".to_string(), "a@test.com".to_string());
        monitor.register_account("acc2".to_string(), "b@test.com".to_string());

        assert_eq!(monitor.disabled_count(), 0);
        assert_eq!(monitor.healthy_count(), 2);

        monitor.record_error("acc1", 500, "error").await;

        assert_eq!(monitor.disabled_count(), 1);
        assert_eq!(monitor.healthy_count(), 1);
    });
}

#[test]
fn test_clear() {
    let monitor = HealthMonitor::new();
    monitor.register_account("acc1".to_string(), "a@test.com".to_string());
    monitor.register_account("acc2".to_string(), "b@test.com".to_string());
    assert_eq!(monitor.healthy_count(), 2);

    monitor.clear();
    assert_eq!(monitor.healthy_count(), 0);
}

#[test]
fn test_truncate_string() {
    fn truncate_string(s: &str, max_len: usize) -> String {
        if s.chars().count() <= max_len {
            s.to_string()
        } else {
            let mut result: String = s.chars().take(max_len).collect();
            result.push('…');
            result
        }
    }
    assert_eq!(truncate_string("short", 10), "short");
    assert_eq!(truncate_string("longer text here", 10), "longer tex…");
}
