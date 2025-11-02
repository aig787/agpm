//! Render cache for template content during installation.
//!
//! This module provides caching functionality to avoid re-rendering the same
//! dependencies multiple times during a single installation operation.

use std::collections::HashMap;

use crate::core::ResourceType;

/// Cache key for rendered template content.
///
/// Uniquely identifies a rendered version of a resource based on:
/// - The source file path (canonical path to the resource)
/// - The resource type (Agent, Snippet, Command, etc.)
/// - Template variable overrides (pre-computed hash)
/// - The specific Git commit (resolved_commit) being rendered
///
/// This ensures that the same resource with different template_vars
/// or different commits produces different cache entries, while identical
/// resources share cached content across multiple parent resources.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderCacheKey {
    /// Canonical path to the resource file in the source repository
    pub(crate) resource_path: String,
    /// Resource type (Agent, Snippet, etc.)
    pub(crate) resource_type: ResourceType,
    /// Tool (claude-code, opencode, etc.) - CRITICAL for cache isolation!
    /// Same path renders differently for different tools.
    pub(crate) tool: Option<String>,
    /// Pre-computed variant_inputs_hash (never recomputed!)
    pub(crate) variant_inputs_hash: String,
    /// Resolved Git commit SHA - CRITICAL for cache isolation!
    /// Same resource path from different commits must have different cache entries.
    pub(crate) resolved_commit: Option<String>,
    /// Dependency hash for proper cache invalidation when dependencies change
    pub(crate) dependency_hash: String,
}

impl RenderCacheKey {
    /// Create a new cache key using the pre-computed variant_inputs_hash
    #[must_use]
    pub(crate) fn new(
        resource_path: String,
        resource_type: ResourceType,
        tool: Option<String>,
        variant_inputs_hash: String,
        resolved_commit: Option<String>,
        dependency_hash: String,
    ) -> Self {
        Self {
            resource_path,
            resource_type,
            tool,
            variant_inputs_hash,
            resolved_commit,
            dependency_hash,
        }
    }
}

/// Cache for rendered template content during installation.
///
/// This cache stores rendered content to avoid re-rendering the same
/// dependencies multiple times. It lives for the duration of a single
/// install operation and is cleared afterward.
///
/// # Performance Impact
///
/// For installations with many transitive dependencies (e.g., 145+ resources),
/// this cache prevents O(N²) rendering complexity by ensuring each unique
/// resource is rendered only once, regardless of how many parents depend on it.
///
/// # Cache Invalidation
///
/// The cache is cleared after each installation completes. It does not
/// persist across operations, ensuring that file changes are always reflected
/// in subsequent installations.
#[derive(Debug, Default)]
pub(crate) struct RenderCache {
    /// Map from cache key to rendered content
    cache: HashMap<RenderCacheKey, String>,
    /// Cache statistics
    hits: usize,
    misses: usize,
}

impl RenderCache {
    /// Create a new empty render cache
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Get cached rendered content if available
    pub(crate) fn get(&mut self, key: &RenderCacheKey) -> Option<&String> {
        if let Some(content) = self.cache.get(key) {
            self.hits += 1;
            Some(content)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert rendered content into the cache
    pub(crate) fn insert(&mut self, key: RenderCacheKey, content: String) {
        self.cache.insert(key, content);
    }

    /// Clear all cached content
    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get cache statistics
    #[must_use]
    pub(crate) fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }

    /// Calculate hit rate as a percentage
    #[must_use]
    pub(crate) fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_cache_key_includes_commit() {
        // Test that different commits produce different cache keys
        let key1 = RenderCacheKey::new(
            "agents/helper.md".to_string(),
            ResourceType::Agent,
            Some("claude-code".to_string()),
            "hash123".to_string(),
            Some("abc123def".to_string()),
            "dep_hash_123".to_string(),
        );

        let key2 = RenderCacheKey::new(
            "agents/helper.md".to_string(),
            ResourceType::Agent,
            Some("claude-code".to_string()),
            "hash123".to_string(),
            Some("def456ghi".to_string()),
            "dep_hash_456".to_string(),
        );

        assert_ne!(key1, key2, "Different commits should have different cache keys");

        // Test that same commits produce same cache keys
        let key3 = RenderCacheKey::new(
            "agents/helper.md".to_string(),
            ResourceType::Agent,
            Some("claude-code".to_string()),
            "hash123".to_string(),
            Some("abc123def".to_string()),
            "dep_hash_123".to_string(),
        );

        assert_eq!(key1, key3, "Same commits should have identical cache keys");

        // Test that None commits are handled correctly
        let key4 = RenderCacheKey::new(
            "agents/helper.md".to_string(),
            ResourceType::Agent,
            Some("claude-code".to_string()),
            "hash123".to_string(),
            None,
            "dep_hash_none".to_string(),
        );

        assert_ne!(key1, key4, "Some(commit) vs None should have different cache keys");
    }

    #[test]
    fn test_render_cache_basic_operations() {
        let mut cache = RenderCache::new();
        let key = RenderCacheKey::new(
            "test.md".to_string(),
            ResourceType::Snippet,
            Some("claude-code".to_string()),
            "hash789".to_string(),
            Some("commit123".to_string()),
            "dep_hash_test".to_string(),
        );

        // Initially empty
        assert_eq!(cache.stats(), (0, 0));
        assert_eq!(cache.hit_rate(), 0.0);

        // Cache miss
        assert!(cache.get(&key).is_none());
        assert_eq!(cache.stats(), (0, 1));

        // Insert and hit
        cache.insert(key.clone(), "rendered content".to_string());
        assert_eq!(cache.get(&key), Some(&"rendered content".to_string()));
        assert_eq!(cache.stats(), (1, 1));
        assert_eq!(cache.hit_rate(), 50.0);

        // Clear cache
        cache.clear();
        assert_eq!(cache.stats(), (0, 0));
        assert!(cache.get(&key).is_none());
    }
}
