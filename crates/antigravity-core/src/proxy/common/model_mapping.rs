//! Model name mapping between Claude/OpenAI and Gemini backends.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Static mapping from Claude/OpenAI model names to Gemini backend models.
static CLAUDE_TO_GEMINI: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("claude-opus-4-5-thinking", "claude-opus-4-5-thinking"),
        ("claude-sonnet-4-5", "claude-sonnet-4-5"),
        ("claude-sonnet-4-5-thinking", "claude-sonnet-4-5-thinking"),
        ("claude-sonnet-4-5-20250929", "claude-sonnet-4-5-thinking"),
        ("claude-3-5-sonnet-20241022", "claude-sonnet-4-5"),
        ("claude-3-5-sonnet-20240620", "claude-sonnet-4-5"),
        ("claude-opus-4", "claude-opus-4-5-thinking"),
        ("claude-opus-4-5", "claude-opus-4-5-thinking"),
        ("claude-opus-4-5-20251101", "claude-opus-4-5-thinking"),
        ("claude-haiku-4", "claude-sonnet-4-5"),
        ("claude-haiku-4-5", "gemini-3-flash"),
        ("claude-3-haiku-20240307", "claude-sonnet-4-5"),
        ("claude-haiku-4-5-20251001", "claude-sonnet-4-5"),
        ("gpt-4", "gemini-2.5-flash"),
        ("gpt-4-turbo", "gemini-2.5-flash"),
        ("gpt-4-turbo-preview", "gemini-2.5-flash"),
        ("gpt-4-0125-preview", "gemini-2.5-flash"),
        ("gpt-4-1106-preview", "gemini-2.5-flash"),
        ("gpt-4-0613", "gemini-2.5-flash"),
        ("gpt-4o", "gemini-2.5-flash"),
        ("gpt-4o-2024-05-13", "gemini-2.5-flash"),
        ("gpt-4o-2024-08-06", "gemini-2.5-flash"),
        ("gpt-4o-mini", "gemini-2.5-flash"),
        ("gpt-4o-mini-2024-07-18", "gemini-2.5-flash"),
        ("gpt-3.5-turbo", "gemini-2.5-flash"),
        ("gpt-3.5-turbo-16k", "gemini-2.5-flash"),
        ("gpt-3.5-turbo-0125", "gemini-2.5-flash"),
        ("gpt-3.5-turbo-1106", "gemini-2.5-flash"),
        ("gpt-3.5-turbo-0613", "gemini-2.5-flash"),
        ("gemini-2.5-flash-lite", "gemini-2.5-flash-lite"),
        ("gemini-2.5-flash-thinking", "gemini-2.5-flash-thinking"),
        ("gemini-3-pro-low", "gemini-3-pro-preview"),
        ("gemini-3-pro-high", "gemini-3-pro-preview"),
        ("gemini-3-pro-preview", "gemini-3-pro-preview"),
        ("gemini-3-pro", "gemini-3-pro-preview"),
        ("gemini-2.5-flash", "gemini-2.5-flash"),
        ("gemini-3-flash", "gemini-3-flash"),
        ("gemini-3-flash-high", "gemini-3-flash"),
        ("gemini-3-flash-preview", "gemini-3-flash"),
        ("gemini-3-pro-image", "gemini-3-pro-image"),
        ("internal-background-task", "gemini-2.5-flash"),
    ])
});

/// Maps model name to actual backend model.
#[must_use]
pub fn map_claude_model_to_gemini(input: &str) -> Option<String> {
    if let Some(mapped) = CLAUDE_TO_GEMINI.get(input) {
        return Some((*mapped).to_owned());
    }

    if input.starts_with("gemini-") || input.contains("thinking") {
        return Some(input.to_owned());
    }

    None
}

/// Get all built-in supported model names.
#[must_use]
pub fn get_supported_models() -> Vec<String> {
    CLAUDE_TO_GEMINI.keys().map(|key| (*key).to_owned()).collect()
}

/// Generate all image model variant IDs (3 resolutions × 7 ratios = 21 variants).
#[must_use]
pub fn generate_image_model_variants() -> Vec<String> {
    let base = "gemini-3-pro-image";
    let resolutions = ["", "-2k", "-4k"];
    let ratios = ["", "-1x1", "-4x3", "-3x4", "-16x9", "-9x16", "-21x9"];
    let mut variants = Vec::with_capacity(21);
    for res in resolutions {
        for ratio in ratios {
            let mut id = base.to_owned();
            id.push_str(res);
            id.push_str(ratio);
            variants.push(id);
        }
    }
    variants
}

/// Collect all available model IDs from loaded accounts, custom mappings, and image variants.
/// This is the Single Point of Truth for model listing across all protocol handlers.
pub async fn collect_all_model_ids(
    token_manager: &crate::proxy::TokenManager,
    custom_mapping: &tokio::sync::RwLock<HashMap<String, String>>,
) -> Vec<String> {
    use std::collections::HashSet;
    let mut model_ids: HashSet<String> = HashSet::new();

    // 1. Real models from loaded accounts
    for model in token_manager.get_all_available_models() {
        let _: bool = model_ids.insert(model);
    }

    // 2. Custom mapping keys
    {
        let mapping = custom_mapping.read().await;
        for key in mapping.keys() {
            let _: bool = model_ids.insert(key.clone());
        }
    }

    // 3. Image model variants
    for variant in generate_image_model_variants() {
        let _: bool = model_ids.insert(variant);
    }

    let mut sorted_ids: Vec<String> = model_ids.into_iter().collect();
    sorted_ids.sort();
    sorted_ids
}

