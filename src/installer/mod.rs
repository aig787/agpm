//! Shared installation utilities for AGPM resources.
//!
//! This module provides common functionality for installing resources from
//! lockfile entries to the project directory. It's shared between the install
//! and update commands to avoid code duplication. The module includes both
//! installation logic and automatic cleanup of removed or relocated artifacts.
//!
//! # SHA-Based Parallel Installation Architecture
//!
//! The installer uses SHA-based worktrees for optimal parallel resource installation:
//! - **SHA-based worktrees**: Each unique commit gets one worktree for maximum deduplication
//! - **Pre-resolved SHAs**: All versions resolved to SHAs before installation begins
//! - **Concurrency control**: Direct parallelism control via --max-parallel flag
//! - **Context-aware logging**: Each operation includes dependency name for debugging
//! - **Efficient cleanup**: Worktrees are managed by the cache layer for reuse
//! - **Pre-warming**: Worktrees created upfront to minimize installation latency
//! - **Automatic artifact cleanup**: Removes old files when paths change or dependencies are removed
//!
//! # Installation Process
//!
//! 1. **SHA validation**: Ensures all resources have valid 40-character commit SHAs
//! 2. **Worktree pre-warming**: Creates SHA-based worktrees for all unique commits
//! 3. **Parallel processing**: Installs multiple resources concurrently using dedicated worktrees
//! 4. **Content validation**: Validates markdown format and structure
//! 5. **Atomic installation**: Files are written atomically to prevent corruption
//! 6. **Progress tracking**: Real-time progress updates during parallel operations
//! 7. **Artifact cleanup**: Automatically removes old files from previous installations when paths change
//!
//! # Artifact Cleanup (v0.3.18+)
//!
//! The module provides automatic cleanup of obsolete artifacts when:
//! - **Dependencies are removed**: Files from removed dependencies are deleted
//! - **Paths are relocated**: Old files are removed when `installed_at` paths change
//! - **Structure changes**: Empty parent directories are cleaned up recursively
//!
//! The cleanup process:
//! 1. Compares old and new lockfiles to identify removed artifacts
//! 2. Removes files that exist in the old lockfile but not in the new one
//! 3. Recursively removes empty parent directories up to `.claude/`
//! 4. Reports the number of cleaned artifacts to the user
//!
//! See [`cleanup_removed_artifacts()`] for implementation details.
//!
//! # Performance Characteristics
//!
//! - **SHA-based deduplication**: Multiple refs to same commit share one worktree
//! - **Parallel processing**: Multiple dependencies installed simultaneously
//! - **Pre-warming optimization**: Worktrees created upfront to minimize latency
//! - **Parallelism-controlled**: User controls concurrency via --max-parallel flag
//! - **Atomic operations**: Fast, safe file installation with proper error handling
//! - **Reduced disk usage**: No duplicate worktrees for identical commits
//! - **Efficient cleanup**: Minimal overhead for artifact cleanup operations

use crate::lockfile::ResourceId;
use crate::utils::progress::{InstallationPhase, MultiPhaseProgress};
use anyhow::Result;

mod cleanup;
mod context;
pub mod gitignore;
mod resource;
mod selective;

use gitignore::ensure_gitignore_state;

#[cfg(test)]
mod gitignore_tests;
#[cfg(test)]
mod tests;

pub use cleanup::cleanup_removed_artifacts;
pub use context::InstallContext;
pub use gitignore::{add_path_to_gitignore, cleanup_gitignore, update_gitignore};
pub use selective::install_updated_resources;

use resource::{
    apply_resource_patches, compute_file_checksum, read_source_content, render_resource_content,
    should_skip_installation, validate_markdown_content, write_resource_to_disk,
};

/// Type alias for complex installation result tuples to improve code readability.
///
/// This type alias simplifies the return type of parallel installation functions
/// that need to return either success information or error details with context.
/// It was introduced in AGPM v0.3.0 to resolve `clippy::type_complexity` warnings
/// while maintaining clear semantics for installation results.
///
/// # Success Variant: `Ok((String, bool, String, Option<String>))`
///
/// When installation succeeds, the tuple contains:
/// - `String`: Resource name that was processed
/// - `bool`: Whether the resource was actually installed (`true`) or already up-to-date (`false`)
/// - `String`: SHA-256 checksum of the installed file content
/// - `Option<String>`: SHA-256 checksum of the template rendering inputs, or None for non-templated resources
///
/// # Error Variant: `Err((String, anyhow::Error))`
///
/// When installation fails, the tuple contains:
/// - `String`: Resource name that failed to install
/// - `anyhow::Error`: Detailed error information for debugging
///
/// # Usage
///
/// This type is primarily used in parallel installation operations where
/// individual resource results need to be collected and processed:
///
/// ```rust,ignore
/// use agpm_cli::installer::InstallResult;
/// use futures::stream::{self, StreamExt};
///
/// # async fn example() -> anyhow::Result<()> {
/// let results: Vec<InstallResult> = stream::iter(vec!["resource1", "resource2"])
///     .map(|resource_name| async move {
///         // Installation logic here
///         Ok((resource_name.to_string(), true, "sha256:abc123".to_string()))
///     })
///     .buffer_unordered(10)
///     .collect()
///     .await;
///
/// // Process results
/// for result in results {
///     match result {
///         Ok((name, installed, checksum)) => {
///             println!("✓ {}: installed={}, checksum={}", name, installed, checksum);
///         }
///         Err((name, error)) => {
///             eprintln!("✗ {}: {}", name, error);
///         }
///     }
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Design Rationale
///
/// The type alias serves several purposes:
/// - **Clippy compliance**: Resolves `type_complexity` warnings for complex generic types
/// - **Code clarity**: Makes function signatures more readable and self-documenting
/// - **Error context**: Preserves resource name context when installation fails
/// - **Batch processing**: Enables efficient collection and processing of parallel results
type InstallResult = Result<
    (
        crate::lockfile::ResourceId,
        bool,
        String,
        Option<String>,
        crate::manifest::patches::AppliedPatches,
    ),
    (crate::lockfile::ResourceId, anyhow::Error),
