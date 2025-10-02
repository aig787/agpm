//! Dependency resolution and conflict detection for CCPM.
//!
//! This module implements the core dependency resolution algorithm that transforms
//! manifest dependencies into locked versions. It handles version constraint solving,
//! conflict detection, redundancy analysis, parallel source synchronization, and
//! relative path preservation during installation.
//!
//! # Architecture Overview
//!
//! The resolver operates using a **two-phase architecture** optimized for SHA-based worktree caching:
//!
//! ## Phase 1: Source Synchronization (`pre_sync_sources`)
//! - **Purpose**: Perform all Git network operations upfront during "Syncing sources" phase
//! - **Operations**: Clone/fetch repositories, update refs, populate cache
//! - **Benefits**: Clear progress reporting, batch network operations, error isolation
//! - **Result**: All required repositories cached locally for phase 2
//!
//! ## Phase 2: Version Resolution (`resolve` or `update`)
//! - **Purpose**: Resolve versions to commit SHAs using cached repositories
//! - **Operations**: Parse dependencies, resolve constraints, detect conflicts, create worktrees, preserve paths
//! - **Benefits**: Fast local operations, no network I/O, deterministic behavior
//! - **Result**: Locked dependencies ready for installation with preserved directory structure
//!
//! This two-phase approach replaces the previous three-phase model and provides better
//! separation of concerns between network operations and dependency resolution logic.
//!
//! ## Algorithm Complexity
//!
//! - **Time**: O(n + s·log(t)) where:
//!   - n = number of dependencies
//!   - s = number of unique sources
//!   - t = average number of tags/branches per source
//! - **Space**: O(n + s) for dependency graph and source cache
//!
//! ## Parallel Processing
//!
//! The resolver leverages async/await for concurrent operations:
//! - Sources are synchronized in parallel using [`tokio::spawn`]
//! - Git operations are batched to minimize network roundtrips
//! - Progress reporting provides real-time feedback on long-running operations
//!
//! # Resolution Process
//!
//! The two-phase dependency resolution follows these steps:
//!
//! ## Phase 1: Source Synchronization
//! 1. **Dependency Collection**: Extract all dependencies from manifest
//! 2. **Source Validation**: Verify all referenced sources exist and are accessible
//! 3. **Repository Preparation**: Use [`version_resolver::VersionResolver`] to collect unique sources
//! 4. **Source Synchronization**: Clone/update source repositories with single fetch per repository
//! 5. **Cache Population**: Store bare repository paths for phase 2 operations
//!
//! ## Phase 2: Version Resolution & Installation
//! 1. **Batch SHA Resolution**: Resolve all collected versions to commit SHAs using cached repositories
//! 2. **SHA-based Worktree Creation**: Create worktrees keyed by commit SHA for maximum deduplication
//! 3. **Conflict Detection**: Check for path conflicts and incompatible versions
//! 4. **Redundancy Analysis**: Identify duplicate resources across sources
//! 5. **Path Processing**: Preserve directory structure via [`extract_relative_path`] for resources from Git sources
//! 6. **Resource Installation**: Copy resources to target locations with checksums
//! 7. **Lockfile Generation**: Create deterministic lockfile entries with resolved SHAs and preserved paths
//!
//! This separation ensures all network operations complete in phase 1, while phase 2
//! operates entirely on cached data for fast, deterministic resolution.
//!
//! ## Version Resolution Strategy
//!
//! Version constraints are resolved using the following precedence:
//! 1. **Exact commits**: SHA hashes are used directly
//! 2. **Tags**: Semantic version tags (e.g., `v1.2.3`) are preferred
//! 3. **Branches**: Branch heads are resolved to current commits
//! 4. **Latest**: Defaults to the default branch (usually `main` or `master`)
//!
//! # Conflict Detection
//!
//! The resolver detects several types of conflicts:
//!
//! ## Version Conflicts
//! ```toml
//! # Incompatible version constraints for the same resource
//! [agents]
//! app = { source = "community", path = "agents/helper.md", version = "v1.0.0" }
//! tool = { source = "community", path = "agents/helper.md", version = "v2.0.0" }
//! ```
//!
//! ## Path Conflicts
//! ```toml
//! # Different resources installing to the same location
//! [agents]
//! helper-v1 = { source = "old", path = "agents/helper.md" }
//! helper-v2 = { source = "new", path = "agents/helper.md" }
//! ```
//!
//! ## Source Conflicts
//! When the same resource path exists in multiple sources with different content,
//! the resolver uses source precedence (global config sources override local manifest sources).
//!
//! # Redundancy Detection
//!
//! The [`redundancy`] submodule provides sophisticated analysis to identify:
//!
//! - **Version Redundancy**: Same resource at different versions
//! - **Source Redundancy**: Identical resources from different sources
//! - **Path Redundancy**: Multiple resources resolving to the same file
//!
//! Redundancy detection is non-blocking - it generates warnings but allows installation
//! to proceed, enabling legitimate use cases like A/B testing or gradual migrations.
//!
//! # Security Considerations
//!
//! The resolver implements several security measures:
//!
//! - **Input Validation**: All Git references are validated before checkout
//! - **Path Sanitization**: Installation paths are validated to prevent directory traversal
//! - **Credential Isolation**: Authentication tokens are never stored in manifest files
//! - **Checksum Verification**: Resources are checksummed for integrity validation
//!
//! # Performance Optimizations
//!
//! - **SHA-based Worktree Caching**: Worktrees keyed by commit SHA maximize reuse across versions
//! - **Batch Version Resolution**: All versions resolved to SHAs upfront via [`version_resolver::VersionResolver`]
//! - **Single Fetch Per Repository**: Command-instance fetch caching eliminates redundant network operations
//! - **Source Caching**: Git repositories are cached globally in `~/.ccpm/cache/`
//! - **Incremental Updates**: Only modified sources are re-synchronized
//! - **Parallel Operations**: Source syncing and version resolution run concurrently
//! - **Progress Batching**: UI updates are throttled to prevent performance impact
//!
//! # Error Handling
//!
//! The resolver provides detailed error context for common failure scenarios:
//!
//! - **Network Issues**: Graceful handling of Git clone/fetch failures
//! - **Authentication**: Clear error messages for credential problems
//! - **Version Mismatches**: Specific guidance for constraint resolution failures
//! - **Path Issues**: Detailed information about file system conflicts
//!
//! # Example Usage
//!
//! ## Two-Phase Resolution Pattern
//! ```rust,no_run
//! use ccpm::resolver::DependencyResolver;
//! use ccpm::manifest::Manifest;
//! use ccpm::cache::Cache;
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let manifest = Manifest::load(Path::new("ccpm.toml"))?;
//! let cache = Cache::new()?;
//! let mut resolver = DependencyResolver::new_with_global(manifest.clone(), cache).await?;
//!
//! // Get all dependencies from manifest
//! let deps: Vec<(String, ccpm::manifest::ResourceDependency)> = manifest
//!     .all_dependencies()
//!     .into_iter()
//!     .map(|(name, dep)| (name.to_string(), dep.clone()))
//!     .collect();
//!
//! // Phase 1: Sync all required sources (network operations)
//! resolver.pre_sync_sources(&deps).await?;
//!
//! // Phase 2: Resolve dependencies using cached repositories (local operations)
//! let lockfile = resolver.resolve().await?;
//!
//! println!("Resolved {} agents and {} snippets",
//!          lockfile.agents.len(), lockfile.snippets.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Update Pattern
//! ```rust,no_run
//! # use ccpm::resolver::DependencyResolver;
//! # use ccpm::manifest::Manifest;
//! # use ccpm::cache::Cache;
//! # use ccpm::lockfile::LockFile;
//! # use std::path::Path;
//! # async fn update_example() -> anyhow::Result<()> {
//! let manifest = Manifest::load(Path::new("ccpm.toml"))?;
//! let mut lockfile = LockFile::load(Path::new("ccpm.lock"))?;
//! let cache = Cache::new()?;
//! let mut resolver = DependencyResolver::with_cache(manifest.clone(), cache);
//!
//! // Get dependencies to update
//! let deps: Vec<(String, ccpm::manifest::ResourceDependency)> = manifest
//!     .all_dependencies()
//!     .into_iter()
//!     .map(|(name, dep)| (name.to_string(), dep.clone()))
//!     .collect();
//!
//! // Phase 1: Sync sources for update
//! resolver.pre_sync_sources(&deps).await?;
//!
//! // Phase 2: Update specific dependencies
//! resolver.update(&mut lockfile, None).await?;
//!
//! lockfile.save(Path::new("ccpm.lock"))?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Redundancy Analysis
//! ```rust,no_run
//! use ccpm::resolver::{DependencyResolver, redundancy::RedundancyDetector};
//! use ccpm::manifest::Manifest;
//! use ccpm::cache::Cache;
//! use std::path::Path;
//!
//! # async fn redundancy_example() -> anyhow::Result<()> {
//! let manifest = Manifest::load("ccpm.toml".as_ref())?;
//! let cache = Cache::new()?;
//! let resolver = DependencyResolver::new(manifest.clone(), cache)?;
//!
//! // Check for redundancies before resolution
//! if let Some(warning) = resolver.check_redundancies() {
//!     println!("Warning: {}", warning);
//! }
//!
//! // Get detailed redundancy information
//! let redundancies = resolver.check_redundancies_with_details();
//! for redundancy in redundancies {
//!     println!("Redundant usage of: {}", redundancy.source_file);
//!     for usage in &redundancy.usages {
//!         println!("  - {}", usage);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Incremental Updates
//! ```rust,no_run
//! use ccpm::resolver::DependencyResolver;
//! use ccpm::lockfile::LockFile;
//! use ccpm::cache::Cache;
//! use std::path::Path;
//!
//! # async fn update_example() -> anyhow::Result<()> {
//! let existing = LockFile::load("ccpm.lock".as_ref())?;
//! let manifest = ccpm::manifest::Manifest::load("ccpm.toml".as_ref())?;
//! let cache = Cache::new()?;
//! let mut resolver = DependencyResolver::new(manifest, cache)?;
//!
//! // Update specific dependencies only
//! let deps_to_update = vec!["agent1".to_string(), "snippet2".to_string()];
//! let deps_count = deps_to_update.len();
//! let updated = resolver.update(&existing, Some(deps_to_update)).await?;
//!
//! println!("Updated {} dependencies", deps_count);
//! # Ok(())
//! # }
//! ```

pub mod redundancy;
pub mod version_resolution;
pub mod version_resolver;

use crate::cache::Cache;
use crate::core::CcpmError;
use crate::git::GitRepo;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency};
use crate::source::SourceManager;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use self::redundancy::RedundancyDetector;
use self::version_resolver::VersionResolver;

/// Core dependency resolver that transforms manifest dependencies into lockfile entries.
///
/// The [`DependencyResolver`] is the main entry point for dependency resolution.
/// It manages source repositories, resolves version constraints, detects conflicts,
/// and generates deterministic lockfile entries using a centralized SHA-based
/// resolution strategy for optimal performance.
///
/// # SHA-Based Resolution Workflow
///
/// Starting in v0.3.2, the resolver uses [`VersionResolver`] for centralized version
/// resolution that minimizes Git operations and maximizes worktree reuse:
/// 1. **Collection**: Gather all (source, version) pairs from dependencies
/// 2. **Batch Resolution**: Resolve all versions to commit SHAs in parallel
/// 3. **SHA-Based Worktrees**: Create worktrees keyed by commit SHA
/// 4. **Deduplication**: Multiple refs to same SHA share one worktree
///
/// # Configuration
///
/// The resolver can be configured in several ways:
/// - **Standard**: Uses manifest sources only via [`new()`]
/// - **Global**: Includes global config sources via [`new_with_global()`]
/// - **Custom Cache**: Uses custom cache directory via [`with_cache()`]
///
/// # Thread Safety
///
/// The resolver is not thread-safe due to its mutable state during resolution.
/// Create separate instances for concurrent operations.
///
/// [`new()`]: DependencyResolver::new
/// [`new_with_global()`]: DependencyResolver::new_with_global
/// [`with_cache()`]: DependencyResolver::with_cache
pub struct DependencyResolver {
    manifest: Manifest,
    /// Manages Git repository operations, source URL resolution, and authentication.
    ///
    /// The source manager handles:
    /// - Mapping source names to Git repository URLs
    /// - Git operations (clone, fetch, checkout) for dependency resolution
    /// - Authentication token management for private repositories
    /// - Source validation and configuration management
    pub source_manager: SourceManager,
    cache: Cache,
    /// Cached per-(source, version) preparation results built during the
    /// analysis stage so individual dependency resolution can reuse worktrees
    /// without triggering additional sync operations.
    prepared_versions: HashMap<String, PreparedSourceVersion>,
    /// Centralized version resolver for efficient SHA-based dependency resolution.
    ///
    /// The `VersionResolver` handles the crucial first phase of dependency resolution
    /// by batch-resolving all version specifications to commit SHAs before any worktree
    /// operations. This strategy enables maximum worktree reuse and minimal Git operations.
    ///
    /// Used by [`prepare_remote_groups`] to resolve all dependencies upfront.
    version_resolver: VersionResolver,
}

