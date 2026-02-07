// Image generation configuration parsing
// Handles aspect ratio and quality settings for Gemini image models

use serde_json::{json, Value};

/// Extended version that accepts OpenAI size and quality parameters
pub fn parse_image_config_with_params(
    model_name: &str,
    size: Option<&str>,
    quality: Option<&str>,
) -> (Value, String) {
    let mut aspect_ratio = "1:1";

    if let Some(s) = size {
        aspect_ratio = calculate_aspect_ratio_from_size(s);
    } else if model_name.contains("-21x9") || model_name.contains("-21-9") {
        aspect_ratio = "21:9";
    } else if model_name.contains("-16x9") || model_name.contains("-16-9") {
        aspect_ratio = "16:9";
    } else if model_name.contains("-9x16") || model_name.contains("-9-16") {
        aspect_ratio = "9:16";
    } else if model_name.contains("-4x3") || model_name.contains("-4-3") {
        aspect_ratio = "4:3";
    } else if model_name.contains("-3x4") || model_name.contains("-3-4") {
        aspect_ratio = "3:4";
    } else if model_name.contains("-3x2") || model_name.contains("-3-2") {
        aspect_ratio = "3:2";
    } else if model_name.contains("-2x3") || model_name.contains("-2-3") {
        aspect_ratio = "2:3";
    } else if model_name.contains("-5x4") || model_name.contains("-5-4") {
        aspect_ratio = "5:4";
    } else if model_name.contains("-4x5") || model_name.contains("-4-5") {
        aspect_ratio = "4:5";
    } else if model_name.contains("-1x1") || model_name.contains("-1-1") {
        aspect_ratio = "1:1";
    }

    let mut config = serde_json::Map::new();
    config.insert("aspectRatio".to_string(), json!(aspect_ratio));

    if let Some(q) = quality {
        match q.to_lowercase().as_str() {
            "hd" | "4k" => {
                config.insert("imageSize".to_string(), json!("4K"));
            },
            "medium" | "2k" => {
                config.insert("imageSize".to_string(), json!("2K"));
            },
            "standard" | "1k" => {
                config.insert("imageSize".to_string(), json!("1K"));
            },
            // Intentionally ignored: unrecognized quality values use default image size
            _ => {},
        }
    } else {
        let is_hd = model_name.contains("-4k") || model_name.contains("-hd");
        let is_2k = model_name.contains("-2k");

        if is_hd {
            config.insert("imageSize".to_string(), json!("4K"));
        } else if is_2k {
            config.insert("imageSize".to_string(), json!("2K"));
        }
    }

    (Value::Object(config), "gemini-3-pro-image".to_string())
}

pub fn calculate_aspect_ratio_from_size(size: &str) -> &'static str {
    match size {
        "21:9" => return "21:9",
        "16:9" => return "16:9",
        "9:16" => return "9:16",
        "4:3" => return "4:3",
        "3:4" => return "3:4",
        "3:2" => return "3:2",
        "2:3" => return "2:3",
        "5:4" => return "5:4",
        "4:5" => return "4:5",
        "1:1" => return "1:1",
        // Not a direct ratio format — fall through to WxH pixel parsing below
        _ => {},
    }

    if let Some((w_str, h_str)) = size.split_once('x') {
        if let (Ok(width), Ok(height)) = (w_str.parse::<f64>(), h_str.parse::<f64>()) {
            if width > 0.0 && height > 0.0 {
                let ratio = width / height;

                if (ratio - 21.0 / 9.0).abs() < 0.05 {
                    return "21:9";
                }
                if (ratio - 16.0 / 9.0).abs() < 0.05 {
                    return "16:9";
                }
                if (ratio - 4.0 / 3.0).abs() < 0.05 {
                    return "4:3";
                }
                if (ratio - 3.0 / 4.0).abs() < 0.05 {
                    return "3:4";
                }
                if (ratio - 9.0 / 16.0).abs() < 0.05 {
                    return "9:16";
                }
                if (ratio - 3.0 / 2.0).abs() < 0.05 {
                    return "3:2";
                }
                if (ratio - 2.0 / 3.0).abs() < 0.05 {
                    return "2:3";
                }
                if (ratio - 5.0 / 4.0).abs() < 0.05 {
                    return "5:4";
                }
                if (ratio - 4.0 / 5.0).abs() < 0.05 {
                    return "4:5";
                }
                if (ratio - 1.0).abs() < 0.05 {
                    return "1:1";
                }
            }
        }
    }

    "1:1"
}
