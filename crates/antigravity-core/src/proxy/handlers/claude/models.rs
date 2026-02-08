//! Model listing and token counting handlers

use crate::proxy::server::AppState;
use axum::http::HeaderMap;
use axum::{
    extract::{Json, State},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};

pub async fn handle_list_models(State(state): State<AppState>) -> impl IntoResponse {
    use crate::proxy::common::model_mapping::collect_all_model_ids;

    let sorted_ids = collect_all_model_ids(
        &state.token_manager.get_all_available_models(),
        &state.custom_mapping,
    )
    .await;

    let data: Vec<_> = sorted_ids
        .into_iter()
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": 1_706_745_600,
                "owned_by": "antigravity"
            })
        })
        .collect();

    Json(json!({
        "object": "list",
        "data": data
    }))
}

pub async fn handle_count_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let zai = state.zai.read().await.clone();
    let zai_enabled =
        zai.enabled && !matches!(zai.dispatch_mode, crate::proxy::ZaiDispatchMode::Off);

    if zai_enabled {
        return crate::proxy::providers::zai_anthropic::forward_anthropic_json(
            &state,
            axum::http::Method::POST,
            "/v1/messages/count_tokens",
            &headers,
            body,
        )
        .await;
    }

    // Non-ZAI fallback: Anthropic format cannot be forwarded to Gemini countTokens
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": {
                "type": "not_implemented",
                "message": "Token counting requires ZAI mode to be enabled"
            }
        })),
    )
        .into_response()
}
