use super::super::models::*;
use crate::proxy::common::media_detect::detect_image_mime;
use percent_encoding::percent_decode_str;
use serde_json::{json, Value};

/// Maximum file size for local file reads (100 MB).
const MAX_LOCAL_FILE_SIZE: u64 = 100 * 1024 * 1024;

pub fn transform_content_block(block: &OpenAIContentBlock) -> Option<Value> {
    match block {
        OpenAIContentBlock::Text { text } => Some(json!({"text": text})),
        OpenAIContentBlock::ImageUrl { image_url } => transform_image_url(image_url),
        OpenAIContentBlock::InputAudio { .. } => {
            if let Some(audio) = block.extract_audio() {
                let mime_type = match audio.format.as_str() {
                    "wav" => "audio/wav",
                    "mp3" => "audio/mp3",
                    "ogg" => "audio/ogg",
                    "flac" => "audio/flac",
                    "m4a" | "aac" => "audio/aac",
                    _ => "audio/wav",
                };
                Some(json!({
                    "inlineData": { "mimeType": mime_type, "data": &audio.data }
                }))
            } else {
                None
            }
        },
        OpenAIContentBlock::VideoUrl { video_url } => transform_video_url(video_url),
    }
}

fn transform_image_url(image_url: &OpenAIImageUrl) -> Option<Value> {
    if image_url.url.starts_with("data:") {
        if let Some(pos) = image_url.url.find(',') {
            let mime_part = &image_url.url[5..pos];
            let mime_type = mime_part.split(';').next().unwrap_or("image/jpeg");
            let data = &image_url.url[pos + 1..];
            let detected = detect_image_mime(data, mime_type);
            return Some(json!({
                "inlineData": { "mimeType": detected, "data": data }
            }));
        }
        None
    } else if image_url.url.starts_with("http") {
        Some(json!({
            "fileData": { "fileUri": &image_url.url, "mimeType": "image/jpeg" }
        }))
    } else {
        transform_local_image(&image_url.url)
    }
}

fn transform_local_image(url: &str) -> Option<Value> {
    let file_path = decode_file_url(url);

    tracing::debug!("[OpenAI-Request] Reading local image: {}", file_path);

    if let Err(e) = check_file_size(&file_path) {
        tracing::warn!("[OpenAI-Request] {}", e);
        return None;
    }

    if let Ok(file_bytes) = std::fs::read(&file_path) {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&file_bytes);

        let mime_type = if file_path.to_lowercase().ends_with(".png") {
            "image/png"
        } else if file_path.to_lowercase().ends_with(".gif") {
            "image/gif"
        } else if file_path.to_lowercase().ends_with(".webp") {
            "image/webp"
        } else {
            "image/jpeg"
        };
        let detected = detect_image_mime(&b64, mime_type);

        tracing::debug!(
            "[OpenAI-Request] Successfully loaded image: {} ({} bytes)",
            file_path,
            file_bytes.len()
        );
        Some(json!({
            "inlineData": { "mimeType": detected, "data": b64 }
        }))
    } else {
        tracing::debug!("[OpenAI-Request] Failed to read local image: {}", file_path);
        None
    }
}

fn transform_video_url(video_url: &VideoUrlContent) -> Option<Value> {
    if video_url.url.starts_with("data:") {
        if let Some(pos) = video_url.url.find(',') {
            let mime_part = &video_url.url[5..pos];
            let mime_type = mime_part.split(';').next().unwrap_or("video/mp4");
            let data = &video_url.url[pos + 1..];
            return Some(json!({
                "inlineData": { "mimeType": mime_type, "data": data }
            }));
        }
        None
    } else if video_url.url.starts_with("http") {
        Some(json!({
            "fileData": { "fileUri": &video_url.url, "mimeType": "video/mp4" }
        }))
    } else {
        transform_local_video(&video_url.url)
    }
}

fn transform_local_video(url: &str) -> Option<Value> {
    let file_path = decode_file_url(url);

    tracing::debug!("[OpenAI-Request] Reading local video: {}", file_path);

    if let Err(e) = check_file_size(&file_path) {
        tracing::warn!("[OpenAI-Request] {}", e);
        return None;
    }

    if let Ok(file_bytes) = std::fs::read(&file_path) {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&file_bytes);

        let mime_type = if file_path.to_lowercase().ends_with(".mp4") {
            "video/mp4"
        } else if file_path.to_lowercase().ends_with(".mov") {
            "video/quicktime"
        } else if file_path.to_lowercase().ends_with(".webm") {
            "video/webm"
        } else if file_path.to_lowercase().ends_with(".avi") {
            "video/x-msvideo"
        } else {
            "video/mp4"
        };

        tracing::debug!(
            "[OpenAI-Request] Successfully loaded video: {} ({} bytes)",
            file_path,
            file_bytes.len()
        );
        Some(json!({
            "inlineData": { "mimeType": mime_type, "data": b64 }
        }))
    } else {
        tracing::debug!("[OpenAI-Request] Failed to read local video: {}", file_path);
        None
    }
}

fn decode_file_url(url: &str) -> String {
    let raw_path = if url.starts_with("file://") {
        #[cfg(target_os = "windows")]
        {
            url.trim_start_matches("file:///").replace('/', "\\")
        }
        #[cfg(not(target_os = "windows"))]
        {
            url.trim_start_matches("file://").to_string()
        }
    } else {
        url.to_string()
    };
    percent_decode_str(&raw_path).decode_utf8_lossy().into_owned()
}

fn check_file_size(path: &str) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(meta) => {
            if meta.len() > MAX_LOCAL_FILE_SIZE {
                Err(format!(
                    "File too large: {} ({} bytes, limit {} bytes)",
                    path,
                    meta.len(),
                    MAX_LOCAL_FILE_SIZE
                ))
            } else {
                Ok(())
            }
        },
        Err(e) => Err(format!("Cannot stat file {}: {}", path, e)),
    }
}
