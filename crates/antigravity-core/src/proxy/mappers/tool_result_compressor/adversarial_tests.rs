#[cfg(test)]
mod tests {
    use super::super::*;
    use serde_json::json;

    #[test]
    fn test_edge_case_exact_threshold() {
        let exact_threshold = "a".repeat(MAX_IMAGE_BASE64_CHARS);
        let block = json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": exact_threshold
            }
        });

        assert!(is_oversized_base64_image(&block).is_none());

        let just_over = "a".repeat(MAX_IMAGE_BASE64_CHARS + 1);
        let block_over = json!({
            "type": "image",
            "source": { "type": "base64", "data": just_over }
        });
        assert_eq!(is_oversized_base64_image(&block_over), Some(MAX_IMAGE_BASE64_CHARS + 1));
    }

    #[test]
    fn test_edge_case_missing_data_field() {
        // Image block but missing the actual data string
        let block = json!({
            "type": "image",
            "source": {
                "media_type": "image/png"
                // "data" is missing
            }
        });
        assert!(
            is_oversized_base64_image(&block).is_none(),
            "Should not crash on missing data field"
        );
    }

    #[test]
    fn test_edge_case_multiple_images_mixed_sizes() {
        let mut blocks = vec![
            json!({"type": "text", "text": "Here are images:"}),
            json!({
                "type": "image",
                "source": { "type": "base64", "data": "small" }
            }),
            json!({
                "type": "image",
                "source": { "type": "base64", "data": "a".repeat(3_000_000) }
            }),
        ];

        sanitize_tool_result_blocks(&mut blocks);

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[1]["type"], "image"); // Small one stays
        assert_eq!(blocks[2]["type"], "text"); // Large one becomes text placeholder
        assert!(blocks[2]["text"].as_str().unwrap().contains("image omitted"));
    }

    #[test]
    fn test_edge_case_malformed_base64() {
        let malformed = "!!!@@@###".repeat(100_000); // 900,000 chars (under limit)
        let block = json!({
            "type": "image",
            "source": { "type": "base64", "data": malformed }
        });

        assert!(is_oversized_base64_image(&block).is_none());
    }
}