>;

/// Results from a successful installation operation.
///
/// This struct encapsulates all the data returned from installing resources,
/// providing a more readable and maintainable alternative to the complex 4-tuple
/// that previously triggered clippy::type_complexity warnings.
///
/// # Fields
///
/// - **installed_count**: Number of resources that were successfully installed
/// - **checksums**: File checksums for each installed resource (ResourceId -> SHA256)
/// - **context_checksums**: Template context checksums for each resource (ResourceId -> SHA256 or None)
/// - **applied_patches**: List of applied patches for each resource (ResourceId -> AppliedPatches)
#[derive(Debug, Clone)]
pub struct InstallationResults {
    /// Number of resources that were successfully installed
    pub installed_count: usize,
    /// File checksums for each installed resource
    pub checksums: Vec<(crate::lockfile::ResourceId, String)>,
    /// Template context checksums for each resource (None if no templating used)
    pub context_checksums: Vec<(crate::lockfile::ResourceId, Option<String>)>,
    /// Applied patch information for each resource
    pub applied_patches:
        Vec<(crate::lockfile::ResourceId, crate::manifest::patches::AppliedPatches)>,
}

impl InstallationResults {
    /// Creates a new InstallationResults instance.
    ///
    /// # Arguments
    ///
    /// * `installed_count` - Number of successfully installed resources
    /// * `checksums` - File checksums for each installed resource
    /// * `context_checksums` - Template context checksums for each resource
    /// * `applied_patches` - Applied patch information for each resource
    pub fn new(
        installed_count: usize,
        checksums: Vec<(crate::lockfile::ResourceId, String)>,
        context_checksums: Vec<(crate::lockfile::ResourceId, Option<String>)>,
        applied_patches: Vec<(
            crate::lockfile::ResourceId,
            crate::manifest::patches::AppliedPatches,
        )>,
    ) -> Self {
        Self {
            installed_count,
            checksums,
            context_checksums,
            applied_patches,
        }
    }

    /// Returns true if no resources were installed.
    pub fn is_empty(&self) -> bool {
        self.installed_count == 0
    }

    /// Returns the number of installed resources.
    pub fn len(&self) -> usize {
        self.installed_count
    }
}

use futures::stream::{self, StreamExt};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cache::Cache;
use crate::core::ResourceIterator;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::Manifest;
use indicatif::ProgressBar;
use std::collections::HashSet;

/// Install a single resource from a lock entry using worktrees for parallel safety.
///
/// This function installs a resource specified by a lockfile entry to the project
/// directory. It uses Git worktrees through the cache layer to enable safe parallel
/// operations without conflicts between concurrent installations.
///
/// # Arguments
///
/// * `entry` - The locked resource to install containing source and version info
/// * `resource_dir` - The subdirectory name for this resource type (e.g., "agents")
/// * `context` - Installation context containing project configuration and cache instance
///
/// # Returns
///
/// Returns `Ok((installed, file_checksum, context_checksum, applied_patches))` where:
/// - `installed` is `true` if the resource was actually installed (new or updated),
///   `false` if the resource already existed and was unchanged
/// - `file_checksum` is the SHA-256 hash of the installed file content (after rendering)
/// - `context_checksum` is the SHA-256 hash of the template rendering inputs, or None for non-templated resources
/// - `applied_patches` contains information about any patches that were applied during installation
///
/// # Worktree Usage
///
/// For remote resources, this function:
/// 1. Uses `cache.get_or_clone_source_worktree_with_context()` to get a worktree
/// 2. Each dependency gets its own isolated worktree for parallel safety
/// 3. Worktrees are automatically managed and reused by the cache layer
/// 4. Context (dependency name) is provided for debugging parallel operations
///
/// # Installation Process
///
/// 1. **Path resolution**: Determines destination based on `installed_at` or defaults
/// 2. **Repository access**: Gets worktree from cache (for remote) or validates local path
/// 3. **Content validation**: Verifies markdown format and structure
/// 4. **Atomic write**: Installs file atomically to prevent corruption
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::installer::{install_resource, InstallContext};
/// use agpm_cli::lockfile::LockedResourceBuilder;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::core::ResourceType;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let entry = LockedResourceBuilder::new(
///     "example-agent".to_string(),
///     "agents/example.md".to_string(),
///     "sha256:...".to_string(),
///     ".claude/agents/example.md".to_string(),
///     ResourceType::Agent,
/// )
/// .source(Some("community".to_string()))
/// .url(Some("https://github.com/example/repo.git".to_string()))
/// .version(Some("v1.0.0".to_string()))
/// .resolved_commit(Some("abc123".to_string()))
/// .tool(Some("claude-code".to_string()))
/// .build();
///
/// let context = InstallContext::builder(Path::new("."), &cache).build();
/// let (installed, checksum, _old_checksum, _patches) = install_resource(&entry, "agents", &context).await?;
/// if installed {
///     println!("Resource was installed with checksum: {}", checksum);
/// } else {
///     println!("Resource already existed and was unchanged");
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Error Handling
///
/// Returns an error if:
/// - The source repository cannot be accessed or cloned
/// - The specified file path doesn't exist in the repository
/// - The file is not valid markdown format
/// - File system operations fail (permissions, disk space)
/// - Worktree creation fails due to Git issues
pub async fn install_resource(
    entry: &LockedResource,
    resource_dir: &str,
    context: &InstallContext<'_>,
) -> Result<(bool, String, Option<String>, crate::manifest::patches::AppliedPatches)> {
    // Determine destination path
    let dest_path = if entry.installed_at.is_empty() {
        context.project_dir.join(resource_dir).join(format!("{}.md", entry.name))
    } else {
        context.project_dir.join(&entry.installed_at)
    };

    // Check if file already exists and compute checksum
    let existing_checksum = if dest_path.exists() {
        let path = dest_path.clone();
        tokio::task::spawn_blocking(move || LockFile::compute_checksum(&path)).await??.into()
    } else {
        None
    };

    // Early-exit optimization: Skip if nothing changed (Git dependencies only)
    if let Some((checksum, context_checksum, patches)) =
        should_skip_installation(entry, &dest_path, existing_checksum.as_ref(), context)
    {
        return Ok((false, checksum, context_checksum, patches));
    }

    // Log local dependency processing
    if entry.resolved_commit.as_deref().is_none_or(str::is_empty) {
        tracing::debug!(
            "Processing local dependency: {} (early-exit optimization skipped)",
            entry.name
        );
    }

    // Read source content from Git or local file
    let content = read_source_content(entry, context).await?;

    // Validate markdown format
    validate_markdown_content(&content)?;

    // Apply patches (before templating)
    let (patched_content, applied_patches) = apply_resource_patches(&content, entry, context)?;

    // Apply templating to markdown files
    let (final_content, _templating_was_applied, context_checksum) =
        render_resource_content(&patched_content, entry, context).await?;

    // Calculate file checksum of final content
    let file_checksum = compute_file_checksum(&final_content);

    // Determine if content has changed
    let content_changed = existing_checksum.as_ref() != Some(&file_checksum);

    // Write to disk if needed
    let should_install = entry.install.unwrap_or(true);
    let actually_installed = write_resource_to_disk(
        &dest_path,
        &final_content,
        should_install,
        content_changed,
        context,
    )
    .await?;

    Ok((actually_installed, file_checksum, context_checksum, applied_patches))
}

