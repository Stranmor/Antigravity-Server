// Shared utilities for image response processing

use serde_json::{json, Value};

/// Extract images from Gemini response and convert to OpenAI format
pub fn extract_images_from_gemini_response(
    gemini_resp: &Value,
    response_format: &str,
) -> Vec<Value> {
    let mut images = Vec::new();
    let raw = gemini_resp.get("response").unwrap_or(gemini_resp);

    if let Some(parts) = raw
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|cand| cand.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|p| p.as_array())
    {
        for part in parts {
            if let Some(img) = part.get("inlineData") {
                let data = img.get("data").and_then(|v| v.as_str()).unwrap_or("");
                if !data.is_empty() {
                    if response_format == "url" {
                        let mime_type = img
                            .get("mimeType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("image/png");
                        images.push(json!({
                            "url": format!("data:{};base64,{}", mime_type, data)
                        }));
                    } else {
                        images.push(json!({
                            "b64_json": data
                        }));
                    }
                }
            }
        }
    }

    images
}

/// Map OpenAI size parameter to Gemini aspect ratio
pub fn size_to_aspect_ratio(size: &str) -> &'static str {
    match size {
        "1792x768" | "2560x1080" => "21:9", // Ultra-wide
        "1792x1024" | "1920x1080" => "16:9",
        "1024x1792" | "1080x1920" => "9:16",
        "1024x768" | "1280x960" => "4:3",
        "768x1024" | "960x1280" => "3:4",
        _ => "1:1", // Default 1024x1024
    }
}

/// Build OpenAI-compatible image response
pub fn build_openai_response(images: Vec<Value>) -> Value {
    json!({
        "created": chrono::Utc::now().timestamp(),
        "data": images
    })
}

/// Enhance prompt based on quality and style parameters
pub fn enhance_prompt(prompt: &str, quality: &str, style: &str) -> String {
    let mut final_prompt = prompt.to_string();
    if quality == "hd" {
        final_prompt.push_str(", (high quality, highly detailed, 4k resolution, hdr)");
    }
    match style {
        "vivid" => final_prompt.push_str(", (vivid colors, dramatic lighting, rich details)"),
        "natural" => final_prompt.push_str(", (natural lighting, realistic, photorealistic)"),
        _ => {}
    }
    final_prompt
}

/// Standard safety settings for image generation
pub fn safety_settings() -> Value {
    json!([
        { "category": "HARM_CATEGORY_HARASSMENT", "threshold": "OFF" },
        { "category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "OFF" },
        { "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "threshold": "OFF" },
        { "category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "OFF" },
    ])
}
