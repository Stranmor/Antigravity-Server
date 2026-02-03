// OpenAI Images Edits handler

use super::response_utils::{
    build_openai_response, extract_images_from_gemini_response, safety_settings,
};
use crate::proxy::server::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use base64::Engine as _;
use serde_json::{json, Value};
use tracing::info;

pub async fn handle_images_edits(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("[Images] Received edit request");

    let mut image_data = None;
    let mut mask_data = None;
    let mut prompt = String::new();
    let mut n = 1;
    let mut size = "1024x1024".to_string();
    let mut response_format = "b64_json".to_string();
    let mut model = "gemini-3-pro-image".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        if name == "image" {
            let data = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Image read error: {}", e)))?;
            image_data = Some(base64::engine::general_purpose::STANDARD.encode(data));
        } else if name == "mask" {
            let data = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Mask read error: {}", e)))?;
            mask_data = Some(base64::engine::general_purpose::STANDARD.encode(data));
        } else if name == "prompt" {
            prompt = field
                .text()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Prompt read error: {}", e)))?;
        } else if name == "n" {
            if let Ok(val) = field.text().await {
                n = val.parse().unwrap_or(1);
            }
        } else if name == "size" {
            if let Ok(val) = field.text().await {
                size = val;
            }
        } else if name == "response_format" {
            if let Ok(val) = field.text().await {
                response_format = val;
            }
        } else if name == "model" {
            if let Ok(val) = field.text().await {
                if !val.is_empty() {
                    model = val;
                }
            }
        }
    }

    if image_data.is_none() {
        return Err((StatusCode::BAD_REQUEST, "Missing image".to_string()));
    }
    if prompt.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Missing prompt".to_string()));
    }

    info!(
        "[Images] Edit Request: model={}, prompt={}, n={}, size={}, mask={}, response_format={}",
        model,
        prompt,
        n,
        size,
        mask_data.is_some(),
        response_format
    );

    let upstream = state.upstream.clone();
    let token_manager = state.token_manager;

    let (access_token, project_id, email, _active_guard) = match token_manager
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

    let mut contents_parts = Vec::new();

    contents_parts.push(json!({
        "text": format!("Edit this image: {}", prompt)
    }));

    if let Some(data) = image_data {
        contents_parts.push(json!({
            "inlineData": {
                "mimeType": "image/png",
                "data": data
            }
        }));
    }

    if let Some(data) = mask_data {
        contents_parts.push(json!({
            "inlineData": {
                "mimeType": "image/png",
                "data": data
            }
        }));
    }

    let gemini_body = json!({
        "project": project_id,
        "requestId": format!("img-edit-{}", uuid::Uuid::new_v4()),
        "model": model,
        "userAgent": "antigravity",
        "requestType": "image_gen",
        "request": {
            "contents": [{
                "role": "user",
                "parts": contents_parts
            }],
            "generationConfig": {
                "candidateCount": 1,
                "maxOutputTokens": 8192,
                "stopSequences": [],
                "temperature": 1.0,
                "topP": 0.95,
                "topK": 40
            },
            "safetySettings": safety_settings()
        }
    });

    let mut tasks = Vec::new();

    let warp_proxy = state.warp_isolation.get_proxy_for_email(&email).await;

    for _ in 0..n {
        let upstream = upstream.clone();
        let access_token = access_token.clone();
        let body = gemini_body.clone();
        let warp_proxy_clone = warp_proxy.clone();

        tasks.push(tokio::spawn(async move {
            match upstream
                .call_v1_internal_with_warp(
                    "generateContent",
                    &access_token,
                    body,
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
                        extract_images_from_gemini_response(&gemini_resp, &response_format);
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
        tracing::error!(
            "[Images] All {} edit requests failed. Errors: {}",
            n,
            error_msg
        );
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

    info!(
        "[Images] Successfully generated {} out of {} requested edited image(s)",
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
