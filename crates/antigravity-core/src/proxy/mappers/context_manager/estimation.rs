//! Token estimation utilities for context management

/// Helper to estimate tokens from text with multi-language awareness
///
/// Improved estimation algorithm:
/// - ASCII/English: ~4 characters per token
/// - Unicode/CJK: ~1.5 characters per token (Chinese, Japanese, Korean are tokenized differently)
/// - Adds 15% safety margin to prevent underestimation
pub fn estimate_tokens_from_str(s: &str) -> u32 {
    if s.is_empty() {
        return 0;
    }

    let mut ascii_chars = 0u32;
    let mut unicode_chars = 0u32;

    for c in s.chars() {
        if c.is_ascii() {
            ascii_chars += 1;
        } else {
            unicode_chars += 1;
        }
    }

    // ASCII: ~4 chars/token, Unicode/CJK: ~1.5 chars/token
    let ascii_tokens = (ascii_chars as f32 / 4.0).ceil() as u32;
    let unicode_tokens = (unicode_chars as f32 / 1.5).ceil() as u32;

    // Add 15% safety margin to account for tokenizer variations
    ((ascii_tokens + unicode_tokens) as f32 * 1.15).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string() {
        assert_eq!(estimate_tokens_from_str(""), 0);
    }

    #[test]
    fn test_ascii_only() {
        let tokens = estimate_tokens_from_str("Hello World");
        assert!(tokens > 0);
        assert!(tokens < 10);
    }

    #[test]
    fn test_unicode() {
        let tokens = estimate_tokens_from_str("你好世界");
        assert!(tokens > 0);
    }
}
