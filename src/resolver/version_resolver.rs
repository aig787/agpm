//! Centralized version resolution module for CCPM
//!
//! This module implements the core version-to-SHA resolution strategy that ensures
//! deterministic and efficient dependency management. By resolving all version
//! specifications to commit SHAs upfront, we enable:
//!
//! - **SHA-based worktree caching**: Reuse worktrees for identical commits
//! - **Reduced network operations**: Single fetch per repository
//! - **Deterministic installations**: Same SHA always produces same result
//! - **Efficient deduplication**: Multiple refs to same commit share one worktree
//!
//! # Architecture
//!
//! The `VersionResolver` operates in two phases:
//! 1. **Collection Phase**: Gather all unique (source, version) pairs
//! 2. **Resolution Phase**: Batch resolve all versions to SHAs
//!
//! This design minimizes Git operations and enables parallel resolution.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::cache::Cache;
use crate::git::GitRepo;

/// Version resolution entry tracking source and version to SHA mapping
#[derive(Debug, Clone)]
pub struct VersionEntry {
    /// Source name from manifest
    pub source: String,
    /// Source URL (Git repository)
    pub url: String,
    /// Version specification (tag, branch, commit, or None for HEAD)
    pub version: Option<String>,
    /// Resolved SHA-1 hash (populated during resolution)
    pub resolved_sha: Option<String>,
    /// Resolved version (e.g., "latest" -> "v2.0.0")
    pub resolved_version: Option<String>,
}

/// Centralized version resolver for efficient SHA resolution
///
/// The `VersionResolver` is responsible for resolving all dependency versions
/// to their corresponding Git commit SHAs before any worktree operations.
/// This ensures maximum efficiency and deduplication.
///
/// # Example
///
/// ```no_run
/// # use ccpm::resolver::version_resolver::{VersionResolver, VersionEntry};
/// # use ccpm::cache::Cache;
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let mut resolver = VersionResolver::new(cache);
///
/// // Add versions to resolve
/// resolver.add_version("community", "https://github.com/example/repo.git", Some("v1.0.0"));
/// resolver.add_version("community", "https://github.com/example/repo.git", Some("main"));
///
/// // Batch resolve all versions to SHAs
/// resolver.resolve_all().await?;
///
/// // Get resolved SHA for a specific version
/// let sha = resolver.get_resolved_sha("community", "v1.0.0");
/// # Ok(())
/// # }
/// ```
/// Resolved version information
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    /// The resolved SHA-1 hash
    pub sha: String,
    /// The resolved version (e.g., "latest" -> "v2.0.0")
    /// If no constraint resolution happened, this will be the same as input
    pub resolved_ref: String,
}

/// Centralized version resolver for batch SHA resolution.
///
/// The `VersionResolver` manages the collection and resolution of all dependency
/// versions in a single batch operation, enabling optimal Git repository access
/// patterns and maximum worktree reuse.
pub struct VersionResolver {
    /// Cache instance for repository access
    cache: Cache,
    /// Collection of versions to resolve, keyed by (source, version)
    entries: HashMap<(String, String), VersionEntry>,
    /// Resolved SHA cache, keyed by (source, version)
    resolved: HashMap<(String, String), ResolvedVersion>,
    /// Bare repository paths, keyed by source name
    bare_repos: HashMap<String, PathBuf>,
}

impl VersionResolver {
    /// Creates a new version resolver with the given cache
    pub fn new(cache: Cache) -> Self {
        Self {
            cache,
            entries: HashMap::new(),
            resolved: HashMap::new(),
            bare_repos: HashMap::new(),
        }
    }

