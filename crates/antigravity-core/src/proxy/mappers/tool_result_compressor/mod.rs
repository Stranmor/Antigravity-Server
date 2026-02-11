//! Tool result output compression module
//!
//! Provides intelligent compression:
//! - Browser snapshot compression (head+tail preservation)
//! - Large file notice compression (extract key info)
//! - Generic truncation (200,000 character limit)

mod strategies;

#[cfg(test)]
mod tests;

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;
use tracing::{debug, info};

fn style_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style>").expect("static regex"))
}

fn script_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script>").expect("static regex"))
}

fn base64_data_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)data:[^;/]+/[^;]+;base64,[A-Za-z0-9+/=]+"#).expect("static regex")
    })
}

fn blank_lines_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\n\s*\n").expect("static regex"))
}

pub use strategies::{
    compact_browser_snapshot, compact_saved_output_notice, SNAPSHOT_DETECTION_THRESHOLD,
};

/// Maximum tool result characters (~200k, prevents prompt overflow)
pub const MAX_TOOL_RESULT_CHARS: usize = 200_000;

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
    // Floor to a valid UTF-8 char boundary to avoid panic on multi-byte chars
    let mut split_pos = if text.is_char_boundary(max_chars) {
        max_chars
    } else {
        text.floor_char_boundary(max_chars)
    };

    // Look back for unclosed tag start
    let sub = &text[..split_pos];
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
    result = style_regex().replace_all(&result, "[style omitted]").to_string();

    // 2. Remove <script>...</script> and contents
    result = script_regex().replace_all(&result, "[script omitted]").to_string();

    // 3. Remove inline Base64 data (e.g., src="data:image/png;base64,...")
    result = base64_data_regex().replace_all(&result, "[base64 omitted]").to_string();

    // 4. Remove redundant whitespace
    result = blank_lines_regex().replace_all(&result, "\n").to_string();

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

    if !blocks.is_empty() {
        info!(
            "[ToolCompressor] Processing {} blocks for truncation (MAX: {} chars)",
            blocks.len(),
            MAX_TOOL_RESULT_CHARS
        );
    }

    for block in blocks.iter() {
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

    info!(
        "[ToolCompressor] Sanitization complete: {} -> {} blocks, {} chars used",
        blocks.len(),
        cleaned_blocks.len(),
        used_chars
    );

    *blocks = cleaned_blocks;
}