/// Install a single resource with progress bar updates for user feedback.
///
/// This function wraps [`install_resource`] with progress bar integration to provide
/// real-time feedback during resource installation. It updates the progress bar
/// message before delegating to the core installation logic.
///
/// # Arguments
///
/// * `entry` - The locked resource containing installation metadata
/// * `project_dir` - Root project directory for installation target
/// * `resource_dir` - Subdirectory name for this resource type (e.g., "agents")
/// * `cache` - Cache instance for Git repository and worktree management
/// * `force_refresh` - Whether to force refresh of cached repositories
/// * `pb` - Progress bar to update with installation status
///
/// # Returns
///
/// Returns a tuple of:
/// - `bool`: Whether the resource was actually installed (`true` for new/updated, `false` for unchanged)
/// - `String`: SHA-256 checksum of the installed file content
/// - `Option<String>`: SHA-256 checksum of the template rendering inputs, or None for non-templated resources
/// - `AppliedPatches`: Information about any patches that were applied during installation
///
/// # Progress Integration
///
/// The function automatically sets the progress bar message to indicate which
/// resource is currently being installed. This provides users with real-time
/// feedback about installation progress.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::installer::{install_resource_with_progress, InstallContext};
/// use agpm_cli::lockfile::{LockedResource, LockedResourceBuilder};
/// use agpm_cli::cache::Cache;
/// use agpm_cli::core::ResourceType;
/// use indicatif::ProgressBar;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let pb = ProgressBar::new(1);
/// let entry = LockedResourceBuilder::new(
///     "example-agent".to_string(),
///     "agents/example.md".to_string(),
///     "sha256:...".to_string(),
///     ".claude/agents/example.md".to_string(),
///     ResourceType::Agent,
/// )
/// .source(Some("community".to_string()))
/// .url(Some("https://github.com/example/repo.git".to_string()))
/// .version(Some("v1.0.0".to_string()))
/// .resolved_commit(Some("abc123".to_string()))
/// .tool(Some("claude-code".to_string()))
/// .build();
///
/// let context = InstallContext::builder(Path::new("."), &cache).build();
/// let (installed, checksum, _old_checksum, _patches) = install_resource_with_progress(
///     &entry,
///     "agents",
///     &context,
///     &pb
/// ).await?;
///
/// pb.inc(1);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns the same errors as [`install_resource`], including:
/// - Repository access failures
/// - File system operation errors
/// - Invalid markdown content
/// - Git worktree creation failures
pub async fn install_resource_with_progress(
    entry: &LockedResource,
    resource_dir: &str,
    context: &InstallContext<'_>,
    pb: &ProgressBar,
) -> Result<(bool, String, Option<String>, crate::manifest::patches::AppliedPatches)> {
    pb.set_message(format!("Installing {}", entry.name));
    install_resource(entry, resource_dir, context).await
}

/// Install a single resource in a thread-safe manner for parallel execution.
///
/// This is a private helper function used by parallel installation operations.
/// It's a thin wrapper around [`install_resource`] designed for use in parallel
/// installation streams.
pub(crate) async fn install_resource_for_parallel(
    entry: &LockedResource,
    resource_dir: &str,
    context: &InstallContext<'_>,
) -> Result<(bool, String, Option<String>, crate::manifest::patches::AppliedPatches)> {
    install_resource(entry, resource_dir, context).await
}

