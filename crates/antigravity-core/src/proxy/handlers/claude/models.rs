//! Model listing and token counting handlers

use crate::proxy::server::AppState;
use axum::http::HeaderMap;
use axum::{
    extract::{Json, State},
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};

pub async fn handle_list_models(State(state): State<AppState>) -> impl IntoResponse {
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

    Json(json!({
        "input_tokens": 0,
        "output_tokens": 0
    }))
    .into_response()
}