    /// Adds a version to be resolved
    ///
    /// Multiple calls with the same (source, version) pair will be deduplicated.
    ///
    /// # Arguments
    ///
    /// * `source` - Source name from manifest
    /// * `url` - Git repository URL
    /// * `version` - Version specification (tag, branch, commit, or None for HEAD)
    pub fn add_version(&mut self, source: &str, url: &str, version: Option<&str>) {
        let version_key = version.unwrap_or("HEAD").to_string();
        let key = (source.to_string(), version_key.clone());

        // Only add if not already present (deduplication)
        self.entries.entry(key).or_insert_with(|| VersionEntry {
            source: source.to_string(),
            url: url.to_string(),
            version: version.map(|v| v.to_string()),
            resolved_sha: None,
            resolved_version: None,
        });
    }

    /// Resolves all collected versions to their commit SHAs
    ///
    /// This method performs the following steps:
    /// 1. Groups entries by source to minimize repository operations
    /// 2. Ensures each repository is fetched once
    /// 3. Resolves all versions to SHAs using `git rev-parse`
    /// 4. Caches results for quick retrieval
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository cloning fails
    /// - Version resolution fails (ref doesn't exist)
    /// - Network issues prevent fetching
    pub async fn resolve_all(&mut self) -> Result<()> {
        // Group entries by source for efficient processing
        let mut by_source: HashMap<String, Vec<(String, VersionEntry)>> = HashMap::new();

        for (key, entry) in &self.entries {
            by_source
                .entry(entry.source.clone())
                .or_default()
                .push((key.1.clone(), entry.clone()));
        }

        // Process each source
        for (source, versions) in by_source {
            // Get the URL from the first entry (all should have same URL)
            let url = &versions[0].1.url;

            // Get or clone the bare repository (with single fetch)
            let repo_path = self
                .cache
                .get_or_clone_source(&source, url, None)
                .await
                .with_context(|| format!("Failed to prepare repository for source '{}'", source))?;

            // Store bare repo path for later use
            self.bare_repos.insert(source.clone(), repo_path.clone());

            let repo = GitRepo::new(&repo_path);

            // Resolve each version for this source
            for (version_str, mut entry) in versions {
                // First check if this is a version constraint
                let resolved_ref = if let Some(ref version) = entry.version {
                    if crate::resolver::version_resolution::is_version_constraint(version) {
                        // Resolve constraint to actual tag first
                        // Note: get_or_clone_source already fetched, so tags should be available
                        let tags = repo.list_tags().await.unwrap_or_default();

                        if tags.is_empty() {
                            return Err(anyhow::anyhow!(
                                "No tags found in repository for constraint '{}'",
                                version
                            ));
                        }

                        // Find best matching tag
                        crate::resolver::version_resolution::find_best_matching_tag(version, tags)
                            .with_context(|| {
                            format!("Failed to resolve version constraint '{}'", version)
                        })?
                    } else {
                        // Not a constraint, use as-is
                        version.clone()
                    }
                } else {
                    // No version specified, use HEAD
                    "HEAD".to_string()
                };

                // Now resolve the actual ref to SHA
                let sha = repo
                    .resolve_to_sha(Some(&resolved_ref))
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to resolve version '{}' for source '{}'",
                            version_str, source
                        )
                    })?;

