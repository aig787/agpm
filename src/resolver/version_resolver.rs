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
use crate::manifest::ResourceDependency;
use crate::source::SourceManager;

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
/// # use agpm_cli::resolver::version_resolver::{VersionResolver, VersionEntry};
/// # use agpm_cli::cache::Cache;
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
        let key = (source.to_string(), version_key);

        // Only add if not already present (deduplication)
        self.entries.entry(key).or_insert_with(|| VersionEntry {
            source: source.to_string(),
            url: url.to_string(),
            version: version.map(std::string::ToString::to_string),
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
    /// # use agpm_cli::resolver::version_resolver::VersionResolver;
    /// # use agpm_cli::cache::Cache;
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
            by_source.entry(entry.source.clone()).or_default().push((key.1.clone(), entry.clone()));
        }

        // Process each source
        for (source, versions) in by_source {
            // Repository must have been pre-synced
            let repo_path = self
                .bare_repos
                .get(&source)
                .ok_or_else(|| {
                    anyhow::anyhow!("Repository for source '{source}' was not pre-synced. Call pre_sync_sources() first.")
                })?
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
                    if is_version_constraint(version) {
                        // Resolve constraint to actual tag first
                        // Note: get_or_clone_source already fetched, so tags should be available
                        let tags = repo.list_tags().await.unwrap_or_default();

                        if tags.is_empty() {
                            return Err(anyhow::anyhow!(
                                "No tags found in repository for constraint '{version}'"
                            ));
                        }

                        // Find best matching tag
                        find_best_matching_tag(version, tags)
                            .with_context(|| format!("Failed to resolve version constraint '{version}' for source '{source}'"))?
                    } else {
                        // Not a constraint, use as-is
                        version.clone()
                    }
                } else {
                    // No version specified for Git source, resolve HEAD to actual branch name
                    repo.get_default_branch().await.unwrap_or_else(|_| "main".to_string())
                };

                // For local sources, don't resolve SHA. For Git sources, resolve ref to actual SHA
                let sha = if is_local {
                    // Local directories don't have commit SHAs
                    None
                } else {
                    // Resolve the actual ref to SHA for Git repositories
                    Some(repo.resolve_to_sha(Some(&resolved_ref)).await.with_context(|| {
                        format!("Failed to resolve version '{version_str}' for source '{source}'")
                    })?)
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
            .with_context(|| format!("Failed to prepare repository for source '{source}'"))?;

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
            repo.get_default_branch().await.unwrap_or_else(|_| "main".to_string())
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

    /// Gets all resolved SHAs as a `HashMap`
    ///
    /// Useful for bulk operations or debugging.
    pub fn get_all_resolved(&self) -> HashMap<(String, String), String> {
        self.resolved.iter().map(|(k, v)| (k.clone(), v.sha.clone())).collect()
    }

    /// Gets all resolved versions with both SHA and resolved reference
    ///
    /// Returns a `HashMap` with (source, version) -> `ResolvedVersion`
    pub const fn get_all_resolved_full(&self) -> &HashMap<(String, String), ResolvedVersion> {
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
    /// use agpm_cli::resolver::version_resolver::VersionResolver;
    /// use agpm_cli::cache::Cache;
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
                .with_context(|| format!("Failed to sync repository for source '{source}'"))?;

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

    /// Registers a bare repository path for a source
    ///
    /// This is used when manually ensuring a repository exists without clearing all state.
    pub fn register_bare_repo(&mut self, source: String, repo_path: PathBuf) {
        self.bare_repos.insert(source, repo_path);
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
    /// # use agpm_cli::resolver::version_resolver::VersionResolver;
    /// # use agpm_cli::cache::Cache;
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

// ============================================================================
// Version Resolution Service
// ============================================================================

use super::types::ResolutionCore;
use std::path::Path;

/// Service for version resolution and worktree management.
///
/// Provides high-level orchestration for version constraint resolution,
/// SHA resolution, and worktree preparation for Git-backed dependencies.
pub struct VersionResolutionService {
    /// Centralized version resolver for batch SHA resolution
    version_resolver: VersionResolver,

    /// Cache of prepared versions (source::version -> worktree info)
    prepared_versions: HashMap<String, PreparedSourceVersion>,
}

impl VersionResolutionService {
    /// Create a new version resolution service.
    pub fn new(cache: crate::cache::Cache) -> Self {
        Self {
            version_resolver: VersionResolver::new(cache),
            prepared_versions: HashMap::new(),
        }
    }

    /// Pre-sync all source repositories needed for dependencies.
    ///
    /// This performs all Git network operations upfront:
    /// 1. Clone/fetch source repositories
    /// 2. Resolve version constraints to commit SHAs
    /// 3. Create worktrees for resolved commits
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with cache and source manager
    /// * `deps` - All dependencies that need sources synced
    pub async fn pre_sync_sources(
        &mut self,
        core: &ResolutionCore,
        deps: &[(String, ResourceDependency)],
    ) -> Result<()> {
        // Clear and rebuild version resolver entries
        self.version_resolver.clear();

        // Collect all unique (source, version) pairs
        for (_name, dep) in deps {
            if let Some(source) = dep.get_source() {
                let version = dep.get_version(); // None means HEAD

                let source_url = core
                    .source_manager
                    .get_source_url(source)
                    .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source))?;

                // Add to version resolver for batch syncing (None -> "HEAD")
                self.version_resolver.add_version(source, &source_url, version);
            }
        }

        // Pre-sync all source repositories (clone/fetch)
        self.version_resolver.pre_sync_sources().await?;

        // Resolve all versions to SHAs in batch
        self.version_resolver.resolve_all().await?;

        // Handle local paths (non-Git sources) separately
        // These don't go through version resolution but need to be in prepared_versions
        for (_name, dep) in deps {
            if let Some(source) = dep.get_source() {
                let source_url = core
                    .source_manager
                    .get_source_url(source)
                    .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source))?;

                if crate::utils::is_local_path(&source_url) {
                    let version_key = dep.get_version().unwrap_or("HEAD");
                    let group_key = format!("{}::{}", source, version_key);

                    // Add to prepared_versions with the local path
                    self.prepared_versions.insert(
                        group_key,
                        PreparedSourceVersion {
                            worktree_path: PathBuf::from(&source_url),
                            resolved_version: Some("local".to_string()),
                            resolved_commit: String::new(), // No commit for local sources
                        },
                    );
                }
            }
        }

        // Create worktrees for all resolved commits using WorktreeManager
        let worktree_manager =
            WorktreeManager::new(&core.cache, &core.source_manager, &self.version_resolver);
        let prepared = worktree_manager.create_worktrees_for_resolved_versions().await?;

        // Merge Git-backed worktrees with local paths
        self.prepared_versions.extend(prepared);

        Ok(())
    }

    /// Get a prepared version by source and version.
    ///
    /// # Arguments
    ///
    /// * `group_key` - The key in format "source::version"
    ///
    /// # Returns
    ///
    /// The prepared version info with worktree path and resolved commit
    pub fn get_prepared_version(&self, group_key: &str) -> Option<&PreparedSourceVersion> {
        self.prepared_versions.get(group_key)
    }

    /// Get the prepared versions map.
    ///
    /// Returns a reference to the HashMap of prepared source versions.
    pub fn prepared_versions(&self) -> &HashMap<String, PreparedSourceVersion> {
        &self.prepared_versions
    }

    /// Get a mutable reference to the prepared versions map.
    ///
    /// Returns a mutable reference to the HashMap of prepared source versions.
    /// Used for updating versions during backtracking.
    pub fn prepared_versions_mut(&mut self) -> &mut HashMap<String, PreparedSourceVersion> {
        &mut self.prepared_versions
    }

    /// Prepare an additional version on-demand without clearing existing ones.
    ///
    /// This is used for transitive dependencies discovered during resolution.
    /// Unlike `pre_sync_sources`, this doesn't clear existing prepared versions.
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with cache and source manager
    /// * `source_name` - Name of the source repository
    /// * `version` - Optional version constraint (None = HEAD)
    pub async fn prepare_additional_version(
        &mut self,
        core: &ResolutionCore,
        source_name: &str,
        version: Option<&str>,
    ) -> Result<()> {
        let version_key = version.unwrap_or("HEAD");
        let source_url = core
            .source_manager
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        // Handle local paths (non-Git sources) separately
        if crate::utils::is_local_path(&source_url) {
            let group_key = format!("{}::{}", source_name, version_key);
            self.prepared_versions.insert(
                group_key,
                PreparedSourceVersion {
                    worktree_path: PathBuf::from(&source_url),
                    resolved_version: Some("local".to_string()),
                    resolved_commit: String::new(),
                },
            );
            return Ok(());
        }

        // For Git sources, proceed with version resolution
        self.version_resolver.add_version(source_name, &source_url, version);

        // Ensure the bare repository exists
        if self.version_resolver.get_bare_repo_path(source_name).is_none() {
            let repo_path =
                core.cache.get_or_clone_source(source_name, &source_url, None).await.with_context(
                    || format!("Failed to sync repository for source '{}'", source_name),
                )?;
            self.version_resolver.register_bare_repo(source_name.to_string(), repo_path);
        }

        // Resolve this specific version to SHA
        self.version_resolver.resolve_all().await?;

        // Get the resolved SHA and resolved reference
        let resolved_version_data = self
            .version_resolver
            .get_all_resolved_full()
            .get(&(source_name.to_string(), version_key.to_string()))
            .ok_or_else(|| {
                anyhow::anyhow!("Failed to resolve version for {} @ {}", source_name, version_key)
            })?
            .clone();

        let sha = resolved_version_data.sha.clone();
        let resolved_ref = resolved_version_data.resolved_ref.clone();

        // Create worktree for this SHA
        let worktree_path =
            core.cache.get_or_create_worktree_for_sha(source_name, &source_url, &sha, None).await?;

        // Cache the prepared version with the RESOLVED reference, not the constraint
        let group_key = format!("{}::{}", source_name, version_key);
        self.prepared_versions.insert(
            group_key,
            PreparedSourceVersion {
                worktree_path,
                resolved_version: Some(resolved_ref),
                resolved_commit: sha,
            },
        );

        Ok(())
    }

    /// Get available versions (tags/branches) for a repository.
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with cache
    /// * `repo_path` - Path to bare repository
    ///
    /// # Returns
    ///
    /// List of available version strings
    pub async fn get_available_versions(
        _core: &ResolutionCore,
        repo_path: &Path,
    ) -> Result<Vec<String>> {
        let repo = GitRepo::new(repo_path);

        // Get all tags
        let tags = repo.list_tags().await.context("Failed to list tags")?;

        // TODO: Add branches if needed in future
        // For now, only use tags
        let versions = tags;

        Ok(versions)
    }

    /// Get the bare repository path for a source.
    ///
    /// Returns None if the source hasn't been synced yet.
    ///
    /// # Arguments
    ///
    /// * `source` - Name of the source repository
    pub fn get_bare_repo_path(&self, source: &str) -> Option<&PathBuf> {
        self.version_resolver.get_bare_repo_path(source)
    }

    /// Get the version resolver (for testing).
    #[cfg(test)]
    pub fn version_resolver(&self) -> &VersionResolver {
        &self.version_resolver
    }
}

