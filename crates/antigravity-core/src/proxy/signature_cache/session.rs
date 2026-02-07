use super::{CacheEntry, SignatureCache, MIN_SIGNATURE_LENGTH, SESSION_CACHE_LIMIT};
use crate::proxy::signature_metrics::record_signature_cache;

impl SignatureCache {
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
            record_signature_cache("session", "store");
            cache.insert(session_id.to_string(), CacheEntry::new(signature.clone()));

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

    /// Check if session has a cached signature WITHOUT recording metrics.
    /// Used for diagnostic probes (e.g., degradation checks) that shouldn't inflate counters.
    pub fn has_session_signature(&self, session_id: &str) -> bool {
        let cache = self.session_signatures.read();
        cache.get(session_id).is_some_and(|entry| !entry.is_expired())
    }

    pub fn get_session_signature(&self, session_id: &str) -> Option<String> {
        let cache = self.session_signatures.read();
        if let Some(entry) = cache.get(session_id) {
            if !entry.is_expired() {
                tracing::debug!(
                    "[SignatureCache] Session {} -> HIT (len={})",
                    session_id,
                    entry.data.len()
                );
                record_signature_cache("session", "hit");
                return Some(entry.data.clone());
            } else {
                tracing::debug!("[SignatureCache] Session {} -> EXPIRED", session_id);
            }
        }
        record_signature_cache("session", "miss");
        None
    }

    pub async fn get_session_signature_with_db(&self, session_id: &str) -> Option<String> {
        // Check in-memory first WITHOUT recording metrics to avoid phantom miss
        // when DB fallback succeeds. Final outcome is recorded below.
        {
            let cache = self.session_signatures.read();
            if let Some(entry) = cache.get(session_id) {
                if !entry.is_expired() {
                    record_signature_cache("session", "hit");
                    return Some(entry.data.clone());
                }
            }
        }

        let pool = self.get_pool()?;

        match crate::modules::signature_storage::get_session_signature(&pool, session_id).await {
            Ok(Some(sig)) => {
                tracing::info!(
                    "[SignatureCache] Session {} -> PostgreSQL HIT (sig_len={})",
                    session_id,
                    sig.len()
                );
                record_signature_cache("session", "db_hit");
                let mut cache = self.session_signatures.write();
                cache.insert(session_id.to_string(), CacheEntry::new(sig.clone()));
                Some(sig)
            },
            Ok(None) => {
                tracing::debug!("[SignatureCache] Session {} -> PostgreSQL MISS", session_id);
                record_signature_cache("session", "miss");
                None
            },
            Err(e) => {
                tracing::warn!(
                    "[SignatureCache] Session {} -> PostgreSQL error: {}",
                    session_id,
                    e
                );
                record_signature_cache("session", "miss");
                None
            },
        }
    }
}