#[derive(Clone, Debug, Default)]
struct PreparedSourceVersion {
    worktree_path: PathBuf,
    resolved_version: Option<String>,
    resolved_commit: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct PreparedGroupDescriptor {
    key: String,
    source: String,
    requested_version: Option<String>,
    version_key: String,
}

impl DependencyResolver {
    fn group_key(source: &str, version: &str) -> String {
        format!("{source}::{version}")
    }

    /// Adds or updates a resource entry in the lockfile based on resource type.
    ///
    /// This helper method eliminates code duplication between the `resolve()` and `update()`
    /// methods by centralizing lockfile entry management logic. It automatically determines
    /// the resource type from the entry name and adds or updates the entry in the appropriate
    /// collection within the lockfile.
    ///
    /// The method performs upsert behavior - if an entry with the same name already exists
    /// in the appropriate collection, it will be updated; otherwise, a new entry is added.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - Mutable reference to the lockfile to modify
    /// * `name` - The name of the resource entry (used to determine resource type)
    /// * `entry` - The [`LockedResource`] entry to add or update
    ///
    /// # Resource Type Detection
    ///
    /// Resource type is determined by calling `get_resource_type()` on the entry name,
    /// which maps to the following lockfile collections:
    /// - `"agent"` → `lockfile.agents`
    /// - `"snippet"` → `lockfile.snippets`
    /// - `"command"` → `lockfile.commands`
    /// - `"script"` → `lockfile.scripts`
    /// - `"hook"` → `lockfile.hooks`
    /// - `"mcp-server"` → `lockfile.mcp_servers`
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use ccpm::lockfile::{LockFile, LockedResource};
    /// # use ccpm::resolver::DependencyResolver;
    /// # let resolver = DependencyResolver::new();
    /// let mut lockfile = LockFile::new();
    /// let entry = LockedResource {
    ///     name: "my-agent".to_string(),
    ///     source: Some("github".to_string()),
    ///     url: Some("https://github.com/org/repo.git".to_string()),
    ///     path: "agents/my-agent.md".to_string(),
    ///     version: Some("v1.0.0".to_string()),
    ///     resolved_commit: Some("abc123def456...".to_string()),
    ///     checksum: "sha256:a1b2c3d4...".to_string(),
    ///     installed_at: ".claude/agents/my-agent.md".to_string(),
    /// };
    ///
    /// // Automatically adds to agents collection based on resource type detection
    /// resolver.add_or_update_lockfile_entry(&mut lockfile, "my-agent", entry);
    /// assert_eq!(lockfile.agents.len(), 1);
    ///
    /// // Subsequent calls update the existing entry
    /// let updated_entry = LockedResource {
    ///     name: "my-agent".to_string(),
    ///     version: Some("v1.1.0".to_string()),
    ///     // ... other fields
    /// #   source: Some("github".to_string()),
    /// #   url: Some("https://github.com/org/repo.git".to_string()),
    /// #   path: "agents/my-agent.md".to_string(),
    /// #   resolved_commit: Some("def456789abc...".to_string()),
    /// #   checksum: "sha256:b2c3d4e5...".to_string()),
    /// #   installed_at: ".claude/agents/my-agent.md".to_string(),
    /// };
    /// resolver.add_or_update_lockfile_entry(&mut lockfile, "my-agent", updated_entry);
    /// assert_eq!(lockfile.agents.len(), 1); // Still one entry, but updated
    /// ```
    fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        name: &str,
        entry: LockedResource,
    ) {
        let resource_type = self.get_resource_type(name);

        match resource_type.as_str() {
            "agent" => {
                if let Some(existing) = lockfile.agents.iter_mut().find(|e| e.name == name) {
                    *existing = entry;
                } else {
                    lockfile.agents.push(entry);
                }
            }
            "snippet" => {
                if let Some(existing) = lockfile.snippets.iter_mut().find(|e| e.name == name) {
                    *existing = entry;
                } else {
                    lockfile.snippets.push(entry);
                }
            }
            "command" => {
                if let Some(existing) = lockfile.commands.iter_mut().find(|e| e.name == name) {
                    *existing = entry;
                } else {
                    lockfile.commands.push(entry);
                }
            }
            "script" => {
                if let Some(existing) = lockfile.scripts.iter_mut().find(|e| e.name == name) {
                    *existing = entry;
                } else {
                    lockfile.scripts.push(entry);
                }
            }
            "hook" => {
                if let Some(existing) = lockfile.hooks.iter_mut().find(|e| e.name == name) {
                    *existing = entry;
                } else {
                    lockfile.hooks.push(entry);
                }
            }
            "mcp-server" => {
                if let Some(existing) = lockfile.mcp_servers.iter_mut().find(|e| e.name == name) {
                    *existing = entry;
                } else {
                    lockfile.mcp_servers.push(entry);
                }
            }
            _ => {
                // Default to snippet
                if let Some(existing) = lockfile.snippets.iter_mut().find(|e| e.name == name) {
                    *existing = entry;
                } else {
                    lockfile.snippets.push(entry);
                }
            }
        }
    }

    /// Detects conflicts where multiple dependencies resolve to the same installation path.
    ///
    /// This method validates that no two dependencies will overwrite each other during
    /// installation. It builds a map of all resolved `installed_at` paths and checks for
    /// collisions across all resource types.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile containing all resolved dependencies
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if no conflicts are detected, or an error describing the conflicts.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Two or more dependencies resolve to the same `installed_at` path
    /// - The error message lists all conflicting dependency names and the shared path
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // This would cause a conflict error:
    /// // [agents]
    /// // v1 = { source = "repo", path = "agents/example.md", version = "v1.0" }
    /// // v2 = { source = "repo", path = "agents/example.md", version = "v2.0" }
    /// // Both resolve to .claude/agents/example.md
    /// ```
    fn detect_target_conflicts(&self, lockfile: &LockFile) -> Result<()> {
        use std::collections::HashMap;

        // Map of installed_at path -> list of dependency names
        let mut path_map: HashMap<String, Vec<String>> = HashMap::new();

        // Collect all resources from lockfile
        let all_resources: Vec<(&str, &LockedResource)> = lockfile
            .agents
            .iter()
            .map(|r| (r.name.as_str(), r))
            .chain(lockfile.snippets.iter().map(|r| (r.name.as_str(), r)))
            .chain(lockfile.commands.iter().map(|r| (r.name.as_str(), r)))
            .chain(lockfile.scripts.iter().map(|r| (r.name.as_str(), r)))
            .chain(lockfile.hooks.iter().map(|r| (r.name.as_str(), r)))
            .chain(lockfile.mcp_servers.iter().map(|r| (r.name.as_str(), r)))
            .collect();

        // Build the path map
        for (name, resource) in all_resources {
            path_map
                .entry(resource.installed_at.clone())
                .or_default()
                .push(name.to_string());
        }

        // Find conflicts (paths with multiple dependencies)
        let conflicts: Vec<_> = path_map
            .iter()
            .filter(|(_, names)| names.len() > 1)
            .collect();

        if !conflicts.is_empty() {
            // Build a detailed error message
            let mut error_msg = String::from(
                "Target path conflicts detected:\n\n\
                 Multiple dependencies resolve to the same installation path.\n\
                 This would cause files to overwrite each other.\n\n",
            );

            for (path, names) in conflicts {
                error_msg.push_str(&format!(
                    "  Path: {}\n  Conflicts: {}\n\n",
                    path,
                    names.join(", ")
                ));
            }

            error_msg.push_str(
                "To resolve:\n\
                 1. Use different dependency names for different versions\n\
                 2. Use custom 'target' field to specify different installation paths\n\
                 3. Ensure pattern dependencies don't overlap with single-file dependencies",
            );

            return Err(anyhow::anyhow!(error_msg));
        }

        Ok(())
    }

    /// Pre-syncs all sources needed for the given dependencies.
    ///
    /// This method implements the first phase of the two-phase resolution architecture.
    /// It should be called during the "Syncing sources" phase to perform all Git
    /// clone/fetch operations upfront, before actual dependency resolution.
    ///
    /// This separation provides several benefits:
    /// - Clear separation of network operations from version resolution logic
    /// - Better progress reporting with distinct phases
    /// - Enables batch processing of Git operations for efficiency
    /// - Allows the `resolve_all()` method to work purely with local cached data
    ///
    /// After calling this method, the internal [`VersionResolver`] will have all
    /// necessary source repositories cached and ready for version-to-SHA resolution.
    ///
    /// # Arguments
    ///
    /// * `deps` - A slice of tuples containing dependency names and their definitions.
    ///   Only dependencies with Git sources will be processed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ccpm::resolver::DependencyResolver;
    /// # use ccpm::manifest::{Manifest, ResourceDependency};
    /// # use ccpm::cache::Cache;
    /// # async fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::new();
    /// let cache = Cache::new()?;
    /// let mut resolver = DependencyResolver::with_cache(manifest.clone(), cache);
    ///
    /// // Get all dependencies from manifest
    /// let deps: Vec<(String, ResourceDependency)> = manifest
    ///     .all_dependencies()
    ///     .into_iter()
    ///     .map(|(name, dep)| (name.to_string(), dep.clone()))
    ///     .collect();
    ///
    /// // Phase 1: Pre-sync all sources (performs Git clone/fetch operations)
    /// resolver.pre_sync_sources(&deps).await?;
    ///
    /// // Phase 2: Now sources are ready for version resolution (no network I/O)
    /// let resolved = resolver.resolve().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Two-Phase Resolution Pattern
    ///
    /// This method is part of CCPM's two-phase resolution architecture:
    ///
    /// 1. **Sync Phase** (`pre_sync_sources`): Clone/fetch all Git repositories
    /// 2. **Resolution Phase** (`resolve` or `update`): Resolve versions to SHAs locally
    ///
    /// This pattern ensures all network operations happen upfront with clear progress
    /// reporting, while version resolution can proceed quickly using cached data.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source repository cloning or fetching fails
    /// - Network connectivity issues occur
    /// - Authentication fails for private repositories
    /// - Source names in dependencies don't match configured sources
    /// - Git operations fail due to repository corruption or disk space issues
    pub async fn pre_sync_sources(&mut self, deps: &[(String, ResourceDependency)]) -> Result<()> {
        // Clear and rebuild the version resolver entries
        self.version_resolver.clear();

        // Collect all unique (source, version) pairs
        for (_, dep) in deps {
            if let Some(source_name) = dep.get_source() {
                let source_url =
                    self.source_manager
                        .get_source_url(source_name)
                        .ok_or_else(|| CcpmError::SourceNotFound {
                            name: source_name.to_string(),
                        })?;

                let version = dep.get_version();

                // Add to version resolver for batch syncing
                self.version_resolver
                    .add_version(source_name, &source_url, version);
            }
        }

        // Pre-sync all sources (performs Git operations)
        self.version_resolver
            .pre_sync_sources()
            .await
            .context("Failed to sync sources")?;

        Ok(())
    }

