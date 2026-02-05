use super::grounding::detects_networking_tool;
use super::image_config::{calculate_aspect_ratio_from_size, parse_image_config_with_params};
use super::resolve_request_config;
use serde_json::json;

#[test]
fn test_high_quality_model_auto_grounding() {
    let config = resolve_request_config("gpt-4o", "gemini-2.5-flash", &None, None, None);
    assert_eq!(config.request_type, "agent");
    assert!(!config.inject_google_search);
}

#[test]
fn test_gemini_native_tool_detection() {
    let tools = Some(vec![json!({
        "functionDeclarations": [
            { "name": "web_search", "parameters": {} }
        ]
    })]);
    assert!(detects_networking_tool(&tools));
}

#[test]
fn test_online_suffix_force_grounding() {
    let config =
        resolve_request_config("gemini-3-flash-online", "gemini-3-flash", &None, None, None);
    assert_eq!(config.request_type, "web_search");
    assert!(config.inject_google_search);
    assert_eq!(config.final_model, "gemini-2.5-flash");
}

#[test]
fn test_image_gen_model_detection() {
    let config =
        resolve_request_config("gemini-3-pro-image-2k-1", "gemini-3-pro-image", &None, None, None);
    assert_eq!(config.request_type, "image_gen");
    assert!(!config.inject_google_search);
}

#[test]
fn test_openai_tool_format_detection() {
    let tools = Some(vec![json!({
        "type": "function",
        "function": {
            "name": "web_search"
        }
    })]);
    assert!(detects_networking_tool(&tools));
}

#[test]
fn test_grounding_only_request_type() {
    // -online suffix triggers web_search mode
    let config =
        resolve_request_config("gemini-3-flash-online", "gemini-3-flash", &None, None, None);
    assert_eq!(config.request_type, "web_search");
    assert!(config.inject_google_search);
}

#[test]
fn test_parse_image_config_with_params_basic() {
    let (config, _) = parse_image_config_with_params("gemini-3-pro-image", None, None);
    assert!(config.is_object());
}

#[test]
fn test_aspect_ratio_calculation() {
    assert_eq!(calculate_aspect_ratio_from_size("1024x1024"), "1:1");
    assert_eq!(calculate_aspect_ratio_from_size("1024x768"), "4:3");
    assert_eq!(calculate_aspect_ratio_from_size("1920x1080"), "16:9");
    assert_eq!(calculate_aspect_ratio_from_size("1080x1920"), "9:16");
    assert_eq!(calculate_aspect_ratio_from_size("2560x1440"), "16:9");
}

#[test]
fn test_aspect_ratio_defaults_to_1_1() {
    assert_eq!(calculate_aspect_ratio_from_size("invalid"), "1:1");
    assert_eq!(calculate_aspect_ratio_from_size("1024"), "1:1");
    assert_eq!(calculate_aspect_ratio_from_size(""), "1:1");
}
