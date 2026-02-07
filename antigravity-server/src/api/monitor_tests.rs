use axum::extract::{Query, State};
use axum::response::Json;

use super::monitor::{
    clear_monitor_logs, get_monitor_requests, get_monitor_stats, get_token_usage_stats,
    MonitorQuery,
};
use crate::test_helpers::test_app_state;

#[tokio::test]
async fn test_get_monitor_stats_empty() {
    let (state, _tmp) = test_app_state().await;
    let Json(stats) = get_monitor_stats(State(state)).await;
    assert_eq!(stats.total_requests, 0);
    assert_eq!(stats.success_count, 0);
    assert_eq!(stats.error_count, 0);
}

#[tokio::test]
async fn test_get_monitor_requests_empty() {
    let (state, _tmp) = test_app_state().await;
    let Json(logs) =
        get_monitor_requests(State(state), Query(MonitorQuery { limit: Some(10) })).await;
    assert!(logs.is_empty());
}

#[tokio::test]
async fn test_clear_monitor_logs() {
    let (state, _tmp) = test_app_state().await;
    let Json(result) = clear_monitor_logs(State(state)).await;
    assert!(result);
}

#[tokio::test]
async fn test_get_token_usage_stats_empty() {
    let (state, _tmp) = test_app_state().await;
    let Json(stats) = get_token_usage_stats(State(state)).await;
    assert_eq!(stats.total_input, 0);
    assert_eq!(stats.total_output, 0);
    assert_eq!(stats.total_requests, 0);
}
