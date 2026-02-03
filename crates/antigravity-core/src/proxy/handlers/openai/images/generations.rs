// OpenAI Images Generations handler

use super::response_utils::{
    build_openai_response, enhance_prompt, extract_images_from_gemini_response, safety_settings,
    size_to_aspect_ratio,
};
use crate::proxy::server::AppState;
use axum::{extract::Json, extract::State, http::StatusCode, response::IntoResponse};
use serde_json::{json, Value};
use tracing::info;

pub async fn handle_images_generations(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let prompt = body.get("prompt").and_then(|v| v.as_str()).ok_or((
        StatusCode::BAD_REQUEST,
        "Missing 'prompt' field".to_string(),
    ))?;

    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gemini-3-pro-image");

    let n = body.get("n").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

    let size = body
        .get("size")
        .and_then(|v| v.as_str())
        .unwrap_or("1024x1024");

    let response_format = body
        .get("response_format")
        .and_then(|v| v.as_str())
        .unwrap_or("b64_json");

    let quality = body
        .get("quality")
        .and_then(|v| v.as_str())
        .unwrap_or("standard");
    let style = body
        .get("style")
        .and_then(|v| v.as_str())
        .unwrap_or("vivid");

    info!(
        "[Images] Received request: model={}, prompt={:.50}..., n={}, size={}, quality={}, style={}",
        model, prompt, n, size, quality, style
    );

    let aspect_ratio = size_to_aspect_ratio(size);
    let final_prompt = enhance_prompt(prompt, quality, style);

    let upstream = state.upstream.clone();
    let token_manager = state.token_manager;

    let (access_token, project_id, email, _guard) = match token_manager
        .get_token("image_gen", false, None, "dall-e-3")
        .await
    {
        Ok(t) => t,
        Err(e) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Token error: {}", e),
            ));
        }
    };

    info!("✓ Using account: {} for image generation", email);

    let warp_proxy = state.warp_isolation.get_proxy_for_email(&email).await;

    let mut tasks = Vec::new();

    for _ in 0..n {
        let upstream = upstream.clone();
        let access_token = access_token.clone();
        let project_id = project_id.clone();
        let final_prompt = final_prompt.clone();
        let aspect_ratio = aspect_ratio.to_string();
        let warp_proxy_clone = warp_proxy.clone();

        tasks.push(tokio::spawn(async move {
            let gemini_body = json!({
                "project": project_id,
                "requestId": format!("img-{}", uuid::Uuid::new_v4()),
                "model": "gemini-3-pro-image",
                "userAgent": "antigravity",
                "requestType": "image_gen",
                "request": {
                    "contents": [{
                        "role": "user",
                        "parts": [{"text": final_prompt}]
                    }],
                    "generationConfig": {
                        "candidateCount": 1,
                        "imageConfig": {
                            "aspectRatio": aspect_ratio
                        }
                    },
                    "safetySettings": safety_settings()
                }
            });

            match upstream
                .call_v1_internal_with_warp(
                    "generateContent",
                    &access_token,
                    gemini_body,
                    None,
                    std::collections::HashMap::new(),
                    warp_proxy_clone.as_deref(),
                )
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    if !status.is_success() {
                        let err_text = response.text().await.unwrap_or_default();
                        return Err(format!("Upstream error {}: {}", status, err_text));
                    }
                    match response.json::<Value>().await {
                        Ok(json) => Ok(json),
                        Err(e) => Err(format!("Parse error: {}", e)),
                    }
                }
                Err(e) => Err(format!("Network error: {}", e)),
            }
        }));
    }

    let mut images: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (idx, task) in tasks.into_iter().enumerate() {
        match task.await {
            Ok(result) => match result {
                Ok(gemini_resp) => {
                    let extracted =
                        extract_images_from_gemini_response(&gemini_resp, response_format);
                    if !extracted.is_empty() {
                        images.extend(extracted);
                        tracing::debug!("[Images] Task {} succeeded", idx);
                    }
                }
                Err(e) => {
                    tracing::error!("[Images] Task {} failed: {}", idx, e);
                    errors.push(e);
                }
            },
            Err(e) => {
                let err_msg = format!("Task join error: {}", e);
                tracing::error!("[Images] Task {} join error: {}", idx, e);
                errors.push(err_msg);
            }
        }
    }

    if images.is_empty() {
        let error_msg = if !errors.is_empty() {
            errors.join("; ")
        } else {
            "No images generated".to_string()
        };
        tracing::error!("[Images] All {} requests failed. Errors: {}", n, error_msg);
        return Err((StatusCode::BAD_GATEWAY, error_msg));
    }

    if !errors.is_empty() {
        tracing::warn!(
            "[Images] Partial success: {} out of {} requests succeeded. Errors: {}",
            images.len(),
            n,
            errors.join("; ")
        );
    }

    tracing::info!(
        "[Images] Successfully generated {} out of {} requested image(s)",
        images.len(),
        n
    );

    let openai_response = build_openai_response(images);

    Ok((
        StatusCode::OK,
        [("X-Account-Email", email.as_str())],
        Json(openai_response),
    )
        .into_response())
}
