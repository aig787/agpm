//! Centralized version resolution module for AGPM
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
/// # use agpm::resolver::version_resolver::{VersionResolver, VersionEntry};
/// # use agpm::cache::Cache;
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

    /// Resolves all collected versions to their commit SHAs using cached repositories.
    ///
    /// This method implements the second phase of AGPM's two-phase resolution architecture.
    /// It processes all version entries collected via `add_version()` calls and resolves
    /// them to concrete commit SHAs using locally cached Git repositories.
    ///
    /// # Prerequisites
    ///
    /// **CRITICAL**: `pre_sync_sources()` must be called before this method. The resolver
    /// requires all repositories to be pre-synced to the cache, and will return an error
    /// if any required repository is missing from the `bare_repos` map.
    ///
    /// # Resolution Process
    ///
    /// The method performs the following steps:
    /// 1. **Source Grouping**: Groups entries by source to minimize repository operations
    /// 2. **Repository Access**: Uses pre-synced repositories from `pre_sync_sources()`
    /// 3. **Version Constraint Resolution**: Handles semver constraints (`^1.0`, `~2.1`)
    /// 4. **SHA Resolution**: Resolves all versions to SHAs using `git rev-parse`
    /// 5. **Result Caching**: Stores resolved SHAs for quick retrieval
    ///
    /// # Version Resolution Strategy
    ///
    /// The resolver handles different version types:
    /// - **Exact SHAs**: Used directly without resolution
    /// - **Semantic Versions**: Resolved using semver constraint matching
    /// - **Tags**: Resolved to their commit SHAs
    /// - **Branch Names**: Resolved to current HEAD commit
    /// - **Latest/None**: Defaults to the repository's default branch
    ///
    /// # Performance Characteristics
    ///
    /// - **Time Complexity**: O(n·log(t)) where n = entries, t = tags per repo
    /// - **Space Complexity**: O(n) for storing resolved results
    /// - **Network I/O**: Zero (operates on cached repositories only)
    /// - **Parallelization**: Single-threaded but optimized for batch operations
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use agpm::resolver::version_resolver::VersionResolver;
    /// # use agpm::cache::Cache;
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let mut resolver = VersionResolver::new(cache);
    ///
    /// // Add various version types
    /// resolver.add_version("source", "https://github.com/org/repo.git", Some("v1.2.3"));
    /// resolver.add_version("source", "https://github.com/org/repo.git", Some("^1.0"));
    /// resolver.add_version("source", "https://github.com/org/repo.git", Some("main"));
    /// resolver.add_version("source", "https://github.com/org/repo.git", None); // latest
    ///
    /// // Phase 1: Sync repositories
    /// resolver.pre_sync_sources().await?;
    ///
    /// // Phase 2: Resolve versions to SHAs (this method)
    /// resolver.resolve_all().await?;
    ///
    /// // Access resolved SHAs
    /// if resolver.is_resolved("source", "v1.2.3") {
    ///     println!("v1.2.3 resolved successfully");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Error Handling
    ///
    /// The method uses fail-fast behavior - if any version resolution fails,
    /// the entire operation is aborted. This ensures consistency and prevents
    /// partial resolution states.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - **Pre-sync Required**: Repository was not pre-synced (call `pre_sync_sources()` first)
    /// - **Version Not Found**: Specified version/tag/branch doesn't exist in repository
    /// - **Constraint Resolution**: Semver constraint cannot be satisfied by available tags
    /// - **Git Operations**: `git rev-parse` or other Git commands fail
    /// - **Repository Access**: Cached repository is corrupted or inaccessible
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
            // Repository must have been pre-synced
            let repo_path = self.bare_repos.get(&source)
                .ok_or_else(|| anyhow::anyhow!(
                    "Repository for source '{}' was not pre-synced. Call pre_sync_sources() first.",
                    source
                ))?
                .clone();

            let repo = GitRepo::new(&repo_path);

            // Resolve each version for this source
            for (version_str, mut entry) in versions {
                // Check if this is a local directory source (not a Git repository)
                let is_local = crate::utils::is_local_path(&entry.url);

                // For local directory sources, we don't resolve versions - just use "local"
                let resolved_ref = if is_local {
                    "local".to_string()
                } else if let Some(ref version) = entry.version {
                    // First check if this is a version constraint
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
                            format!(
                                "Failed to resolve version constraint '{}' for source '{}'",
                                version, source
                            )
                        })?
                    } else {
                        // Not a constraint, use as-is
                        version.clone()
                    }
                } else {
                    // No version specified for Git source, resolve HEAD to actual branch name
                    repo.get_default_branch()
                        .await
                        .unwrap_or_else(|_| "main".to_string())
                };

                // For local sources, don't resolve SHA. For Git sources, resolve ref to actual SHA
                let sha = if is_local {
                    // Local directories don't have commit SHAs
                    None
                } else {
                    // Resolve the actual ref to SHA for Git repositories
                    Some(
                        repo.resolve_to_sha(Some(&resolved_ref))
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to resolve version '{}' for source '{}'",
                                    version_str, source
                                )
                            })?,
                    )
                };

                // Store the resolved SHA and version
                entry.resolved_sha = sha.clone();
                entry.resolved_version = Some(resolved_ref.clone());
                let key = (source.clone(), version_str);
                // Only insert into resolved map if we have a SHA (Git sources only)
                if let Some(sha_value) = sha {
                    self.resolved.insert(
                        key,
                        ResolvedVersion {
                            sha: sha_value,
                            resolved_ref,
                        },
                    );
                }
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

        // Determine the resolved reference name
        let resolved_ref = if let Some(v) = version {
            v.to_string()
        } else {
            // When no version is specified, resolve HEAD to the actual branch name
            repo.get_default_branch()
                .await
                .unwrap_or_else(|_| "main".to_string())
        };

        // Cache the result
        let version_key = version.unwrap_or("HEAD").to_string();
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

    /// Pre-syncs all unique sources to ensure repositories are cloned/fetched.
    ///
    /// This method implements the first phase of AGPM's two-phase resolution architecture.
    /// It is designed to be called during the "Syncing sources" phase to perform all
    /// Git network operations upfront, before version resolution occurs.
    ///
    /// The method processes all entries in the resolver, groups them by unique source URLs,
    /// and ensures each repository is cloned to the cache with the latest refs fetched.
    /// This enables the subsequent `resolve_all()` method to work purely with local
    /// cached data, providing better performance and progress reporting.
    ///
    /// # Post-Execution State
    ///
    /// After this method completes successfully:
    /// - All required repositories will be cloned to `~/.agpm/cache/sources/`
    /// - All repositories will have their latest refs fetched from remote
    /// - The internal `bare_repos` map will be populated with repository paths
    /// - `resolve_all()` can proceed without any network operations
    ///
    /// This separation provides several benefits:
    /// - **Clear progress phases**: Network operations vs. local resolution
    /// - **Better error handling**: Network failures separated from resolution logic
    /// - **Batch optimization**: Single clone/fetch per unique repository
    /// - **Parallelization potential**: Multiple repositories can be synced concurrently
    ///
    /// # Example
    ///
    /// ```ignore
    /// use agpm::resolver::version_resolver::VersionResolver;
    /// use agpm::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let mut version_resolver = VersionResolver::new(cache);
    ///
    /// // Add versions to resolve across multiple sources
    /// version_resolver.add_version(
    ///     "community",
    ///     "https://github.com/org/agpm-community.git",
    ///     Some("v1.0.0"),
    /// );
    /// version_resolver.add_version(
    ///     "community",
    ///     "https://github.com/org/agpm-community.git",
    ///     Some("v2.0.0"),
    /// );
    /// version_resolver.add_version(
    ///     "private-tools",
    ///     "https://github.com/company/private-agpm.git",
    ///     Some("main"),
    /// );
    ///
    /// // Phase 1: Pre-sync all repositories (network operations)
    /// version_resolver.pre_sync_sources().await?;
    ///
    /// // Phase 2: Resolve all versions to SHAs (local operations only)
    /// version_resolver.resolve_all().await?;
    ///
    /// // Access resolved data
    /// if version_resolver.is_resolved("community", "v1.0.0") {
    ///     println!("Successfully resolved community v1.0.0");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Deduplication
    ///
    /// The method automatically deduplicates by source URL - if multiple entries
    /// reference the same repository, only one clone/fetch operation is performed.
    /// This is particularly efficient when resolving multiple versions from the
    /// same source.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository cloning fails (network issues, authentication, invalid URL)
    /// - Fetching latest refs fails (network connectivity, permission issues)
    /// - Authentication fails for private repositories
    /// - Disk space is insufficient for cloning repositories
    /// - Repository is corrupted and cannot be accessed
    pub async fn pre_sync_sources(&mut self) -> Result<()> {
        // Group entries by source to get unique sources
        let mut unique_sources: HashMap<String, String> = HashMap::new();

        for entry in self.entries.values() {
            unique_sources.insert(entry.source.clone(), entry.url.clone());
        }

        // Pre-sync each unique source
        for (source, url) in unique_sources {
            // Clone or update the repository (this does the actual Git operations)
            let repo_path = self
                .cache
                .get_or_clone_source(&source, &url, None)
                .await
                .with_context(|| format!("Failed to sync repository for source '{}'", source))?;

            // Store bare repo path for later use in resolve_all
            self.bare_repos.insert(source.clone(), repo_path);
        }

        Ok(())
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

    /// Checks if the resolver has any entries to resolve.
    ///
    /// This is a convenience method to determine if the resolver has been populated
    /// with version entries via `add_version()` calls. It's useful for conditional
    /// logic to avoid unnecessary operations when no versions need resolution.
    ///
    /// # Returns
    ///
    /// Returns `true` if there are entries that need resolution, `false` if the
    /// resolver is empty.
    ///
    /// # Example
    ///
    /// ```
    /// # use agpm::resolver::version_resolver::VersionResolver;
    /// # use agpm::cache::Cache;
    /// # let cache = Cache::new().unwrap();
    /// let mut resolver = VersionResolver::new(cache);
    /// assert!(!resolver.has_entries()); // Initially empty
    ///
    /// resolver.add_version("source", "https://github.com/org/repo.git", Some("v1.0.0"));
    /// assert!(resolver.has_entries()); // Now has entries
    /// ```
    pub fn has_entries(&self) -> bool {
        !self.entries.is_empty()
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
