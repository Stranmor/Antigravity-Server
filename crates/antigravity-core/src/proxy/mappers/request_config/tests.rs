#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_high_quality_model_auto_grounding() {
        // Auto-grounding is currently disabled by default due to conflict with image gen
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
    fn test_default_no_grounding() {
        let config = resolve_request_config("claude-sonnet", "gemini-3-flash", &None, None, None);
        assert_eq!(config.request_type, "agent");
        assert!(!config.inject_google_search);
    }

    #[test]
    fn test_image_model_excluded() {
        let config = resolve_request_config(
            "gemini-3-pro-image",
            "gemini-3-pro-image",
            &None,
            None,
            None,
        );
        assert_eq!(config.request_type, "image_gen");
        assert!(!config.inject_google_search);
    }

    #[test]
    fn test_image_2k_and_ultrawide_config() {
        // Test 2K
        let (config_2k, _) = parse_image_config("gemini-3-pro-image-2k");
        assert_eq!(config_2k["imageSize"], "2K");

        // Test 21:9
        let (config_21x9, _) = parse_image_config("gemini-3-pro-image-21x9");
        assert_eq!(config_21x9["aspectRatio"], "21:9");

        // Test Combined (if logic allows, though suffix parsing is greedy)
        let (config_combined, _) = parse_image_config("gemini-3-pro-image-2k-21x9");
        assert_eq!(config_combined["imageSize"], "2K");
        assert_eq!(config_combined["aspectRatio"], "21:9");

        // Test 4K + 21:9
        let (config_4k_wide, _) = parse_image_config("gemini-3-pro-image-4k-21x9");
        assert_eq!(config_4k_wide["imageSize"], "4K");
        assert_eq!(config_4k_wide["aspectRatio"], "21:9");
    }

    #[test]
    fn test_parse_image_config_with_openai_params() {
        // Test OpenAI size parameter (e.g., "1024x1024")
        let (config, _) =
            parse_image_config_with_params("gemini-3-pro-image", Some("1024x1024"), None);
        assert_eq!(config["aspectRatio"], "1:1");

        // Test landscape size
        let (config_landscape, _) =
            parse_image_config_with_params("gemini-3-pro-image", Some("1792x1024"), None);
        assert_eq!(config_landscape["aspectRatio"], "16:9");

        // Test portrait size
        let (config_portrait, _) =
            parse_image_config_with_params("gemini-3-pro-image", Some("1024x1792"), None);
        assert_eq!(config_portrait["aspectRatio"], "9:16");

        // Test quality parameter (hd → 4K)
        let (config_hd, _) = parse_image_config_with_params("gemini-3-pro-image", None, Some("hd"));
        assert_eq!(config_hd["imageSize"], "4K");

        // Test quality standard (no change to default)
        let (config_std, _) =
            parse_image_config_with_params("gemini-3-pro-image", None, Some("standard"));
        // Standard should not set 4K
        assert_ne!(
            config_std.get("imageSize").and_then(|v| v.as_str()),
            Some("4K")
        );
    }

    #[test]
    fn test_calculate_aspect_ratio_from_size() {
        // Common OpenAI sizes
        assert_eq!(calculate_aspect_ratio_from_size("1024x1024"), "1:1");
        assert_eq!(calculate_aspect_ratio_from_size("1792x1024"), "16:9");
        assert_eq!(calculate_aspect_ratio_from_size("1024x1792"), "9:16");

        // Custom sizes that should calculate to standard ratios
        assert_eq!(calculate_aspect_ratio_from_size("1920x1080"), "16:9");
        assert_eq!(calculate_aspect_ratio_from_size("1080x1920"), "9:16");

        // Square variants
        assert_eq!(calculate_aspect_ratio_from_size("512x512"), "1:1");
        assert_eq!(calculate_aspect_ratio_from_size("2048x2048"), "1:1");

        // Invalid formats return default "1:1"
        assert_eq!(calculate_aspect_ratio_from_size("invalid"), "1:1");
        assert_eq!(calculate_aspect_ratio_from_size("1024"), "1:1");
        assert_eq!(calculate_aspect_ratio_from_size(""), "1:1");
    }
}
