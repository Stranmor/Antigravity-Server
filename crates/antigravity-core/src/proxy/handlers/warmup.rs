// warmuphandler - internalwarmup API
//
// provide /internal/warmup endpoint，support：
// - specifyaccount（via email）
// - specifymodel（notdomapping，directlyuseoriginalmodelname）
// - reuseproxy allinfrastructure（UpstreamClient、TokenManager）

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::proxy::mappers::gemini::wrapper::wrap_request;
use crate::proxy::server::AppState;

/// warmuprequestbody
#[derive(Debug, Deserialize)]
pub struct WarmupRequest {
    /// accountemail
    pub email: String,
    /// modelname（originalname，notdomapping）
    pub model: String,
    /// optional：directlyprovide Access Token（fornot in TokenManager in account）
    pub access_token: Option<String>,
    /// optional：directlyprovide Project ID
    pub project_id: Option<String>,
}

/// warmupresponse
#[derive(Debug, Serialize)]
pub struct WarmupResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// handlewarmuprequest
pub async fn handle_warmup(
    State(state): State<AppState>,
    Json(req): Json<WarmupRequest>,
) -> Response {
    // ===== pre-check：skip gemini-2.5-* familymodel =====
    // These models return 400 INVALID_ARGUMENT with current warmup protocol
    let model_lower = req.model.to_lowercase();
    if model_lower.contains("2.5-") || model_lower.contains("2-5-") {
        info!(
            "[Warmup-API] SKIP: gemini-2.5-* model not supported for warmup: {} / {}",
            req.email, req.model
        );
        return (
            StatusCode::OK,
            Json(WarmupResponse {
                success: true,
                message: format!("Skipped warmup for {} (2.5 models not supported)", req.model),
                error: None,
            }),
        )
            .into_response();
    }

    info!("[Warmup-API] ========== START: email={}, model={} ==========", req.email, req.model);

    // ===== step 1: get Token =====
    let (access_token, project_id) =
        if let (Some(at), Some(pid)) = (&req.access_token, &req.project_id) {
            (at.clone(), pid.clone())
        } else {
            match state.token_manager.get_token_by_email(&req.email).await {
                Ok((at, pid, _)) => (at, pid),
                Err(e) => {
                    warn!("[Warmup-API] Step 1 FAILED: Token error for {}: {}", req.email, e);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(WarmupResponse {
                            success: false,
                            message: format!("Failed to get token for {}", req.email),
                            error: Some("Upstream request failed".to_string()),
                        }),
                    )
                        .into_response();
                },
            }
        };

    // ===== step 2: based onmodeltypebuildrequestbody =====
    let is_claude = antigravity_types::ModelFamily::from_model_name(&req.model).is_claude();
    let is_image = req.model.to_lowercase().contains("image");

    let body: Value = if is_claude {
        // Claude model：use transform_claude_request_in convert
        let session_id = format!(
            "warmup_{}_{}",
            chrono::Utc::now().timestamp_millis(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let claude_request = crate::proxy::mappers::claude::models::ClaudeRequest {
            model: req.model.clone(),
            messages: vec![crate::proxy::mappers::claude::models::Message {
                role: "user".to_string(),
                content: crate::proxy::mappers::claude::models::MessageContent::String(
                    "ping".to_string(),
                ),
            }],
            max_tokens: Some(1),
            stream: false,
            system: None,
            temperature: None,
            top_p: None,
            top_k: None,
            tools: None,
            metadata: Some(crate::proxy::mappers::claude::models::Metadata {
                user_id: Some(session_id),
            }),
            thinking: None,
            output_config: None,
        };

        match crate::proxy::mappers::claude::transform_claude_request_in(
            &claude_request,
            &project_id,
            false,
        ) {
            Ok(transformed) => transformed,
            Err(e) => {
                warn!("[Warmup-API] Step 2 FAILED: Claude transform error: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(WarmupResponse {
                        success: false,
                        message: format!("Transform error: {}", e),
                        error: Some(e),
                    }),
                )
                    .into_response();
            },
        }
    } else {
        // Gemini model：use wrap_request
        let session_id = format!(
            "warmup_{}_{}",
            chrono::Utc::now().timestamp_millis(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        let mut base_request = json!({
            "contents": [{"role": "user", "parts": [{"text": "Say hi"}]}],
            "generationConfig": {
                "temperature": 0
            }
        });
        if is_image {
            if let Some(gen_config) = base_request.get_mut("generationConfig") {
                gen_config["maxOutputTokens"] = json!(10);
            }
        }

        wrap_request(&base_request, &project_id, &req.model, Some(&session_id))
    };

    // ===== step 3: call UpstreamClient =====
    let model_lower = req.model.to_lowercase();
    let prefer_non_stream = model_lower.contains("flash-lite");

    let (method, query) = if prefer_non_stream {
        (format!("models/{}:generateContent", req.model), None)
    } else {
        (format!("models/{}:streamGenerateContent", req.model), Some("alt=sse"))
    };

    let mut result = state
        .upstream
        .call_v1_internal_with_warp(
            &method,
            &access_token,
            body.clone(),
            query,
            std::collections::HashMap::new(),
            None,
        )
        .await;

    // if streamingRequest failed，attemptnon-streamingrequest
    if result.is_err() && !prefer_non_stream {
        result = state
            .upstream
            .call_v1_internal_with_warp(
                &format!("models/{}:generateContent", req.model),
                &access_token,
                body,
                None,
                std::collections::HashMap::new(),
                None,
            )
            .await;
    }

    // ===== step 4: handleresponse =====
    match result {
        Ok(response) => {
            let status = response.status();
            let mut response = if status.is_success() {
                info!("[Warmup-API] ========== SUCCESS: {} / {} ==========", req.email, req.model);
                (
                    StatusCode::OK,
                    Json(WarmupResponse {
                        success: true,
                        message: format!("Warmup triggered for {}", req.model),
                        error: None,
                    }),
                )
                    .into_response()
            } else {
                let status_code = status.as_u16();
                let error_text = response.text().await.unwrap_or_default();
                let error_detail = if error_text.is_empty() {
                    format!("Upstream returned {}", status_code)
                } else {
                    format!("Upstream returned {}: {}", status_code, error_text)
                };
                tracing::warn!(
                    "[Warmup-API] Upstream error {} for {}/{}: {}",
                    status_code,
                    req.email,
                    req.model,
                    error_text
                );
                (
                    StatusCode::from_u16(status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                    Json(WarmupResponse {
                        success: false,
                        message: format!("Warmup failed: HTTP {}", status_code),
                        error: Some(error_detail),
                    }),
                )
                    .into_response()
            };

            // addresponseheader，letmonitormiddlewarecaptureaccountinfo
            if let Ok(email_val) = axum::http::HeaderValue::from_str(&req.email) {
                response.headers_mut().insert("X-Account-Email", email_val);
            }
            if let Ok(model_val) = axum::http::HeaderValue::from_str(&req.model) {
                response.headers_mut().insert("X-Mapped-Model", model_val);
            }

            response
        },
        Err(e) => {
            warn!(
                "[Warmup-API] ========== ERROR: {} / {} - {} ==========",
                req.email, req.model, e
            );

            let mut response = (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(WarmupResponse {
                    success: false,
                    message: "Warmup request failed".to_string(),
                    error: Some(e),
                }),
            )
                .into_response();

            // even iffailedalsoaddresponseheader，formonitor
            if let Ok(email_val) = axum::http::HeaderValue::from_str(&req.email) {
                response.headers_mut().insert("X-Account-Email", email_val);
            }
            if let Ok(model_val) = axum::http::HeaderValue::from_str(&req.model) {
                response.headers_mut().insert("X-Mapped-Model", model_val);
            }

            response
        },
    }
}
