use super::{CacheEntry, SignatureCache, MIN_SIGNATURE_LENGTH, SESSION_CACHE_LIMIT};

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

    pub async fn get_session_signature_with_db(&self, session_id: &str) -> Option<String> {
        if let Some(sig) = self.get_session_signature(session_id) {
            return Some(sig);
        }

        let pool = self.get_pool()?;

        match crate::modules::signature_storage::get_session_signature(&pool, session_id).await {
            Ok(Some(sig)) => {
                tracing::info!(
                    "[SignatureCache] Session {} -> PostgreSQL HIT (sig_len={})",
                    session_id,
                    sig.len()
                );
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
}