/// Filtering options for resource installation operations.
///
/// This enum controls which resources are processed during installation,
/// enabling both full installations and selective updates. The filter
/// determines which entries from the lockfile are actually installed.
///
/// # Use Cases
///
/// - **Full installations**: Install all resources defined in lockfile
/// - **Selective updates**: Install only resources that have been updated
/// - **Performance optimization**: Avoid reinstalling unchanged resources
/// - **Incremental deployments**: Update only what has changed
///
/// # Variants
///
/// ## All Resources
/// [`ResourceFilter::All`] processes every resource entry in the lockfile,
/// regardless of whether it has changed. This is used by the install command
/// for complete environment setup.
///
/// ## Updated Resources Only
/// [`ResourceFilter::Updated`] processes only resources that have version
/// changes, as tracked by the update command. This enables efficient
/// incremental updates without full reinstallation.
///
/// # Examples
///
/// Install all resources:
/// ```rust,no_run
/// use agpm_cli::installer::ResourceFilter;
///
/// let filter = ResourceFilter::All;
/// // This will install every resource in the lockfile
/// ```
///
/// Install only updated resources:
/// ```rust,no_run
/// use agpm_cli::installer::ResourceFilter;
///
/// let updates = vec![
///     ("agent1".to_string(), None, "v1.0.0".to_string(), "v1.1.0".to_string()),
///     ("tool2".to_string(), Some("community".to_string()), "v2.1.0".to_string(), "v2.2.0".to_string()),
/// ];
/// let filter = ResourceFilter::Updated(updates);
/// // This will install only agent1 and tool2
/// ```
///
/// # Update Tuple Format
///
/// For [`ResourceFilter::Updated`], each tuple contains:
/// - `name`: Resource name as defined in the manifest
/// - `old_version`: Previous version (for logging and tracking)
/// - `new_version`: New version to install
///
/// The old version is primarily used for user feedback and logging,
/// while the new version determines what gets installed.
pub enum ResourceFilter {
    /// Install all resources from the lockfile.
    ///
    /// This option processes every resource entry in the lockfile,
    /// installing or updating each one regardless of whether it has
    /// changed since the last installation.
    All,

    /// Install only specific updated resources.
    ///
    /// This option processes only the resources specified in the update list,
    /// allowing for efficient incremental updates. Each tuple contains:
    /// - Resource name
    /// - Source name (None for local resources)
    /// - Old version (for tracking)
    /// - New version (to install)
    Updated(Vec<(String, Option<String>, String, String)>),
}

/// Resource installation function supporting multiple progress configurations.
///
/// This function consolidates all resource installation patterns into a single, flexible
/// interface that can handle both full installations and selective updates with different
/// progress reporting mechanisms. It represents the modernized installation architecture
/// introduced in AGPM v0.3.0.
///
/// # Architecture Benefits
///
/// - **Single API**: Single function handles install and update commands
/// - **Flexible progress**: Supports dynamic, simple, and quiet progress modes
/// - **Selective installation**: Can install all resources or just updated ones
/// - **Optimal concurrency**: Leverages worktree-based parallel operations
/// - **Cache efficiency**: Integrates with instance-level caching systems
///
/// # Parameters
///
/// * `filter` - Determines which resources to install ([`ResourceFilter::All`] or [`ResourceFilter::Updated`])
/// * `lockfile` - The lockfile containing all resource definitions to install
/// * `manifest` - The project manifest providing configuration and target directories
/// * `project_dir` - Root directory where resources should be installed
/// * `cache` - Cache instance for Git repository and worktree management
/// * `force_refresh` - Whether to force refresh of cached repositories
/// * `max_concurrency` - Optional limit on concurrent operations (None = unlimited)
/// * `progress` - Optional multi-phase progress manager ([`MultiPhaseProgress`])
///
/// # Progress Reporting
///
/// Progress is reported through the optional [`MultiPhaseProgress`] parameter:
/// - **Enabled**: Pass `Some(progress)` for multi-phase progress with live updates
/// - **Disabled**: Pass `None` for quiet operation (scripts and automation)
///
/// # Installation Process
///
/// 1. **Resource filtering**: Collects entries based on filter criteria
/// 2. **Cache warming**: Pre-creates worktrees for all unique repositories
/// 3. **Parallel installation**: Processes resources with configured concurrency
/// 4. **Progress coordination**: Updates progress based on configuration
/// 5. **Error aggregation**: Collects and reports any installation failures
///
/// # Concurrency Behavior
///
/// The function implements advanced parallel processing:
/// - **Pre-warming phase**: Creates all needed worktrees upfront for maximum parallelism
/// - **Parallel execution**: Each resource installed in its own async task
/// - **Concurrency control**: `max_concurrency` limits simultaneous operations
/// - **Thread safety**: Progress updates are atomic and thread-safe
///
/// # Returns
///
/// Returns a tuple of:
/// - The number of resources that were actually installed (new or updated content).
///   Resources that already exist with identical content are not counted.
/// - A vector of (`resource_name`, checksum) pairs for all processed resources
///
/// # Errors
///
/// Returns an error if any resource installation fails. The error includes details
/// about all failed installations with specific error messages for debugging.
///
/// # Examples
///
/// Install all resources with progress tracking:
/// ```rust,no_run
/// use agpm_cli::installer::{install_resources, ResourceFilter};
/// use agpm_cli::utils::progress::MultiPhaseProgress;
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use std::sync::Arc;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let lockfile = Arc::new(LockFile::default());
/// # let manifest = Manifest::default();
/// # let project_dir = Path::new(".");
/// # let cache = Cache::new()?;
/// let progress = Arc::new(MultiPhaseProgress::new(true));
///
/// let results = install_resources(
///     ResourceFilter::All,
///     &lockfile,
///     &manifest,
///     &project_dir,
///     cache,
///     false,
///     Some(8), // Limit to 8 concurrent operations
///     Some(progress),
///     false, // verbose
///     None, // old_lockfile
/// ).await?;
///
/// println!("Installed {} resources", results.installed_count);
/// # Ok(())
/// # }
/// ```
///
/// Install resources quietly (for automation):
/// ```rust,no_run
/// use agpm_cli::installer::{install_resources, ResourceFilter};
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use std::path::Path;
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let lockfile = Arc::new(LockFile::default());
/// # let manifest = Manifest::default();
/// # let project_dir = Path::new(".");
/// # let cache = Cache::new()?;
/// let updates = vec![("agent1".to_string(), None, "v1.0".to_string(), "v1.1".to_string())];
///
/// let results = install_resources(
///     ResourceFilter::Updated(updates),
///     &lockfile,
///     &manifest,
///     &project_dir,
///     cache,
///     false,
///     None, // Unlimited concurrency
///     None, // No progress output
///     false, // verbose
///     None, // old_lockfile
/// ).await?;
///
/// println!("Updated {} resources", results.installed_count);
/// # Ok(())
/// # }
/// ```
/// Collect entries to install based on filter criteria.
///
/// Returns a sorted vector of (LockedResource, target_directory) tuples.
/// Sorting ensures deterministic processing order for consistent context checksums.
fn collect_install_entries(
    filter: &ResourceFilter,
    lockfile: &LockFile,
    manifest: &Manifest,
) -> Vec<(LockedResource, String)> {
    let all_entries: Vec<(LockedResource, String)> = match filter {
        ResourceFilter::All => {
            // Use existing ResourceIterator logic for all entries
            ResourceIterator::collect_all_entries(lockfile, manifest)
                .into_iter()
                .map(|(entry, dir)| (entry.clone(), dir.into_owned()))
                .collect()
        }
        ResourceFilter::Updated(updates) => {
            // Collect only the updated entries
            let mut entries = Vec::new();
            for (name, source, _, _) in updates {
                if let Some((resource_type, entry)) =
                    ResourceIterator::find_resource_by_name_and_source(
                        lockfile,
                        name,
                        source.as_deref(),
                    )
                {
                    // Get artifact configuration path
                    let tool = entry.tool.as_deref().unwrap_or("claude-code");
                    // System invariant: Resource type validated during manifest parsing
                    let artifact_path = manifest
                        .get_artifact_resource_path(tool, resource_type)
                        .expect("Resource type must be supported by configured tools");
                    let target_dir = artifact_path.display().to_string();
                    entries.push((entry.clone(), target_dir));
                }
            }
            entries
        }
    };

    if all_entries.is_empty() {
        return Vec::new();
    }

    // Sort entries for deterministic processing order
    let mut sorted_entries = all_entries;
    sorted_entries.sort_by(|(a, _), (b, _)| {
        a.resource_type.cmp(&b.resource_type).then_with(|| a.name.cmp(&b.name))
    });

    sorted_entries
}

