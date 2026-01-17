//! Extended model mapping utilities
//!
//! Wraps upstream model_mapping with additional functionality:
//! - `resolve_model_route` returning (model, reason) tuple

use std::collections::HashMap;

/// Core model routing engine with routing reason tracking
/// Priority: Exact Match > Wildcard Match > System Default
///
/// # Returns
/// Tuple of (mapped_model, routing_reason)
pub fn resolve_model_route(
    original_model: &str,
    custom_mapping: &HashMap<String, String>,
) -> (String, String) {
    // 1. Exact match (highest priority)
    if let Some(target) = custom_mapping.get(original_model) {
        crate::modules::logger::log_info(&format!(
            "[Router] Exact mapping: {} -> {}",
            original_model, target
        ));
        return (target.clone(), "exact".to_string());
    }

    // 2. Wildcard match
    for (pattern, target) in custom_mapping.iter() {
        if pattern.contains('*') && wildcard_match(pattern, original_model) {
            crate::modules::logger::log_info(&format!(
                "[Router] Wildcard mapping: {} -> {} (rule: {})",
                original_model, target, pattern
            ));
            return (target.clone(), format!("wildcard:{}", pattern));
        }
    }

    // 3. System default mapping (from upstream)
    let result = super::model_mapping::map_claude_model_to_gemini(original_model);
    let reason = if result != original_model {
        crate::modules::logger::log_info(&format!(
            "[Router] System default mapping: {} -> {}",
            original_model, result
        ));
        "system".to_string()
    } else {
        "passthrough".to_string()
    };
    
    (result, reason)
}

/// Wildcard matching helper
/// Supports simple * wildcard matching
fn wildcard_match(pattern: &str, text: &str) -> bool {
    if let Some(star_pos) = pattern.find('*') {
        let prefix = &pattern[..star_pos];
        let suffix = &pattern[star_pos + 1..];
        text.starts_with(prefix) && text.ends_with(suffix)
    } else {
        pattern == text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_route_exact() {
        let mut mapping = HashMap::new();
        mapping.insert("test-model".to_string(), "mapped-model".to_string());
        
        let (model, reason) = resolve_model_route("test-model", &mapping);
        assert_eq!(model, "mapped-model");
        assert_eq!(reason, "exact");
    }

    #[test]
    fn test_resolve_model_route_wildcard() {
        let mut mapping = HashMap::new();
        mapping.insert("gpt-4*".to_string(), "gemini-2.5-pro".to_string());
        
        let (model, reason) = resolve_model_route("gpt-4-turbo", &mapping);
        assert_eq!(model, "gemini-2.5-pro");
        assert!(reason.starts_with("wildcard:"));
    }

    #[test]
    fn test_resolve_model_route_system_default() {
        let mapping = HashMap::new();
        
        let (model, reason) = resolve_model_route("claude-opus-4-5-20251101", &mapping);
        assert_eq!(model, "claude-opus-4-5-thinking");
        assert_eq!(reason, "system");
    }
}
