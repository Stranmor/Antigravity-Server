#![allow(dead_code, reason = "content_signatures reserved for future use")]

use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

const SIGNATURE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MIN_SIGNATURE_LENGTH: usize = 50;

const TOOL_CACHE_LIMIT: usize = 500;
const FAMILY_CACHE_LIMIT: usize = 200;
const SESSION_CACHE_LIMIT: usize = 1000;
const CONTENT_CACHE_LIMIT: usize = 2000;

/// Cache entry with timestamp for TTL
#[derive(Clone, Debug)]
struct CacheEntry<T> {
    data: T,
    timestamp: SystemTime,
}

impl<T> CacheEntry<T> {
    fn new(data: T) -> Self {
        Self { data, timestamp: SystemTime::now() }
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed().unwrap_or(Duration::ZERO) > SIGNATURE_TTL
    }
}

/// Triple-layer signature cache to handle:
/// 1. Signature recovery for tool calls (when clients strip them)
/// 2. Cross-model compatibility checks (preventing Claude signatures on Gemini models)
/// 3. Session-based signature tracking (persisted to PostgreSQL)
pub struct SignatureCache {
    tool_signatures: RwLock<HashMap<String, CacheEntry<String>>>,
    thinking_families: RwLock<HashMap<String, CacheEntry<String>>>,
    session_signatures: RwLock<HashMap<String, CacheEntry<String>>>,
    content_signatures: RwLock<HashMap<String, CacheEntry<ContentSignatureEntry>>>,
    db_pool: RwLock<Option<Arc<PgPool>>>,
}

#[derive(Clone, Debug)]
struct ContentSignatureEntry {
    signature: String,
    model_family: String,
}

impl SignatureCache {
    pub(crate) fn new() -> Self {
        Self {
            tool_signatures: RwLock::new(HashMap::new()),
            thinking_families: RwLock::new(HashMap::new()),
            session_signatures: RwLock::new(HashMap::new()),
            content_signatures: RwLock::new(HashMap::new()),
            db_pool: RwLock::new(None),
        }
    }

