//! Tool result output compression module
//!
//! Provides intelligent compression:
//! - Browser snapshot compression (head+tail preservation)
//! - Large file notice compression (extract key info)
//! - Generic truncation (200,000 character limit)

mod strategies;

#[cfg(test)]
mod tests;

use regex::Regex;
use serde_json::Value;
use tracing::{debug, info};

pub use strategies::{
    compact_browser_snapshot, compact_saved_output_notice, SNAPSHOT_DETECTION_THRESHOLD,
};

/// Maximum tool result characters (~200k, prevents prompt overflow)
pub const MAX_TOOL_RESULT_CHARS: usize = 200_000;

/// Maximum base64 image characters (~1.5M, roughly 1MB of binary data)
pub const MAX_IMAGE_BASE64_CHARS: usize = 1_500_000;

/// Browser snapshot max chars after compression
pub const SNAPSHOT_MAX_CHARS: usize = 16_000;

/// Browser snapshot head preservation ratio
pub const SNAPSHOT_HEAD_RATIO: f64 = 0.7;

/// Compress tool result text
///
/// Automatically selects best compression strategy based on content type:
/// 1. Large file notice -> extract key info
/// 2. Browser snapshot -> head+tail preservation
/// 3. Other -> simple truncation
pub fn compact_tool_result_text(text: &str, max_chars: usize) -> String {
    if text.is_empty() || text.len() <= max_chars {
        return text.to_string();
    }

    // Deep preprocess potential HTML content
    let cleaned_text =
        if text.contains("<html") || text.contains("<body") || text.contains("<!DOCTYPE") {
            let cleaned = deep_clean_html(text);
            debug!(
                "[ToolCompressor] Deep cleaned HTML, reduced {} -> {} chars",
                text.len(),
                cleaned.len()
            );
            cleaned
        } else {
            text.to_string()
        };

    if cleaned_text.len() <= max_chars {
        return cleaned_text;
    }

    // 1. Detect saved output notice pattern
    if let Some(compacted) = compact_saved_output_notice(&cleaned_text, max_chars) {
        debug!(
            "[ToolCompressor] Detected saved output notice, compacted to {} chars",
            compacted.len()
        );
        return compacted;
    }

    // 2. Detect browser snapshot pattern
    if cleaned_text.len() > SNAPSHOT_DETECTION_THRESHOLD {
        if let Some(compacted) = compact_browser_snapshot(&cleaned_text, max_chars) {
            debug!(
                "[ToolCompressor] Detected browser snapshot, compacted to {} chars",
                compacted.len()
            );
            return compacted;
        }
    }

    // 3. Structured truncation
    debug!("[ToolCompressor] Using structured truncation for {} chars", cleaned_text.len());
    truncate_text_safe(&cleaned_text, max_chars)
}

/// Safe text truncation (avoids cutting in middle of tags)
pub fn truncate_text_safe(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }

    // Try to find a safe truncation point (not between < and >)
    let mut split_pos = max_chars;

    // Look back for unclosed tag start
    let sub = &text[..max_chars];
    if let Some(last_open) = sub.rfind('<') {
        if let Some(last_close) = sub.rfind('>') {
            if last_open > last_close {
                // Truncation point is inside a tag, back up to before tag
                split_pos = last_open;
            }
        } else {
            // Only open, no close - back up to before tag
            split_pos = last_open;
        }
    }

    // Also avoid truncating in middle of JSON braces
    if let Some(last_open_brace) = sub.rfind('{') {
        if let Some(last_close_brace) = sub.rfind('}') {
            if last_open_brace > last_close_brace {
                // Possibly in middle of JSON, if close to truncation point, back up
                if max_chars - last_open_brace < 100 {
                    split_pos = split_pos.min(last_open_brace);
                }
            }
        }
    }

    let truncated = &text[..split_pos];
    let omitted = text.len() - split_pos;
    format!("{}\n...[truncated {} chars]", truncated, omitted)
}