/// Pre-warm cache by creating all needed worktrees upfront.
///
/// Creates worktrees for all unique (source, url, sha) combinations to enable
/// parallel installation without worktree creation bottlenecks.
async fn pre_warm_worktrees(
    entries: &[(LockedResource, String)],
    cache: &Cache,
    filter: &ResourceFilter,
    max_concurrency: usize,
) {
    let mut unique_worktrees = HashSet::new();

    // Collect unique worktrees
    for (entry, _) in entries {
        if let Some(source_name) = &entry.source
            && let Some(url) = &entry.url
        {
            // Only pre-warm if we have a valid SHA
            if let Some(sha) = entry.resolved_commit.as_ref().filter(|commit| {
                commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit())
            }) {
                unique_worktrees.insert((source_name.clone(), url.clone(), sha.clone()));
            }
        }
    }

    if unique_worktrees.is_empty() {
        return;
    }

    let context = match filter {
        ResourceFilter::All => "pre-warm",
        ResourceFilter::Updated(_) => "update-pre-warm",
    };

    let total = unique_worktrees.len();

    tracing::debug!(
        "Starting worktree pre-warming for {} worktrees with concurrency {}",
        total,
        max_concurrency
    );

    // Use stream with buffer_unordered to limit concurrency
    stream::iter(unique_worktrees)
        .map(|(source, url, sha)| {
            let cache = cache.clone();

            async move {
                // Format display: source@sha[8]
                let display_name = format!("{}@{}", source, &sha[..8]);

                tracing::trace!("Pre-warming worktree: {}", display_name);

                // Create or get worktree
                let start = std::time::Instant::now();
                cache.get_or_create_worktree_for_sha(&source, &url, &sha, Some(context)).await.ok(); // Ignore errors during pre-warming
                let elapsed = start.elapsed();
                tracing::trace!("Worktree {} took {:?}", display_name, elapsed);
            }
        })
        .buffer_unordered(max_concurrency)
        .collect::<Vec<_>>()
        .await;

    tracing::debug!("Completed worktree pre-warming");
}

