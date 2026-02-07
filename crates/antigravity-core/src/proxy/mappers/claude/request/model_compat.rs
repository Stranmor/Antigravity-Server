//! Model compatibility checking.

use serde_json::Value;

pub fn clean_thinking_fields_recursive(val: &mut Value) {
    match val {
        Value::Object(map) => {
            map.remove("thought");
            map.remove("thoughtSignature");
            for (_, v) in map.iter_mut() {
                clean_thinking_fields_recursive(v);
            }
        },
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                clean_thinking_fields_recursive(v);
            }
        },
        _ => {},
    }
}

/// Extract a model's base family by stripping variant suffixes, dates, and versions.
///
/// Examples:
/// - `gemini-2.5-flash-thinking` → `gemini-2.5-flash`
/// - `gemini-3-pro-preview` → `gemini-3-pro`
/// - `claude-sonnet-4-5-20250929` → `claude-sonnet-4-5`
/// - `gemini-1.5-pro-002` → `gemini-1.5-pro`
fn extract_model_family(model: &str) -> String {
    let mut family = model.to_lowercase();

    // Known variant suffixes to strip (iteratively, since models can stack them)
    let strip_suffixes: &[&str] = &[
        "-thinking",
        "-preview",
        "-high",
        "-low",
        "-lite",
        "-exp",
        "-latest",
        "-online",
        "-image",
    ];

    let mut changed = true;
    while changed {
        changed = false;
        for suffix in strip_suffixes {
            if family.ends_with(suffix) {
                family.truncate(family.len() - suffix.len());
                changed = true;
            }
        }
    }

    // Strip date suffixes (8+ digits at the end, e.g. -20241022)
    if let Some(pos) = family.rfind('-') {
        let date_part = &family[pos + 1..];
        if date_part.len() >= 8 && date_part.chars().all(|c| c.is_ascii_digit()) {
            family.truncate(pos);
        }
    }

    // Strip short numeric version suffixes (e.g. -002, -01)
    if let Some(pos) = family.rfind('-') {
        let ver_part = &family[pos + 1..];
        if !ver_part.is_empty()
            && ver_part.len() <= 3
            && ver_part.chars().all(|c| c.is_ascii_digit())
        {
            family.truncate(pos);
        }
    }

    family
}

/// Check if two model strings are compatible (same family).
///
/// Dynamically extracts the base model family from both strings
/// and compares them. No hardcoded model lists — works for any
/// current and future models.
pub fn is_model_compatible(cached: &str, target: &str) -> bool {
    extract_model_family(cached) == extract_model_family(target)
}
