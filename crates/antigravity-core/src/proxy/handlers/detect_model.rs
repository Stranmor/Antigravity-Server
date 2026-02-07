use crate::proxy::server::AppState;
use axum::{extract::Json, extract::State, http::StatusCode, response::IntoResponse};
use serde_json::{json, Value};

pub async fn handle_detect_model(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let model_name = body.get("model").and_then(|v| v.as_str()).unwrap_or("");

    if model_name.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing 'model' field").into_response();
    }

    let (mapped_model, reason) = match crate::proxy::common::resolve_model_route(
        model_name,
        &*state.custom_mapping.read().await,
    ) {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": e,
                    "model": model_name,
                    "available": false
                })),
            )
                .into_response();
        },
    };

    let config = crate::proxy::mappers::request_config::resolve_request_config(
        model_name,
        &mapped_model,
        &None,
        None,
        None,
    );

    let mut response = json!({
        "model": model_name,
        "mapped_model": mapped_model,
        "mapping_reason": reason,
        "type": config.request_type,
        "features": {
            "has_web_search": config.inject_google_search,
            "is_image_gen": config.request_type == "image_gen"
        }
    });

    if let Some(img_conf) = config.image_config {
        if let Some(obj) = response.as_object_mut() {
            let _: Option<Value> = obj.insert("config".to_string(), img_conf);
        }
    }

    Json(response).into_response()
}
