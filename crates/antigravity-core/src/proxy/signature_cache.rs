use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime};

// Node.js proxy uses 2 hours TTL
const SIGNATURE_TTL: Duration = Duration::from_secs(2 * 60 * 60);
const MIN_SIGNATURE_LENGTH: usize = 50;

// Different cache limits for different layers
const TOOL_CACHE_LIMIT: usize = 500; // Layer 1: Tool-specific signatures
const FAMILY_CACHE_LIMIT: usize = 200; // Layer 2: Model family mappings
const SESSION_CACHE_LIMIT: usize = 1000; // Layer 3: Session-based signatures (largest)

/// Cache entry with timestamp for TTL
#[derive(Clone, Debug)]
struct CacheEntry<T> {
    data: T,
    timestamp: SystemTime,
}

impl<T> CacheEntry<T> {
    fn new(data: T) -> Self {
        Self {
            data,
            timestamp: SystemTime::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed().unwrap_or(Duration::ZERO) > SIGNATURE_TTL
    }
}

/// Triple-layer signature cache to handle:
/// 1. Signature recovery for tool calls (when clients strip them)
/// 2. Cross-model compatibility checks (preventing Claude signatures on Gemini models)
/// 3. Session-based signature tracking (preventing cross-session pollution)
pub struct SignatureCache {
    /// Layer 1: Tool Use ID -> Thinking Signature
    /// Key: tool_use_id (e.g., "toolu_01...")
    /// Value: The thought signature that generated this tool call
    tool_signatures: RwLock<HashMap<String, CacheEntry<String>>>,

    /// Layer 2: Signature -> Model Family
    /// Key: thought signature string
    /// Value: Model family identifier (e.g., "claude-3-5-sonnet", "gemini-2.0-flash")
    thinking_families: RwLock<HashMap<String, CacheEntry<String>>>,

    /// Layer 3: Session ID -> Latest Thinking Signature (NEW)
    /// Key: session fingerprint (e.g., "sid-a1b2c3d4...")
    /// Value: The most recent valid thought signature for this session
    /// This prevents signature pollution between different conversations
    session_signatures: RwLock<HashMap<String, CacheEntry<String>>>,
}

impl SignatureCache {
    pub(crate) fn new() -> Self {
        Self {
            tool_signatures: RwLock::new(HashMap::new()),
            thinking_families: RwLock::new(HashMap::new()),
            session_signatures: RwLock::new(HashMap::new()),
        }
    }

    /// Global singleton instance
    pub fn global() -> &'static SignatureCache {
        static INSTANCE: OnceLock<SignatureCache> = OnceLock::new();
        INSTANCE.get_or_init(SignatureCache::new)
    }

    /// Store a tool call signature
    pub fn cache_tool_signature(&self, tool_use_id: &str, signature: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH {
            return;
        }

        if let Ok(mut cache) = self.tool_signatures.write() {
            tracing::debug!(
                "[SignatureCache] Caching tool signature for id: {}",
                tool_use_id
            );
            cache.insert(tool_use_id.to_string(), CacheEntry::new(signature));

            // Clean up expired entries when limit is reached
            if cache.len() > TOOL_CACHE_LIMIT {
                let before = cache.len();
                cache.retain(|_, v| !v.is_expired());
                let after = cache.len();
                if before != after {
                    tracing::debug!(
                        "[SignatureCache] Tool cache cleanup: {} -> {} entries",
                        before,
                        after
                    );
                }
            }
        }
    }

    /// Retrieve a signature for a tool_use_id
    pub fn get_tool_signature(&self, tool_use_id: &str) -> Option<String> {
        if let Ok(cache) = self.tool_signatures.read() {
            if let Some(entry) = cache.get(tool_use_id) {
                if !entry.is_expired() {
                    tracing::debug!(
                        "[SignatureCache] Hit tool signature for id: {}",
                        tool_use_id
                    );
                    return Some(entry.data.clone());
                }
            }
        }
        None
    }

    /// Store model family for a signature
    pub fn cache_thinking_family(&self, signature: String, family: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH {
            return;
        }

        if let Ok(mut cache) = self.thinking_families.write() {
            tracing::debug!(
                "[SignatureCache] Caching thinking family for sig (len={}): {}",
                signature.len(),
                family
            );
            cache.insert(signature, CacheEntry::new(family));

            if cache.len() > FAMILY_CACHE_LIMIT {
                let before = cache.len();
                cache.retain(|_, v| !v.is_expired());
                let after = cache.len();
                if before != after {
                    tracing::debug!(
                        "[SignatureCache] Family cache cleanup: {} -> {} entries",
                        before,
                        after
                    );
                }
            }
        }
    }

    /// Get model family for a signature
    /// NOTE: Family cache entries NEVER expire (model families are static)
    pub fn get_signature_family(&self, signature: &str) -> Option<String> {
        if let Ok(cache) = self.thinking_families.read() {
            if let Some(entry) = cache.get(signature) {
                return Some(entry.data.clone());
            }
        }
        None
    }

    // ===== Layer 3: Session-based Signature Storage =====

    /// Store the latest thinking signature for a session.
    /// This is the preferred method for tracking signatures across tool loops.
    ///
    /// # Arguments
    /// * `session_id` - Session fingerprint (e.g., "sid-a1b2c3d4...")
    /// * `signature` - The thought signature to store
    pub fn cache_session_signature(&self, session_id: &str, signature: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH {
            return;
        }

        if let Ok(mut cache) = self.session_signatures.write() {
            // Only update if new signature is longer (likely more complete)
            let should_store = match cache.get(session_id) {
                None => true,
                Some(existing) => {
                    // Expired entries should be replaced
                    existing.is_expired() || signature.len() > existing.data.len()
                }
            };

            if should_store {
                tracing::debug!(
                    "[SignatureCache] Session {} -> storing signature (len={})",
                    session_id,
                    signature.len()
                );
                cache.insert(session_id.to_string(), CacheEntry::new(signature));
            }

            // Cleanup when limit is reached (Session cache has largest limit)
            if cache.len() > SESSION_CACHE_LIMIT {
                let before = cache.len();
                cache.retain(|_, v| !v.is_expired());
                let after = cache.len();
                if before != after {
                    tracing::info!(
                        "[SignatureCache] Session cache cleanup: {} -> {} entries (limit: {})",
                        before,
                        after,
                        SESSION_CACHE_LIMIT
                    );
                }
            }
        }
    }

    /// Retrieve the latest thinking signature for a session.
    /// Returns None if not found or expired.
    pub fn get_session_signature(&self, session_id: &str) -> Option<String> {
        if let Ok(cache) = self.session_signatures.read() {
            if let Some(entry) = cache.get(session_id) {
                if !entry.is_expired() {
                    tracing::debug!(
                        "[SignatureCache] Session {} -> HIT (len={})",
                        session_id,
                        entry.data.len()
                    );
                    return Some(entry.data.clone());
                } else {
                    tracing::debug!("[SignatureCache] Session {} -> EXPIRED", session_id);
                }
            }
        }
        None
    }

    /// Clear all caches (for testing or manual reset)
    #[allow(dead_code)] // Used in tests
    pub fn clear(&self) {
        if let Ok(mut cache) = self.tool_signatures.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.thinking_families.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.session_signatures.write() {
            cache.clear();
        }
    }
}
