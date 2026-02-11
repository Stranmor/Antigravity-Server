//! Response handling for streaming and non-streaming Claude responses

use crate::proxy::common::header_constants::{X_ACCOUNT_EMAIL, X_MAPPED_MODEL, X_MAPPING_REASON};
use crate::proxy::mappers::claude::{models::GeminiResponse, transform_response, ClaudeRequest};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

pub struct ResponseContext {
    pub trace_id: String,
    pub email: String,
    pub session_id: String,
    pub mapped_model: String,
    pub reason: String,
    pub scaling_enabled: bool,
    pub context_limit: u32,
}

pub async fn handle_nonstreaming_success(
    response: reqwest::Response,
    request: &ClaudeRequest,
    ctx: &ResponseContext,
) -> Response {
    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, format!("Failed to read body: {}", e))
                .into_response();
        },
    };

    tracing::debug!("Upstream Claude response received, len: {} bytes", bytes.len());

    let gemini_resp: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, format!("Parse error: {}", e)).into_response();
        },
    };

    let raw = gemini_resp.get("response").cloned().unwrap_or(gemini_resp);

    let gemini_response: GeminiResponse = match serde_json::from_value(raw) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Convert error: {}", e))
                .into_response();
        },
    };

    let s_id_owned = Some(ctx.session_id.clone());
    let claude_response = match transform_response(
        &gemini_response,
        ctx.scaling_enabled,
        ctx.context_limit,
        s_id_owned,
        request.model.clone(),
    ) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Transform error: {}", e))
                .into_response();
        },
    };

    let cache_info = if let Some(cached) = claude_response.usage.cache_read_input_tokens {
        format!(", Cached: {}", cached)
    } else {
        String::new()
    };

    tracing::info!(
        "[{}] Request finished. Model: {}, Tokens: In {}, Out {}{}",
        ctx.trace_id,
        request.model,
        claude_response.usage.input_tokens,
        claude_response.usage.output_tokens,
        cache_info
    );

    (
        StatusCode::OK,
        [
            (X_ACCOUNT_EMAIL, ctx.email.as_str()),
            (X_MAPPED_MODEL, ctx.mapped_model.as_str()),
            (X_MAPPING_REASON, ctx.reason.as_str()),
        ],
        Json(claude_response),
    )
        .into_response()
}
