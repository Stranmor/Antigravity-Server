//! Tests for signature cache

use super::signature_cache::SignatureCache;

#[test]
fn test_tool_signature_cache() {
    let cache = SignatureCache::new();
    let sig = "x".repeat(60); // Valid length

    cache.cache_tool_signature("tool_1", sig.clone());
    assert_eq!(cache.get_tool_signature("tool_1"), Some(sig));
    assert_eq!(cache.get_tool_signature("tool_2"), None);
}

#[test]
fn test_min_length() {
    let cache = SignatureCache::new();
    cache.cache_tool_signature("tool_short", "short".to_string());
    assert_eq!(cache.get_tool_signature("tool_short"), None);
}

#[test]
fn test_thinking_family() {
    let cache = SignatureCache::new();
    let sig = "y".repeat(60);

    cache.cache_thinking_family(sig.clone(), "claude".to_string());
    assert_eq!(cache.get_signature_family(&sig), Some("claude".to_string()));
}

#[test]
fn test_session_signature() {
    let cache = SignatureCache::new();
    let sig1 = "a".repeat(60);
    let sig2 = "b".repeat(80); // Longer, should replace
    let sig3 = "c".repeat(40); // Too short, should be ignored

    // Initially empty
    assert!(cache.get_session_signature("sid-test123").is_none());

    // Store first signature
    cache.cache_session_signature("sid-test123", sig1.clone());
    assert_eq!(cache.get_session_signature("sid-test123"), Some(sig1.clone()));

    // Longer signature should replace
    cache.cache_session_signature("sid-test123", sig2.clone());
    assert_eq!(cache.get_session_signature("sid-test123"), Some(sig2.clone()));

    // Shorter valid signature should NOT replace
    cache.cache_session_signature("sid-test123", sig1.clone());
    assert_eq!(cache.get_session_signature("sid-test123"), Some(sig2.clone()));

    // Too short signature should be ignored entirely
    cache.cache_session_signature("sid-test123", sig3);
    assert_eq!(cache.get_session_signature("sid-test123"), Some(sig2));

    // Different session should be isolated
    assert!(cache.get_session_signature("sid-other").is_none());
}

#[test]
fn test_clear_all_caches() {
    let cache = SignatureCache::new();
    let sig = "x".repeat(60);

    cache.cache_tool_signature("tool_1", sig.clone());
    cache.cache_thinking_family(sig.clone(), "model".to_string());
    cache.cache_session_signature("sid-1", sig.clone());

    assert!(cache.get_tool_signature("tool_1").is_some());
    assert!(cache.get_signature_family(&sig).is_some());
    assert!(cache.get_session_signature("sid-1").is_some());

    cache.clear();

    assert!(cache.get_tool_signature("tool_1").is_none());
    assert!(cache.get_signature_family(&sig).is_none());
    assert!(cache.get_session_signature("sid-1").is_none());
}
