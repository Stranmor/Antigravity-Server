use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;

use super::proxy::{
    clear_all_rate_limits, clear_rate_limit, clear_session_bindings, get_proxy_status,
};
use crate::test_helpers::test_app_state;

#[tokio::test]
async fn test_get_proxy_status() {
    let (state, _tmp) = test_app_state().await;
    state.set_bound_port(9999);
    let Json(response) = get_proxy_status(State(state)).await;
    assert!(response.running);
    assert_eq!(response.port, 9999);
    assert_eq!(response.active_accounts, 0);
}

#[tokio::test]
async fn test_clear_session_bindings() {
    let (state, _tmp) = test_app_state().await;
    let Json(result) = clear_session_bindings(State(state)).await;
    assert!(result);
}

#[tokio::test]
async fn test_clear_all_rate_limits() {
    let (state, _tmp) = test_app_state().await;
    let status = clear_all_rate_limits(State(state)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_clear_rate_limit_not_found() {
    let (state, _tmp) = test_app_state().await;
    let status = clear_rate_limit(State(state), Path("nonexistent".to_string())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
