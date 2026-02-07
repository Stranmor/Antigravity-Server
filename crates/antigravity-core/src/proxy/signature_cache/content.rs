//! Content-based signature caching with PostgreSQL persistence.

use super::{
    CacheEntry, ContentSignatureEntry, SignatureCache, CONTENT_CACHE_LIMIT, MIN_SIGNATURE_LENGTH,
};
use crate::proxy::signature_metrics::record_signature_cache;
use sha2::{Digest, Sha256};

impl SignatureCache {
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
            record_signature_cache("content", "store");
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
                record_signature_cache("content", "hit");
                return Some((entry.data.signature.clone(), entry.data.model_family.clone()));
            }
        }
        record_signature_cache("content", "miss");
        None
    }

    pub async fn get_content_signature_with_db(&self, content: &str) -> Option<(String, String)> {
        let content_hash = Self::compute_content_hash(content);

        // Check in-memory first WITHOUT recording metrics to avoid phantom miss
        // when DB fallback succeeds. Final outcome is recorded below.
        {
            let cache = self.content_signatures.read();
            if let Some(entry) = cache.get(&content_hash) {
                if !entry.is_expired() {
                    record_signature_cache("content", "hit");
                    return Some((entry.data.signature.clone(), entry.data.model_family.clone()));
                }
            }
        }

        let pool = self.get_pool()?;

        match crate::modules::signature_storage::get_signature(&pool, &content_hash).await {
            Ok(Some((sig, family))) => {
                tracing::info!(
                    "[SignatureCache] Content {} -> PostgreSQL HIT (sig_len={})",
                    content_hash,
                    sig.len()
                );
                record_signature_cache("content", "db_hit");
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
                record_signature_cache("content", "miss");
                None
            },
            Err(e) => {
                tracing::warn!("[SignatureCache] PostgreSQL read failed: {}", e);
                record_signature_cache("content", "miss");
                None
            },
        }
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