/// Execute parallel installation with progress tracking.
///
/// Processes all entries concurrently with active progress tracking and gitignore updates.
/// Returns vector of installation results for each resource.
#[allow(clippy::too_many_arguments)]
async fn execute_parallel_installation(
    entries: Vec<(LockedResource, String)>,
    project_dir: &Path,
    cache: &Cache,
    manifest: &Manifest,
    lockfile: &Arc<LockFile>,
    force_refresh: bool,
    verbose: bool,
    max_concurrency: Option<usize>,
    progress: Option<Arc<MultiPhaseProgress>>,
    old_lockfile: Option<&LockFile>,
) -> Vec<InstallResult> {
    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let type_counts =
        Arc::new(Mutex::new(std::collections::HashMap::<crate::core::ResourceType, usize>::new()));
    let concurrency = max_concurrency.unwrap_or(usize::MAX).max(1);

    // Create gitignore lock for thread-safe gitignore updates
    let gitignore_lock = Arc::new(Mutex::new(()));

    let total = entries.len();

    // Process installations in parallel with active tracking
    stream::iter(entries)
        .map(|(entry, resource_dir)| {
            let project_dir = project_dir.to_path_buf();
            let installed_count = Arc::clone(&installed_count);
            let type_counts = Arc::clone(&type_counts);
            let cache = cache.clone();
            let progress = progress.clone();
            let gitignore_lock = Arc::clone(&gitignore_lock);
            let entry_type = entry.resource_type;
            async move {
                // Signal that this resource is starting
                if let Some(ref pm) = progress {
                    pm.mark_resource_active(&entry);
                }

                let install_context = InstallContext::with_common_options(
                    &project_dir,
                    &cache,
                    Some(manifest),
                    Some(lockfile),
                    force_refresh,
                    verbose,
                    Some(&gitignore_lock),
                    old_lockfile,
                );

                let res =
                    install_resource_for_parallel(&entry, &resource_dir, &install_context).await;

                // Handle result and track completion
                match res {
                    Ok((actually_installed, file_checksum, context_checksum, applied_patches)) => {
                        // Always increment the counter (regardless of whether file was written)
                        let mut count = installed_count.lock().await;
                        *count += 1;

                        // Track by type for summary (only count those actually written to disk)
                        if actually_installed {
                            *type_counts.lock().await.entry(entry_type).or_insert(0) += 1;
                        }

                        // Signal completion and update counter
                        if let Some(ref pm) = progress {
                            pm.mark_resource_complete(&entry, *count, total);
                        }

                        Ok((
                            entry.id(),
                            actually_installed,
                            file_checksum,
                            context_checksum,
                            applied_patches,
                        ))
                    }
                    Err(err) => {
                        // On error, still increment counter and clear the slot
                        let mut count = installed_count.lock().await;
                        *count += 1;

                        // Clear the slot for this failed resource
                        if let Some(ref pm) = progress {
                            pm.mark_resource_complete(&entry, *count, total);
                        }

                        Err((entry.id(), err))
                    }
                }
            }
        })
        .buffered(concurrency)
        .collect()
        .await
}

/// Process installation results and aggregate checksums.
///
/// Aggregates installation results, handles errors with detailed context,
/// and returns structured results for lockfile updates.
fn process_install_results(
    results: Vec<InstallResult>,
    progress: Option<Arc<MultiPhaseProgress>>,
) -> Result<InstallationResults> {
    // Handle errors and collect checksums, context checksums, and applied patches
    let mut errors = Vec::new();
    let mut checksums = Vec::new();
    let mut context_checksums = Vec::new();
    let mut applied_patches_list = Vec::new();

    for result in results {
        match result {
            Ok((id, _installed, file_checksum, context_checksum, applied_patches)) => {
                checksums.push((id.clone(), file_checksum));
                context_checksums.push((id.clone(), context_checksum));
                applied_patches_list.push((id, applied_patches));
            }
            Err((id, error)) => {
                errors.push((id, error));
            }
        }
    }

    // Complete installation phase
    if let Some(ref pm) = progress {
        if !errors.is_empty() {
            pm.complete_phase_with_window(Some(&format!(
                "Failed to install {} resources",
                errors.len()
            )));
        } else {
            let installed_count = checksums.len();
            if installed_count > 0 {
                pm.complete_phase_with_window(Some(&format!(
                    "Installed {installed_count} resources"
                )));
            }
        }
    }

    // Handle errors with detailed context
    if !errors.is_empty() {
        // Deduplicate errors by ResourceId - same resource may fail multiple times
        // if multiple parents depend on it
        let mut unique_errors: std::collections::HashMap<ResourceId, anyhow::Error> =
            std::collections::HashMap::new();
        for (id, error) in errors {
            unique_errors.entry(id).or_insert(error);
        }

        // Format each error - use enhanced formatting for template errors
        let error_msgs: Vec<String> = unique_errors
            .into_iter()
            .map(|(id, error)| {
                // Check if this is a TemplateError by walking the error chain
                let mut current_error: &dyn std::error::Error = error.as_ref();
                loop {
                    if let Some(template_error) =
                        current_error.downcast_ref::<crate::templating::TemplateError>()
                    {
                        // Found a TemplateError - use its detailed formatting
                        return format!(
                            "  {}:\n{}",
                            id, // Use full ResourceId Display (shows variants)
                            template_error.format_with_context()
                        );
                    }

                    // Move to the next error in the chain
                    match current_error.source() {
                        Some(source) => current_error = source,
                        None => break,
                    }
                }

                // Not a template error - use alternate formatting to show full error chain
                format!("  {}: {:#}", id, error) // Use full ResourceId Display + full error chain
            })
            .collect();

        // Return the formatted errors without wrapping context
        return Err(anyhow::anyhow!(
            "Installation incomplete: {} resource(s) could not be set up\n{}",
            error_msgs.len(),
            error_msgs.join("\n\n")
        ));
    }

    let installed_count = checksums.len();
    Ok(InstallationResults::new(
        installed_count,
        checksums,
        context_checksums,
        applied_patches_list,
    ))
}