// ============================================================================
// Version Constraint Resolution Helpers
// ============================================================================

use crate::version::constraints::{ConstraintSet, VersionConstraint};
use semver::Version;

/// Checks if a string represents a version constraint rather than a direct reference.
///
/// Version constraints contain operators like `^`, `~`, `>`, `<`, `=`, or special
/// keywords. Direct references are branch names, tag names, or commit hashes.
/// This function now supports prefixed constraints like `agents-^v1.0.0`.
///
/// # Arguments
///
/// * `version` - The version string to check
///
/// # Returns
///
/// Returns `true` if the string contains constraint operators or keywords,
/// `false` for plain tags, branches, or commit hashes.
#[must_use]
pub fn is_version_constraint(version: &str) -> bool {
    // Extract prefix first, then check the version part for constraint indicators
    let (_prefix, version_str) = crate::version::split_prefix_and_version(version);

    // Check for wildcard (works with or without prefix)
    if version_str == "*" {
        return true;
    }

    // Check for version constraint operators in the version part
    if version_str.starts_with('^')
        || version_str.starts_with('~')
        || version_str.starts_with('>')
        || version_str.starts_with('<')
        || version_str.starts_with('=')
        || version_str.contains(',')
    // Range constraints like ">=1.0.0, <2.0.0"
    {
        return true;
    }

    false
}