    /// Get available versions (tags) for a repository.
    ///
    /// Lists all tags from a Git repository, which typically represent available versions.
    /// This is useful for checking what versions are available for updates.
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the Git repository (typically in the cache directory)
    ///
    /// # Returns
    ///
    /// A vector of version strings (tag names) available in the repository.
    ///
    /// # Examples
    ///
    /// ```rust,no_run,ignore
    /// use ccpm::resolver::DependencyResolver;
    /// use ccpm::cache::Cache;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache = Cache::new()?;
    /// let repo_path = cache.get_repo_path("community");
    /// let resolver = DependencyResolver::new(manifest, cache, 10)?;
    ///
    /// let versions = resolver.get_available_versions(&repo_path).await?;
    /// for version in versions {
    ///     println!("Available: {}", version);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_available_versions(&self, repo_path: &Path) -> Result<Vec<String>> {
        let repo = GitRepo::new(repo_path);
        repo.list_tags()
            .await
            .with_context(|| format!("Failed to list tags from repository at {:?}", repo_path))
    }

    /// Creates worktrees for all resolved SHAs in parallel.
    ///
    /// This helper method is part of CCPM's SHA-based worktree architecture, processing
    /// all resolved versions from the [`VersionResolver`] and creating Git worktrees
    /// for each unique commit SHA. It leverages async concurrency to create multiple
    /// worktrees in parallel while maintaining proper error propagation.
    ///
    /// The method implements SHA-based deduplication - multiple versions (tags, branches)
    /// that resolve to the same commit SHA will share a single worktree, maximizing
    /// disk space efficiency and reducing clone operations.
    ///
    /// # Implementation Details
    ///
    /// 1. **Parallel Execution**: Uses `futures::future::join_all()` for concurrent worktree creation
    /// 2. **SHA-based Keys**: Worktrees are keyed by commit SHA rather than version strings
    /// 3. **Deduplication**: Multiple refs pointing to the same commit share one worktree
    /// 4. **Error Handling**: Fails fast if any worktree creation fails
    ///
    /// # Returns
    ///
    /// Returns a [`HashMap`] mapping repository keys (format: `"source::version"`) to
    /// [`PreparedSourceVersion`] structs containing:
    /// - `worktree_path`: Absolute path to the created worktree directory
    /// - `resolved_version`: The resolved Git reference (tag, branch, or SHA)
    /// - `resolved_commit`: The final commit SHA for the worktree
    ///
    /// # Example Usage
    ///
    /// ```ignore
    /// // This is called internally after version resolution
    /// let prepared = resolver.create_worktrees_for_resolved_versions().await?;
    ///
    /// // Access worktree for a specific dependency
    /// let key = DependencyResolver::group_key("my-source", "v1.0.0");
    /// if let Some(prepared_version) = prepared.get(&key) {
    ///     println!("Worktree at: {}", prepared_version.worktree_path.display());
    ///     println!("Commit SHA: {}", prepared_version.resolved_commit);
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source URL cannot be found for a resolved version (indicates configuration issue)
    /// - Worktree creation fails for any SHA (disk space, permissions, Git errors)
    /// - File system operations fail (I/O errors, permission denied)
    /// - Git operations fail (corrupted repository, invalid SHA)
    async fn create_worktrees_for_resolved_versions(
        &self,
    ) -> Result<HashMap<String, PreparedSourceVersion>> {
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
                .ok_or_else(|| CcpmError::SourceNotFound {
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
        let results = futures::future::join_all(futures).await;

        // Process results and build the map
        for result in results {
            let (key, prepared) = result?;
            prepared_versions.insert(key, prepared);
        }

        Ok(prepared_versions)
    }

    async fn prepare_remote_groups(&mut self, deps: &[(String, ResourceDependency)]) -> Result<()> {
        self.prepared_versions.clear();

        // Check if we need to rebuild version resolver entries
        // This happens when prepare_remote_groups is called without pre_sync_sources
        // (e.g., during tests or backward compatibility)
        if !self.version_resolver.has_entries() {
            // Rebuild entries for version resolution
            for (_, dep) in deps {
                if let Some(source_name) = dep.get_source() {
                    let source_url =
                        self.source_manager
                            .get_source_url(source_name)
                            .ok_or_else(|| CcpmError::SourceNotFound {
                                name: source_name.to_string(),
                            })?;

                    let version = dep.get_version();

                    // Add to version resolver for batch resolution
                    self.version_resolver
                        .add_version(source_name, &source_url, version);
                }
            }

            // If entries were rebuilt, we need to sync sources first
            self.version_resolver
                .pre_sync_sources()
                .await
                .context("Failed to sync sources")?;
        }

        // Now resolve all versions to SHAs
        self.version_resolver
            .resolve_all()
            .await
            .context("Failed to resolve versions to SHAs")?;

        // Step 3: Create worktrees for all resolved SHAs in parallel
        let prepared_versions = self.create_worktrees_for_resolved_versions().await?;

        // Store the prepared versions
        self.prepared_versions.extend(prepared_versions);

        // Phase completion is handled by the caller

        Ok(())
    }

    /// Creates a new resolver using only manifest-defined sources.
    ///
    /// This constructor creates a resolver that only considers sources defined
    /// in the manifest file. Global configuration sources from `~/.ccpm/config.toml`
    /// are ignored, which may cause resolution failures for private repositories
    /// that require authentication.
    ///
    /// # Usage
    ///
    /// Use this constructor for:
    /// - Public repositories only
    /// - Testing and development
    /// - Backward compatibility with older workflows
    ///
    /// For production use with private repositories, prefer [`new_with_global()`].
    ///
    /// # Errors
    ///
    /// Returns an error if the cache cannot be created.
    ///
    /// [`new_with_global()`]: DependencyResolver::new_with_global
    pub fn new(manifest: Manifest, cache: Cache) -> Result<Self> {
        let source_manager = SourceManager::from_manifest(&manifest)?;
        let version_resolver = VersionResolver::new(cache.clone());

        Ok(Self {
            manifest,
            source_manager,
            cache,
            prepared_versions: HashMap::new(),
            version_resolver,
        })
    }

    /// Creates a new resolver with global configuration support.
    ///
    /// This is the recommended constructor for most use cases. It loads both
    /// manifest sources and global sources from `~/.ccpm/config.toml`, enabling
    /// access to private repositories with authentication tokens.
    ///
    /// # Source Priority
    ///
    /// When sources are defined in both locations:
    /// 1. **Global sources** (from `~/.ccpm/config.toml`) are loaded first
    /// 2. **Local sources** (from `ccpm.toml`) can override global sources
    ///
    /// This allows teams to share project configurations while keeping
    /// authentication tokens in user-specific global config.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The cache cannot be created
    /// - The global config file exists but cannot be parsed
    /// - Network errors occur while validating global sources
    pub async fn new_with_global(manifest: Manifest, cache: Cache) -> Result<Self> {
        let source_manager = SourceManager::from_manifest_with_global(&manifest).await?;
        let version_resolver = VersionResolver::new(cache.clone());

        Ok(Self {
            manifest,
            source_manager,
            cache,
            prepared_versions: HashMap::new(),
            version_resolver,
        })
    }

    /// Creates a new resolver with a custom cache.
    ///
    /// This constructor is primarily used for testing and specialized deployments
    /// where the default cache location (`~/.ccpm/cache/`) is not suitable.
    ///
    /// # Use Cases
    ///
    /// - **Testing**: Isolated cache for test environments
    /// - **CI/CD**: Custom cache locations for build systems
    /// - **Containers**: Non-standard filesystem layouts
    /// - **Multi-user**: Shared cache directories
    ///
    /// # Note
    ///
    /// This constructor does not load global configuration. If you need both
    /// custom cache location and global config support, create the resolver
    /// with [`new_with_global()`] and manually configure the source manager.
    ///
    /// [`new_with_global()`]: DependencyResolver::new_with_global
    #[must_use]
    pub fn with_cache(manifest: Manifest, cache: Cache) -> Self {
        let cache_dir = cache.get_cache_location().to_path_buf();
        let source_manager = SourceManager::from_manifest_with_cache(&manifest, cache_dir);
        let version_resolver = VersionResolver::new(cache.clone());

        Self {
            manifest,
            source_manager,
            cache,
            prepared_versions: HashMap::new(),
            version_resolver,
        }
    }

    /// Resolves all dependencies and generates a complete lockfile.
    ///
    /// This is the main resolution method that processes all dependencies from
    /// the manifest and produces a deterministic lockfile. The process includes:
    ///
    /// 1. **Source Validation**: Verify all referenced sources exist
    /// 2. **Parallel Sync**: Clone/update source repositories concurrently
    /// 3. **Version Resolution**: Resolve constraints to specific commits
    /// 4. **Entry Generation**: Create lockfile entries with checksums
    ///
    /// # Algorithm Details
    ///
    /// The resolution algorithm processes dependencies in dependency order to ensure
    /// consistency. For each dependency:
    /// - Local dependencies are processed immediately (no network access)
    /// - Remote dependencies trigger source synchronization
    /// - Version constraints are resolved using Git tag/branch lookup
    /// - Installation paths are determined based on resource type
    ///
    /// # Parameters
    ///
    /// - `progress`: Optional progress manager for user feedback during long operations
    ///
    /// # Returns
    ///
    /// A [`LockFile`] containing all resolved dependencies with:
    /// - Exact commit hashes for reproducible installations
    /// - Source URLs for traceability
    /// - Installation paths for each resource
    /// - Checksums for integrity verification (computed later during installation)
    ///
    /// # Errors
    ///
    /// Resolution can fail due to:
    /// - **Network Issues**: Git clone/fetch failures
    /// - **Authentication**: Missing or invalid credentials for private sources
    /// - **Version Conflicts**: Incompatible version constraints
    /// - **Missing Resources**: Referenced files don't exist in sources
    /// - **Path Conflicts**: Multiple resources installing to same location
    ///
    /// # Performance
    ///
    /// - **Parallel Source Sync**: Multiple sources are processed concurrently
    /// - **Cache Utilization**: Previously cloned sources are reused
    /// - **Progress Reporting**: Non-blocking UI updates during resolution
    ///
    /// [`LockFile`]: crate::lockfile::LockFile
    pub async fn resolve(&mut self) -> Result<LockFile> {
        let mut lockfile = LockFile::new();
        let mut resolved = HashMap::new();

        // Add sources to lockfile
        for (name, url) in &self.manifest.sources {
            lockfile.add_source(name.clone(), url.clone(), String::new());
        }

        // Get all dependencies to resolve including MCP servers (clone to avoid borrow checker issues)
        let deps: Vec<(String, ResourceDependency)> = self
            .manifest
            .all_dependencies_with_mcp()
            .into_iter()
            .map(|(name, dep)| (name.to_string(), dep.into_owned()))
            .collect();

        // Show initial message about what we're doing
        // Sync sources (phase management is handled by caller)
        self.prepare_remote_groups(&deps).await?;

        // Resolve each dependency
        for (name, dep) in deps.iter() {
            // Progress is tracked at the phase level

            // Check if this is a pattern dependency
            if dep.is_pattern() {
                // Pattern dependencies resolve to multiple resources
                let entries = self.resolve_pattern_dependency(name, dep).await?;

                // Add each resolved entry to the appropriate resource type with deduplication
                let resource_type = self.get_resource_type(name);
                for entry in entries {
                    match resource_type.as_str() {
                        "agent" => {
                            if let Some(existing) =
                                lockfile.agents.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.agents.push(entry);
                            }
                        }
                        "snippet" => {
                            if let Some(existing) =
                                lockfile.snippets.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.snippets.push(entry);
                            }
                        }
                        "command" => {
                            if let Some(existing) =
                                lockfile.commands.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.commands.push(entry);
                            }
                        }
                        "script" => {
                            if let Some(existing) =
                                lockfile.scripts.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.scripts.push(entry);
                            }
                        }
                        "hook" => {
                            if let Some(existing) =
                                lockfile.hooks.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.hooks.push(entry);
                            }
                        }
                        "mcp-server" => {
                            if let Some(existing) = lockfile
                                .mcp_servers
                                .iter_mut()
                                .find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.mcp_servers.push(entry);
                            }
                        }
                        _ => {
                            if let Some(existing) =
                                lockfile.snippets.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.snippets.push(entry);
                            }
                        }
                    }
                }
            } else {
                // Regular single dependency
                let entry = self.resolve_dependency(name, dep).await?;
                resolved.insert(name.to_string(), entry);
            }

            // Progress is tracked by updating messages, no need to increment
        }

        // Progress is tracked at the phase level

        // Add resolved single entries to lockfile
        for (name, entry) in resolved {
            self.add_or_update_lockfile_entry(&mut lockfile, &name, entry);
        }

        // Detect target-path conflicts before finalizing
        self.detect_target_conflicts(&lockfile)?;

        // Progress completion is handled by the caller

        Ok(lockfile)
    }

    /// Resolves a single dependency to a lockfile entry.
    ///
    /// This internal method handles the resolution of one dependency, including
    /// source synchronization, version resolution, and entry creation.
    ///
    /// # Algorithm
    ///
    /// For local dependencies:
    /// 1. Validate the path format
    /// 2. Determine installation location based on resource type
    /// 3. Preserve relative directory structure from source path
    /// 4. Create entry with relative path (no source sync required)
    ///
    /// For remote dependencies:
    /// 1. Validate source exists in manifest or global config
    /// 2. Synchronize source repository (clone or fetch)
    /// 3. Resolve version constraint to specific commit
    /// 4. Preserve relative directory structure from dependency path
    /// 5. Create entry with resolved commit and source information
    ///
    /// # Parameters
    ///
    /// - `name`: The dependency name from the manifest
    /// - `dep`: The dependency specification with source, path, and version
    ///
    /// # Returns
    ///
    /// A [`LockedResource`] with:
    /// - Resolved commit hash (for remote dependencies)
    /// - Source and URL information
    /// - Installation path in the project
    /// - Empty checksum (computed during actual installation)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source is not found in manifest or global config
    /// - Source repository cannot be cloned or accessed
    /// - Version constraint cannot be resolved (tag/branch not found)
    /// - Git operations fail due to network or authentication issues
    ///
    /// [`LockedResource`]: crate::lockfile::LockedResource
    async fn resolve_dependency(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
    ) -> Result<LockedResource> {
        // Check if this is a pattern-based dependency
        if dep.is_pattern() {
            // Pattern dependencies resolve to multiple resources
            // This should be handled by a separate method
            return Err(anyhow::anyhow!(
                "Pattern dependency '{}' should be resolved using resolve_pattern_dependency",
                name
            ));
        }

        if dep.is_local() {
            // Local dependency - just create entry with path
            // Determine the installed location based on resource type, custom target, and custom filename
            let resource_type = self.get_resource_type(name);

            // Determine the filename to use
            let filename = if let Some(custom_filename) = dep.get_filename() {
                // Use custom filename as-is (includes extension)
                custom_filename.to_string()
            } else {
                // Extract relative path from the dependency path to preserve directory structure
                let dep_path = Path::new(dep.get_path());
                let relative_path = extract_relative_path(dep_path, &resource_type);

                // If a relative path exists, preserve it; otherwise use dependency name
                if relative_path.as_os_str().is_empty() || relative_path == dep_path {
                    // No relative path preserved, use default filename
                    let extension = match resource_type.as_str() {
                        "hook" | "mcp-server" => "json",
                        "script" => {
                            // Scripts maintain their original extension
                            dep_path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("sh")
                        }
                        _ => "md",
                    };
                    format!("{}.{}", name, extension)
                } else {
                    // Preserve the relative path structure
                    relative_path.to_string_lossy().to_string()
                }
            };

            // Determine the target directory
            let installed_at = if let Some(custom_target) = dep.get_target() {
                // Custom target is relative to the default resource directory
                let base_target = match resource_type.as_str() {
                    "agent" => &self.manifest.target.agents,
                    "snippet" => &self.manifest.target.snippets,
                    "command" => &self.manifest.target.commands,
                    "script" => &self.manifest.target.scripts,
                    "hook" => &self.manifest.target.hooks,
                    "mcp-server" => &self.manifest.target.mcp_servers,
                    _ => &self.manifest.target.snippets,
                };
                format!("{}/{}", base_target, custom_target.trim_start_matches('/'))
                    .replace("//", "/")
                    + "/"
                    + &filename
            } else {
                // Use default target based on resource type
                let target_dir = match resource_type.as_str() {
                    "agent" => &self.manifest.target.agents,
                    "snippet" => &self.manifest.target.snippets,
                    "command" => &self.manifest.target.commands,
                    "script" => &self.manifest.target.scripts,
                    "hook" => &self.manifest.target.hooks,
                    "mcp-server" => &self.manifest.target.mcp_servers,
                    _ => &self.manifest.target.snippets, // fallback
                };
                format!("{}/{}", target_dir, filename)
            };

            Ok(LockedResource {
                name: name.to_string(),
                source: None,
                url: None,
                path: dep.get_path().to_string(),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at,
            })
        } else {
            // Remote dependency - need to sync and resolve
            let source_name = dep.get_source().ok_or_else(|| CcpmError::ConfigError {
                message: format!("Dependency '{}' has no source specified", name),
            })?;

            // Get source URL
            let source_url = self
                .source_manager
                .get_source_url(source_name)
                .ok_or_else(|| CcpmError::SourceNotFound {
                    name: source_name.to_string(),
                })?;

            let version_key = dep
                .get_version()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "HEAD".to_string());
            let prepared_key = Self::group_key(source_name, &version_key);

            // Check if this dependency has been prepared
            let (resolved_version, resolved_commit) =
                if let Some(prepared) = self.prepared_versions.get(&prepared_key) {
                    // Use prepared version
                    (
                        prepared.resolved_version.clone(),
                        prepared.resolved_commit.clone(),
                    )
                } else {
                    // This dependency wasn't prepared (e.g., when called from `ccpm add`)
                    // We need to prepare it on-demand
                    let deps = vec![(name.to_string(), dep.clone())];
                    self.prepare_remote_groups(&deps).await?;

                    // Now it should be prepared
                    if let Some(prepared) = self.prepared_versions.get(&prepared_key) {
                        (
                            prepared.resolved_version.clone(),
                            prepared.resolved_commit.clone(),
                        )
                    } else {
                        return Err(anyhow::anyhow!(
                            "Failed to prepare dependency '{}' from source '{}' @ '{}'",
                            name,
                            source_name,
                            version_key
                        ));
                    }
                };

            // Determine the installed location based on resource type, custom target, and custom filename
            let resource_type = self.get_resource_type(name);

            // Determine the filename to use
            let filename = if let Some(custom_filename) = dep.get_filename() {
                // Use custom filename as-is (includes extension)
                custom_filename.to_string()
            } else {
                // Extract relative path from the dependency path to preserve directory structure
                let dep_path = Path::new(dep.get_path());
                let relative_path = extract_relative_path(dep_path, &resource_type);

                // If a relative path exists, preserve it; otherwise use dependency name
                if relative_path.as_os_str().is_empty() || relative_path == dep_path {
                    // No relative path preserved, use default filename
                    let extension = match resource_type.as_str() {
                        "hook" | "mcp-server" => "json",
                        "script" => {
                            // Scripts maintain their original extension
                            dep_path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("sh")
                        }
                        _ => "md",
                    };
                    format!("{}.{}", name, extension)
                } else {
                    // Preserve the relative path structure
                    relative_path.to_string_lossy().to_string()
                }
            };

            // Determine the target directory
            let installed_at = if let Some(custom_target) = dep.get_target() {
                // Custom target is relative to the default resource directory
                let base_target = match resource_type.as_str() {
                    "agent" => &self.manifest.target.agents,
                    "snippet" => &self.manifest.target.snippets,
                    "command" => &self.manifest.target.commands,
                    "script" => &self.manifest.target.scripts,
                    "hook" => &self.manifest.target.hooks,
                    "mcp-server" => &self.manifest.target.mcp_servers,
                    _ => &self.manifest.target.snippets,
                };
                format!("{}/{}", base_target, custom_target.trim_start_matches('/'))
                    .replace("//", "/")
                    + "/"
                    + &filename
            } else {
                // Use default target based on resource type
                let target_dir = match resource_type.as_str() {
                    "agent" => &self.manifest.target.agents,
                    "snippet" => &self.manifest.target.snippets,
                    "command" => &self.manifest.target.commands,
                    "script" => &self.manifest.target.scripts,
                    "hook" => &self.manifest.target.hooks,
                    "mcp-server" => &self.manifest.target.mcp_servers,
                    _ => &self.manifest.target.snippets, // fallback
                };
                format!("{}/{}", target_dir, filename)
            };

            Ok(LockedResource {
                name: name.to_string(),
                source: Some(source_name.to_string()),
                url: Some(source_url.clone()),
                path: dep.get_path().to_string(),
                version: resolved_version, // Use the resolved version (e.g., "main")
                resolved_commit: Some(resolved_commit),
                checksum: String::new(), // Will be calculated during installation
                installed_at,
            })
        }
    }

    /// Resolves a pattern-based dependency to multiple locked resources.
    ///
    /// Pattern dependencies match multiple resources using glob patterns,
    /// enabling batch installation of related resources.
    ///
    /// # Process
    ///
    /// 1. Sync the source repository
    /// 2. Checkout the specified version (if any)
    /// 3. Search for files matching the pattern
    /// 4. Preserve relative directory structure for each matched file
    /// 5. Create a locked resource for each match
    ///
    /// # Parameters
    ///
    /// - `name`: The dependency name (used for the collection)
    /// - `dep`: The pattern-based dependency specification
    ///
    /// # Returns
    ///
    /// A vector of [`LockedResource`] entries, one for each matched file.
    async fn resolve_pattern_dependency(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
    ) -> Result<Vec<LockedResource>> {
        // Pattern dependencies use the path field with glob characters
        if !dep.is_pattern() {
            return Err(anyhow::anyhow!(
                "Expected pattern dependency but no glob characters found in path"
            ));
        }

        let pattern = dep.get_path();

        if dep.is_local() {
            // Local pattern dependency - search in filesystem
            // Extract base path from the pattern if it contains an absolute path
            let (base_path, pattern_str) = if pattern.contains('/') || pattern.contains('\\') {
                // Pattern contains path separators, extract base path
                let pattern_path = Path::new(pattern);
                if let Some(parent) = pattern_path.parent() {
                    if parent.is_absolute() || parent.starts_with("..") || parent.starts_with(".") {
                        // Use the parent as base path and just the filename pattern
                        (
                            parent.to_path_buf(),
                            pattern_path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or(pattern)
                                .to_string(),
                        )
                    } else {
                        // Relative path, use current directory as base
                        (PathBuf::from("."), pattern.to_string())
                    }
                } else {
                    // No parent, use current directory
                    (PathBuf::from("."), pattern.to_string())
                }
            } else {
                // Simple pattern without path separators
                (PathBuf::from("."), pattern.to_string())
            };

            let pattern_resolver = crate::pattern::PatternResolver::new();
            let matches = pattern_resolver.resolve(&pattern_str, &base_path)?;

            let resource_type = self.get_resource_type(name);
            let mut resources = Vec::new();

            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);

                // Extract relative path to preserve directory structure
                let relative_path = extract_relative_path(&matched_path, &resource_type);

                // Determine the target directory
                let target_dir = if let Some(custom_target) = dep.get_target() {
                    // Custom target is relative to the default resource directory
                    let base_target = match resource_type.as_str() {
                        "agent" => &self.manifest.target.agents,
                        "snippet" => &self.manifest.target.snippets,
                        "command" => &self.manifest.target.commands,
                        "script" => &self.manifest.target.scripts,
                        "hook" => &self.manifest.target.hooks,
                        "mcp-server" => &self.manifest.target.mcp_servers,
                        _ => &self.manifest.target.snippets,
                    };
                    format!("{}/{}", base_target, custom_target.trim_start_matches('/'))
                        .replace("//", "/")
                } else {
                    match resource_type.as_str() {
                        "agent" => self.manifest.target.agents.clone(),
                        "snippet" => self.manifest.target.snippets.clone(),
                        "command" => self.manifest.target.commands.clone(),
                        "script" => self.manifest.target.scripts.clone(),
                        "hook" => self.manifest.target.hooks.clone(),
                        "mcp-server" => self.manifest.target.mcp_servers.clone(),
                        _ => self.manifest.target.snippets.clone(),
                    }
                };

                // Use relative path if it exists, otherwise use resource name
                let filename =
                    if relative_path.as_os_str().is_empty() || relative_path == matched_path {
                        let extension = matched_path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("md");
                        format!("{}.{}", resource_name, extension)
                    } else {
                        relative_path.to_string_lossy().to_string()
                    };

                let installed_at = format!("{}/{}", target_dir, filename);

                // Construct full relative path from base_path and matched_path
                let full_relative_path = if base_path == Path::new(".") {
                    matched_path.to_string_lossy().to_string()
                } else {
                    format!("{}/{}", base_path.display(), matched_path.display())
                };

                resources.push(LockedResource {
                    name: resource_name.clone(),
                    source: None,
                    url: None,
                    path: full_relative_path,
                    version: None,
                    resolved_commit: None,
                    checksum: String::new(),
                    installed_at,
                });
            }

            Ok(resources)
        } else {
            // Remote pattern dependency - need to sync and search
            let source_name = dep.get_source().ok_or_else(|| CcpmError::ConfigError {
                message: format!("Pattern dependency '{name}' has no source specified"),
            })?;

            let source_url = self
                .source_manager
                .get_source_url(source_name)
                .ok_or_else(|| CcpmError::SourceNotFound {
                    name: source_name.to_string(),
                })?;

            let version_key = dep
                .get_version()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "HEAD".to_string());
            let prepared_key = Self::group_key(source_name, &version_key);

            let prepared = self
                .prepared_versions
                .get(&prepared_key)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Prepared state missing for source '{}' @ '{}'. Stage 1 preparation should have populated this entry.",
                        source_name,
                        version_key
                    )
                })?;

            let repo_path = prepared.worktree_path.clone();
            let resolved_version = prepared.resolved_version.clone();
            let resolved_commit = prepared.resolved_commit.clone();

            // Search for matching files in the repository
            let pattern_resolver = crate::pattern::PatternResolver::new();
            let repo_path_ref = Path::new(&repo_path);
            let matches = pattern_resolver.resolve(pattern, repo_path_ref)?;

            let resource_type = self.get_resource_type(name);
            let mut resources = Vec::new();

            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);

                // Extract relative path to preserve directory structure
                let relative_path = extract_relative_path(&matched_path, &resource_type);

                // Determine the target directory
                let target_dir = if let Some(custom_target) = dep.get_target() {
                    // Custom target is relative to the default resource directory
                    let base_target = match resource_type.as_str() {
                        "agent" => &self.manifest.target.agents,
                        "snippet" => &self.manifest.target.snippets,
                        "command" => &self.manifest.target.commands,
                        "script" => &self.manifest.target.scripts,
                        "hook" => &self.manifest.target.hooks,
                        "mcp-server" => &self.manifest.target.mcp_servers,
                        _ => &self.manifest.target.snippets,
                    };
                    format!("{}/{}", base_target, custom_target.trim_start_matches('/'))
                        .replace("//", "/")
                } else {
                    match resource_type.as_str() {
                        "agent" => self.manifest.target.agents.clone(),
                        "snippet" => self.manifest.target.snippets.clone(),
                        "command" => self.manifest.target.commands.clone(),
                        "script" => self.manifest.target.scripts.clone(),
                        "hook" => self.manifest.target.hooks.clone(),
                        "mcp-server" => self.manifest.target.mcp_servers.clone(),
                        _ => self.manifest.target.snippets.clone(),
                    }
                };

                // Use relative path if it exists, otherwise use resource name
                let filename =
                    if relative_path.as_os_str().is_empty() || relative_path == matched_path {
                        let extension = matched_path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("md");
                        format!("{}.{}", resource_name, extension)
                    } else {
                        relative_path.to_string_lossy().to_string()
                    };

                let installed_at = format!("{}/{}", target_dir, filename);

                resources.push(LockedResource {
                    name: resource_name.clone(),
                    source: Some(source_name.to_string()),
                    url: Some(source_url.clone()),
                    path: matched_path.to_string_lossy().to_string(),
                    version: resolved_version.clone(), // Use the resolved version (e.g., "main")
                    resolved_commit: Some(resolved_commit.clone()),
                    checksum: String::new(),
                    installed_at,
                });
            }

            Ok(resources)
        }
    }

    /// Checks out a specific version in a Git repository.
    ///
    /// This method implements the version resolution strategy by attempting
    /// to checkout Git references in order of preference:
    ///
    /// 1. **Tags**: Exact tag matches (e.g., `v1.2.3`)
    /// 2. **Branches**: Branch heads (e.g., `main`, `develop`)
    /// 3. **Commits**: Direct commit hashes (40-character SHA)
    ///
    /// # Algorithm
    ///
    /// ```text
    /// 1. List all tags in repository
    /// 2. If version matches a tag, checkout tag
    /// 3. Else attempt branch checkout
    /// 4. Else attempt commit hash checkout
    /// 5. Return current HEAD commit hash
    /// ```
    ///
    /// # Performance Note
    ///
    /// Tag listing is cached by Git, making tag lookups efficient.
    /// The method avoids unnecessary network operations by checking
    /// local references first.
    ///
    /// # Parameters
    ///
    /// - `repo`: Git repository handle
    /// - `version`: Version constraint (tag, branch, or commit hash)
    ///
    /// # Returns
    ///
    /// The commit hash (SHA) of the checked out version.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Git repository is in an invalid state
    /// - Version string doesn't match any tag, branch, or valid commit
    /// - Git checkout fails due to conflicts or permissions
    /// - Repository is corrupted or inaccessible
    ///
    /// Determines the resource type (agent or snippet) from a dependency name.
    ///
    /// This method checks which manifest section contains the dependency
    /// to determine where it should be installed in the project.
    ///
    /// # Resource Type Mapping
    ///
    /// - **agents**: Dependencies listed in `[agents]` section
    /// - **snippets**: Dependencies listed in `[snippets]` section
    ///
    /// # Installation Paths
    ///
    /// Resource types determine installation directories:
    /// - Agents install to `{manifest.target.agents}/{name}.md`
    /// - Snippets install to `{manifest.target.snippets}/{name}.md`
    ///
    /// # Parameters
    ///
    /// - `name`: Dependency name as defined in manifest
    ///
    /// # Returns
    ///
    /// Resource type as a string: `"agent"` or `"snippet"`.
    ///
    /// # Default Behavior
    ///
    /// If a dependency is not found in the agents section, it defaults
    /// to `"snippet"`. This handles edge cases and maintains backward compatibility.
    fn get_resource_type(&self, name: &str) -> String {
        if self.manifest.agents.contains_key(name) {
            "agent".to_string()
        } else if self.manifest.snippets.contains_key(name) {
            "snippet".to_string()
        } else if self.manifest.commands.contains_key(name) {
            "command".to_string()
        } else if self.manifest.scripts.contains_key(name) {
            "script".to_string()
        } else if self.manifest.hooks.contains_key(name) {
            "hook".to_string()
        } else if self.manifest.mcp_servers.contains_key(name) {
            "mcp-server".to_string()
        } else {
            "snippet".to_string() // Default fallback
        }
    }

    /// Updates an existing lockfile with new or changed dependencies.
    ///
    /// This method performs incremental dependency resolution by comparing
    /// the current manifest against an existing lockfile and updating only
    /// the specified dependencies (or all if none specified).
    ///
    /// # Update Strategy
    ///
    /// The update process follows these steps:
    /// 1. **Selective Resolution**: Only resolve specified dependencies
    /// 2. **Preserve Existing**: Keep unchanged dependencies from existing lockfile
    /// 3. **In-place Updates**: Replace matching entries with new versions
    /// 4. **New Additions**: Append newly added dependencies
    ///
    /// # Use Cases
    ///
    /// - **Selective Updates**: Update specific outdated dependencies
    /// - **Security Patches**: Update dependencies with known vulnerabilities
    /// - **Feature Updates**: Pull latest versions for active development
    /// - **Manifest Changes**: Reflect additions/modifications to ccpm.toml
    ///
    /// # Parameters
    ///
    /// - `existing`: Current lockfile to update
    /// - `deps_to_update`: Optional list of specific dependencies to update.
    ///   If `None`, all dependencies are updated.
    /// - `progress`: Optional progress bar for user feedback
    ///
    /// # Returns
    ///
    /// A new [`LockFile`] with updated dependencies. The original lockfile
    /// structure is preserved, with only specified entries modified.
    ///
    /// # Algorithm Complexity
    ///
    /// - **Time**: O(u + s·log(t)) where u = dependencies to update
    /// - **Space**: O(n) where n = total dependencies in lockfile
    ///
    /// # Performance Benefits
    ///
    /// - **Network Optimization**: Only syncs sources for updated dependencies
    /// - **Cache Utilization**: Reuses existing source repositories
    /// - **Parallel Processing**: Updates multiple dependencies concurrently
    ///
    /// # Errors
    ///
    /// Update can fail due to:
    /// - Network issues accessing source repositories
    /// - Version constraints that cannot be satisfied
    /// - Authentication failures for private sources
    /// - Corrupted or inaccessible cache directories
    ///
    /// [`LockFile`]: crate::lockfile::LockFile
    pub async fn update(
        &mut self,
        existing: &LockFile,
        deps_to_update: Option<Vec<String>>,
    ) -> Result<LockFile> {
        let mut lockfile = existing.clone();

        // Determine which dependencies to update
        let deps_to_check: HashSet<String> = if let Some(specific) = deps_to_update {
            specific.into_iter().collect()
        } else {
            // Update all dependencies
            self.manifest
                .all_dependencies()
                .iter()
                .map(|(name, _)| (*name).to_string())
                .collect()
        };

        // Resolve updated dependencies (clone to avoid borrow checker issues)
        let deps: Vec<(String, ResourceDependency)> = self
            .manifest
            .all_dependencies()
            .into_iter()
            .map(|(name, dep)| (name.to_string(), dep.clone()))
            .collect();

        // Note: We assume the update command has already called pre_sync_sources
        // during the "Syncing sources" phase, so repositories are already available.
        // We just need to prepare and resolve versions now.

        // Prepare remote groups to resolve versions (reuses pre-synced repos)
        self.prepare_remote_groups(&deps).await?;

        for (name, dep) in deps {
            if !deps_to_check.contains(&name) {
                // Skip this dependency
                continue;
            }

            // Check if this is a pattern dependency
            if dep.is_pattern() {
                // Pattern dependencies resolve to multiple resources
                let entries = self.resolve_pattern_dependency(&name, &dep).await?;

                // Add each resolved entry to the appropriate resource type with deduplication
                let resource_type = self.get_resource_type(&name);
                for entry in entries {
                    match resource_type.as_str() {
                        "agent" => {
                            if let Some(existing) =
                                lockfile.agents.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.agents.push(entry);
                            }
                        }
                        "snippet" => {
                            if let Some(existing) =
                                lockfile.snippets.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.snippets.push(entry);
                            }
                        }
                        "command" => {
                            if let Some(existing) =
                                lockfile.commands.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.commands.push(entry);
                            }
                        }
                        "script" => {
                            if let Some(existing) =
                                lockfile.scripts.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.scripts.push(entry);
                            }
                        }
                        "hook" => {
                            if let Some(existing) =
                                lockfile.hooks.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.hooks.push(entry);
                            }
                        }
                        "mcp-server" => {
                            if let Some(existing) = lockfile
                                .mcp_servers
                                .iter_mut()
                                .find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.mcp_servers.push(entry);
                            }
                        }
                        _ => {
                            if let Some(existing) =
                                lockfile.snippets.iter_mut().find(|e| e.name == entry.name)
                            {
                                *existing = entry;
                            } else {
                                lockfile.snippets.push(entry);
                            }
                        }
                    }
                }
            } else {
                // Regular single dependency
                let entry = self.resolve_dependency(&name, &dep).await?;

                // Use the helper method to add or update the entry
                self.add_or_update_lockfile_entry(&mut lockfile, &name, entry);
            }
        }

        // Progress bar completion is handled by the caller

        // Detect target-path conflicts before finalizing
        self.detect_target_conflicts(&lockfile)?;

        Ok(lockfile)
    }

    /// Checks for redundant dependencies and returns a warning message.
    ///
    /// This method analyzes the manifest for redundant dependencies where
    /// multiple resources reference the same source file but with different
    /// versions or names. Redundancy detection is non-blocking and generates
    /// warnings rather than errors.
    ///
    /// # Redundancy Types Detected
    ///
    /// - **Version Redundancy**: Same resource at different versions
    /// - **Name Redundancy**: Different names for the same resource
    /// - **Mixed Constraints**: Some dependencies use latest, others use specific versions
    ///
    /// # Design Philosophy
    ///
    /// Redundancy detection is advisory rather than prescriptive because:
    /// - Users may intentionally install multiple versions for A/B testing
    /// - Gradual migrations may require temporary redundancy
    /// - Different projects may have different versioning needs
    ///
    /// # Returns
    ///
    /// - `Some(String)`: Warning message if redundancies are detected
    /// - `None`: No redundancies found
    ///
    /// The warning message includes:
    /// - List of redundant resource usages
    /// - Suggested consolidation strategies
    /// - Explanation that redundancy is not an error
    #[must_use]
    pub fn check_redundancies(&self) -> Option<String> {
        let mut detector = RedundancyDetector::new();
        detector.analyze_manifest(&self.manifest);

        let redundancies = detector.detect_redundancies();
        if !redundancies.is_empty() {
            return Some(detector.generate_redundancy_warning(&redundancies));
        }

        None
    }

    /// Analyzes dependencies for redundancies and returns detailed information.
    ///
    /// This method provides programmatic access to redundancy analysis results,
    /// allowing callers to implement custom handling logic or generate
    /// specialized reports.
    ///
    /// # Use Cases
    ///
    /// - **Custom Reporting**: Generate tailored redundancy reports
    /// - **Automated Cleanup**: Implement dependency optimization tools
    /// - **Integration Testing**: Verify redundancy detection logic
    /// - **IDE Extensions**: Provide redundancy warnings in development tools
    ///
    /// # Returns
    ///
    /// A vector of [`Redundancy`] objects, each containing:
    /// - Source file identifier (source:path)
    /// - List of resources using that source file
    /// - Version information for each usage
    ///
    /// # Example Output
    ///
    /// For a manifest with redundant dependencies:
    /// ```text
    /// Redundancy {
    ///     source_file: "community:agents/helper.md",
    ///     usages: [
    ///         ResourceUsage { resource_name: "app-helper", version: Some("v1.0.0") },
    ///         ResourceUsage { resource_name: "tool-helper", version: Some("v2.0.0") },
    ///     ]
    /// }
    /// ```
    ///
    /// [`Redundancy`]: redundancy::Redundancy
    #[must_use]
    pub fn check_redundancies_with_details(&self) -> Vec<redundancy::Redundancy> {
        let mut detector = RedundancyDetector::new();
        detector.analyze_manifest(&self.manifest);
        detector.detect_redundancies()
    }

    /// Verifies that all dependencies can be resolved without performing resolution.
    ///
    /// This method performs a "dry run" validation of the manifest to detect
    /// issues before attempting actual resolution. It's faster than full resolution
    /// since it doesn't clone repositories or resolve specific versions.
    ///
    /// # Validation Steps
    ///
    /// 1. **Redundancy Check**: Analyze and warn about redundant dependencies
    /// 2. **Local Path Validation**: Verify local dependencies exist (for absolute paths)
    /// 3. **Source Validation**: Ensure all referenced sources are defined
    /// 4. **Constraint Validation**: Basic syntax checking of version constraints
    ///
    /// # Validation Scope
    ///
    /// - **Manifest Structure**: Validate TOML structure and required fields
    /// - **Source References**: Ensure all sources used by dependencies exist
    /// - **Local Dependencies**: Check absolute paths exist on filesystem
    /// - **Redundancy Analysis**: Warn about potential optimization opportunities
    ///
    /// # Performance
    ///
    /// Verification is designed to be fast:
    /// - No network operations (doesn't validate remote repositories)
    /// - No Git operations (doesn't check if versions exist)
    /// - Only filesystem access for absolute local paths
    ///
    /// # Parameters
    ///
    /// - `progress`: Optional progress bar for user feedback
    ///
    /// # Returns
    ///
    /// `Ok(())` if all dependencies pass basic validation.
    ///
    /// # Errors
    ///
    /// Verification fails if:
    /// - Local dependencies reference non-existent absolute paths
    /// - Dependencies reference undefined sources
    /// - Manifest structure is invalid or corrupted
    ///
    /// # Note
    ///
    /// Successful verification doesn't guarantee resolution will succeed,
    /// since network issues or missing versions can still cause failures.
    /// Use this method for fast validation before expensive resolution operations.
    pub fn verify(&mut self) -> Result<()> {
        // Check for redundancies and warn (but don't fail)
        if let Some(warning) = self.check_redundancies() {
            eprintln!("{warning}");
        }

        // Then try to resolve all dependencies (clone to avoid borrow checker issues)
        let deps: Vec<(String, ResourceDependency)> = self
            .manifest
            .all_dependencies()
            .into_iter()
            .map(|(name, dep)| (name.to_string(), dep.clone()))
            .collect();
        for (name, dep) in deps {
            if dep.is_local() {
                // Check if local path exists or is relative
                let path = Path::new(dep.get_path());
                if path.is_absolute() && !path.exists() {
                    anyhow::bail!(
                        "Local dependency '{}' not found at: {}",
                        name,
                        path.display()
                    );
                }
            } else {
                // Verify source exists
                let source_name = dep.get_source().ok_or_else(|| CcpmError::ConfigError {
                    message: format!("Dependency '{name}' has no source specified"),
                })?;

                if !self.manifest.sources.contains_key(source_name) {
                    anyhow::bail!(
                        "Dependency '{}' references undefined source: '{}'",
                        name,
                        source_name
                    );
                }
            }
        }

        // Progress bar completion is handled by the caller

        Ok(())
    }
}

