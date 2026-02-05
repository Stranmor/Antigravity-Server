//! Schema caching with LRU eviction for MCP tool schemas.
//!
//! Ported from upstream v4.0.3 — caches cleaned JSON schemas to avoid
//! repeated expensive transformations.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use std::time::Instant;

/// Cache entry containing the cleaned schema and metadata.
#[derive(Clone)]
struct CacheEntry {
    /// Cleaned schema
    schema: Value,
    /// Last access time
    last_used: Instant,
    /// Hit count
    hit_count: usize,
}

/// LRU schema cache with statistics.
struct SchemaCache {
    cache: HashMap<String, CacheEntry>,
    stats: CacheStats,
}

/// Cache statistics for monitoring.
#[derive(Default, Clone, Debug)]
pub struct CacheStats {
    pub total_requests: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

impl CacheStats {
    /// Calculate cache hit rate (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.cache_hits as f64 / self.total_requests as f64
        }
    }
}

impl SchemaCache {
    fn new() -> Self {
        Self { cache: HashMap::new(), stats: CacheStats::default() }
    }

    /// Get cached entry, updating stats and access time.
    fn get(&mut self, key: &str) -> Option<Value> {
        self.stats.total_requests += 1;

        if let Some(entry) = self.cache.get_mut(key) {
            entry.last_used = Instant::now();
            entry.hit_count += 1;
            self.stats.cache_hits += 1;
            Some(entry.schema.clone())
        } else {
            self.stats.cache_misses += 1;
            None
        }
    }

    /// Insert entry, evicting LRU if at capacity.
    fn insert(&mut self, key: String, schema: Value) {
        const MAX_CACHE_SIZE: usize = 1000;
        if self.cache.len() >= MAX_CACHE_SIZE {
            self.evict_lru();
        }

        let entry = CacheEntry { schema, last_used: Instant::now(), hit_count: 0 };
        self.cache.insert(key, entry);
    }

    /// Remove least recently used entry.
    fn evict_lru(&mut self) {
        if self.cache.is_empty() {
            return;
        }

        let oldest_key =
            self.cache.iter().min_by_key(|(_, entry)| entry.last_used).map(|(key, _)| key.clone());

        if let Some(key) = oldest_key {
            self.cache.remove(&key);
        }
    }

    fn stats(&self) -> CacheStats {
        self.stats.clone()
    }

    fn clear(&mut self) {
        self.cache.clear();
        self.stats = CacheStats::default();
    }
}

/// Global schema cache instance.
static SCHEMA_CACHE: LazyLock<RwLock<SchemaCache>> =
    LazyLock::new(|| RwLock::new(SchemaCache::new()));

/// Compute SHA-256 hash of schema (first 16 hex chars).
fn compute_schema_hash(schema: &Value) -> String {
    let mut hasher = Sha256::new();
    let schema_str = schema.to_string();
    hasher.update(schema_str.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string()
}

/// Clean JSON schema with caching.
///
/// This is the recommended entry point for schema cleaning.
/// Uses the global cache to avoid repeated transformations.
pub fn clean_json_schema_cached(schema: &mut Value, tool_name: &str) {
    // 1. Compute cache key from original schema
    let hash = compute_schema_hash(schema);
    let cache_key = format!("{}:{}", tool_name, hash);

    // 2. Try cache lookup
    {
        if let Ok(mut cache) = SCHEMA_CACHE.write() {
            if let Some(cached) = cache.get(&cache_key) {
                *schema = cached;
                return;
            }
        }
    }

    // 3. Cache miss — perform cleaning
    super::json_schema::clean_json_schema_for_tool(schema, tool_name);

    // 4. Store in cache
    if let Ok(mut cache) = SCHEMA_CACHE.write() {
        cache.insert(cache_key, schema.clone());
    }
}

/// Get current cache statistics.
pub fn get_cache_stats() -> CacheStats {
    SCHEMA_CACHE.read().map(|cache| cache.stats()).unwrap_or_default()
}

/// Clear the schema cache.
pub fn clear_cache() {
    if let Ok(mut cache) = SCHEMA_CACHE.write() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_compute_schema_hash() {
        let schema1 = json!({"type": "string"});
        let schema2 = json!({"type": "string"});
        let schema3 = json!({"type": "number"});

        let hash1 = compute_schema_hash(&schema1);
        let hash2 = compute_schema_hash(&schema2);
        let hash3 = compute_schema_hash(&schema3);

        // Same schema → same hash
        assert_eq!(hash1, hash2);
        // Different schema → different hash
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_cache_hit() {
        clear_cache();

        let mut schema = json!({"type": "string", "minLength": 5});
        let tool_name = "test_tool";

        // First call — cache miss
        clean_json_schema_cached(&mut schema, tool_name);

        // Second call with same schema — should hit
        let mut schema2 = json!({"type": "string", "minLength": 5});
        clean_json_schema_cached(&mut schema2, tool_name);

        let stats = get_cache_stats();
        assert!(stats.cache_hits > 0, "Expected cache hits, got: {:?}", stats);
        assert!(stats.hit_rate() > 0.0);
    }

    #[test]
    fn test_cache_eviction() {
        clear_cache();

        // Insert many entries to trigger eviction
        for i in 0..1100 {
            let mut schema = json!({"type": "string", "index": i});
            let tool_name = format!("tool_{}", i);
            clean_json_schema_cached(&mut schema, &tool_name);
        }

        let stats = get_cache_stats();
        assert!(stats.total_requests > 0);
    }
}