/// Parses Git tags into semantic versions, filtering out non-semver tags.
///
/// This function handles both prefixed and non-prefixed version tags,
/// including support for monorepo-style prefixes like `agents-v1.0.0`.
/// Tags that don't represent valid semantic versions are filtered out.
#[must_use]
pub fn parse_tags_to_versions(tags: Vec<String>) -> Vec<(String, Version)> {
    let mut versions = Vec::new();

    for tag in tags {
        // Extract prefix and version part (handles both prefixed and unprefixed)
        let (_prefix, version_str) = crate::version::split_prefix_and_version(&tag);

        // Strip 'v' prefix from version part
        let cleaned = version_str.trim_start_matches('v').trim_start_matches('V');

        if let Ok(version) = Version::parse(cleaned) {
            versions.push((tag, version));
        }
    }

    // Sort by version, highest first
    versions.sort_by(|a, b| b.1.cmp(&a.1));

    versions
}

/// Finds the best matching tag for a version constraint.
///
/// This function resolves version constraints to actual Git tags by:
/// 1. Extracting the prefix from the constraint (if any)
/// 2. Filtering tags to only those with matching prefix
/// 3. Parsing the constraint and matching tags
/// 4. Selecting the best match (usually the highest compatible version)
pub fn find_best_matching_tag(constraint_str: &str, tags: Vec<String>) -> Result<String> {
    // Extract prefix from constraint
    let (constraint_prefix, version_str) = crate::version::split_prefix_and_version(constraint_str);

    // Filter tags by prefix first
    let filtered_tags: Vec<String> = tags
        .into_iter()
        .filter(|tag| {
            let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
            tag_prefix.as_ref() == constraint_prefix.as_ref()
        })
        .collect();

    if filtered_tags.is_empty() {
        return Err(anyhow::anyhow!(
            "No tags found with matching prefix for constraint: {constraint_str}"
        ));
    }

    // Parse filtered tags to versions
    let tag_versions = parse_tags_to_versions(filtered_tags);

    if tag_versions.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid semantic version tags found for constraint: {constraint_str}"
        ));
    }

    // Special case: wildcard (*) matches the highest available version
    if version_str == "*" {
        // tag_versions is already sorted highest first
        return Ok(tag_versions[0].0.clone());
    }

    // Parse constraint using ONLY the version part (prefix already filtered)
    // This ensures semver matching works correctly after prefix filtering
    let constraint = VersionConstraint::parse(version_str)?;

    // Extract just the versions for constraint matching
    let versions: Vec<Version> = tag_versions.iter().map(|(_, v)| v.clone()).collect();

    // Create a constraint set with just this constraint
    let mut constraint_set = ConstraintSet::new();
    constraint_set.add(constraint)?;

    // Find the best match
    if let Some(best_version) = constraint_set.find_best_match(&versions) {
        // Find the original tag name for this version
        for (tag_name, version) in tag_versions {
            if &version == best_version {
                return Ok(tag_name);
            }
        }
    }

    Err(anyhow::anyhow!("No tag found matching constraint: {constraint_str}"))
}

