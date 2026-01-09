//! Texture cache for efficient GPU resource management.
//!
//! Caches decoded images to avoid repeated loading and decoding operations.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::image::TextureData;

/// Entry in the texture cache.
#[derive(Debug)]
struct CacheEntry {
    /// The texture data.
    data: TextureData,
    /// Last access time.
    last_accessed: Instant,
    /// Size in bytes.
    size_bytes: usize,
    /// Reference count for usage tracking.
    ref_count: u32,
}

/// Configuration for the texture cache.
#[derive(Debug, Clone)]
pub struct TextureCacheConfig {
    /// Maximum cache size in bytes.
    pub max_size_bytes: usize,
    /// Maximum age before eviction (if not accessed).
    pub max_age: Duration,
    /// Maximum number of entries.
    pub max_entries: usize,
}

impl Default for TextureCacheConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 256 * 1024 * 1024, // 256 MB
            max_age: Duration::from_secs(300), // 5 minutes
            max_entries: 1000,
        }
    }
}

/// Texture cache for managing decoded image data.
///
/// Provides LRU-based eviction and size-based limits.
pub struct TextureCache {
    /// Cached textures by key.
    entries: HashMap<String, CacheEntry>,
    /// Cache configuration.
    config: TextureCacheConfig,
    /// Current total size in bytes.
    current_size: usize,
    /// Cache statistics.
    stats: CacheStats,
}

/// Cache statistics for monitoring.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Number of cache hits.
    pub hits: u64,
    /// Number of cache misses.
    pub misses: u64,
    /// Number of evictions.
    pub evictions: u64,
    /// Total bytes loaded.
    pub bytes_loaded: u64,
}

