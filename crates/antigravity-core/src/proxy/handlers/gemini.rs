//! Gemini Protocol Handlers
//!
//! Stub handlers for Gemini API compatibility.
//! Currently not implemented - Gemini requests should use OpenAI or Claude protocols.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::proxy::server::AppState;

/// Stub: List available Gemini models
/// GET /v1beta/models
pub async fn handle_list_models(State(_state): State<AppState>) -> impl IntoResponse {
    // Return a minimal model list for compatibility
    Json(json!({
        "models": [
            {
                "name": "models/gemini-2.5-flash",
                "displayName": "Gemini 2.5 Flash",
                "description": "Fast and efficient model"
            },
            {
                "name": "models/gemini-2.5-pro",
                "displayName": "Gemini 2.5 Pro",
                "description": "Most capable model"
            },
            {
                "name": "models/gemini-3-flash",
                "displayName": "Gemini 3 Flash",
                "description": "Latest fast model"
            },
            {
                "name": "models/gemini-3-pro-preview",
                "displayName": "Gemini 3 Pro Preview",
                "description": "Latest pro model"
            }
        ]
    }))
}

/// Stub: Get model details
/// GET /v1beta/models/:model
pub async fn handle_get_model(
    State(_state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> impl IntoResponse {
    Json(json!({
        "name": format!("models/{}", model),
        "displayName": model,
        "description": "Model available via Antigravity proxy"
    }))
}

/// Stub: Generate content (not implemented - use OpenAI/Claude protocols)
/// POST /v1beta/models/:model:generateContent
pub async fn handle_generate(
    State(_state): State<AppState>,
    axum::extract::Path(_model): axum::extract::Path<String>,
    Json(_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": {
                "code": 501,
                "message": "Direct Gemini API not implemented. Use /v1/chat/completions (OpenAI) or /v1/messages (Claude) endpoints instead.",
                "status": "UNIMPLEMENTED"
            }
        })),
    )
}

/// Stub: Count tokens
/// POST /v1beta/models/:model:countTokens
pub async fn handle_count_tokens(
    State(_state): State<AppState>,
    axum::extract::Path(_model): axum::extract::Path<String>,
    Json(_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": {
                "code": 501,
                "message": "Token counting not implemented for direct Gemini API.",
                "status": "UNIMPLEMENTED"
            }
        })),
    )
}