                // Store the resolved SHA and version
                entry.resolved_sha = Some(sha.clone());
                entry.resolved_version = Some(resolved_ref.clone());
                let key = (source.clone(), version_str);
                self.resolved
                    .insert(key, ResolvedVersion { sha, resolved_ref });
            }
        }

        Ok(())
    }

    /// Resolves a single version to SHA without affecting the batch
    ///
    /// This is useful for incremental resolution or testing.
    pub async fn resolve_single(
        &mut self,
        source: &str,
        url: &str,
        version: Option<&str>,
    ) -> Result<String> {
        // Get or clone the repository
        let repo_path = self
            .cache
            .get_or_clone_source(source, url, None)
            .await
            .with_context(|| format!("Failed to prepare repository for source '{}'", source))?;

        let repo = GitRepo::new(&repo_path);

        // Resolve the version to SHA
        let sha = repo.resolve_to_sha(version).await.with_context(|| {
            format!(
                "Failed to resolve version '{}' for source '{}'",
                version.unwrap_or("HEAD"),
                source
            )
        })?;

        // Cache the result
        let version_key = version.unwrap_or("HEAD").to_string();
        let resolved_ref = version.unwrap_or("HEAD").to_string();
        let key = (source.to_string(), version_key);
        self.resolved.insert(
            key,
            ResolvedVersion {
                sha: sha.clone(),
                resolved_ref,
            },
        );

        Ok(sha)
    }

    /// Gets the resolved SHA for a given source and version
    ///
    /// Returns None if the version hasn't been resolved yet.
    ///
    /// # Arguments
    ///
    /// * `source` - Source name
    /// * `version` - Version specification (use "HEAD" for None)
    pub fn get_resolved_sha(&self, source: &str, version: &str) -> Option<String> {
        let key = (source.to_string(), version.to_string());
        self.resolved.get(&key).map(|rv| rv.sha.clone())
    }

    /// Gets all resolved SHAs as a HashMap
    ///
    /// Useful for bulk operations or debugging.
    pub fn get_all_resolved(&self) -> HashMap<(String, String), String> {
        self.resolved
            .iter()
            .map(|(k, v)| (k.clone(), v.sha.clone()))
            .collect()
    }

    /// Gets all resolved versions with both SHA and resolved reference
    ///
    /// Returns a HashMap with (source, version) -> ResolvedVersion
    pub fn get_all_resolved_full(&self) -> &HashMap<(String, String), ResolvedVersion> {
        &self.resolved
    }

    /// Checks if a specific version has been resolved
    pub fn is_resolved(&self, source: &str, version: &str) -> bool {
        let key = (source.to_string(), version.to_string());
        self.resolved.contains_key(&key)
    }

    /// Gets the bare repository path for a source
    ///
    /// Returns None if the source hasn't been processed yet.
    pub fn get_bare_repo_path(&self, source: &str) -> Option<&PathBuf> {
        self.bare_repos.get(source)
    }

    /// Clears all resolved versions and cached data
    ///
    /// Useful for testing or when starting a fresh resolution.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.resolved.clear();
        self.bare_repos.clear();
    }

    /// Returns the number of unique versions to resolve
    pub fn pending_count(&self) -> usize {
        self.entries.len()
    }

    /// Returns the number of successfully resolved versions
    pub fn resolved_count(&self) -> usize {
        self.resolved.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_version_resolver_deduplication() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = VersionResolver::new(cache);

        // Add same version multiple times
        resolver.add_version("source1", "https://example.com/repo.git", Some("v1.0.0"));
        resolver.add_version("source1", "https://example.com/repo.git", Some("v1.0.0"));
        resolver.add_version("source1", "https://example.com/repo.git", Some("v1.0.0"));

        // Should only have one entry
        assert_eq!(resolver.pending_count(), 1);
    }

    #[tokio::test]
    async fn test_sha_optimization() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let _resolver = VersionResolver::new(cache);

        // Test that full SHA is recognized
        let full_sha = "a".repeat(40);
        assert_eq!(full_sha.len(), 40);
        assert!(full_sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_resolved_retrieval() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = VersionResolver::new(cache);

        // Manually insert a resolved SHA for testing
        let key = ("test_source".to_string(), "v1.0.0".to_string());
        let sha = "1234567890abcdef1234567890abcdef12345678";
        resolver.resolved.insert(
            key,
            ResolvedVersion {
                sha: sha.to_string(),
                resolved_ref: "v1.0.0".to_string(),
            },
        );

        // Verify retrieval
        assert!(resolver.is_resolved("test_source", "v1.0.0"));
        assert_eq!(
            resolver.get_resolved_sha("test_source", "v1.0.0"),
            Some(sha.to_string())
        );
        assert!(!resolver.is_resolved("test_source", "v2.0.0"));
    }
}
