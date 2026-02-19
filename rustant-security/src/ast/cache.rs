//! Thread-safe LRU cache for parsed ASTs.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Cache key: (file_path, content_hash).
type CacheKey = (String, String);

/// A thread-safe, LRU AST cache.
///
/// Keyed by `(file_path, content_hash)` so that changes to file contents
/// automatically invalidate stale entries.
pub struct AstCache {
    entries: RwLock<AstCacheInner>,
    max_entries: usize,
}

struct AstCacheInner {
    /// Stored AST data (serialized or type-erased).
    data: HashMap<CacheKey, CachedAst>,
    /// LRU ordering: most recently used at the back.
    access_order: Vec<CacheKey>,
}

/// A cached AST entry.
#[derive(Clone)]
pub struct CachedAst {
    /// The raw source code (for re-parsing if needed).
    pub source: Arc<str>,
    /// Extracted symbols.
    pub symbols: Vec<super::Symbol>,
    /// Cyclomatic complexity per function.
    pub complexity: HashMap<String, u32>,
    /// Language detected.
    pub language: super::Language,
}

impl AstCache {
    /// Create a new cache with the given maximum number of entries.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(AstCacheInner {
                data: HashMap::new(),
                access_order: Vec::new(),
            }),
            max_entries,
        }
    }

    /// Get a cached AST entry.
    pub fn get(&self, file_path: &str, content_hash: &str) -> Option<CachedAst> {
        let key = (file_path.to_string(), content_hash.to_string());
        let mut inner = self.entries.write().ok()?;

        if let Some(entry) = inner.data.get(&key).cloned() {
            // Move to end of access order (most recently used)
            inner.access_order.retain(|k| k != &key);
            inner.access_order.push(key);
            Some(entry)
        } else {
            None
        }
    }

    /// Insert or update a cached AST entry.
    pub fn insert(&self, file_path: &str, content_hash: &str, entry: CachedAst) {
        let key = (file_path.to_string(), content_hash.to_string());
        let mut inner = match self.entries.write() {
            Ok(inner) => inner,
            Err(_) => return,
        };

        // Evict LRU entries if at capacity
        while inner.data.len() >= self.max_entries && !inner.access_order.is_empty() {
            let lru_key = inner.access_order.remove(0);
            inner.data.remove(&lru_key);
        }

        inner.access_order.retain(|k| k != &key);
        inner.access_order.push(key.clone());
        inner.data.insert(key, entry);
    }

    /// Invalidate a specific file (all hashes).
    pub fn invalidate_file(&self, file_path: &str) {
        if let Ok(mut inner) = self.entries.write() {
            let keys_to_remove: Vec<CacheKey> = inner
                .data
                .keys()
                .filter(|(path, _)| path == file_path)
                .cloned()
                .collect();

            for key in &keys_to_remove {
                inner.data.remove(key);
            }
            inner.access_order.retain(|k| !keys_to_remove.contains(k));
        }
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.entries.write() {
            inner.data.clear();
            inner.access_order.clear();
        }
    }

    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        self.entries
            .read()
            .map(|inner| inner.data.len())
            .unwrap_or(0)
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for AstCache {
    fn default() -> Self {
        Self::new(1000) // Default: cache up to 1000 files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Language;

    fn make_entry(source: &str) -> CachedAst {
        CachedAst {
            source: Arc::from(source),
            symbols: Vec::new(),
            complexity: HashMap::new(),
            language: Language::Rust,
        }
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = AstCache::new(10);
        let entry = make_entry("fn main() {}");

        cache.insert("src/main.rs", "abc123", entry.clone());
        assert!(cache.get("src/main.rs", "abc123").is_some());
        assert!(cache.get("src/main.rs", "different_hash").is_none());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = AstCache::new(2);

        cache.insert("a.rs", "h1", make_entry("a"));
        cache.insert("b.rs", "h2", make_entry("b"));
        assert_eq!(cache.len(), 2);

        // This should evict a.rs (LRU)
        cache.insert("c.rs", "h3", make_entry("c"));
        assert_eq!(cache.len(), 2);
        assert!(cache.get("a.rs", "h1").is_none());
        assert!(cache.get("c.rs", "h3").is_some());
    }

    #[test]
    fn test_cache_invalidate_file() {
        let cache = AstCache::new(10);
        cache.insert("a.rs", "h1", make_entry("a1"));
        cache.insert("a.rs", "h2", make_entry("a2"));
        cache.insert("b.rs", "h3", make_entry("b"));

        cache.invalidate_file("a.rs");
        assert_eq!(cache.len(), 1);
        assert!(cache.get("b.rs", "h3").is_some());
    }

    #[test]
    fn test_cache_clear() {
        let cache = AstCache::new(10);
        cache.insert("a.rs", "h1", make_entry("a"));
        cache.insert("b.rs", "h2", make_entry("b"));

        cache.clear();
        assert!(cache.is_empty());
    }
}
