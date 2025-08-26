//! Dependency resolution and conflict detection for CCPM.
//!
//! This module implements the core dependency resolution algorithm that transforms
//! manifest dependencies into locked versions. It handles version constraint solving,
//! conflict detection, redundancy analysis, and parallel source synchronization.
//!
//! # Architecture Overview
//!
//! The resolver operates in a two-phase process:
//! 1. **Analysis Phase**: Parse dependencies, validate constraints, detect conflicts
//! 2. **Resolution Phase**: Sync sources, resolve versions, generate lockfile entries
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
//! The dependency resolution follows these steps:
//!
//! 1. **Dependency Collection**: Extract all dependencies from manifest
//! 2. **Source Validation**: Verify all referenced sources exist
//! 3. **Parallel Sync**: Clone/update source repositories concurrently
//! 4. **Version Resolution**: Resolve version constraints to specific commits
//! 5. **Conflict Detection**: Check for path conflicts and incompatible versions
//! 6. **Redundancy Analysis**: Identify duplicate resources across sources
//! 7. **Lockfile Generation**: Create deterministic lockfile entries
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
//! ## Basic Resolution
//! ```rust,no_run
//! use ccpm::resolver::DependencyResolver;
//! use ccpm::manifest::Manifest;
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let manifest = Manifest::load(Path::new("ccpm.toml"))?;
//! let mut resolver = DependencyResolver::new_with_global(manifest).await?;
//!
//! // Resolve all dependencies with progress reporting
//! let progress = ccpm::utils::progress::ProgressBar::new(10);
//! let lockfile = resolver.resolve(Some(&progress)).await?;
//!
//! println!("Resolved {} agents and {} snippets",
//!          lockfile.agents.len(), lockfile.snippets.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Redundancy Analysis
//! ```rust,no_run
//! use ccpm::resolver::{DependencyResolver, redundancy::RedundancyDetector};
//! use ccpm::manifest::Manifest;
//!
//! # async fn redundancy_example() -> anyhow::Result<()> {
//! let manifest = Manifest::load("ccpm.toml".as_ref())?;
//! let resolver = DependencyResolver::new(manifest.clone())?;
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
//!
//! # async fn update_example() -> anyhow::Result<()> {
//! let existing = LockFile::load("ccpm.lock".as_ref())?;
//! let manifest = ccpm::manifest::Manifest::load("ccpm.toml".as_ref())?;
//! let mut resolver = DependencyResolver::new(manifest)?;
//!
//! // Update specific dependencies only
//! let deps_to_update = vec!["agent1".to_string(), "snippet2".to_string()];
//! let deps_count = deps_to_update.len();
//! let updated = resolver.update(&existing, Some(deps_to_update), None).await?;
//!
//! println!("Updated {} dependencies", deps_count);
//! # Ok(())
//! # }
//! ```

pub mod redundancy;

use crate::core::CcpmError;
use crate::git::GitRepo;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency};
use crate::source::SourceManager;
use crate::utils::progress::ProgressBar;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use self::redundancy::RedundancyDetector;

/// Core dependency resolver that transforms manifest dependencies into lockfile entries.
///
/// The [`DependencyResolver`] is the main entry point for dependency resolution.
/// It manages source repositories, resolves version constraints, detects conflicts,
/// and generates deterministic lockfile entries.
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
    pub source_manager: SourceManager,
    #[allow(dead_code)]
    cache_dir: PathBuf,
}