    /// Global singleton instance
    pub fn global() -> &'static SignatureCache {
        static INSTANCE: OnceLock<SignatureCache> = OnceLock::new();
        INSTANCE.get_or_init(SignatureCache::new)
    }

    pub fn set_db_pool(&self, pool: PgPool) {
        *self.db_pool.write() = Some(Arc::new(pool));
        tracing::info!("[SignatureCache] PostgreSQL pool configured for persistent storage");
    }

    fn get_pool(&self) -> Option<Arc<PgPool>> {
        self.db_pool.read().clone()
    }

    /// Store a tool call signature
    pub fn cache_tool_signature(&self, tool_use_id: &str, signature: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH {
            return;
        }

        let mut cache = self.tool_signatures.write();
        tracing::debug!("[SignatureCache] Caching tool signature for id: {}", tool_use_id);
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

    /// Retrieve a signature for a tool_use_id
    pub fn get_tool_signature(&self, tool_use_id: &str) -> Option<String> {
        let cache = self.tool_signatures.read();
        if let Some(entry) = cache.get(tool_use_id) {
            if !entry.is_expired() {
                tracing::debug!("[SignatureCache] Hit tool signature for id: {}", tool_use_id);
                return Some(entry.data.clone());
            }
        }
        None
    }

    /// Store model family for a signature
    pub fn cache_thinking_family(&self, signature: String, family: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH {
            return;
        }

        let mut cache = self.thinking_families.write();
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

    /// Get model family for a signature
    /// NOTE: Family cache entries NEVER expire (model families are static)
    pub fn get_signature_family(&self, signature: &str) -> Option<String> {
        let cache = self.thinking_families.read();
        cache.get(signature).map(|entry| entry.data.clone())
    }

    // ===== Layer 3: Session-based Signature Storage =====
    // Persisted to PostgreSQL for survival across server restarts.

    /// Store the latest thinking signature for a session.
    /// Writes to both in-memory cache AND PostgreSQL (async).
    pub fn cache_session_signature(&self, session_id: &str, signature: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH {
            return;
        }

        let mut cache = self.session_signatures.write();
        let should_store = match cache.get(session_id) {
            None => true,
            Some(existing) => existing.is_expired() || signature.len() > existing.data.len(),
        };

        if should_store {
            tracing::debug!(
                "[SignatureCache] Session {} -> storing signature (len={})",
                session_id,
                signature.len()
            );
            cache.insert(session_id.to_string(), CacheEntry::new(signature.clone()));

            // Async persist to PostgreSQL
            if let Some(pool) = self.get_pool() {
                let sid = session_id.to_string();
                let sig = signature;
                tokio::spawn(async move {
                    if let Err(e) = crate::modules::signature_storage::store_session_signature(
                        &pool, &sid, &sig,
                    )
                    .await
                    {
                        tracing::warn!(
                            "[SignatureCache] Session DB write failed for {}: {}",
                            sid,
                            e
                        );
                    } else {
                        tracing::debug!(
                            "[SignatureCache] Session {} -> persisted to PostgreSQL",
                            sid
                        );
                    }
                });
            }
        }

        // Cleanup when limit is reached
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

    /// Retrieve session signature from in-memory cache only (fast path).
    pub fn get_session_signature(&self, session_id: &str) -> Option<String> {
        let cache = self.session_signatures.read();
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
        None
    }

    /// Retrieve session signature with PostgreSQL fallback.
    /// Use this when in-memory cache might be cold (e.g. after server restart).
    pub async fn get_session_signature_with_db(&self, session_id: &str) -> Option<String> {
        // Fast path: check in-memory cache first
        if let Some(sig) = self.get_session_signature(session_id) {
            return Some(sig);
        }

        // Slow path: check PostgreSQL
        let pool = self.get_pool()?;

        match crate::modules::signature_storage::get_session_signature(&pool, session_id).await {
            Ok(Some(sig)) => {
                tracing::info!(
                    "[SignatureCache] Session {} -> PostgreSQL HIT (sig_len={})",
                    session_id,
                    sig.len()
                );
                // Backfill in-memory cache
                let mut cache = self.session_signatures.write();
                cache.insert(session_id.to_string(), CacheEntry::new(sig.clone()));
                Some(sig)
            },
            Ok(None) => {
                tracing::debug!("[SignatureCache] Session {} -> PostgreSQL MISS", session_id);
                None
            },
            Err(e) => {
                tracing::warn!(
                    "[SignatureCache] Session {} -> PostgreSQL error: {}",
                    session_id,
                    e
                );
                None
            },
        }
    }

    pub fn compute_content_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        format!("ch-{}", &hash[..16])
    }

    pub fn cache_content_signature(&self, content: &str, signature: String, model_family: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH || content.len() < 20 {
            return;
        }

        let content_hash = Self::compute_content_hash(content);

        {
            let mut cache = self.content_signatures.write();
            tracing::debug!(
                "[SignatureCache] Content {} -> storing signature (len={})",
                content_hash,
                signature.len()
            );
            cache.insert(
                content_hash.clone(),
                CacheEntry::new(ContentSignatureEntry {
                    signature: signature.clone(),
                    model_family: model_family.clone(),
                }),
            );

            if cache.len() > CONTENT_CACHE_LIMIT {
                let before = cache.len();
                cache.retain(|_, v| !v.is_expired());
                tracing::debug!(
                    "[SignatureCache] Content cache cleanup: {} -> {} entries",
                    before,
                    cache.len()
                );
            }
        }

        if let Some(pool) = self.get_pool() {
            let hash = content_hash;
            let sig = signature;
            let family = model_family;
            tokio::spawn(async move {
                if let Err(e) =
                    crate::modules::signature_storage::store_signature(&pool, &hash, &sig, &family)
                        .await
                {
                    tracing::warn!("[SignatureCache] PostgreSQL write failed: {}", e);
                } else {
                    tracing::debug!("[SignatureCache] Content {} -> persisted to PostgreSQL", hash);
                }
            });
        }
    }

    pub fn get_content_signature(&self, content: &str) -> Option<(String, String)> {
        let content_hash = Self::compute_content_hash(content);

        let cache = self.content_signatures.read();
        if let Some(entry) = cache.get(&content_hash) {
            if !entry.is_expired() {
                tracing::info!(
                    "[SignatureCache] Content {} -> HIT (sig_len={})",
                    content_hash,
                    entry.data.signature.len()
                );
                return Some((entry.data.signature.clone(), entry.data.model_family.clone()));
            }
        }
        None
    }

    pub async fn get_content_signature_with_db(&self, content: &str) -> Option<(String, String)> {
        if let Some(cached) = self.get_content_signature(content) {
            return Some(cached);
        }

        let content_hash = Self::compute_content_hash(content);
        let pool = self.get_pool()?;

        match crate::modules::signature_storage::get_signature(&pool, &content_hash).await {
            Ok(Some((sig, family))) => {
                tracing::info!(
                    "[SignatureCache] Content {} -> PostgreSQL HIT (sig_len={})",
                    content_hash,
                    sig.len()
                );
                let mut cache = self.content_signatures.write();
                cache.insert(
                    content_hash,
                    CacheEntry::new(ContentSignatureEntry {
                        signature: sig.clone(),
                        model_family: family.clone(),
                    }),
                );
                Some((sig, family))
            },
            Ok(None) => {
                tracing::debug!("[SignatureCache] Content {} -> PostgreSQL MISS", content_hash);
                None
            },
            Err(e) => {
                tracing::warn!("[SignatureCache] PostgreSQL read failed: {}", e);
                None
            },
        }
    }

    #[allow(dead_code)]
    pub fn clear(&self) {
        self.tool_signatures.write().clear();
        self.thinking_families.write().clear();
        self.session_signatures.write().clear();
        self.content_signatures.write().clear();
    }

    pub async fn preload_signatures_from_db(&self, content_hashes: &[String]) {
        let pool = match self.get_pool() {
            Some(p) => p,
            None => return,
        };

        for hash in content_hashes {
            if self.content_signatures.read().contains_key(hash) {
                continue;
            }

            match crate::modules::signature_storage::get_signature(&pool, hash).await {
                Ok(Some((sig, family))) => {
                    tracing::info!(
                        "[SignatureCache] Preloaded {} from PostgreSQL (sig_len={})",
                        hash,
                        sig.len()
                    );
                    let mut cache = self.content_signatures.write();
                    cache.insert(
                        hash.clone(),
                        CacheEntry::new(ContentSignatureEntry {
                            signature: sig,
                            model_family: family,
                        }),
                    );
                },
                Ok(None) => {},
                Err(e) => {
                    tracing::warn!("[SignatureCache] Preload failed for {}: {}", hash, e);
                },
            }
        }
    }
}
