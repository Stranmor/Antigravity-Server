//! Status, models, and system API calls

use super::{api_get, api_post};
use crate::api_models::*;

#[derive(serde::Deserialize)]
pub struct StatusResponse {
    pub version: String,
    pub proxy_running: bool,
    pub accounts_count: usize,
    pub current_account: Option<String>,
}

pub async fn get_status() -> Result<StatusResponse, String> {
    api_get("/status").await
}

// ========== Models ==========

#[derive(serde::Serialize)]
pub struct ModelDetectRequest {
    pub model: String,
}

#[derive(serde::Deserialize, Clone)]
pub struct ModelDetectResponse {
    pub original_model: String,
    pub mapped_model: String,
    pub mapping_reason: String,
}

pub async fn detect_model(model: &str) -> Result<ModelDetectResponse, String> {
    api_post(
        "/models/detect",
        &ModelDetectRequest {
            model: model.to_string(),
        },
    )
    .await
}

// ========== System ==========

pub async fn check_for_updates() -> Result<UpdateInfo, String> {
    Ok(UpdateInfo {
        available: false,
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        latest_version: env!("CARGO_PKG_VERSION").to_string(),
        release_url: None,
        release_notes: None,
    })
}

pub async fn open_data_folder() -> Result<(), String> {
    Err("Not available in browser".to_string())
}

pub async fn get_data_dir_path() -> Result<String, String> {
    Err("Not available in browser".to_string())
}

pub async fn clear_log_cache() -> Result<(), String> {
    Ok(())
}
