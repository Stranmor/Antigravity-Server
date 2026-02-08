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
    use crate::proxy::common::model_mapping::collect_all_model_ids;

    let sorted_ids = collect_all_model_ids(
        &state.token_manager.get_all_available_models(),
        &state.custom_mapping,
    )
    .await;

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
