//! File utilities for token manager.
#![allow(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "percentage conversion: i64 -> i32 for quota percentages (0-100)"
)]

use std::path::Path;

pub async fn atomic_write_json(path: &Path, content: &serde_json::Value) -> Result<(), String> {
    let temp_path = path.with_extension("json.tmp");
    let json_str =
        serde_json::to_string_pretty(content).map_err(|e| format!("JSON serialize: {}", e))?;

    tokio::fs::write(&temp_path, &json_str)
        .await
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    tokio::fs::rename(&temp_path, path)
        .await
        .map_err(|e| format!("Failed to rename file: {}", e))?;

    Ok(())
}

pub fn truncate_reason(reason: &str, max_len: usize) -> String {
    if reason.chars().count() <= max_len {
        return reason.to_string();
    }
    let mut s: String = reason.chars().take(max_len).collect();
    s.push('…');
    s
}

pub fn calculate_max_quota_percentage(quota: &serde_json::Value) -> Option<i32> {
    let models = quota.get("models")?.as_array()?;
    let mut max_percentage = 0;
    let mut has_data = false;

    for model in models {
        if let Some(pct) = model.get("percentage").and_then(|v| v.as_i64()) {
            let pct_i32 = pct as i32;
            if pct_i32 > max_percentage {
                max_percentage = pct_i32;
            }
            has_data = true;
        }
    }

    if has_data {
        Some(max_percentage)
    } else {
        None
    }
}