/// Deep clean HTML (remove style, script, base64, etc.)
fn deep_clean_html(html: &str) -> String {
    let mut result = html.to_string();

    // 1. Remove <style>...</style> and contents
    if let Ok(re) = Regex::new(r"(?is)<style\b[^>]*>.*?</style>") {
        result = re.replace_all(&result, "[style omitted]").to_string();
    }

    // 2. Remove <script>...</script> and contents
    if let Ok(re) = Regex::new(r"(?is)<script\b[^>]*>.*?</script>") {
        result = re.replace_all(&result, "[script omitted]").to_string();
    }

    // 3. Remove inline Base64 data (e.g., src="data:image/png;base64,...")
    if let Ok(re) = Regex::new(r#"(?i)data:[^;/]+/[^;]+;base64,[A-Za-z0-9+/=]+"#) {
        result = re.replace_all(&result, "[base64 omitted]").to_string();
    }

    // 4. Remove redundant whitespace
    if let Ok(re) = Regex::new(r"\n\s*\n") {
        result = re.replace_all(&result, "\n").to_string();
    }

    result
}

/// Sanitize tool result content blocks
///
/// Processing logic:
/// 1. Remove base64 images (avoid excessive size)
/// 2. Compress text content (using intelligent compression)
/// 3. Limit total characters (default 200,000)
///
/// Reference: anthropicGeminiBridgeService.js:540-597
pub fn sanitize_tool_result_blocks(blocks: &mut Vec<Value>) {
    let mut used_chars = 0;
    let mut cleaned_blocks = Vec::new();
    let mut removed_image_size = None;

    if !blocks.is_empty() {
        info!(
            "[ToolCompressor] Processing {} blocks for truncation (MAX: {} chars)",
            blocks.len(),
            MAX_TOOL_RESULT_CHARS
        );
    }

    for block in blocks.iter() {
        if let Some(size) = is_oversized_base64_image(block) {
            removed_image_size = Some(size);
            debug!(
                "[ToolCompressor] Removed oversized base64 image block ({} chars, threshold: {})",
                size, MAX_IMAGE_BASE64_CHARS
            );
            continue;
        }

        // Compress text content
        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
            let remaining = MAX_TOOL_RESULT_CHARS.saturating_sub(used_chars);
            if remaining == 0 {
                debug!("[ToolCompressor] Reached character limit, stopping");
                break;
            }

            let compacted = compact_tool_result_text(text, remaining);
            let mut new_block = block.clone();
            new_block["text"] = Value::String(compacted.clone());
            cleaned_blocks.push(new_block);
            used_chars += compacted.len();

            debug!(
                "[ToolCompressor] Compacted text block: {} -> {} chars",
                text.len(),
                compacted.len()
            );
        } else {
            let block_size = if block.get("type").and_then(|v| v.as_str()) == Some("image") {
                block
                    .get("source")
                    .and_then(|s| s.get("data"))
                    .and_then(|d| d.as_str())
                    .map(|s| s.len())
                    .unwrap_or(100)
            } else {
                100
            };

            cleaned_blocks.push(block.clone());
            used_chars += block_size;
        }

        if used_chars >= MAX_TOOL_RESULT_CHARS {
            break;
        }
    }

    if let Some(size) = removed_image_size {
        cleaned_blocks.push(serde_json::json!({
            "type": "text",
            "text": format!("[image omitted: {}KB exceeds limit; use the file path in the previous text block]", size / 1024)
        }));
    }

    info!(
        "[ToolCompressor] Sanitization complete: {} -> {} blocks, {} chars used",
        blocks.len(),
        cleaned_blocks.len(),
        used_chars
    );

    *blocks = cleaned_blocks;
}

/// Detect if block is an oversized base64 image.
/// Returns Some(size_in_chars) if image exceeds MAX_IMAGE_BASE64_CHARS, None otherwise.
/// Small/medium images pass through to the model; only truly huge ones are stripped.
pub fn is_oversized_base64_image(block: &Value) -> Option<usize> {
    if block.get("type").and_then(|v| v.as_str()) == Some("image")
        && block.get("source").and_then(|s| s.get("type")).and_then(|v| v.as_str())
            == Some("base64")
    {
        let data_len = block
            .get("source")
            .and_then(|s| s.get("data"))
            .and_then(|v| v.as_str())
            .map(|s| s.len())
            .unwrap_or(0);

        if data_len > MAX_IMAGE_BASE64_CHARS {
            return Some(data_len);
        }
    }
    None
}

#[cfg(test)]
mod adversarial_tests;
