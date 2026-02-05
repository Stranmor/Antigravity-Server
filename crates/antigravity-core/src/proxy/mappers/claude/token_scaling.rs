// Claude helper functions
// JSON Schema cleanup, signature handling, etc.

// Token scaling algorithm uses floating-point math for smooth compression curves.
// All values are bounded: token counts are u32 (max 4B), context limits are ~2M.
// Precision loss in f64 mantissa (52 bits) is acceptable for display purposes.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::as_conversions,
    clippy::arithmetic_side_effects,
    reason = "Token scaling algorithm: bounded u32 values, intentional f64 math for smooth curves"
)]

// Already removed unused Value import

// Note: uppercase_schema_types function already removed (for converting JSON Schema type names to uppercase)
// e.g.: "string" -> "STRING", "integer" -> "INTEGER"

/// Get context token limit based on model name
pub fn get_context_limit_for_model(model: &str) -> u32 {
    if model.contains("pro") {
        2_097_152 // 2M for Pro
    } else {
        // Flash and default: 1M
        1_048_576
    }
}

pub fn to_claude_usage(
    usage_metadata: &super::models::UsageMetadata,
    scaling_enabled: bool,
    context_limit: u32,
) -> super::models::Usage {
    let prompt_tokens = usage_metadata.prompt_token_count.unwrap_or(0);
    let cached_tokens = usage_metadata.cached_content_token_count.unwrap_or(0);

    // [Smart threshold regression algorithm] - Utilize large window while guiding compression at critical points
    let total_raw = prompt_tokens;

    let scaled_total = if scaling_enabled && total_raw > 0 {
        const SCALING_THRESHOLD: u32 = 30_000;
        const TARGET_MAX: f64 = 195_000.0; // Close to Claude's 200k limit

        if total_raw <= SCALING_THRESHOLD {
            total_raw
        } else {
            // Set regression trigger point: start regression when actual usage reaches 70% of limit
            let perception_start = (context_limit as f64 * 0.7) as u32;

            if total_raw <= perception_start {
                // Phase 1: Safe zone - maintain original sqrt aggressive compression
                let excess = (total_raw - SCALING_THRESHOLD) as f64;
                // Coefficient 25.0 makes 100k -> ~50k (maintain same comfort level as original logic)
                let compressed_excess = excess.sqrt() * 25.0;
                (SCALING_THRESHOLD as f64 + compressed_excess) as u32
            } else {
                // Phase 2: Regression zone - linear regression from 70% to 100% towards 195k
                // Calculate current position in 70% - 100% ratio
                let range = context_limit as f64 * 0.3;
                let progress = (total_raw - perception_start) as f64 / range;

                // Calculate phase 1 endpoint value as starting point
                let base_excess = (perception_start - SCALING_THRESHOLD) as f64;
                let start_value = SCALING_THRESHOLD as f64 + base_excess.sqrt() * 25.0;

                // Linear interpolation regression
                let regression = (TARGET_MAX - start_value) * progress;
                (start_value + regression) as u32
            }
        }
    } else {
        total_raw
    };

    // [Debug log] For manual verification
    if scaling_enabled && total_raw > 30_000 {
        tracing::debug!(
            "[Claude-Scaling] Raw Tokens: {}, Scaled Report: {}, Ratio: {:.2}%",
            total_raw,
            scaled_total,
            (scaled_total as f64 / total_raw as f64) * 100.0
        );
    }

    // Distribute scaled total to input and cache_read by ratio
    let (reported_input, reported_cache) = if total_raw > 0 {
        let cache_ratio = (cached_tokens as f64) / (total_raw as f64);
        let sc_cache = (scaled_total as f64 * cache_ratio) as u32;
        (scaled_total.saturating_sub(sc_cache), Some(sc_cache))
    } else {
        (scaled_total, None)
    };

    super::models::Usage {
        input_tokens: reported_input,
        output_tokens: usage_metadata.candidates_token_count.unwrap_or(0),
        cache_read_input_tokens: reported_cache,
        cache_creation_input_tokens: Some(0),
        server_tool_use: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Removed unused serde_json::json

    // Already removed expired test for uppercase_schema_types

    #[test]
    fn test_to_claude_usage() {
        use super::super::models::UsageMetadata;

        let usage = UsageMetadata {
            prompt_token_count: Some(100),
            candidates_token_count: Some(50),
            total_token_count: Some(150),
            cached_content_token_count: None,
        };

        let claude_usage = to_claude_usage(&usage, true, 1_000_000);
        assert_eq!(claude_usage.input_tokens, 100);
        assert_eq!(claude_usage.output_tokens, 50);

        // test 70% load ( percepción_start = 700k )
        let usage_70 = UsageMetadata {
            prompt_token_count: Some(700_000),
            candidates_token_count: Some(10),
            total_token_count: Some(700_010),
            cached_content_token_count: None,
        };
        let res_70 = to_claude_usage(&usage_70, true, 1_000_000);
        // sqrt(670k) * 25 + 30k = 818.5 * 25 + 30k = 20462 + 30k = 50462
        assert!(res_70.input_tokens > 50000 && res_70.input_tokens < 51000);

        // test 100% load ( 1M )
        let usage_100 = UsageMetadata {
            prompt_token_count: Some(1_000_000),
            candidates_token_count: Some(10),
            total_token_count: Some(1_000_010),
            cached_content_token_count: None,
        };
        let res_100 = to_claude_usage(&usage_100, true, 1_000_000);
        // Should be very close to 195,000
        assert_eq!(res_100.input_tokens, 195_000);

        // test 90% load ( 900k )
        let usage_90 = UsageMetadata {
            prompt_token_count: Some(900_000),
            candidates_token_count: Some(10),
            total_token_count: Some(900_010),
            cached_content_token_count: None,
        };
        let res_90 = to_claude_usage(&usage_90, true, 1_000_000);
        // Regression range: 700k -> 1M (300k range)
        // 900k is 2/3 of the way.
        // Start: ~50462, End: 195000. Diff: ~144538.
        // Value: 50462 + 2/3 * 144538 = 50462 + 96358 = 146820
        assert!(res_90.input_tokens > 146_000 && res_90.input_tokens < 147_500);
    }
}
