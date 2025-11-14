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
use dashmap::DashMap;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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

impl VersionEntry {
    /// Format the version entry for display in progress UI.
    ///
    /// Formats as: `source@version` or `source@HEAD` if no version specified.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use agpm_cli::resolver::version_resolver::VersionEntry;
    /// let entry = VersionEntry {
    ///     source: "community".to_string(),
    ///     url: "https://github.com/example/repo.git".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     resolved_sha: None,
    ///     resolved_version: None,
    /// };
    /// assert_eq!(entry.format_display(), "community@v1.0.0");
    /// ```
    pub fn format_display(&self) -> String {
        let version = self.version.as_deref().unwrap_or("HEAD");
        format!("{}@{}", self.source, version)
    }

    /// Create a unique key for tracking this entry in the progress window.
    ///
    /// Uses source and version to create a unique identifier.
    pub fn unique_key(&self) -> String {
        let version = self.version.as_deref().unwrap_or("HEAD");
        format!("{}:{}", self.source, version)
    }
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
/// resolver.resolve_all(None).await?;
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
    entries: Arc<DashMap<(String, String), VersionEntry>>,
    /// Resolved SHA cache, keyed by (source, version)
    resolved: Arc<DashMap<(String, String), ResolvedVersion>>,
    /// Bare repository paths, keyed by source name
    bare_repos: Arc<DashMap<String, PathBuf>>,
    /// Maximum concurrency for parallel version resolution
    max_concurrency: usize,
}

impl VersionResolver {
    /// Creates a new version resolver with the given cache and default concurrency
    ///
    /// Uses the same default concurrency as installation: max(10, 2 × CPU cores)
    pub fn new(cache: Cache) -> Self {
        let cores = std::thread::available_parallelism().map(std::num::NonZero::get).unwrap_or(4);
        let default_concurrency = std::cmp::max(10, cores * 2);

        Self {
            cache,
            entries: Arc::new(DashMap::new()),
            resolved: Arc::new(DashMap::new()),
            bare_repos: Arc::new(DashMap::new()),
            max_concurrency: default_concurrency,
        }
    }

