use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};

use crate::proxy::server::AppState;

pub async fn handle_list_models(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use std::collections::HashSet;

    let mut model_ids: HashSet<String> = HashSet::new();

    for model in state.token_manager.get_all_available_models() {
        let _: bool = model_ids.insert(model);
    }

    {
        let mapping = state.custom_mapping.read().await;
        for key in mapping.keys() {
            let _: bool = model_ids.insert(key.clone());
        }
    }

    let base = "gemini-3-pro-image";
    let resolutions = ["", "-2k", "-4k"];
    let ratios = ["", "-1x1", "-4x3", "-3x4", "-16x9", "-9x16", "-21x9"];
    for res in resolutions {
        for ratio in ratios {
            let mut id = base.to_owned();
            id.push_str(res);
            id.push_str(ratio);
            let _: bool = model_ids.insert(id);
        }
    }

    let mut sorted_ids: Vec<String> = model_ids.into_iter().collect();
    sorted_ids.sort();

    let models: Vec<_> = sorted_ids
        .into_iter()
        .map(|id| {
            json!({
                "name": format!("models/{}", id),
                "version": "001",
                "displayName": id.clone(),
                "inputTokenLimit": 1_048_576,
                "outputTokenLimit": 65536,
                "supportedGenerationMethods": ["generateContent", "countTokens"]
            })
        })
        .collect();
    Ok(Json(json!({ "models": models })))
}

pub async fn handle_get_model(Path(model_name): Path<String>) -> impl IntoResponse {
    Json(json!({ "name": format!("models/{}", model_name), "displayName": model_name }))
}

pub async fn handle_count_tokens(
    State(state): State<AppState>,
    Path(_model_name): Path<String>,
    Json(_body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (_access_token, _project_id, _email, _guard) = state
        .token_manager
        .get_token("gemini", false, None, "gemini")
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, format!("Token error: {}", e)))?;
    Ok(Json(json!({"totalTokens": 0})))
}