#[allow(clippy::too_many_arguments)]
pub async fn install_resources(
    filter: ResourceFilter,
    lockfile: &Arc<LockFile>,
    manifest: &Manifest,
    project_dir: &Path,
    cache: Cache,
    force_refresh: bool,
    max_concurrency: Option<usize>,
    progress: Option<Arc<MultiPhaseProgress>>,
    verbose: bool,
    old_lockfile: Option<&LockFile>,
) -> Result<InstallationResults> {
    // 1. Collect entries to install
    let all_entries = collect_install_entries(&filter, lockfile, manifest);
    if all_entries.is_empty() {
        return Ok(InstallationResults::new(0, Vec::new(), Vec::new(), Vec::new()));
    }

    let total = all_entries.len();

    // Calculate optimal window size
    let concurrency = max_concurrency.unwrap_or_else(|| {
        let cores = std::thread::available_parallelism().map(std::num::NonZero::get).unwrap_or(4);
        std::cmp::max(10, cores * 2)
    });
    let window_size =
        crate::utils::progress::MultiPhaseProgress::calculate_window_size(concurrency);

    // 2. Pre-warm worktrees
    pre_warm_worktrees(&all_entries, &cache, &filter, concurrency).await;

    // 3. Start installation phase with active window tracking
    if let Some(ref pm) = progress {
        pm.start_phase_with_active_tracking(
            InstallationPhase::InstallingResources,
            total,
            window_size,
        );
    }

    // 4. Execute parallel installation
    let results = execute_parallel_installation(
        all_entries,
        project_dir,
        &cache,
        manifest,
        lockfile,
        force_refresh,
        verbose,
        max_concurrency,
        progress.clone(),
        old_lockfile,
    )
    .await;

    // 5. Process results and aggregate checksums
    process_install_results(results, progress)
}

