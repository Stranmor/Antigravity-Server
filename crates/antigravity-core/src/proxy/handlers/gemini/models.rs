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
    Path(model_name): Path<String>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (mapped_model, _reason) =
        crate::proxy::common::resolve_model_route(&model_name, &*state.custom_mapping.read().await)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let (access_token, project_id, _email, _guard) = state
        .token_manager
        .get_token("gemini", false, None, &mapped_model)
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, format!("Token error: {}", e)))?;

    let wrapped_body =
        crate::proxy::mappers::gemini::wrap_request(&body, &project_id, &mapped_model, None);

    let response = state
        .upstream
        .call_v1_internal("countTokens", &access_token, wrapped_body, None)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Upstream error: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let error_text =
            response.text().await.unwrap_or_else(|_| format!("HTTP {}", status.as_u16()));
        return Err((status, error_text));
    }

    let resp: Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

    let unwrapped = crate::proxy::mappers::gemini::unwrap_response(&resp);
    Ok(Json(unwrapped))
}
