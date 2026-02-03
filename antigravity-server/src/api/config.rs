//! Configuration handlers

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};

use antigravity_core::models::AppConfig;
use antigravity_core::modules::config as core_config;

use crate::state::AppState;

pub async fn get_config(
    State(_state): State<AppState>,
) -> Result<Json<AppConfig>, (StatusCode, String)> {
    match core_config::load_config() {
        Ok(config) => Ok(Json(config)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

pub async fn save_config(
    State(state): State<AppState>,
    Json(payload): Json<AppConfig>,
) -> Result<Json<bool>, (StatusCode, String)> {
    match core_config::save_config(&payload) {
        Ok(_) => {
            state.hot_reload_proxy_config().await;
            Ok(Json(true))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
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

    Json(MergeMappingResponse {
        updated_count: updated,
        total_count: total,
    })
}
