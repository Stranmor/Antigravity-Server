//! Config-related API calls

use super::{api_get, api_post};
use crate::api_models::*;

pub(crate) async fn load_config() -> Result<AppConfig, String> {
    api_get("/config").await
}

pub(crate) async fn save_config(config: &AppConfig) -> Result<(), String> {
    let _: bool = api_post("/config", config).await?;
    Ok(())
}
