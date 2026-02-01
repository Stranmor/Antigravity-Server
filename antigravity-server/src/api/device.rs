use axum::{http::StatusCode, response::Json};
use serde::Serialize;

use antigravity_core::modules::device;

#[derive(Serialize)]
pub struct DeviceProfileResponse {
    pub profile: antigravity_types::models::DeviceProfile,
    pub storage_path: String,
}

pub async fn get_device_profile() -> Result<Json<DeviceProfileResponse>, (StatusCode, String)> {
    let storage_path = device::get_storage_path()
        .map_err(|e| (StatusCode::NOT_FOUND, format!("storage_not_found: {}", e)))?;

    let profile = device::read_profile(&storage_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("read_failed: {}", e),
        )
    })?;

    Ok(Json(DeviceProfileResponse {
        profile,
        storage_path: storage_path.display().to_string(),
    }))
}

#[derive(Serialize)]
pub struct CreateProfileResponse {
    pub profile: antigravity_types::models::DeviceProfile,
    pub backup_path: Option<String>,
}

pub async fn create_device_profile() -> Result<Json<CreateProfileResponse>, (StatusCode, String)> {
    let storage_path = device::get_storage_path()
        .map_err(|e| (StatusCode::NOT_FOUND, format!("storage_not_found: {}", e)))?;

    let backup_path = device::backup_storage(&storage_path).ok();

    let profile = device::generate_profile();
    device::write_profile(&storage_path, &profile).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("write_failed: {}", e),
        )
    })?;

    let _ = device::save_global_original(&profile);

    Ok(Json(CreateProfileResponse {
        profile,
        backup_path: backup_path.map(|p| p.display().to_string()),
    }))
}

#[derive(Serialize)]
pub struct BackupResponse {
    pub backup_path: String,
}

pub async fn backup_device_storage() -> Result<Json<BackupResponse>, (StatusCode, String)> {
    let storage_path = device::get_storage_path()
        .map_err(|e| (StatusCode::NOT_FOUND, format!("storage_not_found: {}", e)))?;

    let backup_path = device::backup_storage(&storage_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("backup_failed: {}", e),
        )
    })?;

    Ok(Json(BackupResponse {
        backup_path: backup_path.display().to_string(),
    }))
}

#[derive(Serialize)]
pub struct BaselineResponse {
    pub baseline: Option<antigravity_types::models::DeviceProfile>,
}

pub async fn get_device_baseline() -> Json<BaselineResponse> {
    Json(BaselineResponse {
        baseline: device::load_global_original(),
    })
}
