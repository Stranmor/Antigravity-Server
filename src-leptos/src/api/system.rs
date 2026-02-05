//! Status, models, and system API calls

use super::{api_get, api_post};
use crate::api_models::*;

#[derive(serde::Deserialize, Debug)]
pub(crate) struct StatusResponse {
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    proxy_running: bool,
    #[allow(dead_code)]
    accounts_count: usize,
    #[allow(dead_code)]
    current_account: Option<String>,
}

pub(crate) async fn get_status() -> Result<StatusResponse, String> {
    api_get("/status").await
}

#[derive(serde::Serialize, Debug)]
pub(crate) struct ModelDetectRequest {
    pub(crate) model: String,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub(crate) struct ModelDetectResponse {
    #[allow(dead_code)]
    original_model: String,
    pub(crate) mapped_model: String,
    pub(crate) mapping_reason: String,
}

pub(crate) async fn detect_model(model: &str) -> Result<ModelDetectResponse, String> {
    api_post("/models/detect", &ModelDetectRequest { model: model.to_string() }).await
}

pub(crate) async fn check_for_updates() -> Result<UpdateInfo, String> {
    Ok(UpdateInfo {
        available: false,
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        latest_version: env!("CARGO_PKG_VERSION").to_string(),
        release_url: None,
        release_notes: None,
    })
}

pub(crate) async fn open_data_folder() -> Result<(), String> {
    Err("Not available in browser".to_string())
}

pub(crate) async fn get_data_dir_path() -> Result<String, String> {
    Err("Not available in browser".to_string())
}

pub(crate) async fn clear_log_cache() -> Result<(), String> {
    Ok(())
}
