// Claude 辅助函数
// JSON Schema 清理、签名处理等

// 已移除未使用的 Value 导入

/// 将 JSON Schema 中的类型名称转为大写 (Gemini 要求)
/// 例如: "string" -> "STRING", "integer" -> "INTEGER"
// 已移除未使用的 uppercase_schema_types 函数

pub fn to_claude_usage(usage_metadata: &super::models::UsageMetadata, scaling_enabled: bool) -> super::models::Usage {
    let prompt_tokens = usage_metadata.prompt_token_count.unwrap_or(0);
    let cached_tokens = usage_metadata.cached_content_token_count.unwrap_or(0);
    
    // 【超激进总用量缩放】- 彻底防止客户端压缩
    // 目标：100k -> 约 32k，1M -> 约 40k
    const SCALING_THRESHOLD: u32 = 30_000;  
    let total_raw = prompt_tokens;
    
    let scaled_total = if scaling_enabled && total_raw > SCALING_THRESHOLD {
        // 对超出部分进行平方根缩放，极其激进
        let excess = (total_raw - SCALING_THRESHOLD) as f64;
        let compressed_excess = excess.sqrt() * 25.0; // 调整系数以达到 100k -> 约 30k 的效果
        SCALING_THRESHOLD + compressed_excess as u32
    } else {
        total_raw
    };
    
    // 按比例分配缩放后的总量到 input 和 cache_read
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

/// 提取 thoughtSignature
// 已移除未使用的 extract_thought_signature 函数

#[cfg(test)]
mod tests {
    use super::*;
    // 移除了未使用的 serde_json::json

    // 已移除对 uppercase_schema_types 的过期测试

    #[test]
    fn test_to_claude_usage() {
        use super::super::models::UsageMetadata;

        let usage = UsageMetadata {
            prompt_token_count: Some(100),
            candidates_token_count: Some(50),
            total_token_count: Some(150),
            cached_content_token_count: None,
        };

        let claude_usage = to_claude_usage(&usage);
        assert_eq!(claude_usage.input_tokens, 100);
        assert_eq!(claude_usage.output_tokens, 50);
    }
}
