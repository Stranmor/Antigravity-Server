// Request configuration resolution for all protocols
// Provides unified grounding/networking logic

mod grounding;
mod image_config;
#[cfg(test)]
mod tests;

use serde_json::Value;

pub use grounding::{
    contains_non_networking_tool, deep_clean_undefined, detects_networking_tool,
    inject_google_search_tool,
};
pub use image_config::{calculate_aspect_ratio_from_size, parse_image_config_with_params};

#[derive(Debug, Clone)]
pub struct RequestConfig {
    pub request_type: String,
    pub inject_google_search: bool,
    pub final_model: String,
    pub image_config: Option<Value>,
}

pub fn resolve_request_config(
    original_model: &str,
    mapped_model: &str,
    tools: &Option<Vec<Value>>,
    size: Option<&str>,
    quality: Option<&str>,
) -> RequestConfig {
    // 1. Image Generation Check (Priority)
    if mapped_model.starts_with("gemini-3-pro-image") {
        let (image_config, parsed_base_model) =
            parse_image_config_with_params(original_model, size, quality);

        return RequestConfig {
            request_type: "image_gen".to_string(),
            inject_google_search: false,
            final_model: parsed_base_model,
            image_config: Some(image_config),
        };
    }

    // Detect whether there is a networking tool definition (built-in feature call)
    let has_networking_tool = detects_networking_tool(tools);
    // Detect whether containing non-networking tool (e.g., MCP local tool)
    let _has_non_networking = contains_non_networking_tool(tools);

    // Strip -online suffix from original model if present (to detect networking intent)
    let is_online_suffix = original_model.ends_with("-online");

    // High-quality grounding allowlist (Only for models known to support search and be relatively 'safe')
    let _is_high_quality_model = mapped_model == "gemini-2.5-flash"
        || mapped_model == "gemini-1.5-pro"
        || mapped_model.starts_with("gemini-1.5-pro-")
        || mapped_model.starts_with("gemini-2.5-flash-")
        || mapped_model.starts_with("gemini-2.0-flash")
        || mapped_model.starts_with("gemini-3-")
        || mapped_model.contains("claude-3-5-sonnet")
        || mapped_model.contains("claude-3-opus")
        || mapped_model.contains("claude-sonnet")
        || mapped_model.contains("claude-opus")
        || mapped_model.contains("claude-4");

    // Determine if we should enable networking
    // [FIX] Disable model-based auto-networking logic to prevent image requests from being overwritten by search results.
    // Only enable when user explicitly requests networking: 1) -online suffix 2) carries networking tool definition
    let enable_networking = is_online_suffix || has_networking_tool;

    // The final model to send upstream should be the MAPPED model,
    // but if searching, we MUST ensure the model name is one the backend associates with search.
    // Force a stable search model for search requests.
    let mut final_model = mapped_model.trim_end_matches("-online").to_string();

    // [FIX] Map logic aliases back to physical model names for upstream compatibility
    final_model = match final_model.as_str() {
        "gemini-3-pro-preview" => "gemini-3-pro-high".to_string(), // Preview maps back to High
        "gemini-3-pro-image-preview" => "gemini-3-pro-image".to_string(),
        "gemini-3-flash-preview" => "gemini-3-flash".to_string(),
        _ => final_model,
    };

    if enable_networking {
        // [FIX] Only gemini-2.5-flash supports googleSearch tool
        // All other models (including Gemini 3 Pro, thinking models, Claude aliases) must downgrade
        if final_model != "gemini-2.5-flash" {
            tracing::info!(
                "[Common-Utils] Downgrading {} to gemini-2.5-flash for web search (only gemini-2.5-flash supports googleSearch)",
                final_model
            );
            final_model = "gemini-2.5-flash".to_string();
        }
    }

    RequestConfig {
        request_type: if enable_networking {
            "web_search".to_string()
        } else {
            "agent".to_string()
        },
        inject_google_search: enable_networking,
        final_model,
        image_config: None,
    }
}