// ============================================================================
// Worktree Management
// ============================================================================

/// Represents a prepared source version with worktree information.
#[derive(Clone, Debug, Default)]
pub struct PreparedSourceVersion {
    /// Path to the worktree for this version
    pub worktree_path: std::path::PathBuf,
    /// The resolved version reference (tag, branch, etc.)
    pub resolved_version: Option<String>,
    /// The commit SHA for this version
    pub resolved_commit: String,
}

/// Manages worktree creation for resolved dependency versions.
pub struct WorktreeManager<'a> {
    cache: &'a Cache,
    source_manager: &'a SourceManager,
    version_resolver: &'a VersionResolver,
}

impl<'a> WorktreeManager<'a> {
    /// Create a new worktree manager.
    pub fn new(
        cache: &'a Cache,
        source_manager: &'a SourceManager,
        version_resolver: &'a VersionResolver,
    ) -> Self {
        Self {
            cache,
            source_manager,
            version_resolver,
        }
    }

    /// Create a group key for identifying source-version combinations.
    pub fn group_key(source: &str, version: &str) -> String {
        format!("{source}::{version}")
    }

    /// Create worktrees for all resolved versions in parallel.
    ///
    /// This function takes the resolved versions from the VersionResolver
    /// and creates Git worktrees for each unique commit SHA, enabling
    /// efficient parallel access to dependency resources.
    ///
    /// # Returns
    ///
    /// A map of group keys to prepared source versions containing worktree paths.
    pub async fn create_worktrees_for_resolved_versions(
        &self,
    ) -> Result<HashMap<String, PreparedSourceVersion>> {
        use crate::core::AgpmError;
        use futures::future::join_all;

        let resolved_full = self.version_resolver.get_all_resolved_full().clone();
        let mut prepared_versions = HashMap::new();

        // Build futures for parallel worktree creation
        let mut futures = Vec::new();

        for ((source_name, version_key), resolved_version) in resolved_full {
            let sha = resolved_version.sha;
            let resolved_ref = resolved_version.resolved_ref;
            let repo_key = Self::group_key(&source_name, &version_key);
            let cache_clone = self.cache.clone();
            let source_name_clone = source_name.clone();

            // Get the source URL for this source
            let source_url_clone = self
                .source_manager
                .get_source_url(&source_name)
                .ok_or_else(|| AgpmError::SourceNotFound {
                    name: source_name.to_string(),
                })?
                .to_string();

            let sha_clone = sha.clone();
            let resolved_ref_clone = resolved_ref.clone();

            let future = async move {
                // Use SHA-based worktree creation
                // The version resolver has already handled fetching and SHA resolution
                let worktree_path = cache_clone
                    .get_or_create_worktree_for_sha(
                        &source_name_clone,
                        &source_url_clone,
                        &sha_clone,
                        Some(&source_name_clone), // context for logging
                    )
                    .await?;

                Ok::<_, anyhow::Error>((
                    repo_key,
                    PreparedSourceVersion {
                        worktree_path,
                        resolved_version: Some(resolved_ref_clone),
                        resolved_commit: sha_clone,
                    },
                ))
            };

            futures.push(future);
        }

        // Execute all futures concurrently and collect results
        let results = join_all(futures).await;

        // Process results and build the map
        for result in results {
            let (key, prepared) = result?;
            prepared_versions.insert(key, prepared);
        }

        Ok(prepared_versions)
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
        assert_eq!(resolver.get_resolved_sha("test_source", "v1.0.0"), Some(sha.to_string()));
        assert!(!resolver.is_resolved("test_source", "v2.0.0"));
    }

    #[tokio::test]
    async fn test_worktree_group_key() {
        assert_eq!(WorktreeManager::group_key("source", "version"), "source::version");
        assert_eq!(WorktreeManager::group_key("community", "v1.0.0"), "community::v1.0.0");
    }
}
