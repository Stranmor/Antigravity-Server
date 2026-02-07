#![allow(dead_code, reason = "content_signatures reserved for future use")]

mod content;
mod session;

use crate::proxy::mappers::claude::request::MIN_SIGNATURE_LENGTH;
use crate::proxy::signature_metrics::record_signature_cache;
use parking_lot::RwLock;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

const SIGNATURE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

const TOOL_CACHE_LIMIT: usize = 500;
const FAMILY_CACHE_LIMIT: usize = 200;
const SESSION_CACHE_LIMIT: usize = 1000;
const CONTENT_CACHE_LIMIT: usize = 2000;

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

    pub fn cache_tool_signature(&self, tool_use_id: &str, signature: String) {
        if signature.len() < MIN_SIGNATURE_LENGTH {
            return;
        }

        let mut cache = self.tool_signatures.write();
        tracing::debug!("[SignatureCache] Caching tool signature for id: {}", tool_use_id);
        record_signature_cache("tool", "store");
        cache.insert(tool_use_id.to_string(), CacheEntry::new(signature));

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

    pub fn get_tool_signature(&self, tool_use_id: &str) -> Option<String> {
        let cache = self.tool_signatures.read();
        if let Some(entry) = cache.get(tool_use_id) {
            if !entry.is_expired() {
                tracing::debug!("[SignatureCache] Hit tool signature for id: {}", tool_use_id);
                record_signature_cache("tool", "hit");
                return Some(entry.data.clone());
            }
        }
        record_signature_cache("tool", "miss");
        None
    }

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
        record_signature_cache("family", "store");
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

    pub fn get_signature_family(&self, signature: &str) -> Option<String> {
        let cache = self.thinking_families.read();
        match cache.get(signature) {
            Some(entry) => {
                record_signature_cache("family", "hit");
                Some(entry.data.clone())
            },
            None => {
                record_signature_cache("family", "miss");
                None
            },
        }
    }

    #[allow(dead_code, reason = "used in tests to verify cache clearing behavior")]
    pub fn clear(&self) {
        self.tool_signatures.write().clear();
        self.thinking_families.write().clear();
        self.session_signatures.write().clear();
        self.content_signatures.write().clear();
    }
}