/// Get all available models including custom mappings.
pub async fn get_all_dynamic_models(
    custom_mapping: &tokio::sync::RwLock<HashMap<String, String>>,
) -> Vec<String> {
    use std::collections::HashSet;
    let mut model_ids = HashSet::new();

    for model in get_supported_models() {
        let _: bool = model_ids.insert(model);
    }

    {
        let mapping = custom_mapping.read().await;
        for key in mapping.keys() {
            let _: bool = model_ids.insert(key.clone());
        }
    }

    for variant in generate_image_model_variants() {
        let _: bool = model_ids.insert(variant);
    }

    let _: bool = model_ids.insert("gemini-2.0-flash-exp".to_owned());

    let mut sorted_ids: Vec<_> = model_ids.into_iter().collect();
    sorted_ids.sort();
    sorted_ids
}

/// Normalize model name to standard protection ID.
///
/// Standard IDs for quota protection:
/// - `gemini-3-flash`: Gemini 3 Flash variants
/// - `gemini-3-pro-high`: Gemini 3 Pro variants
/// - `claude-opus-4-5-thinking`: All Claude Opus variants
/// - `claude-sonnet-4-5-thinking`: Claude Sonnet with thinking
/// - `claude-sonnet-4-5`: All Claude Sonnet/Haiku variants
#[must_use]
pub fn normalize_to_standard_id(model_name: &str) -> Option<String> {
    normalize_to_standard_id_with_depth(model_name, 0)
}

/// Recursive normalization with depth limit to prevent infinite loops.
fn normalize_to_standard_id_with_depth(model_name: &str, depth: u8) -> Option<String> {
    const MAX_DEPTH: u8 = 5;
    if depth > MAX_DEPTH {
        return None;
    }

    if let Some(mapped) = CLAUDE_TO_GEMINI.get(model_name) {
        if *mapped != model_name {
            return normalize_to_standard_id_with_depth(mapped, depth.saturating_add(1));
        }
    }

    let lower = model_name.to_lowercase();

    if lower == "gemini-3-flash" || lower.starts_with("gemini-3-flash-") {
        return Some("gemini-3-flash".to_owned());
    }

    if lower.starts_with("gemini-3-pro") {
        return Some("gemini-3-pro-high".to_owned());
    }

    if lower.contains("opus") {
        return Some("claude-opus-4-5-thinking".to_owned());
    }

    if lower.contains("sonnet") && lower.contains("thinking") {
        return Some("claude-sonnet-4-5-thinking".to_owned());
    }

    if lower.contains("sonnet") || lower.contains("haiku") {
        return Some("claude-sonnet-4-5".to_owned());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_mapping() {
        assert_eq!(
            map_claude_model_to_gemini("claude-3-5-sonnet-20241022"),
            Some("claude-sonnet-4-5".to_owned())
        );
        assert_eq!(
            map_claude_model_to_gemini("claude-opus-4"),
            Some("claude-opus-4-5-thinking".to_owned())
        );
        assert_eq!(
            map_claude_model_to_gemini("gemini-2.5-flash-mini-test"),
            Some("gemini-2.5-flash-mini-test".to_owned())
        );
        assert_eq!(
            map_claude_model_to_gemini("gemini-3-pro"),
            Some("gemini-3-pro-preview".to_owned())
        );
    }

    #[test]
    fn test_unknown_model_returns_none() {
        assert_eq!(map_claude_model_to_gemini("unknown-model"), None);
        assert_eq!(map_claude_model_to_gemini("claude-sonnet-5"), None);
        assert_eq!(map_claude_model_to_gemini("gpt-fake"), None);
    }

    #[test]
    fn test_normalize_to_standard_id() {
        assert_eq!(normalize_to_standard_id("gemini-3-flash"), Some("gemini-3-flash".to_owned()));
        assert_eq!(
            normalize_to_standard_id("gemini-3-flash-exp"),
            Some("gemini-3-flash".to_owned())
        );
        assert_eq!(
            normalize_to_standard_id("gemini-3-pro-high"),
            Some("gemini-3-pro-high".to_owned())
        );
        assert_eq!(normalize_to_standard_id("gemini-3-pro"), Some("gemini-3-pro-high".to_owned()));
        assert_eq!(
            normalize_to_standard_id("gemini-3-pro-low"),
            Some("gemini-3-pro-high".to_owned())
        );
        assert_eq!(normalize_to_standard_id("gemini-2.5-flash"), None);
        assert_eq!(normalize_to_standard_id("gemini-2.5-pro"), None);
        assert_eq!(
            normalize_to_standard_id("claude-opus-4-5"),
            Some("claude-opus-4-5-thinking".to_owned())
        );
        assert_eq!(
            normalize_to_standard_id("claude-sonnet-4-5-thinking"),
            Some("claude-sonnet-4-5-thinking".to_owned())
        );
        assert_eq!(
            normalize_to_standard_id("claude-sonnet-4-5-20250929"),
            Some("claude-sonnet-4-5-thinking".to_owned())
        );
        assert_eq!(
            normalize_to_standard_id("claude-sonnet-4-5"),
            Some("claude-sonnet-4-5".to_owned())
        );
        assert_eq!(
            normalize_to_standard_id("claude-3-5-haiku-20241022"),
            Some("claude-sonnet-4-5".to_owned())
        );
        assert_eq!(normalize_to_standard_id("gpt-4o"), None);
    }
}
