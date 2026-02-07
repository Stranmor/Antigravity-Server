//! DEPRECATED: Use `SignatureCache` instead.
//!
//! This module uses a global singleton without content binding, which causes
//! signature loss in multi-turn conversations. `SignatureCache::cache_content_signature()`
//! binds signatures to their content via SHA256 hash for reliable recovery.
//!
//! Kept for backward compatibility with legacy streams (codex_stream, legacy_stream).
//! New code should use `crate::proxy::SignatureCache::global()` methods.

#![allow(deprecated, reason = "module itself is deprecated, self-references use deprecated items")]

use std::sync::{Mutex, OnceLock};

static GLOBAL_THOUGHT_SIG: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[deprecated(
    since = "3.3.46",
    note = "Use SignatureCache::cache_content_signature() for content-based signature storage"
)]
fn get_thought_sig_storage() -> &'static Mutex<Option<String>> {
    GLOBAL_THOUGHT_SIG.get_or_init(|| Mutex::new(None))
}

#[deprecated(since = "3.3.46", note = "Use SignatureCache::cache_content_signature() instead")]
pub fn store_thought_signature(sig: &str) {
    if let Ok(mut guard) = get_thought_sig_storage().lock() {
        let should_store = match &*guard {
            None => true,
            Some(existing) => sig.len() > existing.len(),
        };

        if should_store {
            tracing::debug!(
                "[ThoughtSig] Storing new signature (length: {}, replacing old length: {:?})",
                sig.len(),
                guard.as_ref().map(|s| s.len())
            );
            *guard = Some(sig.to_string());
        } else {
            tracing::debug!(
                "[ThoughtSig] Skipping shorter signature (new length: {}, existing length: {})",
                sig.len(),
                guard.as_ref().map(|s| s.len()).unwrap_or(0)
            );
        }
    }
}

#[deprecated(since = "3.3.46", note = "Use SignatureCache::get_content_signature() instead")]
pub fn get_thought_signature() -> Option<String> {
    if let Ok(guard) = get_thought_sig_storage().lock() {
        guard.clone()
    } else {
        None
    }
}

#[deprecated(since = "3.3.46", note = "Use SignatureCache instead")]
pub fn take_thought_signature() -> Option<String> {
    if let Ok(mut guard) = get_thought_sig_storage().lock() {
        guard.take()
    } else {
        None
    }
}

#[deprecated(since = "3.3.46", note = "Use SignatureCache instead")]
pub fn clear_thought_signature() {
    if let Ok(mut guard) = get_thought_sig_storage().lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_storage() {
        // Clear any existing state
        clear_thought_signature();

        // Should be empty initially
        assert!(get_thought_signature().is_none());

        // Store a signature
        store_thought_signature("test_signature_1234");
        assert_eq!(get_thought_signature(), Some("test_signature_1234".to_string()));

        // Shorter signature should NOT overwrite
        store_thought_signature("short");
        assert_eq!(get_thought_signature(), Some("test_signature_1234".to_string()));

        // Longer signature SHOULD overwrite
        store_thought_signature("test_signature_1234_longer_version");
        assert_eq!(get_thought_signature(), Some("test_signature_1234_longer_version".to_string()));

        // Take should clear
        let taken = take_thought_signature();
        assert_eq!(taken, Some("test_signature_1234_longer_version".to_string()));
        assert!(get_thought_signature().is_none());
    }
}
