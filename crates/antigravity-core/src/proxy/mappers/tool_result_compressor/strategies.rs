//! Compression strategies for specific content types
//!
//! - Browser snapshot compression (head+tail preservation)
//! - Saved output notice compression (extract key info)

use regex::Regex;
use tracing::debug;

use super::{truncate_text_safe, SNAPSHOT_HEAD_RATIO, SNAPSHOT_MAX_CHARS};

/// Browser snapshot detection threshold
pub const SNAPSHOT_DETECTION_THRESHOLD: usize = 20_000;

/// Compress "output saved to file" type notices
///
/// Detects pattern: "result (N characters) exceeds maximum allowed tokens. Output saved to <path>"
/// Strategy: Extract key info (file path, char count, format description)
///
/// Reference: anthropicGeminiBridgeService.js:278-310
pub fn compact_saved_output_notice(text: &str, max_chars: usize) -> Option<String> {
    // Regex match: result (N characters) exceeds maximum allowed tokens. Output saved to <path>
    let re = Regex::new(
        r"(?i)result\s*\(\s*(?P<count>[\d,]+)\s*characters\s*\)\s*exceeds\s+maximum\s+allowed\s+tokens\.\s*Output\s+(?:has\s+been\s+)?saved\s+to\s+(?P<path>[^\r\n]+)"
    ).ok()?;

    let caps = re.captures(text)?;
    let count = caps.name("count")?.as_str();
    let raw_path = caps.name("path")?.as_str();

    // Clean file path (remove trailing brackets, quotes, periods)
    let file_path = raw_path.trim().trim_end_matches(&[')', ']', '"', '\'', '.'][..]).trim();

    // Extract key lines
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

    // Find notice line
    let notice_line = lines.iter()
        .find(|l| l.to_lowercase().contains("exceeds maximum allowed tokens") && l.to_lowercase().contains("saved to"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("result ({} characters) exceeds maximum allowed tokens. Output has been saved to {}", count, file_path));

    // Find format description line
    let format_line = lines
        .iter()
        .find(|l| {
            l.starts_with("Format:")
                || l.contains("JSON array with schema")
                || l.to_lowercase().starts_with("schema:")
        })
        .map(|s| s.to_string());

    // Build compacted output
    let mut compact_lines = vec![notice_line];
    if let Some(fmt) = format_line {
        if !compact_lines.contains(&fmt) {
            compact_lines.push(fmt);
        }
    }
    compact_lines.push(format!(
        "[tool_result omitted to reduce prompt size; read file locally if needed: {}]",
        file_path
    ));

    let result = compact_lines.join("\n");
    Some(truncate_text_safe(&result, max_chars))
}

/// Compress browser snapshot (head+tail preservation strategy)
///
/// Detection: "page snapshot" or large number of "ref=" references
/// Strategy: Keep head 70% + tail 30%, omit middle
///
/// Reference: anthropicGeminiBridgeService.js:312-339
pub fn compact_browser_snapshot(text: &str, max_chars: usize) -> Option<String> {
    // Detect if this is a browser snapshot
    let is_snapshot = text.to_lowercase().contains("page snapshot")
        || text.matches("ref=").count() > 30
        || text.matches("[ref=").count() > 30;

    if !is_snapshot {
        return None;
    }

    let desired_max = max_chars.min(SNAPSHOT_MAX_CHARS);
    if desired_max < 2000 || text.len() <= desired_max {
        return None;
    }

    let meta =
        format!("[page snapshot summarized to reduce prompt size; original {} chars]", text.len());
    let overhead = meta.len() + 200;
    let budget = desired_max.saturating_sub(overhead);

    if budget < 1000 {
        return None;
    }

    // Calculate head and tail lengths
    let head_len = (budget as f64 * SNAPSHOT_HEAD_RATIO).floor() as usize;
    let head_len = head_len.clamp(500, 10_000);
    let tail_len = budget.saturating_sub(head_len).min(3_000);

    let head = &text[..head_len.min(text.len())];
    let tail = if tail_len > 0 && text.len() > head_len {
        let start = text.len().saturating_sub(tail_len);
        &text[start..]
    } else {
        ""
    };

    let omitted = text.len().saturating_sub(head_len).saturating_sub(tail_len);

    let summarized = if tail.is_empty() {
        format!("{}\n---[HEAD]---\n{}\n---[...omitted {} chars]---", meta, head, omitted)
    } else {
        format!(
            "{}\n---[HEAD]---\n{}\n---[...omitted {} chars]---\n---[TAIL]---\n{}",
            meta, head, omitted, tail
        )
    };

    debug!(
        "[ToolCompressor] Browser snapshot compressed: {} -> {} chars",
        text.len(),
        summarized.len()
    );

    Some(truncate_text_safe(&summarized, max_chars))
}
