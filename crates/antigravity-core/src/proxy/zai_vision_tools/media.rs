//! Media encoding and content conversion utilities for ZAI vision tools.

use base64::Engine;
use serde_json::{json, Value};

/// Check if value is an HTTP(S) URL.
pub fn is_http_url(value: &str) -> bool {
    let v = value.trim();
    v.starts_with("http://") || v.starts_with("https://")
}

/// Get MIME type for image file extension.
pub fn mime_for_image_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

/// Get MIME type for video file extension.
pub fn mime_for_video_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "mp4" => Some("video/mp4"),
        "mov" => Some("video/quicktime"),
        "m4v" => Some("video/x-m4v"),
        _ => None,
    }
}

/// Extract file extension from path.
pub fn file_ext(path: &std::path::Path) -> Option<String> {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

/// Encode file contents as base64 data URL.
pub fn encode_file_as_data_url(path: &std::path::Path, mime: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{};base64,{}", mime, encoded))
}

/// Convert image source (URL or file path) to OpenAI-compatible content block.
pub fn image_source_to_content(image_source: &str, max_size_mb: u64) -> Result<Value, String> {
    if is_http_url(image_source) {
        return Ok(json!({
            "type": "image_url",
            "image_url": { "url": image_source }
        }));
    }

    let path = std::path::Path::new(image_source);
    let meta = std::fs::metadata(path).map_err(|_| "Image file not found".to_string())?;
    let max_size = max_size_mb * 1024 * 1024;
    if meta.len() > max_size {
        return Err(format!(
            "Image file too large ({} bytes), max {} MB",
            meta.len(),
            max_size_mb
        ));
    }

    let ext = file_ext(path).ok_or("Unsupported image format".to_string())?;
    let mime = mime_for_image_extension(&ext).ok_or("Unsupported image format".to_string())?;
    let data_url = encode_file_as_data_url(path, mime)?;
    Ok(json!({
        "type": "image_url",
        "image_url": { "url": data_url }
    }))
}

/// Convert video source (URL or file path) to OpenAI-compatible content block.
pub fn video_source_to_content(video_source: &str, max_size_mb: u64) -> Result<Value, String> {
    if is_http_url(video_source) {
        return Ok(json!({
            "type": "video_url",
            "video_url": { "url": video_source }
        }));
    }

    let path = std::path::Path::new(video_source);
    let meta = std::fs::metadata(path).map_err(|_| "Video file not found".to_string())?;
    let max_size = max_size_mb * 1024 * 1024;
    if meta.len() > max_size {
        return Err(format!(
            "Video file too large ({} bytes), max {} MB",
            meta.len(),
            max_size_mb
        ));
    }

    let ext = file_ext(path).ok_or("Unsupported video format".to_string())?;
    let mime = mime_for_video_extension(&ext).ok_or("Unsupported video format".to_string())?;
    let data_url = encode_file_as_data_url(path, mime)?;
    Ok(json!({
        "type": "video_url",
        "video_url": { "url": data_url }
    }))
}

/// Build user message with content array and text prompt.
pub fn user_message_with_content(mut content: Vec<Value>, prompt: &str) -> Value {
    content.push(json!({ "type": "text", "text": prompt }));
    json!({ "role": "user", "content": content })
}