/// Finalize installation by configuring hooks, MCP servers, and updating lockfiles.
///
/// This function performs the final steps shared by install and update commands after
/// resources are installed. It executes multiple operations in sequence, with each
/// step building on the previous.
///
/// # Process Steps
///
/// 1. **Hook Configuration** - Configures Claude Code hooks from source files
/// 2. **MCP Server Setup** - Groups and configures MCP servers by tool type
/// 3. **Patch Application** - Applies and tracks project/private patches
/// 4. **Artifact Cleanup** - Removes old files from previous installations
/// 5. **Lockfile Saving** - Writes main lockfile with checksums (unless --no-lock)
/// 6. **Private Lockfile** - Saves private patches to separate file
/// 7. **Gitignore Update** - Adds installed paths to .gitignore
///
/// # Arguments
///
/// * `lockfile` - Mutable lockfile to update with applied patches
/// * `manifest` - Project manifest for configuration
/// * `project_dir` - Project root directory
/// * `cache` - Cache instance for Git operations
/// * `old_lockfile` - Optional previous lockfile for artifact cleanup
/// * `quiet` - Whether to suppress output messages
/// * `no_lock` - Whether to skip lockfile saving (development mode)
///
/// # Returns
///
/// Returns `(hook_count, server_count)` tuple:
/// - `hook_count`: Number of hooks configured (regardless of changed status)
/// - `server_count`: Number of MCP servers configured (regardless of changed status)
///
/// # Errors
///
/// Returns an error if:
/// - **Hook configuration fails**: Invalid hook source files or permission errors
/// - **MCP handler not found**: Tool type has no registered MCP handler
/// - **Tool not configured**: Tool missing from manifest `[default-tools]` section
/// - **Lockfile save fails**: Permission denied or disk full
/// - **Gitignore update fails**: Rare I/O errors
///
/// # Examples
///
/// ```rust,no_run
/// # use agpm_cli::installer::finalize_installation;
/// # use agpm_cli::lockfile::LockFile;
/// # use agpm_cli::manifest::Manifest;
/// # use agpm_cli::cache::Cache;
/// # use std::path::Path;
/// # async fn example() -> anyhow::Result<()> {
/// let mut lockfile = LockFile::default();
/// let manifest = Manifest::default();
/// let project_dir = Path::new(".");
/// let cache = Cache::new()?;
///
/// let (hooks, servers) = finalize_installation(
///     &mut lockfile,
///     &manifest,
///     project_dir,
///     &cache,
///     None,    // no old lockfile (fresh install)
///     false,   // not quiet
///     false,   // create lockfile
/// ).await?;
///
/// println!("Configured {} hooks and {} servers", hooks, servers);
/// # Ok(())
/// # }
/// ```
///
/// # Implementation Notes
///
/// - Hooks are configured by reading directly from source files (no copying)
/// - MCP servers are grouped by tool type for batch configuration
/// - Patch tracking: project patches stored in lockfile, private in separate file
/// - Artifact cleanup only runs if old lockfile exists (update scenario)
/// - Private lockfile automatically deleted if empty
pub async fn finalize_installation(
    lockfile: &mut LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    cache: &Cache,
    old_lockfile: Option<&LockFile>,
    quiet: bool,
    no_lock: bool,
) -> Result<(usize, usize)> {
    use anyhow::Context;

    let mut hook_count = 0;
    let mut server_count = 0;

    // Handle hooks if present
    if !lockfile.hooks.is_empty() {
        // Configure hooks directly from source files (no copying)
        let hooks_changed = crate::hooks::install_hooks(lockfile, project_dir, cache).await?;
        hook_count = lockfile.hooks.len();

        // Always show hooks configuration feedback with changed count
        if !quiet {
            if hook_count == 1 {
                if hooks_changed == 1 {
                    println!("✓ Configured 1 hook (1 changed)");
                } else {
                    println!("✓ Configured 1 hook ({hooks_changed} changed)");
                }
            } else {
                println!("✓ Configured {hook_count} hooks ({hooks_changed} changed)");
            }
        }
    }

    // Handle MCP servers if present - group by artifact type
    if !lockfile.mcp_servers.is_empty() {
        use crate::mcp::handlers::McpHandler;
        use std::collections::HashMap;

        // Group MCP servers by artifact type
        let mut servers_by_type: HashMap<String, Vec<crate::lockfile::LockedResource>> =
            HashMap::new();
        {
            // Scope to limit the immutable borrow of lockfile
            for server in &lockfile.mcp_servers {
                let tool = server.tool.clone().unwrap_or_else(|| "claude-code".to_string());
                servers_by_type.entry(tool).or_default().push(server.clone());
            }
        }

        // Collect all applied patches to update lockfile after iteration
        let mut all_mcp_patches: Vec<(String, crate::manifest::patches::AppliedPatches)> =
            Vec::new();
        // Track total changed MCP servers
        let mut total_mcp_changed = 0;

        // Configure MCP servers for each artifact type using appropriate handler
        for (artifact_type, servers) in servers_by_type {
            if let Some(handler) = crate::mcp::handlers::get_mcp_handler(&artifact_type) {
                // Get artifact base directory - must be properly configured
                let artifact_base = manifest
                    .get_tool_config(&artifact_type)
                    .map(|c| &c.path)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Tool '{}' is not configured. Please define it in [default-tools] section.",
                            artifact_type
                        )
                    })?;
                let artifact_base = project_dir.join(artifact_base);

                // Configure MCP servers by reading directly from source (no file copying)
                let server_entries = servers.clone();

                // Collect applied patches and changed count
                let (applied_patches_list, changed_count) = handler
                    .configure_mcp_servers(
                        project_dir,
                        &artifact_base,
                        &server_entries,
                        cache,
                        manifest,
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to configure MCP servers for artifact type '{}'",
                            artifact_type
                        )
                    })?;

                // Collect patches for later application
                all_mcp_patches.extend(applied_patches_list);
                total_mcp_changed += changed_count;

                server_count += servers.len();
            }
        }

        // Update lockfile with all collected applied patches
        for (name, applied_patches) in all_mcp_patches {
            lockfile.update_resource_applied_patches(&name, &applied_patches);
        }

        // Use the actual changed count from MCP handlers
        let mcp_servers_changed = total_mcp_changed;

        if server_count > 0 && !quiet {
            if server_count == 1 {
                if mcp_servers_changed == 1 {
                    println!("✓ Configured 1 MCP server (1 changed)");
                } else {
                    println!("✓ Configured 1 MCP server ({mcp_servers_changed} changed)");
                }
            } else {
                println!("✓ Configured {server_count} MCP servers ({mcp_servers_changed} changed)");
            }
        }
    }

    // Clean up removed or moved artifacts if old lockfile provided
    if let Some(old) = old_lockfile {
        if let Ok(removed) = cleanup_removed_artifacts(old, lockfile, project_dir).await {
            if !removed.is_empty() && !quiet {
                println!("✓ Cleaned up {} moved or removed artifact(s)", removed.len());
            }
        }
    }

    if !no_lock {
        // Save lockfile with checksums
        lockfile.save(&project_dir.join("agpm.lock")).with_context(|| {
            format!("Failed to save lockfile to {}", project_dir.join("agpm.lock").display())
        })?;

        // Build and save private lockfile if there are private patches
        use crate::lockfile::PrivateLockFile;
        let mut private_lock = PrivateLockFile::new();

        // Collect private patches for all installed resources
        for (entry, _) in ResourceIterator::collect_all_entries(lockfile, manifest) {
            let resource_type = entry.resource_type.to_plural();
            // Use the lookup_name helper to get the correct name for patch lookups
            let lookup_name = entry.lookup_name();
            if let Some(private_patches) = manifest.private_patches.get(resource_type, lookup_name)
            {
                private_lock.add_private_patches(
                    resource_type,
                    &entry.name,
                    private_patches.clone(),
                );
            }
        }

        // Save private lockfile (automatically deletes if empty)
        private_lock.save(project_dir).with_context(|| "Failed to save private lockfile")?;
    }

    // Update .gitignore with lock for safe concurrent access
    let gitignore_lock = Arc::new(Mutex::new(()));
    ensure_gitignore_state(manifest, lockfile, project_dir, Some(&gitignore_lock)).await?;

    Ok((hook_count, server_count))
}

/// Find parent resources that depend on the given resource.
///
/// This function searches through the lockfile to find resources that list
/// the given resource name in their `dependencies` field. This is useful for
/// error reporting to show which resources depend on a failing resource.
///
/// # Arguments
///
/// * `lockfile` - The lockfile to search
/// * `resource_name` - The canonical name of the resource to find parents for
///
/// # Returns
///
/// A vector of parent resource names (manifest aliases if available, otherwise
/// canonical names) that directly depend on the given resource.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::installer::find_parent_resources;
///
/// let lockfile = LockFile::default();
/// let parents = find_parent_resources(&lockfile, "agents/helper");
/// if !parents.is_empty() {
///     println!("Resource is required by: {}", parents.join(", "));
/// }
/// ```
pub fn find_parent_resources(lockfile: &LockFile, resource_name: &str) -> Vec<String> {
    use crate::core::ResourceIterator;

    let mut parents = Vec::new();

    // Iterate through all resources in the lockfile
    for (entry, _dir) in
        ResourceIterator::collect_all_entries(lockfile, &crate::manifest::Manifest::default())
    {
        // Check if this resource depends on the target resource
        if entry.dependencies.iter().any(|dep| dep == resource_name) {
            // Use manifest_alias if available (user-facing name), otherwise canonical name
            let parent_name = entry.manifest_alias.as_ref().unwrap_or(&entry.name).clone();
            parents.push(parent_name);
        }
    }

    parents
}
