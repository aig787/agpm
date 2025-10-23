//! Dependency resolution and conflict detection for AGPM.
//!
//! This module implements the core dependency resolution algorithm that transforms
//! manifest dependencies into locked versions. It handles version constraint solving,
//! conflict detection, transitive dependency resolution,
//! parallel source synchronization, and relative path preservation during installation.
//!
//! # Module Organization
//!
//! The resolver is organized into focused submodules:
//!
//! - [`dependency_graph`] - Graph-based transitive dependency resolution with cycle detection
//! - [`version_resolution`] - Version constraint solving and conflict detection
//! - [`version_resolver`] - Centralized batch SHA resolution for Git references
//! - [`dependency_helpers`] - Dependency processing and validation utilities
//! - [`install_path_resolver`] - Installation path calculation and conflict detection
//! - [`path_helpers`] - Path manipulation and normalization utilities
//! - `lockfile_builder` - Lockfile entry generation and metadata extraction
//! - `lockfile_operations` - Lockfile manipulation and conflict detection
//! - `pattern_expander` - Glob pattern expansion for bulk dependency installation
//! - `transitive_resolver` - Transitive dependency extraction and resolution
//! - `worktree_manager` - Git worktree creation and SHA-based deduplication
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
//! 5. **Path Processing**: Preserve directory structure by using paths directly from dependencies
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
//! - **Source Caching**: Git repositories are cached globally in `~/.agpm/cache/`
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
//! use agpm_cli::resolver::DependencyResolver;
//! use agpm_cli::manifest::Manifest;
//! use agpm_cli::cache::Cache;
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let manifest = Manifest::load(Path::new("agpm.toml"))?;
//! let cache = Cache::new()?;
//! let mut resolver = DependencyResolver::new_with_global(manifest.clone(), cache).await?;
//!
//! // Get all dependencies from manifest
//! let deps: Vec<(String, agpm_cli::manifest::ResourceDependency)> = manifest
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
//! # use agpm_cli::resolver::DependencyResolver;
//! # use agpm_cli::manifest::Manifest;
//! # use agpm_cli::cache::Cache;
//! # use agpm_cli::lockfile::LockFile;
//! # use std::path::Path;
//! # async fn update_example() -> anyhow::Result<()> {
//! let manifest = Manifest::load(Path::new("agpm.toml"))?;
//! let mut lockfile = LockFile::load(Path::new("agpm.lock"))?;
//! let cache = Cache::new()?;
//! let mut resolver = DependencyResolver::with_cache(manifest.clone(), cache);
//!
//! // Get dependencies to update
//! let deps: Vec<(String, agpm_cli::manifest::ResourceDependency)> = manifest
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
//! lockfile.save(Path::new("agpm.lock"))?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Incremental Updates
//! ```rust,no_run
//! use agpm_cli::resolver::DependencyResolver;
//! use agpm_cli::lockfile::LockFile;
//! use agpm_cli::cache::Cache;
//! use std::path::Path;
//!
//! # async fn update_example() -> anyhow::Result<()> {
//! let existing = LockFile::load("agpm.lock".as_ref())?;
//! let manifest = agpm_cli::manifest::Manifest::load("agpm.toml".as_ref())?;
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

pub mod dependency_graph;
pub mod version_resolution;
pub mod version_resolver;

use crate::cache::Cache;
use crate::core::{AgpmError, OperationContext, ResourceType};
use crate::git::GitRepo;
use crate::lockfile::{LockFile, LockedResource, LockedResourceBuilder};
use crate::manifest::{Manifest, ResourceDependency, json_value_to_toml};
use crate::metadata::MetadataExtractor;
use crate::source::SourceManager;
use crate::utils::{compute_relative_install_path, normalize_path, normalize_path_for_storage};
use crate::version::conflict::ConflictDetector;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use self::dependency_graph::{DependencyGraph, DependencyNode};
use self::version_resolver::VersionResolver;

// Module declarations
pub mod dependency_helpers;
pub mod install_path_resolver;
mod lockfile_builder;
mod lockfile_operations;
pub mod path_helpers;
mod pattern_expander;
mod transitive_resolver;
mod worktree_manager;

#[cfg(test)]
mod tests;

// Re-export public types and functions
pub use lockfile_builder::LockfileBuilder;
pub use pattern_expander::{expand_pattern_to_concrete_deps, generate_dependency_name};
pub use transitive_resolver::resolve_transitive_dependencies;
pub use worktree_manager::{PreparedSourceVersion, WorktreeManager};

/// Key for identifying unique dependencies in maps and sets.
///
/// Combines resource type, name, source, and tool to uniquely identify a dependency.
/// This prevents collisions between same-named resources from different sources or
/// using different tools.
type DependencyKey = (ResourceType, String, Option<String>, Option<String>);

/// Determines if a path is file-relative (starts with `./` or `../`, or is a bare filename).
///
/// File-relative paths are resolved relative to the containing file's directory.
/// This includes both explicit relative paths (`./helper.md`, `../utils.md`)
/// and bare filenames (`helper.md`) which are automatically treated as file-relative.
///
/// # Examples
///
/// ```
/// # use agpm_cli::resolver::is_file_relative_path;
/// assert!(is_file_relative_path("./helper.md"));
/// assert!(is_file_relative_path("../utils.md"));
/// assert!(is_file_relative_path("helper.md"));  // Bare filename
/// assert!(!is_file_relative_path("agents/helper.md"));  // Repo-relative
/// ```
pub fn is_file_relative_path(path: &str) -> bool {
    let p = Path::new(path);
    // Check if path starts with ./ or ../, or is a bare filename (single component)
    path.starts_with("./") || path.starts_with("../") || p.components().count() == 1
}

