//! Dispatch mode decision logic for Claude messages handler

use crate::proxy::mappers::claude::ClaudeRequest;
use crate::proxy::server::AppState;
use crate::proxy::ZaiDispatchMode;
use axum::http::HeaderMap;
use axum::response::Response;
use std::sync::atomic::Ordering;

pub struct DispatchDecision {
    pub use_zai: bool,
    pub normalized_model: String,
}

pub async fn decide_dispatch_mode(
    state: &AppState,
    request: &ClaudeRequest,
    trace_id: &str,
) -> DispatchDecision {
    let zai = state.zai.read().await.clone();
    let zai_enabled = zai.enabled && !matches!(zai.dispatch_mode, ZaiDispatchMode::Off);
    let google_accounts = state.token_manager.len();

    let normalized_model =
        crate::proxy::common::model_mapping::normalize_to_standard_id(&request.model)
            .unwrap_or_else(|| request.model.clone());

    let use_zai = if !zai_enabled {
        false
    } else {
        match zai.dispatch_mode {
            ZaiDispatchMode::Off => false,
            ZaiDispatchMode::Exclusive => true,
            ZaiDispatchMode::Fallback => {
                if google_accounts == 0 {
                    tracing::info!(
                        "[{}] No Google accounts available, using fallback provider",
                        trace_id
                    );
                    true
                } else {
                    let has_available = state
                        .token_manager
                        .has_available_account("claude", &normalized_model)
                        .await;
                    if !has_available {
                        tracing::info!(
                            "[{}] All Google accounts unavailable (rate-limited or quota-protected for {}), using fallback provider",
                            trace_id,
                            request.model
                        );
                    }
                    !has_available
                }
            }
            ZaiDispatchMode::Pooled => {
                let total = google_accounts.saturating_add(1).max(1);
                let slot = state.provider_rr.fetch_add(1, Ordering::Relaxed) % total;
                slot == 0
            }
        }
    };

    DispatchDecision {
        use_zai,
        normalized_model,
    }
}

pub async fn forward_to_zai(
    state: &AppState,
    headers: &HeaderMap,
    request: &ClaudeRequest,
) -> Result<Response, String> {
    let new_body = serde_json::to_value(request)
        .map_err(|e| format!("Failed to serialize fixed request for z.ai: {}", e))?;

    Ok(
        crate::proxy::providers::zai_anthropic::forward_anthropic_json(
            state,
            axum::http::Method::POST,
            "/v1/messages",
            headers,
            new_body,
        )
        .await,
    )
}
