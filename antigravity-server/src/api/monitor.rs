//! Request monitoring handlers

use axum::{extract::State, response::Json};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct MonitorQuery {
    pub limit: Option<usize>,
}

pub async fn get_monitor_requests(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<MonitorQuery>,
) -> Json<Vec<antigravity_types::models::ProxyRequestLog>> {
    let logs = state.get_proxy_logs(query.limit).await;
    Json(logs)
}

pub async fn get_monitor_stats(
    State(state): State<AppState>,
) -> Json<antigravity_types::models::ProxyStats> {
    let stats = state.get_proxy_stats().await;
    Json(stats)
}

pub async fn clear_monitor_logs(State(state): State<AppState>) -> Json<bool> {
    state.clear_proxy_logs().await;
    Json(true)
}