/// Extracts the relative path from a resource by removing the resource type directory prefix.
///
/// This function preserves directory structure when installing resources from Git sources
/// by intelligently stripping the resource type directory (e.g., "agents/", "snippets/")
/// from source repository paths. This allows subdirectories within a resource category
/// to be maintained in the installation target, enabling organized source repositories
/// to retain their structure.
///
/// # Path Processing Strategy
///
/// The function implements a **prefix-aware extraction** algorithm:
/// 1. Converts the resource type string to its expected directory name (e.g., "agent" → "agents")
/// 2. Checks if the path starts with this directory name as its first component
/// 3. If matched, returns the path with the first component stripped
/// 4. If not matched, returns the original path unchanged
///
/// This approach ensures that:
/// - Nested directories within a category are preserved (e.g., `agents/ai/helper.md` → `ai/helper.md`)
/// - Paths without the expected prefix remain unchanged (backwards compatibility)
/// - Cross-platform path handling works correctly (Windows and Unix separators)
///
/// # Arguments
///
/// * `path` - The original resource path from the dependency specification (e.g., from a Git repository)
/// * `resource_type` - The resource type string: `"agent"`, `"snippet"`, `"command"`, `"script"`, `"hook"`, or `"mcp-server"`
///
/// # Returns
///
/// A [`PathBuf`] containing:
/// - The path with the resource type prefix removed (if the prefix matched)
/// - The original path unchanged (if no prefix matched)
///
/// # Resource Type Mapping
///
/// | Input Type   | Expected Directory | Example Input Path      | Example Output Path    |
/// |--------------|-------------------|-------------------------|------------------------|
/// | `"agent"`    | `agents/`         | `agents/ai/helper.md`   | `ai/helper.md`         |
/// | `"snippet"`  | `snippets/`       | `snippets/tools/fmt.md` | `tools/fmt.md`         |
/// | `"command"`  | `commands/`       | `commands/build.md`     | `build.md`             |
/// | `"script"`   | `scripts/`        | `scripts/test.sh`       | `test.sh`              |
/// | `"hook"`     | `hooks/`          | `hooks/pre-commit.json` | `pre-commit.json`      |
/// | `"mcp-server"` | `mcp-servers/`  | `mcp-servers/db.json`   | `db.json`              |
///
/// # Examples
///
/// ## Basic Path Extraction
///
/// ```no_run
/// use std::path::{Path, PathBuf};
/// # use ccpm::resolver::extract_relative_path;
///
/// // Resource type prefix is removed
/// let path = Path::new("snippets/directives/thing.md");
/// let result = extract_relative_path(path, "snippet");
/// assert_eq!(result, PathBuf::from("directives/thing.md"));
///
/// // No matching prefix - path unchanged
/// let path = Path::new("directives/thing.md");
/// let result = extract_relative_path(path, "snippet");
/// assert_eq!(result, PathBuf::from("directives/thing.md"));
///
/// // Works with deeply nested directories
/// let path = Path::new("agents/ai/helper.md");
/// let result = extract_relative_path(path, "agent");
/// assert_eq!(result, PathBuf::from("ai/helper.md"));
/// ```
///
/// ## Preserving Directory Structure
///
/// ```no_run
/// # use std::path::{Path, PathBuf};
/// # use ccpm::resolver::extract_relative_path;
///
/// // Multi-level nested directories are fully preserved
/// let path = Path::new("agents/languages/rust/expert.md");
/// let result = extract_relative_path(path, "agent");
/// assert_eq!(result, PathBuf::from("languages/rust/expert.md"));
/// // This will install to: .claude/agents/languages/rust/expert.md
/// ```
///
/// ## Pattern Matching Use Case
///
/// When used with glob patterns like `agents/**/*.md`, this function ensures each
/// matched file preserves its subdirectory structure:
///
/// ```no_run
/// # use std::path::{Path, PathBuf};
/// # use ccpm::resolver::extract_relative_path;
///
/// // Example: glob pattern "agents/**/*.md" matches these paths
/// let matched_paths = vec![
///     "agents/rust/expert.md",
///     "agents/rust/testing.md",
///     "agents/python/async.md",
///     "agents/go/concurrency.md",
/// ];
///
/// for path_str in matched_paths {
///     let path = Path::new(path_str);
///     let relative = extract_relative_path(path, "agent");
///     // Produces: "rust/expert.md", "rust/testing.md", "python/async.md", "go/concurrency.md"
///     // Each installs to: .claude/agents/<relative_path>
/// }
/// ```
///
/// ## Integration with Custom Targets
///
/// Custom targets work in conjunction with relative path extraction:
///
/// ```toml
/// # In ccpm.toml
/// [agents]
/// # Path: agents/rust/expert.md → extract → rust/expert.md
/// # Target: custom → combined → custom/rust/expert.md
/// # Final: .claude/agents/custom/rust/expert.md
/// rust-agents = {
///     source = "community",
///     path = "agents/rust/*.md",
///     target = "custom",
///     version = "v1.0.0"
/// }
/// ```
///
/// # Use Cases
///
/// ## Organized Source Repository
///
/// For a source repository with categorized resources:
/// ```text
/// ccpm-community/
/// ├── agents/
/// │   ├── languages/
/// │   │   ├── rust/
/// │   │   │   ├── expert.md
/// │   │   │   └── testing.md
/// │   │   └── python/
/// │   │       └── async.md
/// │   └── tools/
/// │       └── git-helper.md
/// └── snippets/
///     ├── directives/
///     │   └── custom.md
///     └── templates/
///         └── api.md
/// ```
///
/// After installation, the structure is preserved:
/// ```text
/// .claude/
/// ├── agents/
/// │   ├── languages/
/// │   │   ├── rust/
/// │   │   │   ├── expert.md
/// │   │   │   └── testing.md
/// │   │   └── python/
/// │   │       └── async.md
/// │   └── tools/
/// │       └── git-helper.md
/// └── snippets/
///     ├── directives/
///     │   └── custom.md
///     └── templates/
///         └── api.md
/// ```
///
/// ## Pattern-Based Installation
///
/// Bulk installation with patterns preserves organization:
/// ```toml
/// [agents]
/// # Installs all Rust agents with subdirectory structure intact
/// rust-tools = { source = "community", path = "agents/languages/rust/**/*.md", version = "v1.0.0" }
/// # Results in: .claude/agents/<files from rust/ and subdirectories>
/// ```
///
/// # Implementation Notes
///
/// - Uses path component analysis for cross-platform compatibility
/// - Only examines the first path component to determine prefix match
/// - Empty paths or invalid components are handled gracefully
/// - Unknown resource types cause the path to be returned unchanged
/// - Works with both absolute and relative paths from source repositories
///
/// # Version History
///
/// - **v0.3.18**: Introduced to support relative path preservation during installation
/// - Works in conjunction with updated lockfile `installed_at` path generation
pub fn extract_relative_path(path: &Path, resource_type: &str) -> PathBuf {
    // Convert resource type to expected directory name
    let expected_prefix = match resource_type {
        "agent" => "agents",
        "snippet" => "snippets",
        "command" => "commands",
        "script" => "scripts",
        "hook" => "hooks",
        "mcp-server" => "mcp-servers",
        _ => return path.to_path_buf(),
    };

    // Check if path starts with the expected prefix
    let components: Vec<_> = path.components().collect();
    if let Some(first) = components.first()
        && let std::path::Component::Normal(name) = first
        && name.to_str() == Some(expected_prefix)
    {
        // Skip the first component and collect the rest
        let remaining: PathBuf = components[1..].iter().collect();
        return remaining;
    }

    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolver_new() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        assert_eq!(resolver.cache.get_cache_location(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_resolve_local_dependency() {
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "local-agent".to_string(),
            ResourceDependency::Simple("../agents/local.md".to_string()),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let entry = &lockfile.agents[0];
        assert_eq!(entry.name, "local-agent");
        assert_eq!(entry.path, "../agents/local.md");
        assert!(entry.source.is_none());
        assert!(entry.url.is_none());
    }

    #[test]
    fn test_check_redundancies() {
        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );

        // Add two dependencies with different versions of the same resource
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        let warning = resolver.check_redundancies();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("Redundant dependencies detected"));
    }

    #[tokio::test]
    async fn test_pre_sync_sources() {
        // Skip test if git is not available
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: git not available");
            return;
        }

        // Create a test Git repository with resources
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("test-repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create test files
        std::fs::create_dir_all(repo_dir.join("agents")).unwrap();
        std::fs::write(
            repo_dir.join("agents/test.md"),
            "# Test Agent\n\nTest content",
        )
        .unwrap();

        // Commit files
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create a manifest with a dependency from this source
        let mut manifest = Manifest::new();
        let source_url = format!("file://{}", repo_dir.display());
        manifest.add_source("test-source".to_string(), source_url.clone());

        manifest.add_dependency(
            "test-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test-source".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        // Create resolver with test cache
        let cache_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(cache_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest.clone(), cache);

        // Prepare dependencies for pre-sync
        let deps: Vec<(String, ResourceDependency)> = manifest
            .all_dependencies()
            .into_iter()
            .map(|(name, dep)| (name.to_string(), dep.clone()))
            .collect();

        // Call pre_sync_sources - this should clone the repository and prepare entries
        resolver.pre_sync_sources(&deps).await.unwrap();

        // Verify that entries and repos are prepared
        assert!(
            resolver.version_resolver.pending_count() > 0,
            "Should have entries after pre-sync"
        );

        let bare_repo = resolver.version_resolver.get_bare_repo_path("test-source");
        assert!(bare_repo.is_some(), "Should have bare repo path cached");

        // Verify the repository exists in cache (uses normalized name)
        let cached_repo_path = resolver.cache.get_cache_location().join("sources");

        // The cache normalizes the source name, so we check if any .git directory exists
        let mut found_repo = false;
        if let Ok(entries) = std::fs::read_dir(&cached_repo_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && name.ends_with(".git")
                {
                    found_repo = true;
                    break;
                }
            }
        }
        assert!(found_repo, "Repository should be cloned to cache");

        // Now call resolve_all() - it should work without cloning again
        resolver.version_resolver.resolve_all().await.unwrap();

        // Verify resolution succeeded by checking we have resolved versions
        let all_resolved = resolver.version_resolver.get_all_resolved();
        assert!(
            !all_resolved.is_empty(),
            "Resolution should produce resolved versions"
        );

        // Check that v1.0.0 was resolved to a SHA
        let key = ("test-source".to_string(), "v1.0.0".to_string());
        assert!(
            all_resolved.contains_key(&key),
            "Should have resolved v1.0.0"
        );

        let sha = all_resolved.get(&key).unwrap();
        assert_eq!(sha.len(), 40, "SHA should be 40 characters");
    }

    #[test]
    fn test_verify_missing_source() {
        let mut manifest = Manifest::new();

        // Add dependency without corresponding source
        manifest.add_dependency(
            "remote-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("nonexistent".to_string()),
                path: "agents/test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let result = resolver.verify();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("undefined source"));
    }

    #[test]
    fn test_get_resource_type() {
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("a.md".to_string()),
            true,
        );
        manifest.add_dependency(
            "snippet1".to_string(),
            ResourceDependency::Simple("s.md".to_string()),
            false,
        );
        // Remove dev-snippet1 test as dev concept is removed

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        assert_eq!(resolver.get_resource_type("agent1"), "agent");
        assert_eq!(resolver.get_resource_type("snippet1"), "snippet");
        // Dev concept removed - no longer testing dev-agent1 and dev-snippet1
    }

    #[tokio::test]
    async fn test_resolve_with_source_dependency() {
        let temp_dir = TempDir::new().unwrap();

        // Create a local mock git repository
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&source_dir)
            .output()
            .expect("Failed to initialize git repository");

        // Create the agents directory and test file
        let agents_dir = source_dir.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("test.md"), "# Test Agent").unwrap();

        // Add and commit the file
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create a tag for version
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        let mut manifest = Manifest::new();
        // Use the absolute path directly for better compatibility with tarpaulin
        let source_url = source_dir.display().to_string();
        manifest.add_source("test".to_string(), source_url);
        manifest.add_dependency(
            "remote-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        // This should now succeed with the local repository
        let result = resolver.resolve().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_with_progress() {
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "local".to_string(),
            ResourceDependency::Simple("test.md".to_string()),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);
    }

    #[test]
    fn test_verify_with_progress() {
        let mut manifest = Manifest::new();
        manifest.add_source(
            "test".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.add_dependency(
            "agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let result = resolver.verify();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_with_git_ref() {
        let temp_dir = TempDir::new().unwrap();

        // Create a local mock git repository
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&source_dir)
            .output()
            .expect("Failed to initialize git repository");

        // Configure git
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create the agents directory and test file
        let agents_dir = source_dir.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("test.md"), "# Test Agent").unwrap();

        // Add and commit the file
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create main branch (git may have created master)
        std::process::Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        let mut manifest = Manifest::new();
        // Use the absolute path directly for better compatibility with tarpaulin
        let source_url = source_dir.display().to_string();
        manifest.add_source("test".to_string(), source_url);
        manifest.add_dependency(
            "git-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        // This should now succeed with the local repository
        let result = resolver.resolve().await;
        if let Err(e) = &result {
            eprintln!("Test failed with error: {:#}", e);
        }
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_new_with_global() {
        let manifest = Manifest::new();
        let cache = Cache::new().unwrap();
        let result = DependencyResolver::new_with_global(manifest, cache).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolver_new_default() {
        let manifest = Manifest::new();
        let cache = Cache::new().unwrap();
        let result = DependencyResolver::new(manifest, cache);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_multiple_dependencies() {
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("a1.md".to_string()),
            true,
        );
        manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Simple("a2.md".to_string()),
            true,
        );
        manifest.add_dependency(
            "snippet1".to_string(),
            ResourceDependency::Simple("s1.md".to_string()),
            false,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 2);
        assert_eq!(lockfile.snippets.len(), 1);
    }

    #[test]
    fn test_check_redundancies_no_redundancy() {
        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test1.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );
        manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test2.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        let warning = resolver.check_redundancies();
        assert!(warning.is_none());
    }

    #[test]
    fn test_verify_local_dependency() {
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "local-agent".to_string(),
            ResourceDependency::Simple("../local/agent.md".to_string()),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let result = resolver.verify();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_with_empty_manifest() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 0);
        assert_eq!(lockfile.snippets.len(), 0);
        assert_eq!(lockfile.sources.len(), 0);
    }

    #[tokio::test]
    async fn test_resolve_with_custom_target() {
        let mut manifest = Manifest::new();

        // Add local dependency with custom target
        manifest.add_dependency(
            "custom-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: Some("integrations/custom".to_string()),
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "custom-agent");
        // Verify the custom target is relative to the default agents directory
        assert!(
            agent
                .installed_at
                .contains(".claude/agents/integrations/custom")
        );
        assert_eq!(
            agent.installed_at,
            ".claude/agents/integrations/custom/custom-agent.md"
        );
    }

    #[tokio::test]
    async fn test_resolve_without_custom_target() {
        let mut manifest = Manifest::new();

        // Add local dependency without custom target
        manifest.add_dependency(
            "standard-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "standard-agent");
        // Verify the default target is used
        assert_eq!(agent.installed_at, ".claude/agents/standard-agent.md");
    }

    #[tokio::test]
    async fn test_resolve_with_custom_filename() {
        let mut manifest = Manifest::new();

        // Add local dependency with custom filename
        manifest.add_dependency(
            "my-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: Some("ai-assistant.txt".to_string()),
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "my-agent");
        // Verify the custom filename is used
        assert_eq!(agent.installed_at, ".claude/agents/ai-assistant.txt");
    }

    #[tokio::test]
    async fn test_resolve_with_custom_filename_and_target() {
        let mut manifest = Manifest::new();

        // Add local dependency with both custom filename and target
        manifest.add_dependency(
            "special-tool".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: Some("tools/ai".to_string()),
                filename: Some("assistant.markdown".to_string()),
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "special-tool");
        // Verify both custom target and filename are used
        // Custom target is relative to default agents directory
        assert_eq!(
            agent.installed_at,
            ".claude/agents/tools/ai/assistant.markdown"
        );
    }

    #[tokio::test]
    async fn test_resolve_script_with_custom_filename() {
        let mut manifest = Manifest::new();

        // Add script with custom filename (different extension)
        manifest.add_dependency(
            "analyzer".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: None,
                path: "../scripts/data-analyzer-v3.py".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: Some("analyze.py".to_string()),
            }),
            false, // script (not agent)
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        // Scripts should be in snippets array for now (based on false flag)
        assert_eq!(lockfile.snippets.len(), 1);

        let script = &lockfile.snippets[0];
        assert_eq!(script.name, "analyzer");
        // Verify custom filename is used (with custom extension)
        assert_eq!(script.installed_at, ".claude/ccpm/snippets/analyze.py");
    }

    // ============ NEW TESTS FOR UNCOVERED AREAS ============

    // Disable pattern tests for now as they require changing directory which breaks parallel test safety
    // These tests would need to be rewritten to not use pattern dependencies or
    // the resolver would need to support absolute base paths for pattern resolution

    #[tokio::test]
    async fn test_resolve_pattern_dependency_local() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create local agent files
        let agents_dir = project_dir.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("helper.md"), "# Helper Agent").unwrap();
        std::fs::write(agents_dir.join("assistant.md"), "# Assistant Agent").unwrap();
        std::fs::write(agents_dir.join("tester.md"), "# Tester Agent").unwrap();

        // Create manifest with local pattern dependency
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "local-agents".to_string(),
            ResourceDependency::Simple(format!("{}/agents/*.md", project_dir.display())),
            true,
        );

        // Create resolver and resolve dependencies
        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();

        // Verify all agents were resolved
        assert_eq!(lockfile.agents.len(), 3);
        let agent_names: Vec<String> = lockfile.agents.iter().map(|a| a.name.clone()).collect();
        assert!(agent_names.contains(&"helper".to_string()));
        assert!(agent_names.contains(&"assistant".to_string()));
        assert!(agent_names.contains(&"tester".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_pattern_dependency_remote() {
        let temp_dir = TempDir::new().unwrap();

        // Create a local mock git repository with pattern-matching files
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&source_dir)
            .output()
            .expect("Failed to initialize git repository");

        // Configure git
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create multiple agent files
        let agents_dir = source_dir.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("python-linter.md"), "# Python Linter").unwrap();
        std::fs::write(agents_dir.join("python-formatter.md"), "# Python Formatter").unwrap();
        std::fs::write(agents_dir.join("rust-linter.md"), "# Rust Linter").unwrap();

        // Add and commit
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Add agents"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create a tag
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        let mut manifest = Manifest::new();
        let source_url = source_dir.display().to_string();
        manifest.add_source("test".to_string(), source_url);

        // Add pattern dependency for python agents
        manifest.add_dependency(
            "python-tools".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/python-*.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true, // agents
        );

        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        // Should have resolved to 2 python agents
        assert_eq!(lockfile.agents.len(), 2);

        // Check that both python agents were found
        let agent_names: Vec<String> = lockfile.agents.iter().map(|a| a.name.clone()).collect();
        assert!(agent_names.contains(&"python-linter".to_string()));
        assert!(agent_names.contains(&"python-formatter".to_string()));
        assert!(!agent_names.contains(&"rust-linter".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_pattern_dependency_with_custom_target() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create local agent files
        let agents_dir = project_dir.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("helper.md"), "# Helper Agent").unwrap();
        std::fs::write(agents_dir.join("assistant.md"), "# Assistant Agent").unwrap();

        // Create manifest with local pattern dependency and custom target
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "custom-agents".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: None,
                path: format!("{}/agents/*.md", project_dir.display()),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: Some("custom/agents".to_string()),
                filename: None,
            }),
            true,
        );

        // Create resolver and resolve dependencies
        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();

        // Verify agents were resolved with custom target
        // Custom target is relative to default agents directory
        assert_eq!(lockfile.agents.len(), 2);
        for agent in &lockfile.agents {
            assert!(
                agent
                    .installed_at
                    .starts_with(".claude/agents/custom/agents/")
            );
        }

        let agent_names: Vec<String> = lockfile.agents.iter().map(|a| a.name.clone()).collect();
        assert!(agent_names.contains(&"helper".to_string()));
        assert!(agent_names.contains(&"assistant".to_string()));
    }

    #[tokio::test]
    async fn test_update_specific_dependencies() {
        let temp_dir = TempDir::new().unwrap();

        // Create a local mock git repository
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&source_dir)
            .output()
            .expect("Failed to initialize git repository");

        // Configure git
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create initial files
        let agents_dir = source_dir.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("agent1.md"), "# Agent 1 v1").unwrap();
        std::fs::write(agents_dir.join("agent2.md"), "# Agent 2 v1").unwrap();

        // Initial commit
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Update agent1 and create v2.0.0
        std::fs::write(agents_dir.join("agent1.md"), "# Agent 1 v2").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Update agent1"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["tag", "v2.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        let mut manifest = Manifest::new();
        let source_url = source_dir.display().to_string();
        manifest.add_source("test".to_string(), source_url);

        // Add dependencies - initially both at v1.0.0
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent1.md".to_string(),
                version: Some("v1.0.0".to_string()), // Start with v1.0.0
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );
        manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent2.md".to_string(),
                version: Some("v1.0.0".to_string()), // Start with v1.0.0
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir.clone()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest.clone(), cache);

        // First resolve with v1.0.0 for both
        let initial_lockfile = resolver.resolve().await.unwrap();
        assert_eq!(initial_lockfile.agents.len(), 2);

        // Create a new manifest with agent1 updated to v2.0.0
        let mut updated_manifest = Manifest::new();
        updated_manifest.add_source("test".to_string(), source_dir.display().to_string());
        updated_manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent1.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );
        updated_manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent2.md".to_string(),
                version: Some("v1.0.0".to_string()), // Keep v1.0.0
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        // Now update only agent1
        let cache2 = Cache::with_dir(cache_dir).unwrap();
        let mut resolver2 = DependencyResolver::with_cache(updated_manifest, cache2);
        let updated_lockfile = resolver2
            .update(&initial_lockfile, Some(vec!["agent1".to_string()]))
            .await
            .unwrap();

        // agent1 should be updated to v2.0.0
        let agent1 = updated_lockfile
            .agents
            .iter()
            .find(|a| a.name == "agent1")
            .unwrap();
        assert_eq!(agent1.version.as_ref().unwrap(), "v2.0.0");

        // agent2 should remain at v1.0.0
        let agent2 = updated_lockfile
            .agents
            .iter()
            .find(|a| a.name == "agent2")
            .unwrap();
        assert_eq!(agent2.version.as_ref().unwrap(), "v1.0.0");
    }

    #[tokio::test]
    async fn test_update_all_dependencies() {
        let mut manifest = Manifest::new();
        manifest.add_dependency(
            "local1".to_string(),
            ResourceDependency::Simple("../a1.md".to_string()),
            true,
        );
        manifest.add_dependency(
            "local2".to_string(),
            ResourceDependency::Simple("../a2.md".to_string()),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest.clone(), cache);

        // Initial resolve
        let initial_lockfile = resolver.resolve().await.unwrap();
        assert_eq!(initial_lockfile.agents.len(), 2);

        // Update all (None means update all)
        let cache2 = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver2 = DependencyResolver::with_cache(manifest, cache2);
        let updated_lockfile = resolver2.update(&initial_lockfile, None).await.unwrap();

        // All dependencies should be present
        assert_eq!(updated_lockfile.agents.len(), 2);
    }

    #[tokio::test]
    async fn test_resolve_hooks_resource_type() {
        let mut manifest = Manifest::new();

        // Add hook dependencies
        manifest.hooks.insert(
            "pre-commit".to_string(),
            ResourceDependency::Simple("../hooks/pre-commit.json".to_string()),
        );
        manifest.hooks.insert(
            "post-commit".to_string(),
            ResourceDependency::Simple("../hooks/post-commit.json".to_string()),
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.hooks.len(), 2);

        // Check that hooks are installed to the correct location
        for hook in &lockfile.hooks {
            assert!(hook.installed_at.contains(".claude/ccpm/hooks/"));
            assert!(hook.installed_at.ends_with(".json"));
        }
    }

    #[tokio::test]
    async fn test_resolve_scripts_resource_type() {
        let mut manifest = Manifest::new();

        // Add script dependencies
        manifest.scripts.insert(
            "build".to_string(),
            ResourceDependency::Simple("../scripts/build.sh".to_string()),
        );
        manifest.scripts.insert(
            "test".to_string(),
            ResourceDependency::Simple("../scripts/test.py".to_string()),
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.scripts.len(), 2);

        // Check that scripts maintain their extensions
        let build_script = lockfile.scripts.iter().find(|s| s.name == "build").unwrap();
        assert!(build_script.installed_at.ends_with("build.sh"));

        let test_script = lockfile.scripts.iter().find(|s| s.name == "test").unwrap();
        assert!(test_script.installed_at.ends_with("test.py"));
    }

    #[tokio::test]
    async fn test_resolve_mcp_servers_resource_type() {
        let mut manifest = Manifest::new();

        // Add MCP server dependencies
        manifest.mcp_servers.insert(
            "filesystem".to_string(),
            ResourceDependency::Simple("../mcp/filesystem.json".to_string()),
        );
        manifest.mcp_servers.insert(
            "database".to_string(),
            ResourceDependency::Simple("../mcp/database.json".to_string()),
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.mcp_servers.len(), 2);

        // Check that MCP servers are tracked correctly
        for server in &lockfile.mcp_servers {
            assert!(server.installed_at.contains(".claude/ccpm/mcp-servers/"));
            assert!(server.installed_at.ends_with(".json"));
        }
    }

    #[tokio::test]
    async fn test_resolve_commands_resource_type() {
        let mut manifest = Manifest::new();

        // Add command dependencies
        manifest.commands.insert(
            "deploy".to_string(),
            ResourceDependency::Simple("../commands/deploy.md".to_string()),
        );
        manifest.commands.insert(
            "lint".to_string(),
            ResourceDependency::Simple("../commands/lint.md".to_string()),
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.commands.len(), 2);

        // Check that commands are installed to the correct location
        for command in &lockfile.commands {
            assert!(command.installed_at.contains(".claude/commands/"));
            assert!(command.installed_at.ends_with(".md"));
        }
    }

    #[tokio::test]
    async fn test_checkout_version_with_constraint() {
        let temp_dir = TempDir::new().unwrap();

        // Create a git repo with multiple version tags
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&source_dir)
            .output()
            .expect("Failed to initialize git repository");

        // Configure git
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create file and make commits with version tags
        let test_file = source_dir.join("test.txt");

        // v1.0.0
        std::fs::write(&test_file, "v1.0.0").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "v1.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // v1.1.0
        std::fs::write(&test_file, "v1.1.0").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "v1.1.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["tag", "v1.1.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // v1.2.0
        std::fs::write(&test_file, "v1.2.0").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "v1.2.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["tag", "v1.2.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // v2.0.0
        std::fs::write(&test_file, "v2.0.0").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "v2.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["tag", "v2.0.0"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        let mut manifest = Manifest::new();
        let source_url = source_dir.display().to_string();
        manifest.add_source("test".to_string(), source_url);

        // Test version constraint resolution (^1.0.0 should resolve to 1.2.0)
        manifest.add_dependency(
            "constrained-dep".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "test.txt".to_string(),
                version: Some("^1.0.0".to_string()), // Constraint: compatible with 1.x.x
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        // Should resolve to highest 1.x version (1.2.0), not 2.0.0
        assert_eq!(agent.version.as_ref().unwrap(), "v1.2.0");
    }

    #[tokio::test]
    async fn test_verify_absolute_path_error() {
        let mut manifest = Manifest::new();

        // Add dependency with non-existent absolute path
        // Use platform-specific absolute path
        let nonexistent_path = if cfg!(windows) {
            "C:\\nonexistent\\path\\agent.md"
        } else {
            "/nonexistent/path/agent.md"
        };

        manifest.add_dependency(
            "missing-agent".to_string(),
            ResourceDependency::Simple(nonexistent_path.to_string()),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let result = resolver.verify();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_resolve_pattern_dependency_error() {
        let mut manifest = Manifest::new();

        // Add pattern dependency without source (should error in resolve_pattern_dependency)
        manifest.add_dependency(
            "pattern-dep".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("nonexistent".to_string()),
                path: "agents/*.md".to_string(), // Pattern path
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let result = resolver.resolve().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_checkout_version_with_branch() {
        let temp_dir = TempDir::new().unwrap();

        // Create a git repo with a branch
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&source_dir)
            .output()
            .expect("Failed to initialize git repository");

        // Configure git
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create initial commit on main
        let test_file = source_dir.join("test.txt");
        std::fs::write(&test_file, "main").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create and switch to develop branch
        std::process::Command::new("git")
            .args(["checkout", "-b", "develop"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Make a commit on develop
        std::fs::write(&test_file, "develop").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Develop commit"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        let mut manifest = Manifest::new();
        let source_url = source_dir.display().to_string();
        manifest.add_source("test".to_string(), source_url);

        // Test branch checkout
        manifest.add_dependency(
            "branch-dep".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "test.txt".to_string(),
                version: Some("develop".to_string()), // Branch name
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        // Should have resolved to develop branch
        let agent = &lockfile.agents[0];
        assert!(agent.resolved_commit.is_some());
    }

    #[tokio::test]
    async fn test_checkout_version_with_commit_hash() {
        let temp_dir = TempDir::new().unwrap();

        // Create a git repo
        let source_dir = temp_dir.path().join("test-source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&source_dir)
            .output()
            .expect("Failed to initialize git repository");

        // Configure git
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Create a commit
        let test_file = source_dir.join("test.txt");
        std::fs::write(&test_file, "content").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Test commit"])
            .current_dir(&source_dir)
            .output()
            .unwrap();

        // Get the commit hash
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&source_dir)
            .output()
            .unwrap();
        let commit_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let mut manifest = Manifest::new();
        let source_url = source_dir.display().to_string();
        manifest.add_source("test".to_string(), source_url);

        // Test commit hash checkout (use first 7 chars for short hash)
        manifest.add_dependency(
            "commit-dep".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "test.txt".to_string(),
                version: Some(commit_hash[..7].to_string()), // Short commit hash
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert!(agent.resolved_commit.is_some());
        // The resolved commit should start with our short hash
        assert!(
            agent
                .resolved_commit
                .as_ref()
                .unwrap()
                .starts_with(&commit_hash[..7])
        );
    }

    #[test]
    fn test_check_redundancies_with_details() {
        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );

        // Add redundant dependencies
        manifest.add_dependency(
            "helper-v1".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/helper.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        manifest.add_dependency(
            "helper-v2".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/helper.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        let redundancies = resolver.check_redundancies_with_details();
        assert!(!redundancies.is_empty());

        // Should have detected the redundancy
        let redundancy = &redundancies[0];
        assert_eq!(redundancy.source_file, "official:agents/helper.md");
        assert_eq!(redundancy.usages.len(), 2);
    }

    #[tokio::test]
    async fn test_mixed_resource_types() {
        let mut manifest = Manifest::new();

        // Add various resource types
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Simple("../agents/a1.md".to_string()),
            true,
        );

        manifest.scripts.insert(
            "build".to_string(),
            ResourceDependency::Simple("../scripts/build.sh".to_string()),
        );

        manifest.hooks.insert(
            "pre-commit".to_string(),
            ResourceDependency::Simple("../hooks/pre-commit.json".to_string()),
        );

        manifest.commands.insert(
            "deploy".to_string(),
            ResourceDependency::Simple("../commands/deploy.md".to_string()),
        );

        manifest.mcp_servers.insert(
            "filesystem".to_string(),
            ResourceDependency::Simple("../mcp/filesystem.json".to_string()),
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();

        // Check all resource types are resolved
        assert_eq!(lockfile.agents.len(), 1);
        assert_eq!(lockfile.scripts.len(), 1);
        assert_eq!(lockfile.hooks.len(), 1);
        assert_eq!(lockfile.commands.len(), 1);
        assert_eq!(lockfile.mcp_servers.len(), 1);
    }
}