impl TextureCache {
    /// Create a new texture cache with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(TextureCacheConfig::default())
    }

    /// Create a new texture cache with custom configuration.
    #[must_use]
    pub fn with_config(config: TextureCacheConfig) -> Self {
        Self {
            entries: HashMap::new(),
            config,
            current_size: 0,
            stats: CacheStats::default(),
        }
    }

    /// Get a texture from the cache.
    ///
    /// Returns `None` if the texture is not cached.
    pub fn get(&mut self, key: &str) -> Option<&TextureData> {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_accessed = Instant::now();
            entry.ref_count += 1;
            self.stats.hits += 1;
            Some(&entry.data)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Insert a texture into the cache.
    ///
    /// May trigger eviction if cache limits are exceeded.
    pub fn insert(&mut self, key: String, data: TextureData) {
        let size_bytes = data.data.len();

        // Remove old entry if exists
        if let Some(old) = self.entries.remove(&key) {
            self.current_size -= old.size_bytes;
        }

        // Evict if necessary
        self.evict_if_needed(size_bytes);

        // Insert new entry
        self.current_size += size_bytes;
        self.stats.bytes_loaded += size_bytes as u64;

        self.entries.insert(
            key,
            CacheEntry {
                data,
                last_accessed: Instant::now(),
                size_bytes,
                ref_count: 1,
            },
        );
    }

    /// Get or create a texture.
    ///
    /// If the texture is not in cache, calls the loader function to create it.
    pub fn get_or_insert_with<F>(&mut self, key: &str, loader: F) -> &TextureData
    where
        F: FnOnce() -> TextureData,
    {
        if !self.entries.contains_key(key) {
            let data = loader();
            self.insert(key.to_string(), data);
        }

        // Update access time and return
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_accessed = Instant::now();
            entry.ref_count += 1;
            self.stats.hits += 1;
            &entry.data
        } else {
            unreachable!("Entry should exist after insert")
        }
    }

    /// Remove a texture from the cache.
    pub fn remove(&mut self, key: &str) -> Option<TextureData> {
        if let Some(entry) = self.entries.remove(key) {
            self.current_size -= entry.size_bytes;
            Some(entry.data)
        } else {
            None
        }
    }

    /// Check if a texture is cached.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Clear all cached textures.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_size = 0;
    }

    /// Get the current number of cached textures.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the current cache size in bytes.
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        self.current_size
    }

    /// Get cache statistics.
    #[must_use]
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Evict old entries if needed to make room for a new entry.
    fn evict_if_needed(&mut self, needed_bytes: usize) {
        // Evict if over size limit
        while self.current_size + needed_bytes > self.config.max_size_bytes
            && !self.entries.is_empty()
        {
            self.evict_lru();
        }

        // Evict if over entry limit
        while self.entries.len() >= self.config.max_entries && !self.entries.is_empty() {
            self.evict_lru();
        }

        // Evict expired entries
        self.evict_expired();
    }

    /// Evict the least recently used entry.
    fn evict_lru(&mut self) {
        let oldest_key = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone());

        if let Some(key) = oldest_key {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_size -= entry.size_bytes;
                self.stats.evictions += 1;
            }
        }
    }

    /// Evict entries that haven't been accessed recently.
    fn evict_expired(&mut self) {
        let now = Instant::now();
        let max_age = self.config.max_age;

        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_accessed) > max_age)
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_size -= entry.size_bytes;
                self.stats.evictions += 1;
            }
        }
    }

    /// Perform cache maintenance (call periodically).
    pub fn maintenance(&mut self) {
        self.evict_expired();
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe texture cache wrapper.
#[cfg(feature = "gpu")]
pub mod sync {
    use std::sync::{Arc, RwLock};

    use super::{CacheStats, TextureCache, TextureCacheConfig};
    use crate::image::TextureData;

    /// Thread-safe texture cache.
    #[derive(Clone)]
    pub struct SyncTextureCache {
        inner: Arc<RwLock<TextureCache>>,
    }

    impl SyncTextureCache {
        /// Create a new thread-safe texture cache.
        #[must_use]
        pub fn new() -> Self {
            Self {
                inner: Arc::new(RwLock::new(TextureCache::new())),
            }
        }

        /// Create with custom configuration.
        #[must_use]
        pub fn with_config(config: TextureCacheConfig) -> Self {
            Self {
                inner: Arc::new(RwLock::new(TextureCache::with_config(config))),
            }
        }

        /// Get a cloned texture from the cache.
        #[must_use]
        pub fn get(&self, key: &str) -> Option<TextureData> {
            let mut cache = self.inner.write().ok()?;
            cache.get(key).cloned()
        }

        /// Insert a texture into the cache.
        pub fn insert(&self, key: String, data: TextureData) {
            if let Ok(mut cache) = self.inner.write() {
                cache.insert(key, data);
            }
        }

        /// Check if a texture is cached.
        #[must_use]
        pub fn contains(&self, key: &str) -> bool {
            self.inner
                .read()
                .map(|cache| cache.contains(key))
                .unwrap_or(false)
        }

        /// Get cache statistics.
        #[must_use]
        pub fn stats(&self) -> Option<CacheStats> {
            self.inner.read().ok().map(|cache| cache.stats().clone())
        }
    }

    impl Default for SyncTextureCache {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{TextureCache, TextureCacheConfig};
    use crate::image::create_solid_color;

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = TextureCache::new();
        let texture = create_solid_color(10, 10, 255, 0, 0, 255);

        cache.insert("test".to_string(), texture.clone());

        assert!(cache.contains("test"));
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get("test");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().width, 10);
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = TextureCache::new();
        assert!(cache.get("nonexistent").is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_cache_eviction_by_size() {
        let config = TextureCacheConfig {
            max_size_bytes: 1000, // Very small
            max_age: Duration::from_secs(3600),
            max_entries: 100,
        };

        let mut cache = TextureCache::with_config(config);

        // Insert a texture larger than max size
        let texture = create_solid_color(20, 20, 255, 0, 0, 255); // 1600 bytes
        cache.insert("big".to_string(), texture);

        // Should still be inserted (we don't reject, just evict old)
        assert!(cache.contains("big"));
    }

    #[test]
    fn test_cache_eviction_by_count() {
        let config = TextureCacheConfig {
            max_size_bytes: 1024 * 1024,
            max_age: Duration::from_secs(3600),
            max_entries: 2,
        };

        let mut cache = TextureCache::with_config(config);

        cache.insert("a".to_string(), create_solid_color(2, 2, 255, 0, 0, 255));
        cache.insert("b".to_string(), create_solid_color(2, 2, 0, 255, 0, 255));
        cache.insert("c".to_string(), create_solid_color(2, 2, 0, 0, 255, 255));

        // Should have evicted the oldest
        assert!(cache.len() <= 2);
    }

    #[test]
    fn test_cache_remove() {
        let mut cache = TextureCache::new();
        let texture = create_solid_color(10, 10, 255, 0, 0, 255);

        cache.insert("test".to_string(), texture);
        assert!(cache.contains("test"));

        let removed = cache.remove("test");
        assert!(removed.is_some());
        assert!(!cache.contains("test"));
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = TextureCache::new();

        cache.insert("a".to_string(), create_solid_color(2, 2, 255, 0, 0, 255));
        cache.insert("b".to_string(), create_solid_color(2, 2, 0, 255, 0, 255));

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert_eq!(cache.size_bytes(), 0);
    }

    #[test]
    fn test_get_or_insert_with() {
        let mut cache = TextureCache::new();

        // First call should invoke loader
        let data = cache.get_or_insert_with("lazy", || create_solid_color(5, 5, 128, 128, 128, 255));

        assert_eq!(data.width, 5);
        assert!(cache.contains("lazy"));

        // Second call should use cache
        let data2 = cache.get_or_insert_with("lazy", || create_solid_color(10, 10, 0, 0, 0, 255));

        assert_eq!(data2.width, 5); // Still the original
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = TextureCache::new();

        cache.insert("a".to_string(), create_solid_color(2, 2, 255, 0, 0, 255));

        let _ = cache.get("a"); // Hit
        let _ = cache.get("b"); // Miss
        let _ = cache.get("a"); // Hit

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
    }
}
