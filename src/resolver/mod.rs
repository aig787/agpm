//! Dependency resolution and conflict detection for AGPM.
//!
//! This module implements the core dependency resolution algorithm that transforms
//! manifest dependencies into locked versions. It handles version constraint solving,
//! conflict detection, transitive dependency resolution,
//! parallel source synchronization, and relative path preservation during installation.
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
use crate::core::{AgpmError, ResourceType};
use crate::git::GitRepo;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency};
use crate::metadata::MetadataExtractor;
use crate::source::SourceManager;
use crate::utils::{compute_relative_install_path, normalize_path, normalize_path_for_storage};
use crate::version::conflict::ConflictDetector;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use self::dependency_graph::{DependencyGraph, DependencyNode};
use self::version_resolver::VersionResolver;

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
    ///
    /// Used by [`prepare_remote_groups`] to resolve all dependencies upfront.
    version_resolver: VersionResolver,
    /// Dependency graph tracking which resources depend on which others.
    ///
    /// Maps from (`resource_type`, name, source) to a list of dependencies in the format
    /// "`resource_type/name`". This is populated during transitive dependency
    /// resolution and used to fill the dependencies field in `LockedResource` entries.
    /// The source is included to prevent cross-source dependency contamination.
    dependency_map: HashMap<(crate::core::ResourceType, String, Option<String>), Vec<String>>,
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
    /// The method performs upsert behavior - if an entry with matching name and source
    /// already exists in the appropriate collection, it will be updated (including version);
    /// otherwise, a new entry is added. This allows version updates (e.g., v1.0 → v2.0)
    /// to replace the existing entry rather than creating duplicates.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - Mutable reference to the lockfile to modify
    /// * `name` - The name of the resource entry (used to determine resource type)
    /// * `entry` - The [`LockedResource`] entry to add or update
    ///
    /// # Resource Type Detection
    ///
    /// Resource types are threaded from the manifest throughout the resolution pipeline,
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
    /// # use agpm_cli::lockfile::{LockFile, LockedResource};
    /// # use agpm_cli::core::ResourceType;
    /// # use agpm_cli::resolver::DependencyResolver;
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
    ///     dependencies: vec![],
    ///     resource_type: ResourceType::Agent,
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
    ///     dependencies: vec![],
    ///     resource_type: ResourceType::Agent,
    /// };
    /// resolver.add_or_update_lockfile_entry(&mut lockfile, "my-agent", updated_entry);
    /// assert_eq!(lockfile.agents.len(), 1); // Still one entry, but updated
    /// ```
    fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        _name: &str,
        entry: LockedResource,
    ) {
        // Get the appropriate resource collection based on the entry's type
        let resources = lockfile.get_resources_mut(entry.resource_type);

        // Use (name, source) matching for deduplication
        // This allows multiple entries with the same name from different sources,
        // which will be caught by conflict detection if they map to the same path
        if let Some(existing) =
            resources.iter_mut().find(|e| e.name == entry.name && e.source == entry.source)
        {
            *existing = entry;
        } else {
            resources.push(entry);
        }
    }

    /// Removes lockfile entries for manifest dependencies that will be re-resolved.
    ///
    /// This method removes old entries for direct manifest dependencies before updating,
    /// which handles the case where a dependency's source or resource type changes.
    /// This prevents duplicate entries with the same name but different sources.
    ///
    /// Pattern-expanded and transitive dependencies are preserved because:
    /// - Pattern expansions will be re-added during resolution with (name, source) matching
    /// - Transitive dependencies aren't manifest keys and won't be removed
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The mutable lockfile to clean
    /// * `manifest_keys` - Set of manifest dependency keys being updated
    fn remove_manifest_entries_for_update(
        &self,
        lockfile: &mut LockFile,
        manifest_keys: &HashSet<String>,
    ) {
        use std::collections::HashSet;

        // Collect (name, source) pairs to remove
        // We use (name, source) tuples to distinguish same-named resources from different sources
        let mut entries_to_remove: HashSet<(String, Option<String>)> = HashSet::new();

        // Step 1: Find direct manifest entries and collect them for transitive traversal
        let mut direct_entries: Vec<(String, Option<String>)> = Vec::new();

        for resource_type in crate::core::ResourceType::all() {
            let resources = lockfile.get_resources(*resource_type);
            for entry in resources {
                // Check if this entry originates from a manifest key being updated
                if manifest_keys.contains(&entry.name)
                    || entry
                        .manifest_alias
                        .as_ref()
                        .is_some_and(|alias| manifest_keys.contains(alias))
                {
                    let key = (entry.name.clone(), entry.source.clone());
                    entries_to_remove.insert(key.clone());
                    direct_entries.push(key);
                }
            }
        }

        // Step 2: For each direct entry, recursively collect its transitive children
        // This ensures that when "agent-A" changes from repo1 to repo2, we also remove
        // all transitive dependencies that came from repo1 via agent-A
        for (parent_name, parent_source) in direct_entries {
            // Find the parent entry in the lockfile
            for resource_type in crate::core::ResourceType::all() {
                if let Some(parent_entry) = lockfile
                    .get_resources(*resource_type)
                    .iter()
                    .find(|e| e.name == parent_name && e.source == parent_source)
                {
                    // Walk its dependency tree
                    Self::collect_transitive_children(
                        lockfile,
                        parent_entry,
                        &mut entries_to_remove,
                    );
                }
            }
        }

        // Step 3: Remove all marked entries
        let should_remove = |entry: &crate::lockfile::LockedResource| {
            entries_to_remove.contains(&(entry.name.clone(), entry.source.clone()))
        };

        lockfile.agents.retain(|entry| !should_remove(entry));
        lockfile.snippets.retain(|entry| !should_remove(entry));
        lockfile.commands.retain(|entry| !should_remove(entry));
        lockfile.scripts.retain(|entry| !should_remove(entry));
        lockfile.hooks.retain(|entry| !should_remove(entry));
        lockfile.mcp_servers.retain(|entry| !should_remove(entry));
    }

    /// Recursively collect all transitive children of a lockfile entry.
    ///
    /// This walks the dependency graph starting from `parent`, following the `dependencies`
    /// field to find all resources that transitively depend on the parent. Only dependencies
    /// with the same source as the parent are collected (to avoid removing unrelated resources).
    ///
    /// The `dependencies` field contains strings in the format:
    /// - `"resource_type/name"` for dependencies from the same source
    /// - `"source:resource_type/name:version"` for explicit source references
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile to search for dependencies
    /// * `parent` - The parent entry whose children we want to collect
    /// * `entries_to_remove` - Set of (name, source) pairs to populate with found children
    fn collect_transitive_children(
        lockfile: &crate::lockfile::LockFile,
        parent: &crate::lockfile::LockedResource,
        entries_to_remove: &mut std::collections::HashSet<(String, Option<String>)>,
    ) {
        // For each dependency declared by this parent
        for dep_ref in &parent.dependencies {
            // Parse dependency reference: "source:resource_type/name:version" or "resource_type/name"
            // Examples: "repo1:snippet/utils:v1.0.0" or "agent/helper"
            let (dep_source, dep_name) = if let Some(colon_pos) = dep_ref.find(':') {
                // Format: "source:resource_type/name:version"
                let source_part = &dep_ref[..colon_pos];
                let rest = &dep_ref[colon_pos + 1..];
                // Find the resource_type/name part (before optional :version)
                let type_name_part = if let Some(ver_colon) = rest.rfind(':') {
                    &rest[..ver_colon]
                } else {
                    rest
                };
                // Extract name from "resource_type/name"
                if let Some(slash_pos) = type_name_part.find('/') {
                    let name = &type_name_part[slash_pos + 1..];
                    (Some(source_part.to_string()), name.to_string())
                } else {
                    continue; // Invalid format, skip
                }
            } else {
                // Format: "resource_type/name"
                if let Some(slash_pos) = dep_ref.find('/') {
                    let name = &dep_ref[slash_pos + 1..];
                    // Inherit parent's source
                    (parent.source.clone(), name.to_string())
                } else {
                    continue; // Invalid format, skip
                }
            };

            // Find the dependency entry with matching name and source
            for resource_type in crate::core::ResourceType::all() {
                if let Some(dep_entry) = lockfile
                    .get_resources(*resource_type)
                    .iter()
                    .find(|e| e.name == dep_name && e.source == dep_source)
                {
                    let key = (dep_entry.name.clone(), dep_entry.source.clone());

                    // Add to removal set and recurse (if not already processed)
                    if !entries_to_remove.contains(&key) {
                        entries_to_remove.insert(key);
                        // Recursively collect this dependency's children
                        Self::collect_transitive_children(lockfile, dep_entry, entries_to_remove);
                    }
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

        // Map of (installed_at path, resolved_commit) -> list of dependency names
        // Two dependencies with the same path AND same commit are NOT a conflict
        let mut path_map: HashMap<(String, Option<String>), Vec<String>> = HashMap::new();

        // Collect all resources from lockfile
        // Note: Hooks and MCP servers are excluded because they're configuration-only
        // resources that are designed to share config files (.claude/settings.local.json
        // for hooks, .mcp.json for MCP servers), not individual files that would conflict.
        let all_resources: Vec<(&str, &LockedResource)> = lockfile
            .agents
            .iter()
            .map(|r| (r.name.as_str(), r))
            .chain(lockfile.snippets.iter().map(|r| (r.name.as_str(), r)))
            .chain(lockfile.commands.iter().map(|r| (r.name.as_str(), r)))
            .chain(lockfile.scripts.iter().map(|r| (r.name.as_str(), r)))
            // Hooks and MCP servers intentionally omitted - they share config files
            .collect();

        // Build the path map with commit information
        for (name, resource) in &all_resources {
            let key = (resource.installed_at.clone(), resource.resolved_commit.clone());
            path_map.entry(key).or_default().push((*name).to_string());
        }

        // Now check for actual conflicts: same path but DIFFERENT commits
        // Group by path only to find potential conflicts
        let mut path_only_map: HashMap<String, Vec<(&str, &LockedResource)>> = HashMap::new();
        for (name, resource) in &all_resources {
            path_only_map.entry(resource.installed_at.clone()).or_default().push((name, resource));
        }

        // Find conflicts (same path with different commits)
        let mut conflicts: Vec<(String, Vec<String>)> = Vec::new();
        for (path, resources) in path_only_map {
            if resources.len() > 1 {
                // Check if they have different commits
                let commits: std::collections::HashSet<_> =
                    resources.iter().map(|(_, r)| &r.resolved_commit).collect();

                // Only a conflict if different commits
                if commits.len() > 1 {
                    let names: Vec<String> =
                        resources.iter().map(|(n, _)| (*n).to_string()).collect();
                    conflicts.push((path, names));
                }
            }
        }

        if !conflicts.is_empty() {
            // Build a detailed error message
            let mut error_msg = String::from(
                "Target path conflicts detected:\n\n\
                 Multiple dependencies resolve to the same installation path with different content.\n\
                 This would cause files to overwrite each other.\n\n",
            );

            for (path, names) in &conflicts {
                error_msg.push_str(&format!(
                    "  Path: {}\n  Conflicts: {}\n\n",
                    path,
                    names.join(", ")
                ));
            }

            error_msg.push_str(
                "To resolve this conflict:\n\
                 1. Use custom 'target' field to specify different installation paths:\n\
                    Example: target = \"custom/subdir/file.md\"\n\n\
                 2. Use custom 'filename' field to specify different filenames:\n\
                    Example: filename = \"utils-v2.md\"\n\n\
                 3. For transitive dependencies, add them as direct dependencies with custom target/filename\n\n\
                 4. Ensure pattern dependencies don't overlap with single-file dependencies\n\n\
                 Note: This often occurs when different dependencies have transitive dependencies\n\
                 with the same name but from different sources.",
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
        // Use (resource_type, name, source) as key to distinguish same-named resources from different sources
        let mut all_deps: HashMap<
            (crate::core::ResourceType, String, Option<String>),
            ResourceDependency,
        > = HashMap::new();
        let mut processed: HashSet<(crate::core::ResourceType, String, Option<String>)> =
            HashSet::new();
        let mut queue: Vec<(String, ResourceDependency, Option<crate::core::ResourceType>)> =
            Vec::new();

        // Add initial dependencies to queue with their threaded types
        for (name, dep, resource_type) in base_deps {
            let source = dep.get_source().map(std::string::ToString::to_string);
            queue.push((name.clone(), dep.clone(), Some(*resource_type)));
            all_deps.insert((*resource_type, name.clone(), source), dep.clone());
        }

        // Process queue to discover transitive dependencies
        while let Some((name, dep, resource_type)) = queue.pop() {
            let source = dep.get_source().map(std::string::ToString::to_string);
            let resource_type =
                resource_type.expect("resource_type should always be threaded through queue");
            let key = (resource_type, name.clone(), source.clone());

            // Check if this queue entry is stale (superseded by conflict resolution)
            // IMPORTANT: This must come BEFORE the processed check so that conflict-resolved
            // entries can be reprocessed even if an older version was already processed.
            // If all_deps has a different version than what we popped, skip this entry
            if let Some(current_dep) = all_deps.get(&key) {
                if current_dep.get_version() != dep.get_version() {
                    // This entry was superseded by conflict resolution, skip it
                    continue;
                }
            }

            if processed.contains(&key) {
                continue;
            }

            processed.insert(key.clone());

            // Handle pattern dependencies by expanding them to concrete files
            if dep.is_pattern() {
                // Expand the pattern to get all matching files
                match self.expand_pattern_to_concrete_deps(&name, &dep, resource_type).await {
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
                            let concrete_key =
                                (resource_type, concrete_name.clone(), concrete_source);

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

            // Get the resource content to extract metadata
            let content = self.fetch_resource_content(&name, &dep).await
                .with_context(|| format!(
                    "Failed to fetch resource '{name}' for transitive dependency extraction"
                ))?;

            // Extract metadata from the resource
            let path = PathBuf::from(dep.get_path());
            let metadata =
                MetadataExtractor::extract(&path, &content, self.manifest.project.as_ref())?;

            // Process transitive dependencies if present
            if let Some(deps_map) = metadata.dependencies {
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
                            .with_context(|| format!(
                                "Failed to get parent path for transitive dependencies of '{}'",
                                name
                            ))?;

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
                        } else {
                            // For regular paths, fully resolve and canonicalize
                            crate::utils::resolve_file_relative_path(
                                &parent_file_path,
                                &dep_spec.path,
                            )
                            .with_context(|| format!(
                                "Failed to resolve transitive dependency '{}' for '{}'",
                                dep_spec.path, name
                            ))?
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
                                // Explicit tool specified in YAML frontmatter - use it
                                Some(explicit_tool.clone())
                            } else {
                                // No explicit tool - use configured default for this resource type
                                // This respects [default-tools] configuration in the manifest
                                Some(self.manifest.get_default_tool(dep_resource_type))
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
                                // Explicit tool specified in YAML frontmatter - use it
                                Some(explicit_tool.clone())
                            } else {
                                // No explicit tool - use configured default for this resource type
                                // This respects [default-tools] configuration in the manifest
                                Some(self.manifest.get_default_tool(dep_resource_type))
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
                            }))
                        };

                        // Generate a name for the transitive dependency
                        // Use explicit name from DependencySpec if provided, otherwise derive from path
                        let trans_name = dep_spec
                            .name
                            .clone()
                            .unwrap_or_else(|| self.generate_dependency_name(trans_dep.get_path()));

                        // Add to graph (use source-aware nodes to prevent false cycles)
                        let trans_source =
                            trans_dep.get_source().map(std::string::ToString::to_string);
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
                        let from_key = (resource_type, name.clone(), source.clone());
                        let dep_ref = format!("{dep_resource_type}/{trans_name}");
                        self.dependency_map.entry(from_key).or_default().push(dep_ref);

                        // Add to conflict detector for tracking version requirements
                        self.add_to_conflict_detector(&trans_name, &trans_dep, &name);

                        // Check for version conflicts and resolve them
                        let trans_key =
                            (dep_resource_type, trans_name.clone(), trans_source.clone());

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
    /// * `name` - The dependency name (used for logging)
    /// * `dep` - The pattern-based dependency to expand
    /// * `resource_type` - The resource type (for path construction)
    ///
    /// # Returns
    ///
    /// A vector of (name, ResourceDependency) tuples for each matched file.
    async fn expand_pattern_to_concrete_deps(
        &self,
        _name: &str,
        dep: &ResourceDependency,
        _resource_type: crate::core::ResourceType,
    ) -> Result<Vec<(String, ResourceDependency)>> {
        use crate::manifest::DetailedDependency;

        let pattern = dep.get_path();

        if dep.is_local() {
            // Local pattern dependency - search in filesystem
            // For absolute patterns, use the parent directory as base and strip the pattern to just the filename part
            // For relative patterns, use current directory
            let pattern_path = Path::new(pattern);
            let (base_path, search_pattern) = if pattern_path.is_absolute() {
                // Absolute pattern: extract base directory and relative pattern
                // Example: "/tmp/xyz/agents/*.md" -> base="/tmp/xyz", pattern="agents/*.md"
                let components: Vec<_> = pattern_path.components().collect();

                // Find the first component with a glob character
                let glob_idx = components.iter().position(|c| {
                    let s = c.as_os_str().to_string_lossy();
                    s.contains('*') || s.contains('?') || s.contains('[')
                });

                if let Some(idx) = glob_idx {
                    // Split at the glob component
                    let base_components = &components[..idx];
                    let pattern_components = &components[idx..];

                    let base: PathBuf = base_components.iter().collect();
                    let pattern: String = pattern_components
                        .iter()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/");

                    (base, pattern)
                } else {
                    // No glob characters, use as-is
                    (PathBuf::from("."), pattern.to_string())
                }
            } else {
                // Relative pattern, use current directory
                (PathBuf::from("."), pattern.to_string())
            };

            let pattern_resolver = crate::pattern::PatternResolver::new();
            let matches = pattern_resolver.resolve(&search_pattern, &base_path)?;

            // Get tool, target, and flatten from parent pattern dependency
            let (tool, target, flatten) = match dep {
                ResourceDependency::Detailed(d) => (d.tool.clone(), d.target.clone(), d.flatten),
                _ => (None, None, None),
            };

            let mut concrete_deps = Vec::new();
            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);
                // Convert matched path to absolute by joining with base_path
                let absolute_path = base_path.join(&matched_path);
                let concrete_path = absolute_path.to_string_lossy().to_string();

                // Create a non-pattern Detailed dependency, inheriting tool, target, and flatten from parent
                concrete_deps.push((
                    resource_name,
                    ResourceDependency::Detailed(Box::new(DetailedDependency {
                        source: None,
                        path: concrete_path,
                        version: None,
                        branch: None,
                        rev: None,
                        command: None,
                        args: None,
                        target: target.clone(),
                        filename: None,
                        dependencies: None,
                        tool: tool.clone(),
                        flatten,
                    })),
                ));
            }

            Ok(concrete_deps)
        } else {
            // Git-based remote dependency - need worktree
            let source_name = dep
                .get_source()
                .ok_or_else(|| anyhow::anyhow!("Pattern dependency missing source"))?;

            // Get the prepared worktree for this source/version
            let version_key = dep
                .get_version()
                .map_or_else(|| "HEAD".to_string(), std::string::ToString::to_string);
            let prepared_key = Self::group_key(source_name, &version_key);

            let prepared = self.prepared_versions.get(&prepared_key).ok_or_else(|| {
                anyhow::anyhow!(
                    "Prepared state missing for source '{}' @ '{}'. Pattern expansion requires prepared worktree.",
                    source_name,
                    version_key
                )
            })?;

            let repo_path = &prepared.worktree_path;

            // Search for matching files in the repository
            let pattern_resolver = crate::pattern::PatternResolver::new();
            let matches = pattern_resolver.resolve(pattern, Path::new(repo_path))?;

            // Create concrete dependencies for each match
            let mut concrete_deps = Vec::new();
            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);

                // Use the matched path as-is (it's already relative to repo root)
                // The matched path includes the resource type directory (e.g., "snippets/helper-one.md")
                let concrete_path = matched_path.to_string_lossy().to_string();

                // Get tool, target, and flatten from parent
                let (tool, target, flatten) = match dep {
                    ResourceDependency::Detailed(d) => {
                        (d.tool.clone(), d.target.clone(), d.flatten)
                    }
                    _ => (None, None, None),
                };

                // Create a non-pattern Detailed dependency, inheriting tool, target, and flatten from parent
                concrete_deps.push((
                    resource_name,
                    ResourceDependency::Detailed(Box::new(DetailedDependency {
                        source: Some(source_name.to_string()),
                        path: concrete_path,
                        version: dep.get_version().map(std::string::ToString::to_string),
                        branch: None,
                        rev: None,
                        command: None,
                        args: None,
                        target, // Inherit custom target from parent pattern
                        filename: None,
                        dependencies: None,
                        tool,
                        flatten,
                    })),
                ));
            }

            Ok(concrete_deps)
        }
    }

    /// Generate a dependency name from a path.
    /// Creates collision-resistant names by preserving directory structure.
    fn generate_dependency_name(&self, path: &str) -> String {
        // Convert path to a collision-resistant name
        // Example: "agents/ai/helper.md" -> "ai/helper"
        // Example: "snippets/commands/commit.md" -> "commands/commit"
        // Example: "commit.md" -> "commit"
        // Example (absolute): "/private/tmp/shared/snippets/utils.md" -> "/private/tmp/shared/snippets/utils"
        // Example (Windows absolute): "C:/team/tools/foo.md" -> "C:/team/tools/foo"
        // Example (parent-relative): "../shared/utils.md" -> "../shared/utils"

        let path = Path::new(path);

        // Get the path without extension
        let without_ext = path.with_extension("");

        // Convert to string and normalize separators to forward slashes
        // This ensures consistent behavior on Windows where Path::to_string_lossy()
        // produces backslashes, which would break our split('/') logic below
        let path_str = without_ext.to_string_lossy().replace('\\', "/");

        // Check if this is an absolute path or starts with ../ (cross-directory)
        // Note: With the fix to always use manifest-relative paths (even with ../),
        // lockfiles should never contain absolute paths. We check path.is_absolute()
        // defensively for manually-edited lockfiles.
        let is_absolute = path.is_absolute();
        let is_cross_directory = path_str.starts_with("../");

        // If the path has multiple components, skip the first directory (resource type)
        // to avoid redundancy, but keep subdirectories for uniqueness
        // EXCEPTIONS that keep all components to avoid collisions:
        // 1. Absolute paths (e.g., C:/team/tools/foo.md vs D:/team/tools/foo.md)
        // 2. Cross-directory paths with ../ (e.g., ../shared/a.md vs ../other/a.md)
        let components: Vec<&str> = path_str.split('/').collect();
        if is_absolute || is_cross_directory {
            // Keep all components to avoid collisions
            path_str.to_string()
        } else if components.len() > 1 {
            // Regular relative path: skip the first component (e.g., "agents", "snippets") and join the rest
            components[1..].join("/")
        } else {
            // Single component, just return it
            components[0].to_string()
        }
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
                            // Provide helpful error message with context
                            let base_msg = format!(
                                "Resource type '{}' is not supported by tool '{}' for dependency '{}'",
                                resource_type, artifact_type, name
                            );

                            // Check if this looks like a tool name was used as resource type
                            let resource_type_str = resource_type.to_string();
                            let hint = if ["claude-code", "opencode", "agpm"].contains(&resource_type_str.as_str()) {
                                format!(
                                    "\n\nIt looks like '{}' is a tool name, not a resource type.\n\
                                    In transitive dependencies, use resource types (agents, snippets, commands)\n\
                                    as section headers, then specify 'tool: {}' within each dependency.",
                                    resource_type_str, resource_type_str
                                )
                            } else {
                                format!(
                                    "\n\nValid resource types: agent, command, snippet, hook, mcp-server, script\n\
                                    Source file: {}",
                                    dep.get_path()
                                )
                            };

                            anyhow::anyhow!("{}{}", base_msg, hint)
                        })?;

                    // Determine flatten behavior: use explicit setting or tool config default
                    let flatten = dep
                        .get_flatten()
                        .or_else(|| {
                            // Get flatten default from tool config
                            self.manifest
                                .get_tool_config(artifact_type)
                                .and_then(|config| config.resources.get(resource_type.to_plural()))
                                .and_then(|resource_config| resource_config.flatten)
                        })
                        .unwrap_or(false); // Default to false if not configured

                    let path = if let Some(custom_target) = dep.get_target() {
                        // Custom target is relative to the artifact's resource directory
                        let base_target = PathBuf::from(artifact_path.display().to_string())
                            .join(custom_target.trim_start_matches('/'));
                        // For custom targets, still strip prefix based on the original artifact path
                        // (not the custom target), to avoid duplicate directories
                        let relative_path = compute_relative_install_path(
                            &artifact_path,
                            Path::new(&filename),
                            flatten,
                        );
                        base_target.join(relative_path)
                    } else {
                        // Use artifact configuration for default path
                        let relative_path = compute_relative_install_path(
                            &artifact_path,
                            Path::new(&filename),
                            flatten,
                        );
                        artifact_path.join(relative_path)
                    };
                    normalize_path_for_storage(normalize_path(&path))
                }
            };

            Ok(LockedResource {
                name: unique_name,
                source: None,
                url: None,
                path: normalize_path_for_storage(dep.get_path()),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at,
                dependencies: self.get_dependencies_for(name, None, resource_type),
                resource_type,
                tool: Some(
                    dep.get_tool()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| self.manifest.get_default_tool(resource_type)),
                ),
                manifest_alias: self
                    .pattern_alias_map
                    .get(&(resource_type, name.to_string()))
                    .cloned(), // Check if this came from a pattern expansion
                applied_patches: HashMap::new(), // Populated during installation, not resolution
            })
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
                            // Provide helpful error message with context
                            let base_msg = format!(
                                "Resource type '{}' is not supported by tool '{}' for dependency '{}'",
                                resource_type, artifact_type, name
                            );

                            // Check if this looks like a tool name was used as resource type
                            let resource_type_str = resource_type.to_string();
                            let hint = if ["claude-code", "opencode", "agpm"].contains(&resource_type_str.as_str()) {
                                format!(
                                    "\n\nIt looks like '{}' is a tool name, not a resource type.\n\
                                    In transitive dependencies, use resource types (agents, snippets, commands)\n\
                                    as section headers, then specify 'tool: {}' within each dependency.",
                                    resource_type_str, resource_type_str
                                )
                            } else {
                                format!(
                                    "\n\nValid resource types: agent, command, snippet, hook, mcp-server, script\n\
                                    Source file: {}",
                                    dep.get_path()
                                )
                            };

                            anyhow::anyhow!("{}{}", base_msg, hint)
                        })?;

                    // Determine flatten behavior: use explicit setting or tool config default
                    let flatten = dep
                        .get_flatten()
                        .or_else(|| {
                            // Get flatten default from tool config
                            self.manifest
                                .get_tool_config(artifact_type)
                                .and_then(|config| config.resources.get(resource_type.to_plural()))
                                .and_then(|resource_config| resource_config.flatten)
                        })
                        .unwrap_or(false); // Default to false if not configured

                    let path = if let Some(custom_target) = dep.get_target() {
                        // Custom target is relative to the artifact's resource directory
                        let base_target = PathBuf::from(artifact_path.display().to_string())
                            .join(custom_target.trim_start_matches('/'));
                        // For custom targets, still strip prefix based on the original artifact path
                        // (not the custom target), to avoid duplicate directories
                        let relative_path = compute_relative_install_path(
                            &artifact_path,
                            Path::new(&filename),
                            flatten,
                        );
                        base_target.join(relative_path)
                    } else {
                        // Use artifact configuration for default path
                        let relative_path = compute_relative_install_path(
                            &artifact_path,
                            Path::new(&filename),
                            flatten,
                        );
                        artifact_path.join(relative_path)
                    };
                    normalize_path_for_storage(normalize_path(&path))
                }
            };

            Ok(LockedResource {
                name: unique_name,
                source: Some(source_name.to_string()),
                url: Some(source_url.clone()),
                path: normalize_path_for_storage(dep.get_path()),
                version: resolved_version, // Resolved version (tag/branch like "v2.1.4" or "main")
                resolved_commit: Some(resolved_commit),
                checksum: String::new(), // Will be calculated during installation
                installed_at,
                dependencies: self.get_dependencies_for(name, Some(source_name), resource_type),
                resource_type,
                tool: Some(artifact_type_string),
                manifest_alias: self
                    .pattern_alias_map
                    .get(&(resource_type, name.to_string()))
                    .cloned(), // Check if this came from a pattern expansion
                applied_patches: HashMap::new(), // Populated during installation, not resolution
            })
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
    ) -> Vec<String> {
        // Use the threaded resource_type parameter from the manifest
        // This prevents type misclassification when same names exist across types
        let key = (resource_type, name.to_string(), source.map(std::string::ToString::to_string));
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
                let full_relative_path = if base_path == Path::new(".") {
                    crate::utils::normalize_path_for_storage(
                        matched_path.to_string_lossy().to_string(),
                    )
                } else {
                    crate::utils::normalize_path_for_storage(format!(
                        "{}/{}",
                        base_path.display(),
                        matched_path.display()
                    ))
                };

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
                        let filename = {
                            // Construct full path from base_path and matched_path for extraction
                            let full_path = if base_path == Path::new(".") {
                                matched_path.to_path_buf()
                            } else {
                                base_path.join(&matched_path)
                            };
                            extract_meaningful_path(&full_path)
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
                    source: None,
                    url: None,
                    path: full_relative_path,
                    version: None,
                    resolved_commit: None,
                    checksum: String::new(),
                    installed_at,
                    dependencies: self.get_dependencies_for(&resource_name, None, resource_type),
                    resource_type,
                    tool: Some(
                        dep.get_tool()
                            .map(std::string::ToString::to_string)
                            .unwrap_or_else(|| self.manifest.get_default_tool(resource_type)),
                    ),
                    manifest_alias: Some(name.to_string()), // Pattern dependency: preserve original alias
                    applied_patches: HashMap::new(), // Populated during installation, not resolution
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
                    ),
                    resource_type,
                    tool: Some(
                        dep.get_tool()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| self.manifest.get_default_tool(resource_type)),
                    ),
                    manifest_alias: Some(name.to_string()), // Pattern dependency: preserve original alias
                    applied_patches: HashMap::new(), // Populated during installation, not resolution
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

        // Build resource identifier: source:path
        let source = dep.get_source().unwrap_or("unknown");
        let path = dep.get_path();
        let resource_id = format!("{source}:{path}");

        // Get version constraint (None means HEAD/unspecified)
        if let Some(version) = dep.get_version() {
            // Add to conflict detector
            self.conflict_detector.add_requirement(&resource_id, required_by, version);
        } else {
            // No version specified - use HEAD marker
            self.conflict_detector.add_requirement(&resource_id, required_by, "HEAD");
        }
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

        // Helper to normalize path (strip leading ./, etc.)
        let normalize_path = |path: &str| -> String { path.trim_start_matches("./").to_string() };

        // Helper to extract filename from path
        let extract_filename = |path: &str| -> Option<String> {
            path.split('/').next_back().map(std::string::ToString::to_string)
        };

        // Build lookup map from all lockfile entries
        for entry in &lockfile.agents {
            let normalized_path = normalize_path(&entry.path);
            // Store by full path
            lookup_map.insert(
                (crate::core::ResourceType::Agent, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );
            // Also store by filename for backward compatibility
            if let Some(filename) = extract_filename(&entry.path) {
                lookup_map.insert(
                    (crate::core::ResourceType::Agent, filename, entry.source.clone()),
                    entry.name.clone(),
                );
            }
        }
        for entry in &lockfile.snippets {
            let normalized_path = normalize_path(&entry.path);
            lookup_map.insert(
                (crate::core::ResourceType::Snippet, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );
            if let Some(filename) = extract_filename(&entry.path) {
                lookup_map.insert(
                    (crate::core::ResourceType::Snippet, filename, entry.source.clone()),
                    entry.name.clone(),
                );
            }
        }
        for entry in &lockfile.commands {
            let normalized_path = normalize_path(&entry.path);
            lookup_map.insert(
                (crate::core::ResourceType::Command, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );
            if let Some(filename) = extract_filename(&entry.path) {
                lookup_map.insert(
                    (crate::core::ResourceType::Command, filename, entry.source.clone()),
                    entry.name.clone(),
                );
            }
        }
        for entry in &lockfile.scripts {
            let normalized_path = normalize_path(&entry.path);
            lookup_map.insert(
                (crate::core::ResourceType::Script, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );
            if let Some(filename) = extract_filename(&entry.path) {
                lookup_map.insert(
                    (crate::core::ResourceType::Script, filename, entry.source.clone()),
                    entry.name.clone(),
                );
            }
        }
        for entry in &lockfile.hooks {
            let normalized_path = normalize_path(&entry.path);
            lookup_map.insert(
                (crate::core::ResourceType::Hook, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );
            if let Some(filename) = extract_filename(&entry.path) {
                lookup_map.insert(
                    (crate::core::ResourceType::Hook, filename, entry.source.clone()),
                    entry.name.clone(),
                );
            }
        }
        for entry in &lockfile.mcp_servers {
            let normalized_path = normalize_path(&entry.path);
            lookup_map.insert(
                (
                    crate::core::ResourceType::McpServer,
                    normalized_path.clone(),
                    entry.source.clone(),
                ),
                entry.name.clone(),
            );
            if let Some(filename) = extract_filename(&entry.path) {
                lookup_map.insert(
                    (crate::core::ResourceType::McpServer, filename, entry.source.clone()),
                    entry.name.clone(),
                );
            }
        }

        // Build a complete map of (resource_type, name, source) -> (source, version) for cross-source lookup
        // This needs to be done before we start mutating entries
        let mut resource_info_map: HashMap<ResourceKey, ResourceInfo> = HashMap::new();

        for entry in &lockfile.agents {
            resource_info_map.insert(
                (crate::core::ResourceType::Agent, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
        for entry in &lockfile.snippets {
            resource_info_map.insert(
                (crate::core::ResourceType::Snippet, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
        for entry in &lockfile.commands {
            resource_info_map.insert(
                (crate::core::ResourceType::Command, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
        for entry in &lockfile.scripts {
            resource_info_map.insert(
                (crate::core::ResourceType::Script, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
        for entry in &lockfile.hooks {
            resource_info_map.insert(
                (crate::core::ResourceType::Hook, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
        for entry in &lockfile.mcp_servers {
            resource_info_map.insert(
                (crate::core::ResourceType::McpServer, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }

        // Helper function to update dependencies in a vector of entries
        let update_deps = |entries: &mut Vec<LockedResource>| {
            for entry in entries {
                let parent_source = entry.source.clone();

                let updated_deps: Vec<String> =
                    entry
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
                                    let dep_filename = normalize_path(dep_path);

                                    // Look up the resource in the lookup map (same source as parent)
                                    if let Some(dep_name) = lookup_map.get(&(
                                        resource_type,
                                        dep_filename.clone(),
                                        parent_source.clone(),
                                    )) {
                                        // Found resource in same source - use singular form from enum
                                        return format!("{resource_type}/{dep_name}");
                                    }

                                    // If not found with same source, try adding .md extension
                                    let dep_filename_with_ext = format!("{dep_filename}.md");
                                    if let Some(dep_name) = lookup_map.get(&(
                                        resource_type,
                                        dep_filename_with_ext.clone(),
                                        parent_source.clone(),
                                    )) {
                                        return format!("{resource_type}/{dep_name}");
                                    }

                                    // Try looking for resource from ANY source (cross-source dependency)
                                    // Format: source:type/name:version
                                    for ((rt, filename, src), name) in &lookup_map {
                                        if *rt == resource_type
                                            && (filename == &dep_filename
                                                || filename == &dep_filename_with_ext)
                                        {
                                            // Found in different source - need to include source and version
                                            // Use the pre-built resource info map
                                            if let Some((source, version)) = resource_info_map
                                                .get(&(resource_type, name.clone(), src.clone()))
                                            {
                                                // Build full reference: source:type/name:version
                                                let mut dep_ref = String::new();
                                                if let Some(src) = source {
                                                    dep_ref.push_str(src);
                                                    dep_ref.push(':');
                                                }
                                                dep_ref.push_str(&resource_type.to_string());
                                                dep_ref.push('/');
                                                dep_ref.push_str(name);
                                                if let Some(ver) = version {
                                                    dep_ref.push(':');
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DetailedDependency;
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
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());
        manifest.add_dependency(
            "local-agent".to_string(),
            ResourceDependency::Simple("../agents/local.md".to_string()),
            true,
        );

        // Create dummy file to allow transitive dependency extraction
        let agents_dir = temp_dir.path().parent().unwrap().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("local.md"), "# Local Agent").unwrap();

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

    #[tokio::test]
    async fn test_pre_sync_sources() {
        // Skip test if git is not available
        if std::process::Command::new("git").arg("--version").output().is_err() {
            eprintln!("Skipping test: git not available");
            return;
        }

        // Create a test Git repository with resources
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("test-repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();

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
        std::fs::write(repo_dir.join("agents/test.md"), "# Test Agent\n\nTest content").unwrap();

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
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test-source".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        assert!(!all_resolved.is_empty(), "Resolution should produce resolved versions");

        // Check that v1.0.0 was resolved to a SHA
        let key = ("test-source".to_string(), "v1.0.0".to_string());
        assert!(all_resolved.contains_key(&key), "Should have resolved v1.0.0");

        let sha = all_resolved.get(&key).unwrap();
        assert_eq!(sha.len(), 40, "SHA should be 40 characters");
    }

    #[test]
    fn test_verify_missing_source() {
        let mut manifest = Manifest::new();

        // Add dependency without corresponding source
        manifest.add_dependency(
            "remote-agent".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("nonexistent".to_string()),
                path: "agents/test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let result = resolver.verify();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("undefined source"));
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        let source_url = format!("file://{}", source_dir.display());
        manifest.add_source("test".to_string(), source_url);
        manifest.add_dependency(
            "remote-agent".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());
        manifest.add_dependency(
            "local".to_string(),
            ResourceDependency::Simple("test.md".to_string()),
            true,
        );

        // Create dummy file to allow transitive dependency extraction
        std::fs::write(temp_dir.path().join("test.md"), "# Test").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);
    }

    #[test]
    fn test_verify_with_progress() {
        let mut manifest = Manifest::new();
        manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());
        manifest.add_dependency(
            "agent".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        let source_url = format!("file://{}", source_dir.display());
        manifest.add_source("test".to_string(), source_url);
        manifest.add_dependency(
            "git-agent".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());
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

        // Create dummy files to allow transitive dependency extraction
        std::fs::write(temp_dir.path().join("a1.md"), "# Agent 1").unwrap();
        std::fs::write(temp_dir.path().join("a2.md"), "# Agent 2").unwrap();
        std::fs::write(temp_dir.path().join("s1.md"), "# Snippet 1").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 2);
        assert_eq!(lockfile.snippets.len(), 1);
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
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add local dependency with custom target
        manifest.add_dependency(
            "custom-agent".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: Some("integrations/custom".to_string()),
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );

        // Create dummy file to allow transitive dependency extraction
        std::fs::write(temp_dir.path().parent().unwrap().join("test.md"), "# Test").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "custom-agent");
        // Verify the custom target is relative to the default agents directory
        // Normalize path separators for cross-platform testing
        let normalized_path = normalize_path_for_storage(&agent.installed_at);
        assert!(normalized_path.contains(".claude/agents/integrations/custom"));
        // Path ../test.md extracts to test.md after stripping parent components
        assert_eq!(normalized_path, ".claude/agents/integrations/custom/test.md");
    }

    #[tokio::test]
    async fn test_resolve_without_custom_target() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add local dependency without custom target
        manifest.add_dependency(
            "standard-agent".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );

        // Create dummy file to allow transitive dependency extraction
        std::fs::write(temp_dir.path().parent().unwrap().join("test.md"), "# Test").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "standard-agent");
        // Verify the default target is used
        // Normalize path separators for cross-platform testing
        let normalized_path = normalize_path_for_storage(&agent.installed_at);
        // Path ../test.md extracts to test.md after stripping parent components
        assert_eq!(normalized_path, ".claude/agents/test.md");
    }

    #[tokio::test]
    async fn test_resolve_with_custom_filename() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add local dependency with custom filename
        manifest.add_dependency(
            "my-agent".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: Some("ai-assistant.txt".to_string()),
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );

        // Create dummy file to allow transitive dependency extraction
        std::fs::write(temp_dir.path().parent().unwrap().join("test.md"), "# Test").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "my-agent");
        // Verify the custom filename is used
        // Normalize path separators for cross-platform testing
        let normalized_path = normalize_path_for_storage(&agent.installed_at);
        assert_eq!(normalized_path, ".claude/agents/ai-assistant.txt");
    }

    #[tokio::test]
    async fn test_resolve_with_custom_filename_and_target() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add local dependency with both custom filename and target
        manifest.add_dependency(
            "special-tool".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: None,
                path: "../test.md".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: Some("tools/ai".to_string()),
                filename: Some("assistant.markdown".to_string()),
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );

        // Create dummy file to allow transitive dependency extraction
        std::fs::write(temp_dir.path().parent().unwrap().join("test.md"), "# Test").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.agents.len(), 1);

        let agent = &lockfile.agents[0];
        assert_eq!(agent.name, "special-tool");
        // Verify both custom target and filename are used
        // Custom target is relative to default agents directory
        // Normalize path separators for cross-platform testing
        let normalized_path = normalize_path_for_storage(&agent.installed_at);
        assert_eq!(normalized_path, ".claude/agents/tools/ai/assistant.markdown");
    }

    #[tokio::test]
    async fn test_resolve_script_with_custom_filename() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add script with custom filename (different extension)
        manifest.add_dependency(
            "analyzer".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: None,
                path: "../scripts/data-analyzer-v3.py".to_string(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: Some("analyze.py".to_string()),
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            false, // script (not agent)
        );

        // Create dummy script file to allow transitive dependency extraction
        let scripts_dir = temp_dir.path().parent().unwrap().join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::write(scripts_dir.join("data-analyzer-v3.py"), "#!/usr/bin/env python3").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        // Scripts should be in snippets array for now (based on false flag)
        assert_eq!(lockfile.snippets.len(), 1);

        let script = &lockfile.snippets[0];
        assert_eq!(script.name, "analyzer");
        // Verify custom filename is used (with custom extension)
        // Normalize path separators for cross-platform testing
        // Uses claude-code tool, so snippets go to .claude/snippets/
        let normalized_path = normalize_path_for_storage(&script.installed_at);
        assert_eq!(normalized_path, ".claude/snippets/analyze.py");
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
        manifest.manifest_dir = Some(project_dir.to_path_buf());
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        let source_url = format!("file://{}", source_dir.display());
        manifest.add_source("test".to_string(), source_url);

        // Add pattern dependency for python agents
        manifest.add_dependency(
            "python-tools".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/python-*.md".to_string(),
                version: Some("v1.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        manifest.manifest_dir = Some(project_dir.to_path_buf());
        manifest.add_dependency(
            "custom-agents".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: None,
                path: format!("{}/agents/*.md", project_dir.display()),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: Some("custom/agents".to_string()),
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
            assert!(agent.installed_at.starts_with(".claude/agents/custom/agents/"));
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        let source_url = format!("file://{}", source_dir.display());
        manifest.add_source("test".to_string(), source_url);

        // Add dependencies - initially both at v1.0.0
        manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent1.md".to_string(),
                version: Some("v1.0.0".to_string()), // Start with v1.0.0
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );
        manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent2.md".to_string(),
                version: Some("v1.0.0".to_string()), // Start with v1.0.0
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        updated_manifest.add_source("test".to_string(), format!("file://{}", source_dir.display()));
        updated_manifest.add_dependency(
            "agent1".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent1.md".to_string(),
                version: Some("v2.0.0".to_string()),
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );
        updated_manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/agent2.md".to_string(),
                version: Some("v1.0.0".to_string()), // Keep v1.0.0
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
            true,
        );

        // Now update only agent1
        let cache2 = Cache::with_dir(cache_dir).unwrap();
        let mut resolver2 = DependencyResolver::with_cache(updated_manifest, cache2);
        let updated_lockfile =
            resolver2.update(&initial_lockfile, Some(vec!["agent1".to_string()])).await.unwrap();

        // agent1 should be updated to v2.0.0
        let agent1 = updated_lockfile
            .agents
            .iter()
            .find(|a| a.name == "agent1" && a.version.as_deref() == Some("v2.0.0"))
            .unwrap();
        assert_eq!(agent1.version.as_ref().unwrap(), "v2.0.0");

        // agent2 should remain at v1.0.0
        let agent2 = updated_lockfile
            .agents
            .iter()
            .find(|a| a.name == "agent2" && a.version.as_deref() == Some("v1.0.0"))
            .unwrap();
        assert_eq!(agent2.version.as_ref().unwrap(), "v1.0.0");
    }

    #[tokio::test]
    async fn test_update_all_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());
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

        // Create dummy files to allow transitive dependency extraction
        let parent = temp_dir.path().parent().unwrap();
        std::fs::write(parent.join("a1.md"), "# Agent 1").unwrap();
        std::fs::write(parent.join("a2.md"), "# Agent 2").unwrap();

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

    // NOTE: Comprehensive integration tests for update() with transitive dependencies
    // are in tests/integration_incremental_add.rs. These provide end-to-end testing
    // of the incremental `agpm add dep` scenario which exercises the update() method.

    #[tokio::test]
    async fn test_resolve_hooks_resource_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add hook dependencies
        manifest.hooks.insert(
            "pre-commit".to_string(),
            ResourceDependency::Simple("../hooks/pre-commit.json".to_string()),
        );
        manifest.hooks.insert(
            "post-commit".to_string(),
            ResourceDependency::Simple("../hooks/post-commit.json".to_string()),
        );

        // Create dummy hook files to allow transitive dependency extraction
        let hooks_dir = temp_dir.path().parent().unwrap().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-commit.json"), "{}").unwrap();
        std::fs::write(hooks_dir.join("post-commit.json"), "{}").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.hooks.len(), 2);

        // Check that hooks point to the config file where they're configured
        for hook in &lockfile.hooks {
            assert_eq!(
                hook.installed_at, ".claude/settings.local.json",
                "Hooks should reference the config file where they're configured"
            );
        }
    }

    #[tokio::test]
    async fn test_resolve_scripts_resource_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add script dependencies
        manifest.scripts.insert(
            "build".to_string(),
            ResourceDependency::Simple("../scripts/build.sh".to_string()),
        );
        manifest.scripts.insert(
            "test".to_string(),
            ResourceDependency::Simple("../scripts/test.py".to_string()),
        );

        // Create dummy script files to allow transitive dependency extraction
        let scripts_dir = temp_dir.path().parent().unwrap().join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::write(scripts_dir.join("build.sh"), "#!/bin/bash").unwrap();
        std::fs::write(scripts_dir.join("test.py"), "#!/usr/bin/env python3").unwrap();

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
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add MCP server dependencies
        manifest.mcp_servers.insert(
            "filesystem".to_string(),
            ResourceDependency::Simple("../mcp/filesystem.json".to_string()),
        );
        manifest.mcp_servers.insert(
            "database".to_string(),
            ResourceDependency::Simple("../mcp/database.json".to_string()),
        );

        // Create dummy MCP server files to allow transitive dependency extraction
        let mcp_dir = temp_dir.path().parent().unwrap().join("mcp");
        std::fs::create_dir_all(&mcp_dir).unwrap();
        std::fs::write(mcp_dir.join("filesystem.json"), "{}").unwrap();
        std::fs::write(mcp_dir.join("database.json"), "{}").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.mcp_servers.len(), 2);

        // Check that MCP servers point to the config file where they're configured
        for server in &lockfile.mcp_servers {
            assert_eq!(
                server.installed_at, ".mcp.json",
                "MCP servers should reference the config file where they're configured"
            );
        }
    }

    #[tokio::test]
    async fn test_resolve_commands_resource_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

        // Add command dependencies
        manifest.commands.insert(
            "deploy".to_string(),
            ResourceDependency::Simple("../commands/deploy.md".to_string()),
        );
        manifest.commands.insert(
            "lint".to_string(),
            ResourceDependency::Simple("../commands/lint.md".to_string()),
        );

        // Create dummy command files to allow transitive dependency extraction
        let commands_dir = temp_dir.path().parent().unwrap().join("commands");
        std::fs::create_dir_all(&commands_dir).unwrap();
        std::fs::write(commands_dir.join("deploy.md"), "# Deploy").unwrap();
        std::fs::write(commands_dir.join("lint.md"), "# Lint").unwrap();

        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, cache);

        let lockfile = resolver.resolve().await.unwrap();
        assert_eq!(lockfile.commands.len(), 2);

        // Check that commands are installed to the correct location
        for command in &lockfile.commands {
            // Normalize path separators for cross-platform testing
            let normalized_path = normalize_path_for_storage(&command.installed_at);
            assert!(normalized_path.contains(".claude/commands/"));
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        let source_url = format!("file://{}", source_dir.display());
        manifest.add_source("test".to_string(), source_url);

        // Test version constraint resolution (^1.0.0 should resolve to 1.2.0)
        manifest.add_dependency(
            "constrained-dep".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "test.txt".to_string(),
                version: Some("^1.0.0".to_string()), // Constraint: compatible with 1.x.x
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("nonexistent".to_string()),
                path: "agents/*.md".to_string(), // Pattern path
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        let source_url = format!("file://{}", source_dir.display());
        manifest.add_source("test".to_string(), source_url);

        // Test branch checkout
        manifest.add_dependency(
            "branch-dep".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "test.txt".to_string(),
                version: Some("develop".to_string()), // Branch name
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        // Use file:// URL to ensure it's treated as a Git source, not a local path
        let source_url = format!("file://{}", source_dir.display());
        manifest.add_source("test".to_string(), source_url);

        // Test commit hash checkout (use first 7 chars for short hash)
        manifest.add_dependency(
            "commit-dep".to_string(),
            ResourceDependency::Detailed(Box::new(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "test.txt".to_string(),
                version: Some(commit_hash[..7].to_string()), // Short commit hash
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
                tool: Some("claude-code".to_string()),
                flatten: None,
            })),
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
        assert!(agent.resolved_commit.as_ref().unwrap().starts_with(&commit_hash[..7]));
    }

    #[tokio::test]
    async fn test_mixed_resource_types() {
        let temp_dir = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.manifest_dir = Some(temp_dir.path().to_path_buf());

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

        // Create dummy files for all resource types to allow transitive dependency extraction
        let parent = temp_dir.path().parent().unwrap();
        std::fs::create_dir_all(parent.join("agents")).unwrap();
        std::fs::create_dir_all(parent.join("scripts")).unwrap();
        std::fs::create_dir_all(parent.join("hooks")).unwrap();
        std::fs::create_dir_all(parent.join("commands")).unwrap();
        std::fs::create_dir_all(parent.join("mcp")).unwrap();
        std::fs::write(parent.join("agents/a1.md"), "# Agent").unwrap();
        std::fs::write(parent.join("scripts/build.sh"), "#!/bin/bash").unwrap();
        std::fs::write(parent.join("hooks/pre-commit.json"), "{}").unwrap();
        std::fs::write(parent.join("commands/deploy.md"), "# Deploy").unwrap();
        std::fs::write(parent.join("mcp/filesystem.json"), "{}").unwrap();

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

    #[tokio::test]
    async fn test_resolve_version_conflict_semver_preference() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        // Test: semver version preferred over git branch
        let existing_semver = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
        }));

        let new_branch = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: None,
            branch: Some("main".to_string()),
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
        }));

        let result = resolver
            .resolve_version_conflict("test-agent", &existing_semver, &new_branch, "app1")
            .await;
        assert!(result.is_ok(), "Should succeed with semver preference");
        let resolved = result.unwrap();
        assert_eq!(
            resolved.get_version(),
            Some("v1.0.0"),
            "Should prefer semver version over branch"
        );

        // Test reverse: git branch vs semver version
        let result2 = resolver
            .resolve_version_conflict("test-agent", &new_branch, &existing_semver, "app2")
            .await;
        assert!(result2.is_ok(), "Should succeed with semver preference");
        let resolved2 = result2.unwrap();
        assert_eq!(
            resolved2.get_version(),
            Some("v1.0.0"),
            "Should prefer semver version over branch (reversed order)"
        );
    }

    #[tokio::test]
    async fn test_resolve_version_conflict_semver_comparison() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        // Test: higher semver version wins
        let existing_v1 = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.5.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
        }));

        let new_v2 = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v2.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
        }));

        let result =
            resolver.resolve_version_conflict("test-agent", &existing_v1, &new_v2, "app1").await;
        assert!(result.is_ok(), "Should succeed with higher version");
        let resolved = result.unwrap();
        assert_eq!(resolved.get_version(), Some("v2.0.0"), "Should use higher semver version");

        // Test reverse order
        let result2 =
            resolver.resolve_version_conflict("test-agent", &new_v2, &existing_v1, "app2").await;
        assert!(result2.is_ok(), "Should succeed with higher version");
        let resolved2 = result2.unwrap();
        assert_eq!(
            resolved2.get_version(),
            Some("v2.0.0"),
            "Should use higher semver version (reversed order)"
        );
    }

    #[tokio::test]
    async fn test_resolve_version_conflict_git_refs() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        // Test: git refs use alphabetical ordering
        let existing_main = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: None,
            branch: Some("main".to_string()),
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
        }));

        let new_develop = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: None,
            branch: Some("develop".to_string()),
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
        }));

        let result = resolver
            .resolve_version_conflict("test-agent", &existing_main, &new_develop, "app1")
            .await;
        assert!(result.is_ok(), "Should succeed with alphabetical ordering");
        let resolved = result.unwrap();
        assert_eq!(
            resolved.get_version(),
            Some("develop"),
            "Should use alphabetically first git ref"
        );
    }

    #[tokio::test]
    async fn test_resolve_version_conflict_head_vs_specific() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        // Test: specific version preferred over HEAD (None)
        let existing_head = ResourceDependency::Simple("agents/test.md".to_string());

        let new_specific = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
        }));

        let result = resolver
            .resolve_version_conflict("test-agent", &existing_head, &new_specific, "app1")
            .await;
        assert!(result.is_ok(), "Should succeed with specific version");
        let resolved = result.unwrap();
        assert_eq!(
            resolved.get_version(),
            Some("v1.0.0"),
            "Should prefer specific version over HEAD"
        );
    }

    #[test]
    fn test_generate_dependency_name_manifest_relative() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let resolver = DependencyResolver::with_cache(manifest, cache);

        // Regular relative paths - strip resource type directory
        assert_eq!(resolver.generate_dependency_name("agents/helper.md"), "helper");
        assert_eq!(resolver.generate_dependency_name("snippets/utils.md"), "utils");

        // Cross-directory paths with ../ - keep all components to avoid collisions
        assert_eq!(resolver.generate_dependency_name("../shared/utils.md"), "../shared/utils");
        assert_eq!(resolver.generate_dependency_name("../other/utils.md"), "../other/utils");

        // Ensure different cross-directory paths get different names
        let name1 = resolver.generate_dependency_name("../shared/foo.md");
        let name2 = resolver.generate_dependency_name("../other/foo.md");
        assert_ne!(name1, name2, "Different parent directories should produce different names");
    }
}
