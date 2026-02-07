use axum::extract::State;
use axum::response::Json;

use super::resilience::{get_aimd_status, get_circuit_status, get_health_status};
use crate::test_helpers::test_app_state;

#[tokio::test]
async fn test_get_health_status_no_accounts() {
    let (state, _tmp) = test_app_state().await;
    let Json(response) = get_health_status(State(state)).await;
    assert_eq!(response.healthy_accounts, 0);
    assert_eq!(response.disabled_accounts, 0);
    assert!(!response.overall_healthy);
}

#[tokio::test]
async fn test_get_circuit_status_all_closed() {
    let (state, _tmp) = test_app_state().await;
    let Json(response) = get_circuit_status(State(state)).await;
    assert_eq!(response.circuits.len(), 3);
    assert_eq!(response.circuits.get("anthropic").map(String::as_str), Some("closed"));
    assert_eq!(response.circuits.get("google").map(String::as_str), Some("closed"));
    assert_eq!(response.circuits.get("openai").map(String::as_str), Some("closed"));
}

#[tokio::test]
async fn test_get_aimd_status_empty() {
    let (state, _tmp) = test_app_state().await;
    let Json(response) = get_aimd_status(State(state)).await;
    assert_eq!(response.tracked_accounts, 0);
    assert!(response.accounts.is_empty());
}