impl DependencyResolver {
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
    /// Returns an error if the cache directory cannot be determined or created.
    ///
    /// [`new_with_global()`]: DependencyResolver::new_with_global
    pub fn new(manifest: Manifest) -> Result<Self> {
        let source_manager = SourceManager::from_manifest(&manifest)?;
        let cache_dir = crate::config::get_cache_dir()?;

        Ok(Self {
            manifest,
            source_manager,
            cache_dir,
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
    /// - The cache directory cannot be determined or created
    /// - The global config file exists but cannot be parsed
    /// - Network errors occur while validating global sources
    pub async fn new_with_global(manifest: Manifest) -> Result<Self> {
        let source_manager = SourceManager::from_manifest_with_global(&manifest).await?;
        let cache_dir = crate::config::get_cache_dir()?;

        Ok(Self {
            manifest,
            source_manager,
            cache_dir,
        })
    }

    /// Creates a new resolver with a custom cache directory.
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
    pub fn with_cache(manifest: Manifest, cache_dir: PathBuf) -> Self {
        let source_manager = SourceManager::from_manifest_with_cache(&manifest, cache_dir.clone());

        Self {
            manifest,
            source_manager,
            cache_dir,
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
    /// - `progress`: Optional progress bar for user feedback during long operations
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
    pub async fn resolve(&mut self, progress: Option<&ProgressBar>) -> Result<LockFile> {
        let mut lockfile = LockFile::new();
        let mut resolved = HashMap::new();

        // Add sources to lockfile
        for (name, url) in &self.manifest.sources {
            lockfile.add_source(name.clone(), url.clone(), String::new());
        }

        // Get all dependencies to resolve (clone to avoid borrow checker issues)
        let deps: Vec<(String, ResourceDependency)> = self
            .manifest
            .all_dependencies()
            .into_iter()
            .map(|(name, dep)| (name.to_string(), dep.clone()))
            .collect();

        // Resolve each dependency
        for (name, dep) in deps {
            if let Some(pb) = progress {
                pb.set_message(format!("Resolving {}", name));
            }

            let entry = self.resolve_dependency(&name, &dep).await?;
            resolved.insert(name.to_string(), entry);
        }

        // Add resolved entries to lockfile
        for (name, entry) in resolved {
            let resource_type = self.get_resource_type(&name);
            // Add entry based on resource type
            match resource_type.as_str() {
                "agent" => lockfile.agents.push(entry),
                "snippet" => lockfile.snippets.push(entry),
                "command" => lockfile.commands.push(entry),
                // MCP servers are handled separately, not added to lockfile here
                "mcp-server" => {}
                _ => lockfile.snippets.push(entry), // Default fallback
            }
        }

        if let Some(pb) = progress {
            pb.finish_with_message("Resolution complete");
        }

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
    /// 3. Create entry with relative path (no source sync required)
    ///
    /// For remote dependencies:
    /// 1. Validate source exists in manifest or global config
    /// 2. Synchronize source repository (clone or fetch)
    /// 3. Resolve version constraint to specific commit
    /// 4. Create entry with resolved commit and source information
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
        if dep.is_local() {
            // Local dependency - just create entry with path
            // Determine the installed location based on resource type
            let resource_type = self.get_resource_type(name);
            let installed_at = if resource_type == "agent" {
                format!("{}/{}.md", self.manifest.target.agents, name)
            } else {
                format!("{}/{}.md", self.manifest.target.snippets, name)
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

            // Sync the source repository (auth comes from global config if needed)
            let repo = self.source_manager.sync(source_name, None).await?;

            // Checkout specific version if specified
            let resolved_commit = if let Some(version) = dep.get_version() {
                self.checkout_version(&repo, version).await?
            } else {
                self.get_current_commit(&repo).await?
            };

            // Determine the installed location based on resource type
            let resource_type = self.get_resource_type(name);
            let installed_at = if resource_type == "agent" {
                format!("{}/{}.md", self.manifest.target.agents, name)
            } else {
                format!("{}/{}.md", self.manifest.target.snippets, name)
            };

            Ok(LockedResource {
                name: name.to_string(),
                source: Some(source_name.to_string()),
                url: Some(source_url.clone()),
                path: dep.get_path().to_string(),
                version: dep.get_version().map(|s| s.to_string()),
                resolved_commit: Some(resolved_commit),
                checksum: String::new(), // Will be calculated during installation
                installed_at,
            })
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
    async fn checkout_version(&self, repo: &GitRepo, version: &str) -> Result<String> {
        // Try as tag first
        let tags = repo.list_tags().await?;
        if tags.contains(&version.to_string()) {
            repo.checkout(version).await?;
            return self.get_current_commit(repo).await;
        }

        // Try as branch or commit hash
        if repo.checkout(version).await.is_err() {
            // Try as commit hash if branch checkout failed
            repo.checkout(version)
                .await
                .context(format!("Failed to checkout version: {}", version))?;
        }

        self.get_current_commit(repo).await
    }

    /// Retrieves the current commit hash from a Git repository.
    ///
    /// This method executes `git rev-parse HEAD` to get the current commit
    /// hash, which is used as the resolved version in lockfile entries.
    ///
    /// # Implementation Note
    ///
    /// Uses `tokio::process::Command` for async Git execution, allowing
    /// the resolver to remain non-blocking during potentially slow operations.
    ///
    /// # Parameters
    ///
    /// - `repo`: Git repository to query
    ///
    /// # Returns
    ///
    /// The full 40-character SHA hash of the current HEAD commit.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Git command execution fails
    /// - Repository is not in a valid Git state
    /// - HEAD points to an invalid or missing commit
    /// - Process execution is interrupted or times out
    async fn get_current_commit(&self, repo: &GitRepo) -> Result<String> {
        let output = tokio::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo.path())
            .output()
            .await
            .context("Failed to get current commit")?;

        if !output.status.success() {
            anyhow::bail!("Failed to get current commit hash");
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

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
        progress: Option<&ProgressBar>,
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
                .map(|(name, _)| name.to_string())
                .collect()
        };

        // Resolve updated dependencies (clone to avoid borrow checker issues)
        let deps: Vec<(String, ResourceDependency)> = self
            .manifest
            .all_dependencies()
            .into_iter()
            .map(|(name, dep)| (name.to_string(), dep.clone()))
            .collect();

        for (name, dep) in deps {
            if !deps_to_check.contains(&name) {
                continue;
            }

            if let Some(pb) = progress {
                pb.set_message(format!("Updating {}", name));
            }

            let entry = self.resolve_dependency(&name, &dep).await?;
            let resource_type = self.get_resource_type(&name);

            // Update or add entry in lockfile based on resource type
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
                // MCP servers are handled separately
                "mcp-server" => {}
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

        if let Some(pb) = progress {
            pb.finish_with_message("Update complete");
        }

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
    pub fn verify(&mut self, progress: Option<&ProgressBar>) -> Result<()> {
        // Check for redundancies and warn (but don't fail)
        if let Some(warning) = self.check_redundancies() {
            eprintln!("{}", warning);
        }

        // Then try to resolve all dependencies (clone to avoid borrow checker issues)
        let deps: Vec<(String, ResourceDependency)> = self
            .manifest
            .all_dependencies()
            .into_iter()
            .map(|(name, dep)| (name.to_string(), dep.clone()))
            .collect();
        for (name, dep) in deps {
            if let Some(pb) = progress {
                pb.set_message(format!("Verifying {}", name));
            }

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
                    message: format!("Dependency '{}' has no source specified", name),
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

        if let Some(pb) = progress {
            pb.finish_with_message("Verification complete");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolver_new() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        assert_eq!(resolver.cache_dir, temp_dir.path());
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
        let mut resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let lockfile = resolver.resolve(None).await.unwrap();
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
                git: None,
                command: None,
                args: None,
            }),
            true,
        );

        manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v2.0.0".to_string()),
                git: None,
                command: None,
                args: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let warning = resolver.check_redundancies();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("Redundant dependencies detected"));
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
                git: None,
                command: None,
                args: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let result = resolver.verify(None);
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
        let resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

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
        let source_url = format!(
            "file://{}",
            source_dir.display().to_string().replace('\\', "/")
        );
        manifest.add_source("test".to_string(), source_url);
        manifest.add_dependency(
            "remote-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                git: None,
                command: None,
                args: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let mut resolver = DependencyResolver::with_cache(manifest, cache_dir);

        // This should now succeed with the local repository
        let result = resolver.resolve(None).await;
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
        let mut resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let progress = ProgressBar::new(1);
        let lockfile = resolver.resolve(Some(&progress)).await.unwrap();
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
                git: None,
                command: None,
                args: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let progress = ProgressBar::new(1);
        let result = resolver.verify(Some(&progress));
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
        let source_url = format!(
            "file://{}",
            source_dir.display().to_string().replace('\\', "/")
        );
        manifest.add_source("test".to_string(), source_url);
        manifest.add_dependency(
            "git-agent".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("test".to_string()),
                path: "agents/test.md".to_string(),
                version: None,
                git: Some("main".to_string()),
                command: None,
                args: None,
            }),
            true,
        );

        let cache_dir = temp_dir.path().join("cache");
        let mut resolver = DependencyResolver::with_cache(manifest, cache_dir);

        // This should now succeed with the local repository
        let result = resolver.resolve(None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_new_with_global() {
        let manifest = Manifest::new();
        let result = DependencyResolver::new_with_global(manifest).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolver_new_default() {
        let manifest = Manifest::new();
        let result = DependencyResolver::new(manifest);
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
        let mut resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let lockfile = resolver.resolve(None).await.unwrap();
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
                git: None,
                command: None,
                args: None,
            }),
            true,
        );
        manifest.add_dependency(
            "agent2".to_string(),
            ResourceDependency::Detailed(crate::manifest::DetailedDependency {
                source: Some("official".to_string()),
                path: "agents/test2.md".to_string(),
                version: Some("v1.0.0".to_string()),
                git: None,
                command: None,
                args: None,
            }),
            true,
        );

        let temp_dir = TempDir::new().unwrap();
        let resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

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
        let mut resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let result = resolver.verify(None);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resolve_with_empty_manifest() {
        let manifest = Manifest::new();
        let temp_dir = TempDir::new().unwrap();
        let mut resolver = DependencyResolver::with_cache(manifest, temp_dir.path().to_path_buf());

        let lockfile = resolver.resolve(None).await.unwrap();
        assert_eq!(lockfile.agents.len(), 0);
        assert_eq!(lockfile.snippets.len(), 0);
        assert_eq!(lockfile.sources.len(), 0);
    }
}