/// Normalizes a bare filename to a file-relative path by prepending the current directory.
///
/// If the path is already explicitly relative (starts with `./` or `../`),
/// returns it unchanged. Otherwise, joins it with the current directory (`.`)
/// to make it unambiguous.
///
/// # Examples
///
/// ```
/// # use agpm_cli::resolver::normalize_bare_filename;
/// # use std::path::Path;
/// // On Unix: "./helper.md", on Windows: ".\\helper.md"
/// assert_eq!(
///     normalize_bare_filename("helper.md"),
///     Path::new(".").join("helper.md").to_string_lossy().to_string()
/// );
/// assert_eq!(normalize_bare_filename("./helper.md"), "./helper.md");
/// assert_eq!(normalize_bare_filename("../utils.md"), "../utils.md");
/// ```
pub fn normalize_bare_filename(path: &str) -> String {
    let p = Path::new(path);
    // If it's a bare filename (single component), join with current directory
    if p.components().count() == 1 {
        Path::new(".").join(path).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

/// Extract meaningful path structure from a dependency path.
///
/// This function handles three cases:
/// 1. Relative paths with `../` - strips all parent directory components
/// 2. Absolute paths - resolves `..` components and strips root/prefix
/// 3. Clean relative paths - uses as-is with normalized separators
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// # use agpm_cli::resolver::extract_meaningful_path;
///
/// // Relative paths with parent navigation
/// assert_eq!(extract_meaningful_path(Path::new("../../snippets/dir/file.md")), "snippets/dir/file.md");
///
/// // Absolute paths (root stripped, .. resolved) - Unix-style path
/// #[cfg(unix)]
/// assert_eq!(extract_meaningful_path(Path::new("/tmp/foo/../bar/agent.md")), "tmp/bar/agent.md");
///
/// // Absolute paths (root stripped, .. resolved) - Windows-style path
/// #[cfg(windows)]
/// assert_eq!(extract_meaningful_path(Path::new("C:\\tmp\\foo\\..\\bar\\agent.md")), "tmp/bar/agent.md");
///
/// // Clean relative paths
/// assert_eq!(extract_meaningful_path(Path::new("agents/test.md")), "agents/test.md");
/// ```
pub fn extract_meaningful_path(path: &Path) -> String {
    let components: Vec<_> = path.components().collect();

    if path.is_absolute() {
        // Case 2: Absolute path - resolve ".." components first, then strip root
        let mut resolved = Vec::new();

        for component in components.iter() {
            match component {
                std::path::Component::Normal(name) => {
                    resolved.push(name.to_str().unwrap_or(""));
                }
                std::path::Component::ParentDir => {
                    // Pop the last component if there is one
                    resolved.pop();
                }
                // Skip RootDir, Prefix, and CurDir
                _ => {}
            }
        }

        resolved.join("/")
    } else if components.iter().any(|c| matches!(c, std::path::Component::ParentDir)) {
        // Case 1: Relative path with "../" - skip all parent components
        let start_idx = components
            .iter()
            .position(|c| matches!(c, std::path::Component::Normal(_)))
            .unwrap_or(0);

        components[start_idx..]
            .iter()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/")
    } else {
        // Case 3: Clean relative path - use as-is
        path.to_str().unwrap_or("").replace('\\', "/") // Normalize to forward slashes
    }
}

/// Type alias for resource lookup key: (`ResourceType`, name, source).
///
/// Used internally by the resolver to uniquely identify resources across different sources.
/// This enables precise lookups when multiple resources share the same name but come from
/// different sources (common with transitive dependencies).
///
/// # Components
///
/// * `ResourceType` - The type of resource (Agent, Snippet, Command, Script, Hook, `McpServer`)
/// * `String` - The resource name as defined in the manifest
/// * `Option<String>` - The source name (None for local resources without a source)
///
/// # Usage
///
/// This key is used in `HashMap<ResourceKey, ResourceInfo>` to build a complete map
/// of all resources in the lockfile for efficient cross-source dependency resolution.
type ResourceKey = (crate::core::ResourceType, String, Option<String>);

/// Type alias for resource information: (source, version).
///
/// Stores the source and version information for a resolved resource. Used in conjunction
/// with [`ResourceKey`] to enable efficient lookups during transitive dependency resolution.
///
/// # Components
///
/// * First `Option<String>` - The source name (None for local resources)
/// * Second `Option<String>` - The version constraint (None for unpinned resources)
///
/// # Usage
///
/// Paired with [`ResourceKey`] in a `HashMap<ResourceKey, ResourceInfo>` to store
/// resource metadata for cross-source dependency resolution. This allows the resolver
/// to construct fully-qualified dependency references like `source:type/name:version`.
type ResourceInfo = (Option<String>, Option<String>);

/// Read a file with retry logic to handle cross-process filesystem cache coherency issues.
///
/// This function wraps `std::fs::read_to_string` with retry logic to handle cases where
/// files created by Git subprocesses are not immediately visible to the parent Rust process
/// due to filesystem cache propagation delays. This is particularly important in CI
/// environments with network-attached storage where cache coherency delays can be significant.
///
/// # Arguments
///
/// * `path` - The file path to read
///
/// # Returns
///
/// Returns the file content as a `String`, or an error if the file cannot be read after retries.
///
/// # Retry Strategy
///
/// - Initial delay: 10ms
/// - Max delay: 500ms (capped exponential backoff)
/// - Max attempts: 10
/// - Total max time: ~5 seconds
///
/// Only `NotFound` errors are retried, as these indicate cache coherency issues.
/// Other errors (permissions, I/O errors) fail immediately.
fn read_with_cache_retry_sync(path: &Path) -> Result<String> {
    use std::io;
    use std::thread;

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 10;

    loop {
        match std::fs::read_to_string(path) {
            Ok(content) => return Ok(content),
            Err(e) if e.kind() == io::ErrorKind::NotFound && attempts < MAX_ATTEMPTS => {
                attempts += 1;
                // Exponential backoff: 10ms, 20ms, 40ms, 80ms, 160ms, 320ms, 500ms (capped)
                let delay_ms = std::cmp::min(10 * (1 << attempts), 500);
                let delay = Duration::from_millis(delay_ms);

                tracing::debug!(
                    "File not yet visible (attempt {}/{}): {}",
                    attempts,
                    MAX_ATTEMPTS,
                    path.display()
                );

                thread::sleep(delay);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to read file: {}", path.display()).context(e));
            }
        }
    }
}

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
    version_resolver: VersionResolver,
    /// Dependency graph tracking which resources depend on which others.
    ///
    /// Maps from (`resource_type`, name, source, tool) to a list of dependencies in the format
    /// "`resource_type/name`". This is populated during transitive dependency
    /// resolution and used to fill the dependencies field in `LockedResource` entries.
    /// The source and tool are included to prevent cross-source and cross-tool dependency contamination.
    dependency_map: HashMap<DependencyKey, Vec<String>>,
    /// Conflict detector for identifying version conflicts.
    ///
    /// Tracks version requirements across all dependencies (direct and transitive)
    /// and detects incompatible version constraints before lockfile creation.
    conflict_detector: ConflictDetector,
    /// Maps resource names to their original pattern alias for pattern-expanded dependencies.
    ///
    /// When a pattern dependency (e.g., `all-helpers = { path = "agents/helpers/*.md" }`)
    /// expands to multiple concrete dependencies (helper-alpha, helper-beta, etc.), this
    /// map tracks which pattern alias each expanded resource came from. This enables
    /// patches defined under the pattern alias to be correctly applied to all matched resources.
    ///
    /// Example: If "all-helpers" expands to "helper-alpha" and "helper-beta", this map contains:
    /// - (ResourceType::Agent, "helper-alpha") -> "all-helpers"
    /// - (ResourceType::Agent, "helper-beta") -> "all-helpers"
    ///
    /// The key includes ResourceType to prevent collisions when different resource types
    /// have the same concrete name (e.g., agents/deploy.md and commands/deploy.md).
    pattern_alias_map: HashMap<(ResourceType, String), String>,
    /// Maps transitive dependency keys to their custom names from DependencySpec.name field.
    ///
    /// When a transitive dependency has an explicit `name` field in the parent resource's
    /// dependency declaration, this map stores that custom name. It's used as the
    /// manifest_alias when creating the LockedResource entry, allowing template variables
    /// to use the custom name (e.g., `{{ agpm.deps.snippets.base.content }}`) even though
    /// the internal resource name is path-based for uniqueness.
    ///
    /// Key: (ResourceType, internal_name, source, tool)
    /// Value: custom name from DependencySpec.name
    transitive_custom_names: HashMap<DependencyKey, String>,
    /// Optional operation context for warning deduplication.
    ///
    /// When provided, this context is used to deduplicate warning messages
    /// during dependency resolution and transitive dependency extraction.
    /// This prevents duplicate warnings when the same file is processed
    /// multiple times (e.g., during transitive dependency resolution).
    ///
    /// Uses `Arc` for efficient sharing across async operations without cloning the entire context.
    operation_context: Option<Arc<OperationContext>>,
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
    /// Generate unique key for grouping dependencies by source and version.
    fn group_key(source: &str, version: &str) -> String {
        lockfile_operations::group_key(source, version)
    }

    /// Build complete merged template variable context for a dependency.
    ///
    /// Combines global project config with dependency-specific template_vars overrides.
    fn build_merged_template_vars(
        &self,
        dep: &crate::manifest::ResourceDependency,
    ) -> serde_json::Value {
        lockfile_operations::build_merged_template_vars(&self.manifest, dep)
    }

    /// Add or update a resource entry in the lockfile.
    ///
    /// Uses upsert behavior - existing entries with matching name, source, and tool are updated;
    /// otherwise a new entry is added.
    fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        _name: &str,
        entry: LockedResource,
    ) {
        lockfile_operations::add_or_update_lockfile_entry(lockfile, entry)
    }

    /// Remove lockfile entries for dependencies no longer in the manifest.
    ///
    /// Removes both direct dependencies and their transitive children to maintain consistency.
    fn remove_stale_manifest_entries(&self, lockfile: &mut LockFile) {
        lockfile_operations::remove_stale_manifest_entries(&self.manifest, lockfile)
    }

    /// Remove lockfile entries for manifest dependencies being re-resolved during update.
    ///
    /// Handles source changes by removing old entries and their transitive dependencies.
    fn remove_manifest_entries_for_update(
        &self,
        lockfile: &mut LockFile,
        manifest_keys: &HashSet<String>,
    ) {
        lockfile_operations::remove_manifest_entries_for_update(lockfile, manifest_keys)
    }

    /// Detect conflicts where multiple dependencies resolve to the same installation path.
    ///
    /// Returns an error if dependencies with different content would overwrite each other.
    fn detect_target_conflicts(&self, lockfile: &LockFile) -> Result<()> {
        lockfile_operations::detect_target_conflicts(lockfile)
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
    /// # use agpm_cli::resolver::DependencyResolver;
    /// # use agpm_cli::manifest::{Manifest, ResourceDependency};
    /// # use agpm_cli::cache::Cache;
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
    /// This method is part of AGPM's two-phase resolution architecture:
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
                    self.source_manager.get_source_url(source_name).ok_or_else(|| {
                        AgpmError::SourceNotFound {
                            name: source_name.to_string(),
                        }
                    })?;

                let version = dep.get_version();

                // Add to version resolver for batch syncing
                self.version_resolver.add_version(source_name, &source_url, version);
            }
        }

        // Pre-sync all sources (performs Git operations)
        self.version_resolver.pre_sync_sources().await.context("Failed to sync sources")?;

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
    /// use agpm_cli::resolver::DependencyResolver;
    /// use agpm_cli::cache::Cache;
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
            .with_context(|| format!("Failed to list tags from repository at {repo_path:?}"))
    }

    /// Creates worktrees for all resolved SHAs in parallel.
    ///
    /// This helper method is part of AGPM's SHA-based worktree architecture, processing
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
                        self.source_manager.get_source_url(source_name).ok_or_else(|| {
                            AgpmError::SourceNotFound {
                                name: source_name.to_string(),
                            }
                        })?;

                    let version = dep.get_version();

                    // Add to version resolver for batch resolution
                    self.version_resolver.add_version(source_name, &source_url, version);
                }
            }

            // If entries were rebuilt, we need to sync sources first
            self.version_resolver.pre_sync_sources().await.context("Failed to sync sources")?;
        }

        // Now resolve all versions to SHAs
        self.version_resolver.resolve_all().await.context("Failed to resolve versions to SHAs")?;

        // Step 3: Create worktrees for all resolved SHAs in parallel
        let prepared_versions = self.create_worktrees_for_resolved_versions().await?;

        // Store the prepared versions
        self.prepared_versions.extend(prepared_versions);

        // Step 4: Handle local sources separately (they don't need worktrees)
        for (_, dep) in deps {
            if let Some(source_name) = dep.get_source() {
                let source_url =
                    self.source_manager.get_source_url(source_name).ok_or_else(|| {
                        AgpmError::SourceNotFound {
                            name: source_name.to_string(),
                        }
                    })?;

                // Check if this is a local directory source
                if crate::utils::is_local_path(&source_url) {
                    let version_key = dep.get_version().unwrap_or("HEAD");
                    let group_key = Self::group_key(source_name, version_key);

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

        // Phase completion is handled by the caller

        Ok(())
    }

    /// Creates a new resolver using only manifest-defined sources.
    ///
    /// This constructor creates a resolver that only considers sources defined
    /// in the manifest file. Global configuration sources from `~/.agpm/config.toml`
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
        Self::with_context(manifest, cache, None)
    }

    /// Creates a new dependency resolver with optional operation context.
    ///
    /// This is the preferred constructor for new code that wants to use
    /// operation-scoped warning deduplication.
    ///
    /// # Arguments
    ///
    /// * `manifest` - The AGPM manifest containing dependency specifications
    /// * `cache` - The cache for Git repositories and worktrees
    /// * `context` - Optional operation context for warning deduplication
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::core::OperationContext;
    /// use agpm_cli::resolver::DependencyResolver;
    /// use agpm_cli::manifest::Manifest;
    /// use agpm_cli::cache::Cache;
    /// use std::sync::Arc;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::default();
    /// let cache = Cache::new()?;
    /// let ctx = Arc::new(OperationContext::new());
    ///
    /// let resolver = DependencyResolver::with_context(
    ///     manifest,
    ///     cache,
    ///     Some(ctx)
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_context(
        manifest: Manifest,
        cache: Cache,
        context: Option<Arc<OperationContext>>,
    ) -> Result<Self> {
        let source_manager = SourceManager::from_manifest(&manifest)?;
        let version_resolver = VersionResolver::new(cache.clone());

        Ok(Self {
            manifest,
            source_manager,
            cache,
            prepared_versions: HashMap::new(),
            version_resolver,
            dependency_map: HashMap::new(),
            conflict_detector: ConflictDetector::new(),
            pattern_alias_map: HashMap::new(),
            transitive_custom_names: HashMap::new(),
            operation_context: context,
        })
    }

    /// Creates a new resolver with global configuration support.
    ///
    /// This is the recommended constructor for most use cases. It loads both
    /// manifest sources and global sources from `~/.agpm/config.toml`, enabling
    /// access to private repositories with authentication tokens.
    ///
    /// # Source Priority
    ///
    /// When sources are defined in both locations:
    /// 1. **Global sources** (from `~/.agpm/config.toml`) are loaded first
    /// 2. **Local sources** (from `agpm.toml`) can override global sources
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
            dependency_map: HashMap::new(),
            conflict_detector: ConflictDetector::new(),
            pattern_alias_map: HashMap::new(),
            transitive_custom_names: HashMap::new(),
            operation_context: None,
        })
    }

    /// Creates a new resolver with a custom cache.
    ///
    /// This constructor is primarily used for testing and specialized deployments
    /// where the default cache location (`~/.agpm/cache/`) is not suitable.
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
            dependency_map: HashMap::new(),
            conflict_detector: ConflictDetector::new(),
            pattern_alias_map: HashMap::new(),
            transitive_custom_names: HashMap::new(),
            operation_context: None,
        }
    }

    /// Set the operation context for warning deduplication.
    ///
    /// This allows setting the context after resolver creation, which is useful
    /// when using constructors like `new_with_global()` that don't accept a context parameter.
    ///
    /// # Arguments
    ///
    /// * `context` - The operation context to use for warning deduplication
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::core::OperationContext;
    /// use agpm_cli::resolver::DependencyResolver;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// # let manifest = agpm_cli::manifest::Manifest::default();
    /// # let cache = agpm_cli::cache::Cache::new()?;
    /// let ctx = Arc::new(OperationContext::new());
    /// let mut resolver = DependencyResolver::new_with_global(manifest, cache).await?;
    /// resolver.set_operation_context(ctx);
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_operation_context(&mut self, context: Arc<OperationContext>) {
        self.operation_context = Some(context);
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
    ///
    /// Resolve transitive dependencies by extracting metadata from resource files.
    ///
    /// This method builds a dependency graph by:
    /// 1. Starting with direct manifest dependencies
    /// 2. Extracting metadata from each resolved resource
    /// 3. Adding discovered transitive dependencies to the graph
    /// 4. Checking for circular dependencies
    /// 5. Returning all dependencies in topological order
    ///
    /// # Cross-Source Handling
    ///
    /// Resources are uniquely identified by `(ResourceType, name, source)`, allowing multiple
    /// resources with the same name from different sources to coexist. During topological
    /// ordering, all resources with matching name and type are included, even if they come
    /// from different sources.
    ///
    /// # Arguments
    /// * `base_deps` - The initial dependencies from the manifest
    /// * `enable_transitive` - Whether to resolve transitive dependencies
    ///
    /// # Returns
    /// A vector of all dependencies (direct + transitive) in topological order, including
    /// all same-named resources from different sources. Each tuple contains:
    /// - Dependency name
    /// - Resource dependency details
    /// - Resource type (to avoid ambiguity when same name exists across types)
    async fn resolve_transitive_dependencies(
        &mut self,
        base_deps: &[(String, ResourceDependency, crate::core::ResourceType)],
        enable_transitive: bool,
    ) -> Result<Vec<(String, ResourceDependency, crate::core::ResourceType)>> {
        // Clear state from any previous resolution to prevent stale data
        // IMPORTANT: Must clear before early return to avoid contaminating non-transitive runs
        self.dependency_map.clear();
        // NOTE: Don't reset conflict_detector here - it was already populated with direct dependencies

        if !enable_transitive {
            // If transitive resolution is disabled, return base dependencies as-is
            // with their resource types already threaded from the manifest
            return Ok(base_deps.to_vec());
        }

        let mut graph = DependencyGraph::new();
        // Use (resource_type, name, source, tool) as key to distinguish same-named resources from different sources and tools
        let mut all_deps: HashMap<
            (crate::core::ResourceType, String, Option<String>, Option<String>),
            ResourceDependency,
        > = HashMap::new();
        let mut processed: HashSet<(
            crate::core::ResourceType,
            String,
            Option<String>,
            Option<String>,
        )> = HashSet::new();
        let mut queue: Vec<(String, ResourceDependency, Option<crate::core::ResourceType>)> =
            Vec::new();

        // Add initial dependencies to queue with their threaded types
        for (name, dep, resource_type) in base_deps {
            let source = dep.get_source().map(std::string::ToString::to_string);
            let tool = dep.get_tool().map(std::string::ToString::to_string);
            queue.push((name.clone(), dep.clone(), Some(*resource_type)));
            all_deps.insert((*resource_type, name.clone(), source, tool), dep.clone());
        }

        // Process queue to discover transitive dependencies
        while let Some((name, dep, resource_type)) = queue.pop() {
            let source = dep.get_source().map(std::string::ToString::to_string);
            let tool = dep.get_tool().map(std::string::ToString::to_string);
            let resource_type =
                resource_type.expect("resource_type should always be threaded through queue");
            let key = (resource_type, name.clone(), source.clone(), tool.clone());

            tracing::debug!(
                "[QUEUE_POP] Popped from queue: '{}' (type: {:?}, source: {:?}, tool: {:?})",
                name,
                resource_type,
                source,
                tool
            );

            // Check if this queue entry is stale (superseded by conflict resolution)
            // IMPORTANT: This must come BEFORE the processed check so that conflict-resolved
            // entries can be reprocessed even if an older version was already processed.
            // If all_deps has a different version than what we popped, skip this entry
            if let Some(current_dep) = all_deps.get(&key) {
                if current_dep.get_version() != dep.get_version() {
                    // This entry was superseded by conflict resolution, skip it
                    tracing::debug!("[QUEUE_POP] SKIPPED (stale): '{}' - version mismatch", name);
                    continue;
                }
            }

            if processed.contains(&key) {
                tracing::debug!("[QUEUE_POP] SKIPPED (already processed): '{}'", name);
                continue;
            }

            tracing::debug!("[QUEUE_POP] PROCESSING: '{}'", name);
            processed.insert(key.clone());

            // Handle pattern dependencies by expanding them to concrete files
            if dep.is_pattern() {
                tracing::debug!("[QUEUE_POP] '{}' is a PATTERN, expanding to concrete deps", name);
                // Expand the pattern to get all matching files
                match self.expand_pattern_to_concrete_deps(&dep, resource_type).await {
                    Ok(concrete_deps) => {
                        // Queue each concrete dependency for transitive resolution
                        for (concrete_name, concrete_dep) in concrete_deps {
                            // Record the mapping from concrete resource name to pattern alias
                            // This enables patches defined under the pattern alias to be applied to all matched resources
                            // Key includes ResourceType to prevent collisions between different resource types
                            self.pattern_alias_map
                                .insert((resource_type, concrete_name.clone()), name.clone());

                            let concrete_source =
                                concrete_dep.get_source().map(std::string::ToString::to_string);
                            let concrete_tool =
                                concrete_dep.get_tool().map(std::string::ToString::to_string);
                            let concrete_key = (
                                resource_type,
                                concrete_name.clone(),
                                concrete_source,
                                concrete_tool,
                            );

                            // Only add if not already processed or queued
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                all_deps.entry(concrete_key)
                            {
                                e.insert(concrete_dep.clone());
                                queue.push((concrete_name, concrete_dep, Some(resource_type)));
                            }
                        }
                    }
                    Err(e) => {
                        anyhow::bail!(
                            "Failed to expand pattern '{}' for transitive dependency extraction: {}",
                            dep.get_path(),
                            e
                        );
                    }
                }
                continue; // Skip to next queue item
            }

            tracing::debug!(
                "[QUEUE_POP] '{}' is NOT a pattern, fetching content for metadata extraction",
                name
            );

            // Get the resource content to extract metadata
            let content = self.fetch_resource_content(&name, &dep).await.with_context(|| {
                format!("Failed to fetch resource '{name}' for transitive dependency extraction")
            })?;

            tracing::debug!(
                "[QUEUE_POP] '{}' content fetched ({} bytes), extracting metadata",
                name,
                content.len()
            );

            // Merge resource-specific template_vars with global project config
            // This ensures transitive dependencies are resolved using the resource's context
            let project_config = if let Some(template_vars) = dep.get_template_vars() {
                // Resource has template_vars - extract "project" key and deep merge with global config
                use crate::manifest::ProjectConfig;
                use crate::templating::deep_merge_json;

                // Extract the "project" key from template_vars (if it exists)
                let project_overrides = template_vars
                    .get("project")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                let global_json = self
                    .manifest
                    .project
                    .as_ref()
                    .map(|p| p.to_json_value())
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                // Deep merge: global config + resource-specific project overrides
                let merged_json = deep_merge_json(global_json, &project_overrides);

                // Convert merged JSON back to TOML for ProjectConfig
                let mut config_map = toml::map::Map::new();
                if let Some(merged_obj) = merged_json.as_object() {
                    for (key, value) in merged_obj {
                        config_map.insert(key.clone(), json_value_to_toml(value));
                    }
                }

                Some(ProjectConfig::from(config_map))
            } else {
                // No template_vars - use global config
                self.manifest.project.clone()
            };

            // Extract metadata from the resource with merged config
            let path = PathBuf::from(dep.get_path());
            let metadata = MetadataExtractor::extract(
                &path,
                &content,
                project_config.as_ref(),
                self.operation_context.as_deref(),
            )?;

            // Process transitive dependencies if present (checks both root-level and nested)
            if let Some(deps_map) = metadata.get_dependencies() {
                tracing::debug!(
                    "Processing transitive deps for: {} (has source: {:?})",
                    name,
                    dep.get_source()
                );

                for (dep_resource_type_str, dep_specs) in deps_map {
                    // Convert plural form from YAML (e.g., "agents") to ResourceType enum
                    // The ResourceType::FromStr accepts both plural and singular forms
                    let dep_resource_type: crate::core::ResourceType =
                        dep_resource_type_str.parse().unwrap_or(crate::core::ResourceType::Snippet);

                    for dep_spec in dep_specs {
                        // UNIFIED APPROACH: File-relative path resolution for all transitive dependencies

                        // Get the canonical path to the parent resource file
                        let parent_file_path = self
                            .get_canonical_path_for_dependency(&dep)
                            .await
                            .with_context(|| {
                            format!(
                                "Failed to get parent path for transitive dependencies of '{}'",
                                name
                            )
                        })?;

                        // Check if this is a glob pattern
                        let is_pattern = dep_spec.path.contains('*')
                            || dep_spec.path.contains('?')
                            || dep_spec.path.contains('[');

                        let trans_canonical = if is_pattern {
                            // For patterns, normalize (resolve .. and .) but don't canonicalize
                            let parent_dir = parent_file_path.parent()
                                .ok_or_else(|| anyhow::anyhow!(
                                    "Failed to resolve transitive dependency '{}' for '{}': parent file has no directory",
                                    dep_spec.path, name
                                ))?;
                            let resolved = parent_dir.join(&dep_spec.path);
                            // IMPORTANT: Preserve the root component when normalizing
                            let mut result = PathBuf::new();
                            for component in resolved.components() {
                                match component {
                                    std::path::Component::RootDir => {
                                        result.push(component);
                                    } // Preserve root!
                                    std::path::Component::ParentDir => {
                                        result.pop();
                                    }
                                    std::path::Component::CurDir => {}
                                    _ => {
                                        result.push(component);
                                    }
                                }
                            }
                            result
                        } else if is_file_relative_path(&dep_spec.path) {
                            // File-relative path (used in Markdown YAML frontmatter)
                            // Also treat bare filenames (no path separators) as file-relative
                            // e.g., "helper.md" is automatically treated as "./helper.md"
                            let normalized_path = normalize_bare_filename(&dep_spec.path);

                            crate::utils::resolve_file_relative_path(
                                &parent_file_path,
                                &normalized_path,
                            )
                            .with_context(|| {
                                format!(
                                    "Failed to resolve transitive dependency '{}' for '{}'",
                                    dep_spec.path, name
                                )
                            })?
                        } else {
                            // Repo-relative path (used in JSON dependencies field)
                            // Get the repository root (worktree path for Git sources, source path for local)
                            let repo_root = if dep.get_source().is_some() {
                                // For Git sources, the parent_file_path is inside a worktree
                                // Find the worktree root by going up until we find it
                                parent_file_path.ancestors()
                                    .find(|p| {
                                        // Worktree directories have format: owner_repo_sha8
                                        p.file_name()
                                            .and_then(|n| n.to_str())
                                            .map(|s| s.contains('_'))
                                            .unwrap_or(false)
                                    })
                                    .ok_or_else(|| anyhow::anyhow!(
                                        "Failed to find worktree root for transitive dependency '{}'",
                                        dep_spec.path
                                    ))?
                            } else {
                                // For local sources, go up to the source root
                                parent_file_path.ancestors()
                                    .nth(2) // Go up 2 levels from the file (e.g., from commands/file.json to repo root)
                                    .ok_or_else(|| anyhow::anyhow!(
                                        "Failed to find source root for transitive dependency '{}'",
                                        dep_spec.path
                                    ))?
                            };

                            let full_path = repo_root.join(&dep_spec.path);
                            full_path.canonicalize().with_context(|| {
                                format!(
                                    "Failed to resolve repo-relative transitive dependency '{}' for '{}': {} (repo root: {})",
                                    dep_spec.path, name, full_path.display(), repo_root.display()
                                )
                            })?
                        };

                        // Create the transitive dependency based on whether parent is Git or path-only
                        use crate::manifest::DetailedDependency;
                        let trans_dep = if dep.get_source().is_none() {
                            // Path-only transitive dep (parent is path-only)
                            // The path was resolved relative to the parent file (file-relative resolution)
                            // CRITICAL: Always store as manifest-relative path (even with ../) for lockfile portability
                            // Absolute paths in lockfiles break cross-machine sharing

                            let manifest_dir = self.manifest.manifest_dir.as_ref()
                                .ok_or_else(|| anyhow::anyhow!("Manifest directory not available for path-only transitive dep"))?;

                            // Always compute relative path from manifest to target
                            // This handles both inside (agents/foo.md) and outside (../shared/utils.md) cases
                            let dep_path_str = match manifest_dir.canonicalize() {
                                Ok(canonical_manifest) => {
                                    // Normal case: both paths are canonical, compute relative path
                                    crate::utils::compute_relative_path(
                                        &canonical_manifest,
                                        &trans_canonical,
                                    )
                                }
                                Err(e) => {
                                    // Canonicalization failed (broken symlink, permissions, etc.)
                                    // We MUST still produce a relative path for lockfile portability.
                                    // Use the non-canonical manifest_dir - not ideal but better than absolute paths.
                                    eprintln!(
                                        "Warning: Could not canonicalize manifest directory {}: {}. Using non-canonical path for relative path computation.",
                                        manifest_dir.display(),
                                        e
                                    );
                                    crate::utils::compute_relative_path(
                                        manifest_dir,
                                        &trans_canonical,
                                    )
                                }
                            };

                            // Determine tool for transitive dependency
                            let trans_tool = if let Some(explicit_tool) = &dep_spec.tool {
                                Some(explicit_tool.clone())
                            } else {
                                let parent_tool =
                                    dep.get_tool().map(str::to_string).unwrap_or_else(|| {
                                        self.manifest.get_default_tool(resource_type)
                                    });
                                if self
                                    .manifest
                                    .is_resource_supported(&parent_tool, dep_resource_type)
                                {
                                    Some(parent_tool)
                                } else {
                                    Some(self.manifest.get_default_tool(dep_resource_type))
                                }
                            };

                            ResourceDependency::Detailed(Box::new(DetailedDependency {
                                source: None,
                                path: crate::utils::normalize_path_for_storage(dep_path_str),
                                version: None,
                                branch: None,
                                rev: None,
                                command: None,
                                args: None,
                                target: None,
                                filename: None,
                                dependencies: None,
                                tool: trans_tool,
                                flatten: None,
                                install: dep_spec.install.or(Some(true)),
                                template_vars: Some(self.build_merged_template_vars(&dep)),
                            }))
                        } else {
                            // Git-backed transitive dep (parent is Git-backed)
                            // The resolved path is within the worktree - need to convert back to repo-relative
                            let source_name = dep.get_source().ok_or_else(|| {
                                anyhow::anyhow!("Expected source for Git-backed dependency")
                            })?;
                            let version = dep.get_version().unwrap_or("main").to_string();
                            let source_url =
                                self.source_manager.get_source_url(source_name).ok_or_else(
                                    || anyhow::anyhow!("Source '{source_name}' not found"),
                                )?;

                            // Get repo-relative path by stripping the appropriate prefix
                            let repo_relative = if crate::utils::is_local_path(&source_url) {
                                // For local directory sources, strip the source path to get relative path
                                let source_path = PathBuf::from(&source_url).canonicalize()?;
                                trans_canonical.strip_prefix(&source_path)
                                    .with_context(|| format!(
                                        "Transitive dep resolved outside parent's source directory: {} not under {}",
                                        trans_canonical.display(),
                                        source_path.display()
                                    ))?
                                    .to_path_buf()
                            } else {
                                // For Git sources, get worktree and strip it
                                let sha = self
                                    .prepared_versions
                                    .get(&Self::group_key(source_name, &version))
                                    .ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "Parent version not resolved for {}",
                                            source_name
                                        )
                                    })?
                                    .resolved_commit
                                    .clone();
                                let worktree_path = self
                                    .cache
                                    .get_or_create_worktree_for_sha(
                                        source_name,
                                        &source_url,
                                        &sha,
                                        None,
                                    )
                                    .await?;

                                // Canonicalize worktree path to handle symlinks (e.g., /var -> /private/var on macOS)
                                // and ensure consistent path formats on Windows (\\?\ prefix)
                                let canonical_worktree =
                                    worktree_path.canonicalize().with_context(|| {
                                        format!(
                                            "Failed to canonicalize worktree path: {}",
                                            worktree_path.display()
                                        )
                                    })?;

                                trans_canonical.strip_prefix(&canonical_worktree)
                                    .with_context(|| format!(
                                        "Transitive dep resolved outside parent's worktree: {} not under {}",
                                        trans_canonical.display(),
                                        canonical_worktree.display()
                                    ))?
                                    .to_path_buf()
                            };

                            // Determine tool for transitive dependency
                            let trans_tool = if let Some(explicit_tool) = &dep_spec.tool {
                                Some(explicit_tool.clone())
                            } else {
                                let parent_tool =
                                    dep.get_tool().map(str::to_string).unwrap_or_else(|| {
                                        self.manifest.get_default_tool(resource_type)
                                    });
                                if self
                                    .manifest
                                    .is_resource_supported(&parent_tool, dep_resource_type)
                                {
                                    Some(parent_tool)
                                } else {
                                    Some(self.manifest.get_default_tool(dep_resource_type))
                                }
                            };

                            ResourceDependency::Detailed(Box::new(DetailedDependency {
                                source: Some(source_name.to_string()),
                                path: crate::utils::normalize_path_for_storage(
                                    repo_relative.to_string_lossy().to_string(),
                                ),
                                version: dep_spec
                                    .version
                                    .clone()
                                    .or_else(|| dep.get_version().map(|v| v.to_string())),
                                branch: None,
                                rev: None,
                                command: None,
                                args: None,
                                target: None,
                                filename: None,
                                dependencies: None,
                                tool: trans_tool,
                                flatten: None,
                                install: dep_spec.install.or(Some(true)),
                                template_vars: Some(self.build_merged_template_vars(&dep)),
                            }))
                        };

                        // Generate a name for the transitive dependency
                        // ALWAYS derive from path to ensure uniqueness and avoid collisions
                        // The `name` field can be stored as manifest_alias for template variable names
                        let trans_name = generate_dependency_name(trans_dep.get_path());

                        // Add to graph (use source-aware nodes to prevent false cycles)
                        let trans_source =
                            trans_dep.get_source().map(std::string::ToString::to_string);
                        let trans_tool = trans_dep.get_tool().map(std::string::ToString::to_string);

                        // Store custom name if provided, for use as manifest_alias
                        if let Some(custom_name) = &dep_spec.name {
                            let trans_key = (
                                dep_resource_type,
                                trans_name.clone(),
                                trans_source.clone(),
                                trans_tool.clone(),
                            );
                            self.transitive_custom_names.insert(trans_key, custom_name.clone());
                            tracing::debug!(
                                "Storing custom name '{}' for transitive dependency '{}'",
                                custom_name,
                                trans_name
                            );
                        }

                        let from_node =
                            DependencyNode::with_source(resource_type, &name, source.clone());
                        let to_node = DependencyNode::with_source(
                            dep_resource_type,
                            &trans_name,
                            trans_source.clone(),
                        );
                        graph.add_dependency(from_node.clone(), to_node.clone());

                        // Track in dependency map (use singular form from enum for dependency references)
                        // Include source to prevent cross-source contamination
                        let from_key = (resource_type, name.clone(), source.clone(), tool.clone());
                        let dep_ref = format!("{dep_resource_type}/{trans_name}");
                        self.dependency_map.entry(from_key).or_default().push(dep_ref);

                        // Add to conflict detector for tracking version requirements
                        self.add_to_conflict_detector(&trans_name, &trans_dep, &name);

                        // Check for version conflicts and resolve them
                        let trans_key = (
                            dep_resource_type,
                            trans_name.clone(),
                            trans_source.clone(),
                            trans_tool.clone(),
                        );

                        if let Some(existing_dep) = all_deps.get(&trans_key) {
                            // Version conflict detected (same name and source, different version)
                            let resolved_dep = self
                                .resolve_version_conflict(
                                    &trans_name,
                                    existing_dep,
                                    &trans_dep,
                                    &name, // Who requires this version
                                )
                                .await?;

                            // Only re-queue if the resolved version differs from existing
                            let needs_reprocess =
                                resolved_dep.get_version() != existing_dep.get_version();

                            all_deps.insert(trans_key.clone(), resolved_dep.clone());

                            if needs_reprocess {
                                // Remove from processed so the resolved version can be processed
                                // This ensures we fetch metadata for the correct version
                                processed.remove(&trans_key);
                                // Re-queue the resolved dependency
                                queue.push((
                                    trans_name.clone(),
                                    resolved_dep,
                                    Some(dep_resource_type),
                                ));
                            }
                        } else {
                            // No conflict, add the dependency
                            tracing::debug!(
                                "Adding transitive dep '{}' to all_deps and queue (parent: {})",
                                trans_name,
                                name
                            );
                            all_deps.insert(trans_key.clone(), trans_dep.clone());
                            queue.push((trans_name, trans_dep, Some(dep_resource_type)));
                        }
                    }
                }
            }
        }

        // Check for circular dependencies
        graph.detect_cycles()?;

        // Get topological order for dependencies that have relationships
        let ordered_nodes = graph.topological_order()?;

        // Build result: start with topologically ordered dependencies
        let mut result = Vec::new();
        let mut added_keys = HashSet::new();

        tracing::debug!(
            "Transitive resolution - topological order has {} nodes, all_deps has {} entries",
            ordered_nodes.len(),
            all_deps.len()
        );

        for node in ordered_nodes {
            tracing::debug!(
                "Processing ordered node: {}/{} (source: {:?})",
                node.resource_type,
                node.name,
                node.source
            );
            // Find matching dependency - now that nodes include source, we can match precisely
            for (key, dep) in &all_deps {
                if key.0 == node.resource_type && key.1 == node.name && key.2 == node.source {
                    tracing::debug!(
                        "  -> Found match in all_deps, adding to result with type {:?}",
                        node.resource_type
                    );
                    result.push((node.name.clone(), dep.clone(), node.resource_type));
                    added_keys.insert(key.clone());
                    break; // Exact match found, no need to continue
                }
            }
        }

        // Add remaining dependencies that weren't in the graph (no transitive deps)
        // These can be added in any order since they have no dependencies
        // IMPORTANT: Filter out patterns - they should only serve as expansion points,
        // not final dependencies. The concrete deps from expansion are what we want.
        for (key, dep) in all_deps {
            if !added_keys.contains(&key) {
                // Skip pattern dependencies - they were expanded to concrete deps
                if dep.is_pattern() {
                    tracing::debug!(
                        "Skipping pattern dependency in final result: {}/{} (source: {:?})",
                        key.0,
                        key.1,
                        key.2
                    );
                    continue;
                }

                tracing::debug!(
                    "Adding non-graph dependency: {}/{} (source: {:?}) with type {:?}",
                    key.0,
                    key.1,
                    key.2,
                    key.0
                );
                result.push((key.1.clone(), dep.clone(), key.0));
            }
        }

        tracing::debug!("Transitive resolution returning {} dependencies", result.len());

        Ok(result)
    }

    /// Fetch the content of a resource for metadata extraction.
    async fn fetch_resource_content(
        &mut self,
        _name: &str,
        dep: &ResourceDependency,
    ) -> Result<String> {
        match dep {
            ResourceDependency::Simple(path) => {
                // Local file - resolve relative to manifest directory
                let manifest_dir = self.manifest.manifest_dir.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Manifest directory not available for Simple dependency")
                })?;
                let full_path =
                    crate::utils::resolve_path_relative_to_manifest(manifest_dir, path)?;
                std::fs::read_to_string(&full_path)
                    .with_context(|| format!("Failed to read local file: {}", full_path.display()))
            }
            ResourceDependency::Detailed(detailed) => {
                if let Some(source_name) = &detailed.source {
                    let source_url = self
                        .source_manager
                        .get_source_url(source_name)
                        .ok_or_else(|| anyhow::anyhow!("Source '{source_name}' not found"))?;

                    // Check if this is a local directory source
                    if crate::utils::is_local_path(&source_url) {
                        // Local directory source - read directly from path
                        let file_path = PathBuf::from(&source_url).join(&detailed.path);
                        std::fs::read_to_string(&file_path).with_context(|| {
                            format!("Failed to read local file: {}", file_path.display())
                        })
                    } else {
                        // Git-based remote dependency - need to checkout and read
                        // Use get_version() to respect rev > branch > version precedence
                        let version = dep.get_version().unwrap_or("main").to_string();

                        // Check if we already have this version resolved
                        let sha = if let Some(prepared) =
                            self.prepared_versions.get(&Self::group_key(source_name, &version))
                        {
                            prepared.resolved_commit.clone()
                        } else {
                            // Need to resolve this version
                            self.version_resolver.add_version(
                                source_name,
                                &source_url,
                                Some(&version),
                            );
                            self.version_resolver.resolve_all().await?;

                            let resolved_sha = self
                                .version_resolver
                                .get_resolved_sha(source_name, &version)
                                .ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "Failed to resolve version for {source_name} @ {version}"
                                    )
                                })?;

                            // Cache the resolved version for nested transitive dependency resolution
                            // Note: worktree_path will be set when get_or_create_worktree_for_sha is called below
                            self.prepared_versions.insert(
                                Self::group_key(source_name, &version),
                                PreparedSourceVersion {
                                    worktree_path: PathBuf::new(), // Placeholder, will be set after worktree creation
                                    resolved_version: Some(version.clone()),
                                    resolved_commit: resolved_sha.clone(),
                                },
                            );

                            resolved_sha
                        };

                        // Get worktree for this SHA
                        let worktree_path = self
                            .cache
                            .get_or_create_worktree_for_sha(source_name, &source_url, &sha, None)
                            .await?;

                        // Read the file from worktree (with cache coherency retry)
                        let file_path = worktree_path.join(&detailed.path);
                        read_with_cache_retry_sync(&file_path)
                    }
                } else {
                    // Local dependency with detailed spec - resolve relative to manifest directory
                    let manifest_dir = self.manifest.manifest_dir.as_ref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "Manifest directory not available for local Detailed dependency"
                        )
                    })?;
                    let full_path = crate::utils::resolve_path_relative_to_manifest(
                        manifest_dir,
                        &detailed.path,
                    )?;
                    std::fs::read_to_string(&full_path).with_context(|| {
                        format!("Failed to read local file: {}", full_path.display())
                    })
                }
            }
        }
    }

    /// Gets the canonical file path for a dependency (unified for Git and path-only).
    ///
    /// For path-only deps: resolves from manifest directory
    /// For Git-backed deps: resolves from worktree path
    async fn get_canonical_path_for_dependency(
        &mut self,
        dep: &ResourceDependency,
    ) -> Result<PathBuf> {
        if dep.get_source().is_none() {
            // Path-only: resolve from manifest directory
            let manifest_dir = self
                .manifest
                .manifest_dir
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Manifest directory not available"))?;
            crate::utils::resolve_path_relative_to_manifest(manifest_dir, dep.get_path())
        } else {
            // Git-backed: get worktree path and join with repo-relative path
            let source_name = dep
                .get_source()
                .ok_or_else(|| anyhow::anyhow!("Cannot get worktree for path-only dependency"))?;
            let version = dep.get_version().unwrap_or("main").to_string();

            // Get source URL
            let source_url = self
                .source_manager
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{source_name}' not found"))?;

            // Check if this is a local directory source (not a Git repo)
            if crate::utils::is_local_path(&source_url) {
                // Local directory source - resolve directly from source path
                let file_path = PathBuf::from(&source_url).join(dep.get_path());
                file_path.canonicalize().with_context(|| {
                    format!("Failed to canonicalize local source resource: {}", file_path.display())
                })
            } else {
                // Git-backed: resolve from worktree
                // Get the resolved SHA
                let sha = if let Some(prepared) =
                    self.prepared_versions.get(&Self::group_key(source_name, &version))
                {
                    prepared.resolved_commit.clone()
                } else {
                    // Need to resolve this version
                    self.version_resolver.add_version(source_name, &source_url, Some(&version));
                    self.version_resolver.resolve_all().await?;

                    self.version_resolver.get_resolved_sha(source_name, &version).ok_or_else(
                        || {
                            anyhow::anyhow!(
                                "Failed to resolve version for {source_name} @ {version}"
                            )
                        },
                    )?
                };

                // Get worktree path
                let worktree_path = self
                    .cache
                    .get_or_create_worktree_for_sha(source_name, &source_url, &sha, None)
                    .await?;

                // Join with repo-relative path and canonicalize
                let full_path = worktree_path.join(dep.get_path());
                full_path.canonicalize().with_context(|| {
                    format!("Failed to canonicalize Git resource: {}", full_path.display())
                })
            }
        }
    }

    /// Expands a pattern dependency to concrete (non-pattern) dependencies.
    ///
    /// This method is used during transitive dependency resolution to handle
    /// glob patterns declared in resource frontmatter. It expands the pattern
    /// to all matching files and creates a concrete ResourceDependency for each.
    ///
    /// # Arguments
    ///
    /// * `dep` - The pattern-based dependency to expand
    /// * `resource_type` - The resource type (for path construction)
    ///
    /// # Returns
    ///
    /// A vector of (name, ResourceDependency) tuples for each matched file.
    async fn expand_pattern_to_concrete_deps(
        &self,
        dep: &ResourceDependency,
        resource_type: crate::core::ResourceType,
    ) -> Result<Vec<(String, ResourceDependency)>> {
        pattern_expander::expand_pattern_to_concrete_deps(
            dep,
            resource_type,
            &self.source_manager,
            &self.cache,
            self.manifest.manifest_dir.as_deref(),
        )
        .await
    }

    /// Resolve all manifest dependencies into a deterministic lockfile.
    ///
    /// This is the primary entry point for dependency resolution. It resolves all
    /// dependencies from the manifest (including transitive dependencies) and
    /// generates a complete lockfile with resolved versions and commit SHAs.
    ///
    /// By default, this method enables transitive dependency resolution. Resources
    /// can declare their own dependencies via YAML frontmatter (Markdown) or JSON
    /// fields, which will be automatically discovered and resolved.
    ///
    /// # Transitive Dependency Resolution
    ///
    /// When enabled (default), the resolver:
    /// 1. Resolves direct manifest dependencies
    /// 2. Extracts dependency metadata from resource files
    /// 3. Builds a dependency graph with cycle detection
    /// 4. Resolves transitive dependencies in topological order
    ///
    /// # Returns
    ///
    /// A complete [`LockFile`] with all resolved dependencies including:
    /// - Resolved commit SHAs for reproducible installations
    /// - Checksums for integrity verification
    /// - Installation paths for all resources
    /// - Source repository information
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source repositories cannot be accessed
    /// - Version constraints cannot be satisfied
    /// - Circular dependencies are detected
    /// - Resource files cannot be read or parsed
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use agpm_cli::resolver::DependencyResolver;
    /// # use agpm_cli::manifest::Manifest;
    /// # use agpm_cli::cache::Cache;
    /// # async fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::load("agpm.toml".as_ref())?;
    /// let cache = Cache::new()?;
    /// let mut resolver = DependencyResolver::new(manifest, cache)?;
    ///
    /// // Resolve all dependencies including transitive ones
    /// let lockfile = resolver.resolve().await?;
    ///
    /// lockfile.save("agpm.lock".as_ref())?;
    /// println!("Resolved {} total resources",
    ///          lockfile.agents.len() + lockfile.snippets.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resolve(&mut self) -> Result<LockFile> {
        self.resolve_with_options(true).await
    }

    /// Resolve dependencies with configurable transitive dependency support.
    ///
    /// This method provides fine-grained control over dependency resolution behavior,
    /// allowing you to disable transitive dependency resolution when needed. This is
    /// useful for debugging, testing, or when you want to install only direct
    /// dependencies without their transitive requirements.
    ///
    /// # Arguments
    ///
    /// * `enable_transitive` - Whether to resolve transitive dependencies
    ///   - `true`: Full transitive resolution (default behavior)
    ///   - `false`: Only direct manifest dependencies
    ///
    /// # Transitive Resolution Details
    ///
    /// When `enable_transitive` is `true`:
    /// - Resources are checked for embedded dependency metadata
    /// - Markdown files (.md): YAML frontmatter between `---` delimiters
    /// - JSON files (.json): Top-level `dependencies` field
    /// - Dependency graph is built with cycle detection
    /// - Dependencies are resolved in topological order
    ///
    /// When `enable_transitive` is `false`:
    /// - Only dependencies explicitly declared in `agpm.toml` are resolved
    /// - Resource metadata is not extracted or processed
    /// - Faster resolution for known dependency trees
    ///
    /// # Returns
    ///
    /// A [`LockFile`] containing all resolved dependencies according to the
    /// configuration. When transitive resolution is disabled, the lockfile will
    /// only contain direct dependencies from the manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source repositories are inaccessible or invalid
    /// - Version constraints conflict or cannot be satisfied
    /// - Circular dependencies are detected (when `enable_transitive` is true)
    /// - Resource files cannot be read or contain invalid metadata
    /// - Network operations fail during source synchronization
    ///
    /// # Performance Considerations
    ///
    /// Disabling transitive resolution (`enable_transitive = false`) can improve
    /// performance when:
    /// - You know all required dependencies are explicitly listed
    /// - Testing specific dependency combinations
    /// - Debugging dependency resolution issues
    /// - Working with large resources that have expensive metadata extraction
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use agpm_cli::resolver::DependencyResolver;
    /// # use agpm_cli::manifest::Manifest;
    /// # use agpm_cli::cache::Cache;
    /// # async fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::load("agpm.toml".as_ref())?;
    /// let cache = Cache::new()?;
    /// let mut resolver = DependencyResolver::new(manifest, cache)?;
    ///
    /// // Resolve only direct dependencies without transitive resolution
    /// let lockfile = resolver.resolve_with_options(false).await?;
    ///
    /// println!("Resolved {} direct dependencies",
    ///          lockfile.agents.len() + lockfile.snippets.len());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # See Also
    ///
    /// - [`resolve()`]: Convenience method that enables transitive resolution by default
    /// - [`DependencyGraph`]: Graph structure used for cycle detection and ordering
    /// - [`DependencySpec`](crate::manifest::DependencySpec): Specification format for transitive dependencies
    ///
    /// [`resolve()`]: DependencyResolver::resolve
    pub async fn resolve_with_options(&mut self, enable_transitive: bool) -> Result<LockFile> {
        let mut lockfile = LockFile::new();

        // Add sources to lockfile
        for (name, url) in &self.manifest.sources {
            lockfile.add_source(name.clone(), url.clone(), String::new());
        }

        // Get all dependencies with their types to avoid mis-typing same-named resources
        let base_deps: Vec<(String, ResourceDependency, crate::core::ResourceType)> = self
            .manifest
            .all_dependencies_with_types()
            .into_iter()
            .map(|(name, dep, resource_type)| (name.to_string(), dep.into_owned(), resource_type))
            .collect();

        // Add direct dependencies to conflict detector
        for (name, dep, _) in &base_deps {
            self.add_to_conflict_detector(name, dep, "manifest");
        }

        // Show initial message about what we're doing
        // Sync sources (phase management is handled by caller)
        // prepare_remote_groups only needs name and dep, not type
        let base_deps_for_prep: Vec<(String, ResourceDependency)> =
            base_deps.iter().map(|(name, dep, _)| (name.clone(), dep.clone())).collect();
        self.prepare_remote_groups(&base_deps_for_prep).await?;

        // Resolve transitive dependencies if enabled
        let deps = self.resolve_transitive_dependencies(&base_deps, enable_transitive).await?;

        // Resolve each dependency (including transitive ones)
        tracing::debug!("resolve_with_options - about to resolve {} dependencies", deps.len());
        for (name, dep, resource_type) in &deps {
            tracing::debug!(
                "Resolving dependency: {} -> {} (type: {:?})",
                name,
                dep.get_path(),
                resource_type
            );
            // Progress is tracked at the phase level

            // Check if this is a pattern dependency
            if dep.is_pattern() {
                // Pattern dependencies resolve to multiple resources
                let entries = self.resolve_pattern_dependency(name, dep, *resource_type).await?;

                // Add each resolved entry to the appropriate resource type with deduplication
                // Resource type was already determined during transitive resolution
                for entry in entries {
                    match *resource_type {
                        crate::core::ResourceType::Agent => {
                            // Match by (name, source) to allow same-named resources from different sources
                            if let Some(existing) = lockfile
                                .agents
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.agents.push(entry);
                            }
                        }
                        crate::core::ResourceType::Snippet => {
                            if let Some(existing) = lockfile
                                .snippets
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.snippets.push(entry);
                            }
                        }
                        crate::core::ResourceType::Command => {
                            if let Some(existing) = lockfile
                                .commands
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.commands.push(entry);
                            }
                        }
                        crate::core::ResourceType::Script => {
                            if let Some(existing) = lockfile
                                .scripts
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.scripts.push(entry);
                            }
                        }
                        crate::core::ResourceType::Hook => {
                            if let Some(existing) = lockfile
                                .hooks
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.hooks.push(entry);
                            }
                        }
                        crate::core::ResourceType::McpServer => {
                            if let Some(existing) = lockfile
                                .mcp_servers
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.mcp_servers.push(entry);
                            }
                        }
                    }
                }
            } else {
                // Regular single dependency
                // Pass the resource type from transitive resolution to ensure correct type
                // This handles cases where transitive dependencies share the same name
                // as direct manifest dependencies but have a different type
                let entry = self.resolve_dependency(name, dep, *resource_type).await?;
                tracing::debug!(
                    "Resolved {} to resource_type={:?}, installed_at={}",
                    name,
                    entry.resource_type,
                    entry.installed_at
                );
                // Add directly to lockfile to preserve (name, source) uniqueness
                self.add_or_update_lockfile_entry(&mut lockfile, name, entry);
            }

            // Progress is tracked by updating messages, no need to increment
        }

        // Progress is tracked at the phase level

        // Progress completion is handled by the caller

        // Detect version conflicts before creating lockfile
        let conflicts = self.conflict_detector.detect_conflicts();
        if !conflicts.is_empty() {
            let mut error_msg = String::from("Version conflicts detected:\n\n");
            for conflict in &conflicts {
                error_msg.push_str(&format!("{conflict}\n"));
            }
            return Err(AgpmError::Other {
                message: error_msg,
            }
            .into());
        }

        // Post-process dependencies to add version information
        self.add_version_to_dependencies(&mut lockfile)?;

        // Detect target-path conflicts before finalizing
        self.detect_target_conflicts(&lockfile)?;

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
        resource_type: crate::core::ResourceType,
    ) -> Result<LockedResource> {
        // Check if this is a pattern-based dependency
        if dep.is_pattern() {
            // Pattern dependencies resolve to multiple resources
            // This should be handled by a separate method
            return Err(anyhow::anyhow!(
                "Pattern dependency '{name}' should be resolved using resolve_pattern_dependency"
            ));
        }

        if dep.is_local() {
            // Local dependency - just create entry with path
            // Resource type is passed as a parameter to ensure correct type resolution
            // for both direct and transitive dependencies

            // Determine the filename to use
            let filename = if let Some(custom_filename) = dep.get_filename() {
                // Use custom filename as-is (includes extension)
                custom_filename.to_string()
            } else {
                // Extract meaningful path structure from dependency path
                extract_meaningful_path(Path::new(dep.get_path()))
            };

            // Determine artifact type - use get_tool() method, then apply defaults
            let artifact_type_string = dep
                .get_tool()
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.manifest.get_default_tool(resource_type));
            let artifact_type = artifact_type_string.as_str();

            // For local resources without a source, just use the name (no version suffix)
            let unique_name = name.to_string();

            // Compute installation path using helper
            let installed_at = install_path_resolver::resolve_install_path(
                &self.manifest,
                dep,
                artifact_type,
                resource_type,
                &filename,
            )
            .with_context(|| {
                format!("Failed to resolve installation path for dependency '{}'", name)
            })?;

            Ok(LockedResourceBuilder::new(
                unique_name,
                normalize_path_for_storage(dep.get_path()),
                String::new(),
                installed_at,
                resource_type,
            )
            .dependencies(self.get_dependencies_for(name, None, resource_type, dep.get_tool()))
            .tool(Some({
                let tool_value = dep
                    .get_tool()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_else(|| self.manifest.get_default_tool(resource_type));
                tool_value.clone()
            }))
            .manifest_alias(self.pattern_alias_map.get(&(resource_type, name.to_string())).cloned())
            .applied_patches(lockfile_operations::get_patches_for_resource(
                &self.manifest,
                resource_type,
                name,
            ))
            .install(dep.get_install())
            .template_vars(self.build_merged_template_vars(dep))
            .build())
        } else {
            // Remote dependency - need to sync and resolve
            let source_name = dep.get_source().ok_or_else(|| AgpmError::ConfigError {
                message: format!("Dependency '{name}' has no source specified"),
            })?;

            // Get source URL
            let source_url = self.source_manager.get_source_url(source_name).ok_or_else(|| {
                AgpmError::SourceNotFound {
                    name: source_name.to_string(),
                }
            })?;

            let version_key = dep
                .get_version()
                .map_or_else(|| "HEAD".to_string(), std::string::ToString::to_string);
            let prepared_key = Self::group_key(source_name, &version_key);

            // Check if this dependency has been prepared
            let (resolved_version, resolved_commit) = if let Some(prepared) =
                self.prepared_versions.get(&prepared_key)
            {
                // Use prepared version
                (prepared.resolved_version.clone(), prepared.resolved_commit.clone())
            } else {
                // This dependency wasn't prepared (e.g., when called from `agpm add`)
                // We need to prepare it on-demand
                let deps = vec![(name.to_string(), dep.clone())];
                self.prepare_remote_groups(&deps).await?;

                // Now it should be prepared
                if let Some(prepared) = self.prepared_versions.get(&prepared_key) {
                    (prepared.resolved_version.clone(), prepared.resolved_commit.clone())
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to prepare dependency '{name}' from source '{source_name}' @ '{version_key}'"
                    ));
                }
            };

            // Resource type is passed as a parameter to ensure correct type resolution
            // for both direct and transitive dependencies

            // Determine the filename to use
            let filename = if let Some(custom_filename) = dep.get_filename() {
                // Use custom filename as-is (includes extension)
                custom_filename.to_string()
            } else {
                // Preserve the dependency path structure directly
                let dep_path = Path::new(dep.get_path());
                dep_path.to_string_lossy().to_string()
            };

            // Determine artifact type - use get_tool() method, then apply defaults
            // Extract artifact_type from dependency - convert to String for lockfile
            let artifact_type_string = dep
                .get_tool()
                .map(std::string::ToString::to_string)
                .unwrap_or_else(|| self.manifest.get_default_tool(resource_type));
            let artifact_type = artifact_type_string.as_str();

            // Use simple name from manifest - lockfile entries are identified by (name, source)
            // Multiple entries with the same name but different sources can coexist
            // Version updates replace the existing entry for the same (name, source) pair
            let unique_name = name.to_string();

            // Compute installation path using helper
            let installed_at = install_path_resolver::resolve_install_path(
                &self.manifest,
                dep,
                artifact_type,
                resource_type,
                &filename,
            )
            .with_context(|| {
                format!("Failed to resolve installation path for dependency '{}'", name)
            })?;

            Ok(LockedResourceBuilder::new(
                unique_name,
                normalize_path_for_storage(dep.get_path()),
                String::new(), // Will be calculated during installation
                installed_at,
                resource_type,
            )
            .source(Some(source_name.to_string()))
            .url(Some(source_url.clone()))
            .version(resolved_version) // Resolved version (tag/branch like "v2.1.4" or "main")
            .resolved_commit(Some(resolved_commit))
            .dependencies(self.get_dependencies_for(
                name,
                Some(source_name),
                resource_type,
                dep.get_tool(),
            ))
            .tool(Some(artifact_type_string.clone()))
            .manifest_alias(self.pattern_alias_map.get(&(resource_type, name.to_string())).cloned())
            .applied_patches(lockfile_operations::get_patches_for_resource(
                &self.manifest,
                resource_type,
                name,
            ))
            .install(dep.get_install())
            .template_vars(self.build_merged_template_vars(dep))
            .build())
        }
    }

    /// Gets the dependencies for a resource from the dependency map.
    ///
    /// Returns a list of dependencies in the format "`resource_type/name`".
    ///
    /// # Parameters
    /// - `name`: The resource name
    /// - `source`: The source name (None for local dependencies)
    fn get_dependencies_for(
        &self,
        name: &str,
        source: Option<&str>,
        resource_type: crate::core::ResourceType,
        tool: Option<&str>,
    ) -> Vec<String> {
        // Use the threaded resource_type parameter from the manifest
        // This prevents type misclassification when same names exist across types
        let key = (
            resource_type,
            name.to_string(),
            source.map(std::string::ToString::to_string),
            tool.map(std::string::ToString::to_string),
        );
        self.dependency_map.get(&key).cloned().unwrap_or_default()
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
        resource_type: crate::core::ResourceType,
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
            let (base_path, pattern_str) = path_helpers::parse_pattern_base_path(pattern);

            let pattern_resolver = crate::pattern::PatternResolver::new();
            let matches = pattern_resolver.resolve(&pattern_str, &base_path)?;

            // Use the threaded resource_type from the parameter
            let mut resources = Vec::new();

            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);

                // Determine artifact type - use get_tool() method, then apply defaults
                let artifact_type_string = dep
                    .get_tool()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| self.manifest.get_default_tool(resource_type));
                let artifact_type = artifact_type_string.as_str();

                // Construct full relative path from base_path and matched_path
                let full_relative_path =
                    path_helpers::construct_full_relative_path(&base_path, &matched_path);

                // Use the threaded resource_type (pattern dependencies inherit from parent)

                // Compute installation path using helper
                let filename = path_helpers::extract_pattern_filename(&base_path, &matched_path);
                let installed_at = install_path_resolver::resolve_install_path(
                    &self.manifest,
                    dep,
                    artifact_type,
                    resource_type,
                    &filename,
                )?;

                resources.push(LockedResource {
                    name: resource_name.clone(),
                    source: None,
                    url: None,
                    path: full_relative_path,
                    version: None,
                    resolved_commit: None,
                    checksum: String::new(),
                    installed_at,
                    dependencies: self.get_dependencies_for(
                        &resource_name,
                        None,
                        resource_type,
                        dep.get_tool(),
                    ),
                    resource_type,
                    tool: Some(
                        dep.get_tool()
                            .map(std::string::ToString::to_string)
                            .unwrap_or_else(|| self.manifest.get_default_tool(resource_type)),
                    ),
                    manifest_alias: Some(name.to_string()), // Pattern dependency: preserve original alias
                    applied_patches: lockfile_operations::get_patches_for_resource(
                        &self.manifest,
                        resource_type,
                        name,
                    ),
                    install: dep.get_install(),
                    template_vars: "{}".to_string(),
                });
            }

            Ok(resources)
        } else {
            // Remote pattern dependency - need to sync and search
            let source_name = dep.get_source().ok_or_else(|| AgpmError::ConfigError {
                message: format!("Pattern dependency '{name}' has no source specified"),
            })?;

            let source_url = self.source_manager.get_source_url(source_name).ok_or_else(|| {
                AgpmError::SourceNotFound {
                    name: source_name.to_string(),
                }
            })?;

            let version_key = dep
                .get_version()
                .map_or_else(|| "HEAD".to_string(), std::string::ToString::to_string);
            let prepared_key = Self::group_key(source_name, &version_key);

            let prepared = self
                .prepared_versions
                .get(&prepared_key)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Prepared state missing for source '{source_name}' @ '{version_key}'. Stage 1 preparation should have populated this entry."
                    )
                })?;

            let repo_path = prepared.worktree_path.clone();
            let resolved_version = prepared.resolved_version.clone();
            let resolved_commit = prepared.resolved_commit.clone();

            // Search for matching files in the repository
            let pattern_resolver = crate::pattern::PatternResolver::new();
            let repo_path_ref = Path::new(&repo_path);
            let matches = pattern_resolver.resolve(pattern, repo_path_ref)?;

            // Use the threaded resource_type parameter (no need to recompute)
            let mut resources = Vec::new();

            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);

                // Determine artifact type - use get_tool() method, then apply defaults
                let artifact_type_string = dep
                    .get_tool()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| self.manifest.get_default_tool(resource_type));
                let artifact_type = artifact_type_string.as_str();

                // Use the threaded resource_type (pattern dependencies inherit from parent)

                // Hooks and MCP servers are configured in config files, not installed as artifact files
                let installed_at = match resource_type {
                    crate::core::ResourceType::Hook | crate::core::ResourceType::McpServer => {
                        // Use configured merge target, with fallback to hardcoded defaults
                        if let Some(merge_target) =
                            self.manifest.get_merge_target(artifact_type, resource_type)
                        {
                            normalize_path_for_storage(merge_target.display().to_string())
                        } else {
                            // Fallback to hardcoded defaults if not configured
                            match resource_type {
                                crate::core::ResourceType::Hook => {
                                    ".claude/settings.local.json".to_string()
                                }
                                crate::core::ResourceType::McpServer => {
                                    if artifact_type == "opencode" {
                                        ".opencode/opencode.json".to_string()
                                    } else {
                                        ".mcp.json".to_string()
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                    _ => {
                        // For regular resources, get the artifact path
                        let artifact_path = self
                            .manifest
                            .get_artifact_resource_path(artifact_type, resource_type)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Resource type '{}' is not supported by tool '{}'",
                                    resource_type,
                                    artifact_type
                                )
                            })?;

                        // Determine flatten behavior: use explicit setting or tool config default
                        let dep_flatten = dep.get_flatten();
                        let tool_flatten = self
                            .manifest
                            .get_tool_config(artifact_type)
                            .and_then(|config| config.resources.get(resource_type.to_plural()))
                            .and_then(|resource_config| resource_config.flatten);

                        let flatten = dep_flatten.or(tool_flatten).unwrap_or(false); // Default to false if not configured

                        // Determine the base target directory
                        let base_target = if let Some(custom_target) = dep.get_target() {
                            // Custom target is relative to the artifact's resource directory
                            PathBuf::from(artifact_path.display().to_string())
                                .join(custom_target.trim_start_matches('/'))
                        } else {
                            artifact_path.to_path_buf()
                        };

                        // Extract the meaningful path structure
                        // 1. For relative paths with "../", strip parent components: "../../snippets/dir/file.md" → "snippets/dir/file.md"
                        // 2. For absolute paths, resolve ".." first then strip root: "/tmp/foo/../bar/agent.md" → "tmp/bar/agent.md"
                        // 3. For clean relative paths, use as-is: "agents/test.md" → "agents/test.md"
                        let filename = {
                            // Construct full path from repo_path and matched_path for extraction
                            let full_path = repo_path_ref.join(&matched_path);
                            let components: Vec<_> = full_path.components().collect();

                            if full_path.is_absolute() {
                                // Case 2: Absolute path - resolve ".." components first, then strip root
                                let mut resolved = Vec::new();

                                for component in components.iter() {
                                    match component {
                                        std::path::Component::Normal(name) => {
                                            resolved.push(name.to_str().unwrap_or(""));
                                        }
                                        std::path::Component::ParentDir => {
                                            // Pop the last component if there is one
                                            resolved.pop();
                                        }
                                        // Skip RootDir, Prefix, and CurDir
                                        _ => {}
                                    }
                                }

                                resolved.join("/")
                            } else if components
                                .iter()
                                .any(|c| matches!(c, std::path::Component::ParentDir))
                            {
                                // Case 1: Relative path with "../" - skip all parent components
                                let start_idx = components
                                    .iter()
                                    .position(|c| matches!(c, std::path::Component::Normal(_)))
                                    .unwrap_or(0);

                                components[start_idx..]
                                    .iter()
                                    .filter_map(|c| c.as_os_str().to_str())
                                    .collect::<Vec<_>>()
                                    .join("/")
                            } else {
                                // Case 3: Clean relative path - use as-is
                                full_path.to_str().unwrap_or("").replace('\\', "/") // Normalize to forward slashes
                            }
                        };

                        // Use compute_relative_install_path to avoid redundant prefixes
                        let relative_path = compute_relative_install_path(
                            &base_target,
                            Path::new(&filename),
                            flatten,
                        );
                        normalize_path_for_storage(normalize_path(&base_target.join(relative_path)))
                    }
                };

                resources.push(LockedResource {
                    name: resource_name.clone(),
                    source: Some(source_name.to_string()),
                    url: Some(source_url.clone()),
                    path: normalize_path_for_storage(matched_path.to_string_lossy().to_string()),
                    version: resolved_version.clone(), // Use the resolved version (e.g., "main")
                    resolved_commit: Some(resolved_commit.clone()),
                    checksum: String::new(),
                    installed_at,
                    dependencies: self.get_dependencies_for(
                        &resource_name,
                        Some(source_name),
                        resource_type,
                        dep.get_tool(),
                    ),
                    resource_type,
                    tool: Some(
                        dep.get_tool()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| self.manifest.get_default_tool(resource_type)),
                    ),
                    manifest_alias: Some(name.to_string()), // Pattern dependency: preserve original alias
                    applied_patches: lockfile_operations::get_patches_for_resource(
                        &self.manifest,
                        resource_type,
                        name,
                    ),
                    install: dep.get_install(),
                    template_vars: "{}".to_string(),
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
    /// Resource types determine installation directories based on tool configuration:
    /// - Agents typically install to `.claude/agents/{name}.md` (claude-code tool)
    /// - Snippets typically install to `.agpm/snippets/{name}.md` (agpm tool)
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
    /// Resolve version conflicts between two dependencies.
    ///
    /// This method implements version conflict resolution strategies when the same
    /// resource is required with different versions by different dependencies.
    ///
    /// # Resolution Strategy
    ///
    /// The current implementation uses a "highest compatible version" strategy:
    /// 1. If one dependency has no version (latest), use the other's version
    /// 2. If both have versions, prefer semantic version comparison
    /// 3. For incompatible versions, warn and use the higher version
    ///
    /// # Future Enhancements
    ///
    /// - Support for version ranges (^1.0.0, ~2.1.0)
    /// - User-configurable resolution strategies
    /// - Interactive conflict resolution
    ///
    /// # Parameters
    ///
    /// - `resource_name`: Name of the conflicting resource
    /// - `existing`: Current version in the dependency map
    /// - `new_dep`: New version being requested
    /// - `requester`: Name of the dependency requesting the new version
    ///
    /// # Returns
    ///
    /// The resolved dependency that satisfies both requirements if possible,
    /// or the higher version with a warning if not compatible.
    async fn resolve_version_conflict(
        &self,
        resource_name: &str,
        existing: &ResourceDependency,
        new_dep: &ResourceDependency,
        requester: &str,
    ) -> Result<ResourceDependency> {
        let existing_version = existing.get_version();
        let new_version = new_dep.get_version();

        // If versions are identical, no conflict
        if existing_version == new_version {
            return Ok(existing.clone());
        }

        // Check if either version is a semver range (not an exact version)
        let is_existing_range = existing_version.is_some_and(|v| {
            v.starts_with('^') || v.starts_with('~') || v.starts_with('>') || v.starts_with('<')
        });
        let is_new_range = new_version.is_some_and(|v| {
            v.starts_with('^') || v.starts_with('~') || v.starts_with('>') || v.starts_with('<')
        });

        // If we have semver ranges, resolve them to a concrete version
        if is_existing_range || is_new_range {
            return self
                .resolve_semver_range_conflict(resource_name, existing, new_dep, requester)
                .await;
        }

        // Log the conflict for user awareness
        tracing::warn!(
            "Version conflict for '{}': existing version {:?} vs {:?} required by '{}'",
            resource_name,
            existing_version.unwrap_or("HEAD"),
            new_version.unwrap_or("HEAD"),
            requester
        );

        // Resolution strategy
        match (existing_version, new_version) {
            (None, Some(_)) => {
                // Existing wants HEAD, new wants specific - use specific
                Ok(new_dep.clone())
            }
            (Some(_), None) => {
                // Existing wants specific, new wants HEAD - keep specific
                Ok(existing.clone())
            }
            (Some(v1), Some(v2)) => {
                // Both have versions - use semver-aware comparison
                use semver::Version;

                // Try to parse as semver (strip 'v' prefix if present)
                let v1_semver = Version::parse(v1.trim_start_matches('v')).ok();
                let v2_semver = Version::parse(v2.trim_start_matches('v')).ok();

                match (v1_semver, v2_semver) {
                    (Some(sv1), Some(sv2)) => {
                        // Both are valid semver - use proper semver comparison
                        if sv1 >= sv2 {
                            tracing::info!(
                                "Resolving conflict: using version {} for {} (semver: {} >= {})",
                                v1,
                                resource_name,
                                sv1,
                                sv2
                            );
                            Ok(existing.clone())
                        } else {
                            tracing::info!(
                                "Resolving conflict: using version {} for {} (semver: {} < {})",
                                v2,
                                resource_name,
                                sv1,
                                sv2
                            );
                            Ok(new_dep.clone())
                        }
                    }
                    (Some(_), None) => {
                        // v1 is semver, v2 is not (branch/commit) - prefer semver
                        tracing::info!(
                            "Resolving conflict: preferring semver version {} over git ref {} for {}",
                            v1,
                            v2,
                            resource_name
                        );
                        Ok(existing.clone())
                    }
                    (None, Some(_)) => {
                        // v1 is not semver (branch/commit), v2 is - prefer semver
                        tracing::info!(
                            "Resolving conflict: preferring semver version {} over git ref {} for {}",
                            v2,
                            v1,
                            resource_name
                        );
                        Ok(new_dep.clone())
                    }
                    (None, None) => {
                        // Neither is semver (both branches/commits)
                        // Use deterministic ordering: alphabetical
                        if v1 <= v2 {
                            tracing::info!(
                                "Resolving conflict: using git ref {} for {} (alphabetically first)",
                                v1,
                                resource_name
                            );
                            Ok(existing.clone())
                        } else {
                            tracing::info!(
                                "Resolving conflict: using git ref {} for {} (alphabetically first)",
                                v2,
                                resource_name
                            );
                            Ok(new_dep.clone())
                        }
                    }
                }
            }
            (None, None) => {
                // Both want HEAD - no conflict
                Ok(existing.clone())
            }
        }
    }

    /// Resolve a version conflict where at least one version is a semver range.
    ///
    /// This method handles version resolution when one or both dependencies specify
    /// semver ranges (like `^1.0.0`, `>=1.5.0`) instead of exact versions. It:
    /// 1. Fetches available versions from the Git repository
    /// 2. Uses the `ConflictDetector` to check if ranges are compatible
    /// 3. Finds the best version that satisfies both constraints
    /// 4. Returns a resolved dependency with the concrete version
    ///
    /// # Arguments
    ///
    /// * `resource_name` - Name of the conflicting resource
    /// * `existing` - Current dependency with version range
    /// * `new_dep` - New dependency with version range
    /// * `requester` - Name of the dependency requesting the new version
    ///
    /// # Returns
    ///
    /// A `ResourceDependency` with the resolved concrete version that satisfies both ranges.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Dependencies are local (no source) but have version ranges
    /// - Source hasn't been synced yet
    /// - Version ranges are incompatible (no common version)
    /// - No available versions satisfy the constraints
    async fn resolve_semver_range_conflict(
        &self,
        resource_name: &str,
        existing: &ResourceDependency,
        new_dep: &ResourceDependency,
        requester: &str,
    ) -> Result<ResourceDependency> {
        use crate::manifest::DetailedDependency;
        use crate::resolver::version_resolution::parse_tags_to_versions;
        use semver::Version;
        use std::collections::HashMap;

        let existing_version = existing.get_version().unwrap_or("HEAD");
        let new_version = new_dep.get_version().unwrap_or("HEAD");

        tracing::info!(
            "Resolving semver range conflict for '{}': existing '{}' vs required '{}'  by '{}'",
            resource_name,
            existing_version,
            new_version,
            requester
        );

        // Get source (both should have same source for transitive deps)
        let source = existing.get_source().or_else(|| new_dep.get_source()).ok_or_else(|| {
            AgpmError::Other {
                message: format!(
                    "Cannot resolve semver ranges for local dependencies: {}",
                    resource_name
                ),
            }
        })?;

        // Get bare repo path
        let repo_path =
            self.version_resolver.get_bare_repo_path(source).ok_or_else(|| AgpmError::Other {
                message: format!("Source '{}' not synced yet", source),
            })?;

        // List available tags from repository
        let tags = self.get_available_versions(repo_path).await?;
        tracing::debug!("Found {} tags for source '{}'", tags.len(), source);

        // Parse tags to semver versions
        let parsed_versions = parse_tags_to_versions(tags);
        let available_versions: Vec<Version> =
            parsed_versions.iter().map(|(_, v)| v.clone()).collect();

        if available_versions.is_empty() {
            return Err(AgpmError::Other {
                message: format!(
                    "No valid semver tags found for source '{}' to resolve range conflict",
                    source
                ),
            }
            .into());
        }

        // Use ConflictDetector to find best version
        let mut detector = ConflictDetector::new();
        let resource_id = format!("{}:{}", source, existing.get_path());
        detector.add_requirement(&resource_id, "existing", existing_version);
        detector.add_requirement(&resource_id, requester, new_version);

        // Check for conflicts
        let conflicts = detector.detect_conflicts();
        if !conflicts.is_empty() {
            return Err(AgpmError::Other {
                message: format!(
                    "Incompatible version ranges for '{}': existing '{}' vs required '{}' by '{}'",
                    resource_name, existing_version, new_version, requester
                ),
            }
            .into());
        }

        // Find best version that satisfies both ranges
        let mut versions_map = HashMap::new();
        versions_map.insert(resource_id.clone(), available_versions);

        let resolved = detector.resolve_conflicts(&versions_map)?;
        let best_version = resolved.get(&resource_id).ok_or_else(|| AgpmError::Other {
            message: format!("Failed to resolve version for '{}'", resource_name),
        })?;

        // Find the tag for this version
        let best_tag = parsed_versions
            .iter()
            .find(|(_, v)| v == best_version)
            .map(|(tag, _)| tag.clone())
            .ok_or_else(|| AgpmError::Other {
                message: format!("Version {} not found in tags", best_version),
            })?;

        tracing::info!(
            "Resolved '{}' to version {} (satisfies both '{}' and '{}')",
            resource_name,
            best_tag,
            existing_version,
            new_version
        );

        // Create new dependency with resolved version
        // We need to create a Detailed variant with the concrete version
        match new_dep {
            ResourceDependency::Detailed(d) => {
                let mut resolved_dep = (**d).clone();
                resolved_dep.version = Some(best_tag);
                resolved_dep.branch = None; // Clear branch/rev since we have concrete version
                resolved_dep.rev = None;
                Ok(ResourceDependency::Detailed(Box::new(resolved_dep)))
            }
            ResourceDependency::Simple(_) => {
                // Create a DetailedDependency from the simple path
                // This shouldn't happen since Simple deps don't have versions, but handle it
                Ok(ResourceDependency::Detailed(Box::new(DetailedDependency {
                    source: Some(source.to_string()),
                    path: existing.get_path().to_string(),
                    version: Some(best_tag),
                    branch: None,
                    rev: None,
                    command: None,
                    args: None,
                    target: None,
                    filename: None,
                    dependencies: None,
                    tool: Some("claude-code".to_string()), // Default tool
                    flatten: None,
                    install: None,

                    template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
                })))
            }
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
    /// - **Manifest Changes**: Reflect additions/modifications to agpm.toml
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

        // Update sources from manifest (handles source URL changes and new sources)
        lockfile.sources.clear();
        for (name, url) in &self.manifest.sources {
            lockfile.add_source(name.clone(), url.clone(), String::new());
        }

        // Determine which dependencies to update
        let deps_to_check: HashSet<String> = if let Some(specific) = deps_to_update {
            specific.into_iter().collect()
        } else {
            // Update all dependencies
            self.manifest.all_dependencies().iter().map(|(name, _)| (*name).to_string()).collect()
        };

        // Get all base dependencies with their types to avoid mis-typing same-named resources
        let base_deps: Vec<(String, ResourceDependency, crate::core::ResourceType)> = self
            .manifest
            .all_dependencies_with_types()
            .into_iter()
            .map(|(name, dep, resource_type)| (name.to_string(), dep.into_owned(), resource_type))
            .collect();

        // Note: We assume the update command has already called pre_sync_sources
        // during the "Syncing sources" phase, so repositories are already available.
        // We just need to prepare and resolve versions now.

        // Prepare remote groups to resolve versions (reuses pre-synced repos)
        // prepare_remote_groups only needs name and dep, not type
        let base_deps_for_prep: Vec<(String, ResourceDependency)> =
            base_deps.iter().map(|(name, dep, _)| (name.clone(), dep.clone())).collect();
        self.prepare_remote_groups(&base_deps_for_prep).await?;

        // First, remove stale entries that are no longer in the manifest
        // This prevents conflicts from commented-out or removed dependencies
        self.remove_stale_manifest_entries(&mut lockfile);

        // Remove old entries for manifest dependencies being updated
        // This handles source changes (e.g., Git -> local path) and type changes
        self.remove_manifest_entries_for_update(&mut lockfile, &deps_to_check);

        // Resolve transitive dependencies (always enabled for update to maintain consistency)
        let deps = self.resolve_transitive_dependencies(&base_deps, true).await?;

        for (name, dep, resource_type) in deps {
            // Determine if this dependency should be skipped:
            // Skip ONLY if it's a manifest dependency that's NOT being updated
            // Always process:
            // - Manifest deps being updated (name in deps_to_check)
            // - Pattern expansions whose alias is being updated
            // - Transitive deps (not in manifest at all)

            let is_manifest_dep = self.manifest.all_dependencies().iter().any(|(n, _)| *n == name);
            let pattern_alias = self.pattern_alias_map.get(&(resource_type, name.clone()));

            let should_skip = is_manifest_dep
                && !deps_to_check.contains(&name)
                && !pattern_alias.is_some_and(|alias| deps_to_check.contains(alias));

            if should_skip {
                // This is a manifest dependency not being updated - skip to avoid unnecessary work
                continue;
            }

            // Check if this is a pattern dependency
            if dep.is_pattern() {
                // Pattern dependencies resolve to multiple resources
                let entries = self.resolve_pattern_dependency(&name, &dep, resource_type).await?;

                // Add each resolved entry to the appropriate resource type with deduplication
                // Use resource_type from transitive resolution to avoid recomputing
                for entry in entries {
                    match resource_type {
                        crate::core::ResourceType::Agent => {
                            // Match by (name, source) to allow same-named resources from different sources
                            if let Some(existing) = lockfile
                                .agents
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.agents.push(entry);
                            }
                        }
                        crate::core::ResourceType::Snippet => {
                            if let Some(existing) = lockfile
                                .snippets
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.snippets.push(entry);
                            }
                        }
                        crate::core::ResourceType::Command => {
                            if let Some(existing) = lockfile
                                .commands
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.commands.push(entry);
                            }
                        }
                        crate::core::ResourceType::Script => {
                            if let Some(existing) = lockfile
                                .scripts
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.scripts.push(entry);
                            }
                        }
                        crate::core::ResourceType::Hook => {
                            if let Some(existing) = lockfile
                                .hooks
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.hooks.push(entry);
                            }
                        }
                        crate::core::ResourceType::McpServer => {
                            if let Some(existing) = lockfile
                                .mcp_servers
                                .iter_mut()
                                .find(|e| e.name == entry.name && e.source == entry.source)
                            {
                                *existing = entry;
                            } else {
                                lockfile.mcp_servers.push(entry);
                            }
                        }
                    }
                }
            } else {
                // Regular single dependency
                // Use resource_type from transitive resolution to avoid recomputing
                let entry = self.resolve_dependency(&name, &dep, resource_type).await?;

                // Use the helper method to add or update the entry
                self.add_or_update_lockfile_entry(&mut lockfile, &name, entry);
            }
        }

        // Progress bar completion is handled by the caller

        // Post-process dependencies to add version information
        self.add_version_to_dependencies(&mut lockfile)?;

        // Detect target-path conflicts before finalizing
        self.detect_target_conflicts(&lockfile)?;

        Ok(lockfile)
    }

    /// Adds a dependency to the conflict detector.
    ///
    /// Builds a resource identifier from the dependency's source and path,
    /// and records it along with the requirer and version constraint.
    ///
    /// # Parameters
    ///
    /// - `_name`: The dependency name (unused, kept for consistency)
    /// - `dep`: The dependency specification
    /// - `required_by`: Identifier of the resource requiring this dependency
    fn add_to_conflict_detector(
        &mut self,
        _name: &str,
        dep: &ResourceDependency,
        required_by: &str,
    ) {
        // Skip local dependencies (no version conflicts possible)
        if dep.is_local() {
            return;
        }

        // Build resource identifier using helper
        let resource_id = dependency_helpers::build_resource_id(dep);

        // Get version constraint (None means HEAD/unspecified)
        let version = dep.get_version().unwrap_or("HEAD");

        // Add to conflict detector
        self.conflict_detector.add_requirement(&resource_id, required_by, version);
    }

    /// Post-processes lockfile entries to add version information to dependencies.
    ///
    /// Updates the `dependencies` field in each lockfile entry from the format
    /// `"resource_type/name"` to `"resource_type/name@version"` by looking up
    /// the resolved version in the lockfile.
    fn add_version_to_dependencies(&self, lockfile: &mut LockFile) -> Result<()> {
        // Build a lookup map: (resource_type, path, source) -> unique_name
        // This allows us to resolve dependency paths to lockfile names
        // We store both the full path and just the filename for flexible lookup
        let mut lookup_map: HashMap<(crate::core::ResourceType, String, Option<String>), String> =
            HashMap::new();

        // Build lookup map from all lockfile entries
        for resource_type in crate::core::ResourceType::all() {
            for entry in lockfile.get_resources(*resource_type) {
                let normalized_path = dependency_helpers::normalize_lookup_path(&entry.path);
                // Store by full path
                lookup_map.insert(
                    (*resource_type, normalized_path.clone(), entry.source.clone()),
                    entry.name.clone(),
                );
                // Also store by filename for backward compatibility
                if let Some(filename) = dependency_helpers::extract_filename_from_path(&entry.path)
                {
                    lookup_map.insert(
                        (*resource_type, filename, entry.source.clone()),
                        entry.name.clone(),
                    );
                }
                // Also store by type-stripped path (for nested resources like agents/helpers/foo.md -> helpers/foo)
                if let Some(stripped) =
                    dependency_helpers::strip_resource_type_directory(&normalized_path)
                {
                    lookup_map.insert(
                        (*resource_type, stripped, entry.source.clone()),
                        entry.name.clone(),
                    );
                }
            }
        }

        // Build a complete map of (resource_type, name, source) -> (source, version) for cross-source lookup
        // This needs to be done before we start mutating entries
        let mut resource_info_map: HashMap<ResourceKey, ResourceInfo> = HashMap::new();

        for resource_type in crate::core::ResourceType::all() {
            for entry in lockfile.get_resources(*resource_type) {
                resource_info_map.insert(
                    (*resource_type, entry.name.clone(), entry.source.clone()),
                    (entry.source.clone(), entry.version.clone()),
                );
            }
        }

        // Helper function to update dependencies in a vector of entries
        let update_deps = |entries: &mut Vec<LockedResource>| {
            for entry in entries {
                let parent_source = entry.source.clone();

                let updated_deps: Vec<String> = entry
                    .dependencies
                    .iter()
                    .map(|dep| {
                        // Parse "resource_type/path" format (e.g., "agent/rust-haiku.md" or "snippet/utils.md")
                        if let Some((_resource_type_str, dep_path)) = dep.split_once('/') {
                            // Parse resource type from string form (accepts both singular and plural)
                            if let Ok(resource_type) =
                                _resource_type_str.parse::<crate::core::ResourceType>()
                            {
                                // Normalize the path for lookup
                                let dep_filename =
                                    dependency_helpers::normalize_lookup_path(dep_path);

                                // Look up the resource in the lookup map (same source as parent)
                                if let Some(dep_name) = lookup_map.get(&(
                                    resource_type,
                                    dep_filename.clone(),
                                    parent_source.clone(),
                                )) {
                                    // Found resource in same source - add version metadata
                                    if let Some((_source, Some(ver))) = resource_info_map.get(&(
                                        resource_type,
                                        dep_name.clone(),
                                        parent_source.clone(),
                                    )) {
                                        return format!("{resource_type}/{dep_name}@{ver}");
                                    }
                                    // Fallback without version if not found in resource_info_map
                                    return format!("{resource_type}/{dep_name}");
                                }

                                // If not found with same source, try adding .md extension
                                let dep_filename_with_ext = format!("{}.md", dep_filename);
                                if let Some(dep_name) = lookup_map.get(&(
                                    resource_type,
                                    dep_filename_with_ext.clone(),
                                    parent_source.clone(),
                                )) {
                                    // Found resource in same source - add version metadata
                                    if let Some((_source, Some(ver))) = resource_info_map.get(&(
                                        resource_type,
                                        dep_name.clone(),
                                        parent_source.clone(),
                                    )) {
                                        return format!("{resource_type}/{dep_name}@{ver}");
                                    }
                                    // Fallback without version if not found in resource_info_map
                                    return format!("{resource_type}/{dep_name}");
                                }

                                // Try looking for resource from ANY source (cross-source dependency)
                                // Format: source:type/name@version
                                for ((rt, filename, src), name) in &lookup_map {
                                    if *rt == resource_type
                                        && (filename == &dep_filename
                                            || filename == &dep_filename_with_ext)
                                    {
                                        // Found in different source - need to include source and version
                                        // Use the pre-built resource info map
                                        if let Some((source, version)) = resource_info_map.get(&(
                                            resource_type,
                                            name.clone(),
                                            src.clone(),
                                        )) {
                                            // Build full reference: source:type/name@version
                                            let mut dep_ref = String::new();
                                            if let Some(src) = source {
                                                dep_ref.push_str(src);
                                                dep_ref.push(':');
                                            }
                                            dep_ref.push_str(&resource_type.to_string());
                                            dep_ref.push('/');
                                            dep_ref.push_str(name);
                                            if let Some(ver) = version {
                                                dep_ref.push('@');
                                                dep_ref.push_str(ver);
                                            }
                                            return dep_ref;
                                        }
                                    }
                                }
                            }
                        }
                        // If parsing fails or resource not found, return as-is
                        dep.clone()
                    })
                    .collect();

                entry.dependencies = updated_deps;
            }
        };

        // Update all entry types
        update_deps(&mut lockfile.agents);
        update_deps(&mut lockfile.snippets);
        update_deps(&mut lockfile.commands);
        update_deps(&mut lockfile.scripts);
        update_deps(&mut lockfile.hooks);
        update_deps(&mut lockfile.mcp_servers);

        Ok(())
    }

    /// Verifies that all dependencies can be resolved without performing resolution.
    ///
    /// This method performs a "dry run" validation of the manifest to detect
    /// issues before attempting actual resolution. It's faster than full resolution
    /// since it doesn't clone repositories or resolve specific versions.
    ///
    /// # Validation Steps
    ///
    /// 1. **Local Path Validation**: Verify local dependencies exist (for absolute paths)
    /// 2. **Source Validation**: Ensure all referenced sources are defined
    /// 3. **Constraint Validation**: Basic syntax checking of version constraints
    ///
    /// # Validation Scope
    ///
    /// - **Manifest Structure**: Validate TOML structure and required fields
    /// - **Source References**: Ensure all sources used by dependencies exist
    /// - **Local Dependencies**: Check absolute paths exist on filesystem
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
        // Try to resolve all dependencies
        let deps: Vec<(&str, std::borrow::Cow<'_, ResourceDependency>, crate::core::ResourceType)> =
            self.manifest.all_dependencies_with_types();

        for (name, dep, _resource_type) in deps {
            if dep.is_local() {
                // Check if local path exists or is relative
                let path = Path::new(dep.get_path());
                if path.is_absolute() && !path.exists() {
                    anyhow::bail!("Local dependency '{}' not found at: {}", name, path.display());
                }
            } else {
                // Verify source exists
                let source_name = dep.get_source().ok_or_else(|| AgpmError::ConfigError {
                    message: format!("Dependency '{name}' has no source specified"),
                })?;

                if !self.manifest.sources.contains_key(source_name) {
                    anyhow::bail!(
                        "Dependency '{name}' references undefined source: '{source_name}'"
                    );
                }
            }
        }

        // Progress bar completion is handled by the caller

        Ok(())
    }
}
