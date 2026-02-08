use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use tracing::{debug, info};
use uuid::Uuid;

use crate::proxy::{audio::AudioProcessor, server::AppState};

/// handleaudiotranscriptionrequest (OpenAI Whisper API compatible)
pub async fn handle_audio_transcription(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut audio_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let model = "gemini-2.5-flash".to_string();
    let mut prompt = "Generate a transcript of the speech.".to_string();

    // 1. parse multipart/form-data
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("parsetablefailed: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                audio_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, format!("readfilefailed: {}", e)))?
                        .to_vec(),
                );
            },
            "model" => {
                // Ignore client-provided model (whisper-1, etc.) — always use gemini-3-pro for audio
                let _ = field.text().await;
            },
            "prompt" => {
                prompt = field.text().await.unwrap_or(prompt);
            },
            // Intentionally ignored: unknown multipart fields (language, response_format, etc.)
            _ => {
                tracing::trace!("Ignoring unknown multipart field: {}", name);
            },
        }
    }

    let audio_bytes =
        audio_data.ok_or((StatusCode::BAD_REQUEST, "Missing audiofile".to_string()))?;

    let file_name =
        filename.ok_or((StatusCode::BAD_REQUEST, "Failed to getfilename".to_string()))?;

    info!(
        "receivedaudiotranscriptionrequest: file={}, size={} bytes, model={}",
        file_name,
        audio_bytes.len(),
        model
    );

    // 2. Detect MIME type from magic bytes (more secure than extension)
    let mime_type = AudioProcessor::detect_mime_type(&file_name, &audio_bytes)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // 3. verifyfilesize
    if AudioProcessor::exceeds_size_limit(audio_bytes.len()) {
        let size_mb = audio_bytes.len() as f64 / (1024.0 * 1024.0);
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "audiofiletoo large ({:.1} MB)。maxsupport 15 MB (approximately 16 minute MP3)。suggest: 1) compressaudioquality 2) segmentupload",
                size_mb
            ),
        ));
    }

    // 4. use Inline Data method
    debug!("use Inline Data methodhandle");
    let base64_audio = AudioProcessor::encode_to_base64(&audio_bytes);

    // 5. build Gemini request
    let gemini_request = json!({
        "contents": [{
            "role": "user",
            "parts": [
                {"text": prompt},
                {
                    "inlineData": {
                        "mimeType": mime_type,
                        "data": base64_audio
                    }
                }
            ]
        }]
    });

    // 6. get Token  and upstreamclient
    let token_manager = state.token_manager;
    let (access_token, project_id, email, _guard) = token_manager
        .get_token("text", false, None, &model)
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;

    info!("useaccount: {}", email);

    // 7. wraprequestas v1internal format
    // Use model as-is — Antigravity API expects "gemini-3-pro" directly
    let mapped_model = model.clone();
    let wrapped_body = json!({
        "project": project_id,
        "requestId": format!("audio-{}", Uuid::new_v4()),
        "request": gemini_request,
        "model": mapped_model,
        "userAgent": "antigravity",
        "requestType": "text"
    });

    // 8. sendrequestto Gemini
    let upstream = state.upstream.clone();

    let response = upstream
        .call_v1_internal_with_warp(
            "generateContent",
            &access_token,
            wrapped_body,
            None,
            std::collections::HashMap::new(),
            None,
        )
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("upstreamRequest failed: {}", e)))?;

    if !response.status().is_success() {
        let status_code = response.status().as_u16();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        tracing::warn!("Audio upstream error {}: {}", status_code, error_text);
        return Err((
            StatusCode::BAD_GATEWAY,
            crate::proxy::common::sanitize_upstream_error(status_code, &error_text),
        ));
    }

    let result: Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("parseResponse failed: {}", e)))?;

    // 9. extracttextresponse（unwrap v1internal response）
    let inner_response = result.get("response").unwrap_or(&result);
    let text = inner_response
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    info!("audiotranscriptioncomplete，return {} character", text.len());

    // 10. returnstandardformatresponse
    Ok((
        StatusCode::OK,
        [("X-Account-Email", email.as_str())],
        Json(json!({
            "text": text
        })),
    )
        .into_response())
}
