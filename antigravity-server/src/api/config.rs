//! Configuration handlers

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};

use antigravity_core::models::AppConfig;
use antigravity_core::modules::config as core_config;

use crate::state::AppState;

pub async fn get_config(
    State(_state): State<AppState>,
) -> Result<Json<AppConfig>, (StatusCode, String)> {
    match tokio::task::spawn_blocking(core_config::load_config).await {
        Ok(Ok(config)) => Ok(Json(config)),
        Ok(Err(e)) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))),
    }
}

pub async fn save_config(
    State(state): State<AppState>,
    Json(payload): Json<AppConfig>,
) -> Result<Json<bool>, (StatusCode, String)> {
    match tokio::task::spawn_blocking(move || core_config::save_config(&payload)).await {
        Ok(Ok(())) => {
            state.hot_reload_proxy_config().await;
            Ok(Json(true))
        },
        Ok(Err(e)) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))),
    }
}

pub async fn get_syncable_mapping(
    State(state): State<AppState>,
) -> Json<antigravity_types::SyncableMapping> {
    Json(state.get_syncable_mapping().await)
}

#[derive(Deserialize)]
pub struct MergeMappingRequest {
    pub mapping: antigravity_types::SyncableMapping,
}

#[derive(Serialize)]
pub struct MergeMappingResponse {
    pub updated_count: usize,
    pub total_count: usize,
}

pub async fn merge_remote_mapping(
    State(state): State<AppState>,
    Json(payload): Json<MergeMappingRequest>,
) -> Json<MergeMappingResponse> {
    let updated = state.merge_remote_mapping(&payload.mapping).await;
    let total = state.get_syncable_mapping().await.len();

    Json(MergeMappingResponse { updated_count: updated, total_count: total })
}

pub async fn get_proxy_assignments(
    State(state): State<AppState>,
) -> Json<antigravity_types::SyncableProxyAssignments> {
    Json(state.get_syncable_proxy_assignments().await)
}

#[derive(Deserialize)]
pub struct MergeProxyAssignmentsRequest {
    pub assignments: antigravity_types::SyncableProxyAssignments,
}

#[derive(Serialize)]
pub struct MergeProxyAssignmentsResponse {
    pub updated_count: usize,
    pub total_count: usize,
}

pub async fn merge_proxy_assignments(
    State(state): State<AppState>,
    Json(payload): Json<MergeProxyAssignmentsRequest>,
) -> Json<MergeProxyAssignmentsResponse> {
    let updated = state.merge_remote_proxy_assignments(&payload.assignments).await;
    let total = state.get_syncable_proxy_assignments().await.len();

    Json(MergeProxyAssignmentsResponse { updated_count: updated, total_count: total })
}
