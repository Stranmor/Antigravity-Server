//! Request preparation for Claude messages handler

use crate::proxy::mappers::claude::{transform_claude_request_in, ClaudeRequest};
use crate::proxy::mappers::request_config::resolve_request_config;
use crate::proxy::server::AppState;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use super::background_detection::{detect_background_task_type, select_background_model};
use super::error_recovery::{apply_background_task_cleanup, apply_user_request_cleanup};

pub struct PreparedRequest {
    pub mapped_model: String,
    pub request_with_mapped: ClaudeRequest,
    pub gemini_body: Value,
}

pub async fn prepare_request(
    state: &AppState,
    request_for_body: &ClaudeRequest,
    project_id: &str,
    trace_id: &str,
    retried_without_thinking: bool,
) -> Result<PreparedRequest, Response> {
    let (mut mapped_model, _reason) = crate::proxy::common::resolve_model_route(
        &request_for_body.model,
        &*state.custom_mapping.read().await,
    );

    let tools_val: Option<Vec<Value>> = request_for_body.tools.as_ref().map(|list| {
        list.iter()
            .map(|t| serde_json::to_value(t).unwrap_or(json!({})))
            .collect()
    });

    let _config = resolve_request_config(
        &request_for_body.model,
        &mapped_model,
        &tools_val,
        None,
        None,
    );

    let background_task_type = detect_background_task_type(request_for_body);
    let mut request_with_mapped = request_for_body.clone();

    if let Some(task_type) = background_task_type {
        let downgrade_model = select_background_model(task_type);
        apply_background_task_cleanup(
            &mut request_with_mapped,
            downgrade_model,
            trace_id,
            &mapped_model,
        );
        mapped_model = downgrade_model.to_string();
    } else {
        apply_user_request_cleanup(&mut request_with_mapped, trace_id, &mapped_model);
    }

    request_with_mapped.model = mapped_model.clone();

    let gemini_body = match transform_claude_request_in(
        &request_with_mapped,
        project_id,
        retried_without_thinking,
    ) {
        Ok(b) => {
            tracing::debug!(
                "[{}] Transformed Gemini Body: {}",
                trace_id,
                serde_json::to_string_pretty(&b).unwrap_or_default()
            );
            b
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("Transform error: {}", e)
                    }
                })),
            )
                .into_response());
        }
    };

    Ok(PreparedRequest {
        mapped_model,
        request_with_mapped,
        gemini_body,
    })
}
