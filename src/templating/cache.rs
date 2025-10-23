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
/// - Template variable overrides (hashed for efficient comparison)
///
/// This ensures that the same resource with different template_vars
/// produces different cache entries, while identical resources share
/// cached content across multiple parent resources.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderCacheKey {
    /// Canonical path to the resource file in the source repository
    pub(crate) resource_path: String,
    /// Resource type (Agent, Snippet, etc.)
    pub(crate) resource_type: ResourceType,
    /// Hash of template_vars JSON string
    pub(crate) template_vars_hash: u64,
}

impl RenderCacheKey {
    /// Create a new cache key with template variable hash
    pub(crate) fn new(
        resource_path: String,
        resource_type: ResourceType,
        template_vars: &str,
    ) -> Self {
        let template_vars_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            template_vars.hash(&mut hasher);
            hasher.finish()
        };

        Self {
            resource_path,
            resource_type,
            template_vars_hash,
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
    pub(crate) fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }

    /// Calculate hit rate as a percentage
    pub(crate) fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}
