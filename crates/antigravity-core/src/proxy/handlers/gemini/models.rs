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
        state.custom_mapping.as_ref(),
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

pub async fn handle_get_model(
    State(state): State<AppState>,
    Path(model_name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use crate::proxy::common::model_mapping::collect_all_model_ids;

    let available_ids = collect_all_model_ids(
        &state.token_manager.get_all_available_models(),
        state.custom_mapping.as_ref(),
    )
    .await;

    if !available_ids.contains(&model_name) {
        return Err((StatusCode::NOT_FOUND, format!("Model {} not found", model_name)));
    }

    // Determine limits based on model family
    let (input_limit, output_limit) = if model_name.contains("pro") {
        (2_097_152, 8192)
    } else if model_name.contains("flash") {
        (1_048_576, 8192)
    } else {
        (1_048_576, 4096)
    };

    Ok(Json(json!({
        "name": format!("models/{}", model_name),
        "version": "001",
        "displayName": model_name,
        "inputTokenLimit": input_limit,
        "outputTokenLimit": output_limit,
        "supportedGenerationMethods": ["generateContent", "countTokens"]
    })))
}

pub async fn handle_count_tokens(
    State(state): State<AppState>,
    Path(model_name): Path<String>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (mapped_model, _reason) =
        crate::proxy::common::resolve_model_route(&model_name, &*state.custom_mapping.read().await)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let (access_token, project_id, _email, _guard) =
        state.token_manager.get_token("gemini", false, None, &mapped_model).await.map_err(|e| {
            tracing::warn!("countTokens token error for {}: {}", mapped_model, e);
            (StatusCode::SERVICE_UNAVAILABLE, "No available accounts".to_string())
        })?;

    let wrapped_body =
        crate::proxy::mappers::gemini::wrap_request(&body, &project_id, &mapped_model, None);

    let response = state
        .upstream
        .call_v1_internal(
            &format!("models/{}:countTokens", mapped_model),
            &access_token,
            wrapped_body,
            None,
        )
        .await
        .map_err(|e| {
            tracing::error!("countTokens upstream error: {}", e);
            (StatusCode::BAD_GATEWAY, "Upstream request failed".to_string())
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        tracing::warn!(
            "countTokens upstream {} for {}: {}",
            status.as_u16(),
            model_name,
            error_text
        );
        return Err((status, format!("Upstream returned {}", status.as_u16())));
    }

    let resp: Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)))?;

    Ok(Json(resp))
}