    /// Creates a new version resolver with explicit concurrency limit
    pub fn with_concurrency(cache: Cache, max_concurrency: usize) -> Self {
        Self {
            cache,
            entries: Arc::new(DashMap::new()),
            resolved: Arc::new(DashMap::new()),
            bare_repos: Arc::new(DashMap::new()),
            max_concurrency,
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
    pub fn add_version(&self, source: &str, url: &str, version: Option<&str>) {
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
    /// This is the second phase of AGPM's two-phase resolution architecture. Call after `pre_sync_sources()`.
    /// See documentation for detailed resolution process and performance characteristics.
    ///
    /// # Prerequisites
    ///
    /// **CRITICAL**: `pre_sync_sources()` must be called first to populate the cache.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use agpm_cli::resolver::version_resolver::VersionResolver;
    /// # use agpm_cli::cache::Cache;
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let mut resolver = VersionResolver::new(cache);
    /// resolver.add_version("source", "https://github.com/org/repo.git", Some("v1.2.3"));
    ///
    /// resolver.pre_sync_sources().await?;
    /// resolver.resolve_all(None).await?;  // Pass None for no progress tracking
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository not pre-synced (call `pre_sync_sources()` first)
    /// - Version/tag/branch not found or constraint unsatisfied
    /// - Git operations fail or repository inaccessible
    pub async fn resolve_all(
        &self,
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<()> {
        // Group entries by source for efficient processing
        let mut by_source: HashMap<String, Vec<(String, VersionEntry)>> = HashMap::new();

        for entry_ref in self.entries.iter() {
            let (key, entry) = entry_ref.pair();
            by_source.entry(entry.source.clone()).or_default().push((key.1.clone(), entry.clone()));
        }

        // Calculate total versions to resolve for progress tracking
        let total_versions: usize = by_source.values().map(|v| v.len()).sum();

        // Note: Phase is started by caller (resolve_with_options), not here.
        // This is because version resolution is part of the larger "Resolving Dependencies"
        // phase which includes transitive resolution and conflict detection.

        // Thread-safe counter for completed versions
        let completed_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

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

            // Pre-fetch tags once per source if any version uses constraints
            // This optimization avoids repeated git tag -l calls for the same repository
            let needs_tags = versions.iter().any(|(_, entry)| {
                !crate::utils::is_local_path(&entry.url)
                    && entry.version.as_ref().is_some_and(|v| is_version_constraint(v))
            });

            let tags_cache = if needs_tags {
                let tags = repo.list_tags().await.unwrap_or_default();
                if tags.is_empty() {
                    return Err(anyhow::anyhow!(
                        "No tags found in repository '{source}' but version constraints require tags"
                    ));
                }
                Some(tags)
            } else {
                None
            };

            // Resolve each version for this source in parallel
            // Use configured concurrency limit to avoid overwhelming git processes
            let concurrency = std::cmp::min(self.max_concurrency, versions.len());

            let resolved_versions = stream::iter(versions)
                .map(|(version_str, entry)| {
                    let repo_path = repo_path.clone();
                    let source = source.clone();
                    let tags_cache = tags_cache.clone();
                    let progress = progress.clone();
                    let completed_counter = completed_counter.clone();
                    let total = total_versions;

                    async move {
                        // Mark this version as active in the progress window
                        if let Some(ref pm) = progress {
                            let display = entry.format_display();
                            let key = entry.unique_key();
                            pm.mark_item_active(&display, &key);
                        }

                        // Create a new GitRepo instance for this parallel task
                        let repo = GitRepo::new(&repo_path);
                        // Check if this is a local directory source (not a Git repository)
                        let is_local = crate::utils::is_local_path(&entry.url);

                        // For local directory sources, we don't resolve versions - just use "local"
                        let resolved_ref = if is_local {
                            "local".to_string()
                        } else if let Some(ref version) = entry.version {
                            // First check if this is a version constraint
                            if is_version_constraint(version) {
                                // Use pre-fetched tags from cache
                                let tags = tags_cache.as_ref().ok_or_else(|| {
                                    anyhow::anyhow!("Tags should have been pre-fetched for constraint '{version}'")
                                })?;

                                // Find best matching tag
                                find_best_matching_tag(version, tags.clone())
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
                            tracing::debug!(
                                "RESOLVE: source='{}' version='{}' resolved_ref='{}' -> resolving to SHA...",
                                source,
                                version_str,
                                resolved_ref
                            );
                            let resolved_sha =
                                repo.resolve_to_sha(Some(&resolved_ref)).await.with_context(|| {
                                    format!(
                                        "Failed to resolve version '{version_str}' for source '{source}'"
                                    )
                                })?;
                            tracing::debug!(
                                "RESOLVE: source='{}' version='{}' resolved_ref='{}' -> SHA={}",
                                source,
                                version_str,
                                resolved_ref,
                                &resolved_sha[..8.min(resolved_sha.len())]
                            );
                            Some(resolved_sha)
                        };

                        // Mark this version as complete in the progress window
                        if let Some(ref pm) = progress {
                            let completed = completed_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                            let display = entry.format_display();
                            let key = entry.unique_key();
                            pm.mark_item_complete(&key, Some(&display), completed, total, "Resolving dependencies");
                        }

                        Ok::<_, anyhow::Error>((version_str, resolved_ref, sha))
                    }
                })
                .buffer_unordered(concurrency)
                .collect::<Vec<_>>()
                .await;

            // Store all resolved versions
            for result in resolved_versions {
                let (version_str, resolved_ref, sha) = result?;
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

        // Note: Progress phase is NOT completed here - it continues through
        // conflict detection and will be completed at the end of resolve_with_options()

        Ok(())
    }

    /// Resolves a single version to SHA without affecting the batch
    ///
    /// This is useful for incremental resolution or testing.
    pub async fn resolve_single(
        &self,
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
        self.resolved.iter().map(|entry| (entry.key().clone(), entry.value().sha.clone())).collect()
    }

    /// Gets all resolved versions with both SHA and resolved reference
    ///
    /// Returns a `HashMap` with (source, version) -> `ResolvedVersion`
    pub fn get_all_resolved_full(&self) -> HashMap<(String, String), ResolvedVersion> {
        self.resolved.iter().map(|entry| (entry.key().clone(), entry.value().clone())).collect()
    }

    /// Checks if a specific version has been resolved
    pub fn is_resolved(&self, source: &str, version: &str) -> bool {
        let key = (source.to_string(), version.to_string());
        self.resolved.contains_key(&key)
    }

    /// Pre-syncs all unique sources to ensure repositories are cloned/fetched.
    ///
    /// This is the first phase of AGPM's two-phase resolution architecture. Performs all
    /// Git network operations upfront before `resolve_all()`. Automatically deduplicates
    /// by source URL for efficiency.
    ///
    /// # Prerequisites
    ///
    /// Call this method after adding versions via `add_version()` calls.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use agpm_cli::resolver::version_resolver::VersionResolver;
    /// use agpm_cli::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let mut resolver = VersionResolver::new(cache);
    /// resolver.add_version("source", "https://github.com/org/repo.git", Some("v1.0.0"));
    ///
    /// // Phase 1: Sync repositories (network operations)
    /// resolver.pre_sync_sources().await?;
    ///
    /// // Phase 2: Resolve versions to SHAs (local operations)
    /// resolver.resolve_all(None).await?;  // Pass None for no progress tracking
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository cloning or fetching fails (network, auth, invalid URL)
    /// - Authentication fails for private repositories
    /// - Insufficient disk space or repository corruption
    pub async fn pre_sync_sources(&self) -> Result<()> {
        // Group entries by source to get unique sources
        let mut unique_sources: HashMap<String, String> = HashMap::new();

        for entry_ref in self.entries.iter() {
            let entry = entry_ref.value();
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
    pub fn get_bare_repo_path(&self, source: &str) -> Option<PathBuf> {
        self.bare_repos.get(source).map(|entry| entry.value().clone())
    }

    /// Registers a bare repository path for a source
    ///
    /// This is used when manually ensuring a repository exists without clearing all state.
    pub fn register_bare_repo(&self, source: String, repo_path: PathBuf) {
        self.bare_repos.insert(source, repo_path);
    }

    /// Clears all resolved versions and cached data
    ///
    /// Useful for testing or when starting a fresh resolution.
    pub fn clear(&self) {
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
    /// ```no_run
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
    /// Uses DashMap for concurrent access during parallel dependency resolution
    prepared_versions: std::sync::Arc<dashmap::DashMap<String, PreparedSourceVersion>>,
}

impl VersionResolutionService {
    /// Create a new version resolution service with default concurrency.
    pub fn new(cache: crate::cache::Cache) -> Self {
        Self {
            version_resolver: VersionResolver::new(cache),
            prepared_versions: std::sync::Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Create a new version resolution service with explicit concurrency limit.
    pub fn with_concurrency(cache: crate::cache::Cache, max_concurrency: usize) -> Self {
        Self {
            version_resolver: VersionResolver::with_concurrency(cache, max_concurrency),
            prepared_versions: std::sync::Arc::new(dashmap::DashMap::new()),
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
    /// * `progress` - Optional progress tracker for UI updates
    pub async fn pre_sync_sources(
        &self,
        core: &ResolutionCore,
        deps: &[(String, ResourceDependency)],
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
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
        self.version_resolver.resolve_all(progress).await?;

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
                            resource_variants: dashmap::DashMap::new(),
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
        // DashMap doesn't support extend with Arc, so iterate and insert
        for (key, value) in prepared {
            self.prepared_versions.insert(key, value);
        }

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
    pub fn get_prepared_version(
        &self,
        group_key: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, PreparedSourceVersion>> {
        self.prepared_versions.get(group_key)
    }

    /// Get the prepared versions map.
    ///
    /// Returns a reference to the DashMap of prepared source versions.
    pub fn prepared_versions(
        &self,
    ) -> &std::sync::Arc<dashmap::DashMap<String, PreparedSourceVersion>> {
        &self.prepared_versions
    }

    /// Get a clone of the prepared versions map Arc.
    ///
    /// Returns a cloned Arc to the DashMap of prepared source versions.
    /// Used for concurrent access during parallel resolution.
    pub fn prepared_versions_arc(
        &self,
    ) -> std::sync::Arc<dashmap::DashMap<String, PreparedSourceVersion>> {
        std::sync::Arc::clone(&self.prepared_versions)
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
        &self,
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
                    resource_variants: dashmap::DashMap::new(),
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
        // Note: No progress tracking for single version resolution
        self.version_resolver.resolve_all(None).await?;

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
                resource_variants: dashmap::DashMap::new(),
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
    pub fn get_bare_repo_path(&self, source: &str) -> Option<PathBuf> {
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

/// Sorts tag-version pairs by semantic version (descending), with deterministic tie-breaking.
///
/// When versions compare equal, uses tag name (lexicographic order) as a tie-breaker.
/// This ensures consistent ordering across runs, which is critical for reproducible
/// dependency resolution.
///
/// # Arguments
///
/// * `pairs` - Mutable reference to vector of (tag_name, semver::Version) pairs
///
/// # Examples
///
/// ```no_run
/// use semver::Version;
///
/// let mut versions = vec![
///     ("a-v1.0.0".to_string(), Version::new(1, 0, 0)),
///     ("z-v1.0.0".to_string(), Version::new(1, 0, 0)),  // Same version
///     ("b-v2.0.0".to_string(), Version::new(2, 0, 0)),
/// ];
/// agpm_cli::resolver::version_resolver::sort_versions_deterministic(&mut versions);
/// // After sorting: b-v2.0.0 (highest), then a-v1.0.0, z-v1.0.0 (alphabetical)
/// ```
pub fn sort_versions_deterministic(pairs: &mut [(String, Version)]) {
    pairs.sort_by(|a, b| match b.1.cmp(&a.1) {
        std::cmp::Ordering::Equal => a.0.cmp(&b.0), // Tag name tie-breaker
        other => other,
    });
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

    // Sort deterministically: highest version first, tag name for ties
    sort_versions_deterministic(&mut versions);

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
#[derive(Clone, Debug)]
pub struct PreparedSourceVersion {
    /// Path to the worktree for this version
    pub worktree_path: std::path::PathBuf,
    /// The resolved version reference (tag, branch, etc.)
    pub resolved_version: Option<String>,
    /// The commit SHA for this version
    pub resolved_commit: String,
    /// Template variables for each resource in this version.
    /// Maps resource_id (format: "source:path") to variant_inputs (template variables).
    /// Used during backtracking to preserve template variables when changing versions.
    /// Uses DashMap for concurrent access during parallel dependency resolution.
    pub resource_variants: dashmap::DashMap<String, Option<serde_json::Value>>,
}

impl Default for PreparedSourceVersion {
    fn default() -> Self {
        Self {
            worktree_path: std::path::PathBuf::new(),
            resolved_version: None,
            resolved_commit: String::new(),
            resource_variants: dashmap::DashMap::new(),
        }
    }
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
                        resource_variants: dashmap::DashMap::new(),
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
        let resolver = VersionResolver::new(cache);

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
        let resolver = VersionResolver::new(cache);

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
