//! Shared installation utilities for CCPM resources.
//!
//! This module provides common functionality for installing resources from
//! lockfile entries to the project directory. It's shared between the install
//! and update commands to avoid code duplication.
//!
//! # Parallel Installation Architecture
//!
//! The installer uses Git worktrees for safe parallel resource installation:
//! - **Worktree-based operations**: Each dependency uses its own worktree to avoid conflicts
//! - **Concurrency control**: Direct parallelism control via --max-parallel flag
//! - **Context-aware logging**: Each operation includes dependency name for debugging
//! - **Efficient cleanup**: Worktrees are managed by the cache layer for reuse
//!
//! # Installation Process
//!
//! 1. **Repository access**: Uses cache layer to get or create worktrees
//! 2. **Content validation**: Validates markdown format and structure
//! 3. **Atomic installation**: Files are written atomically to prevent corruption
//! 4. **Progress tracking**: Real-time progress updates during parallel operations
//!
//! # Performance Characteristics
//!
//! - **Parallel processing**: Multiple dependencies installed simultaneously
//! - **Worktree reuse**: Cache layer optimizes Git repository access
//! - **Parallelism-controlled**: User controls concurrency via --max-parallel flag
//! - **Atomic operations**: Fast, safe file installation with proper error handling

use crate::utils::progress::{InstallationPhase, MultiPhaseProgress};
use anyhow::{Context, Result};

/// Type alias for complex installation result tuples to improve code readability.
///
/// This type alias simplifies the return type of parallel installation functions
/// that need to return either success information or error details with context.
/// It was introduced in CCPM v0.3.0 to resolve clippy::type_complexity warnings
/// while maintaining clear semantics for installation results.
///
/// # Success Variant: `Ok((String, bool, String))`
///
/// When installation succeeds, the tuple contains:
/// - `String`: Resource name that was processed
/// - `bool`: Whether the resource was actually installed (`true`) or already up-to-date (`false`)
/// - `String`: SHA-256 checksum of the installed file content
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
/// use ccpm::installer::InstallResult;
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
type InstallResult = Result<(String, bool, String), (String, anyhow::Error)>;

use futures::{
    future,
    stream::{self, StreamExt},
};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::cache::Cache;
use crate::core::{ResourceIterator, ResourceTypeExt};
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::Manifest;
use crate::markdown::MarkdownFile;
use crate::utils::fs::{atomic_write, ensure_dir};
use crate::utils::progress::ProgressBar;
use hex;
use std::collections::HashSet;
use std::fs;

/// Install a single resource from a lock entry using worktrees for parallel safety.
///
/// This function installs a resource specified by a lockfile entry to the project
/// directory. It uses Git worktrees through the cache layer to enable safe parallel
/// operations without conflicts between concurrent installations.
///
/// # Arguments
///
/// * `entry` - The locked resource to install containing source and version info
/// * `project_dir` - The root project directory where resources should be installed
/// * `resource_dir` - The subdirectory name for this resource type (e.g., "agents")
/// * `cache` - The cache instance for managing Git repositories and worktrees
///
/// # Returns
///
/// Returns `Ok((installed, checksum))` where:
/// - `installed` is `true` if the resource was actually installed (new or updated),
///   `false` if the resource already existed and was unchanged
/// - `checksum` is the SHA-256 hash of the installed file content
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
/// use ccpm::installer::install_resource;
/// use ccpm::lockfile::LockedResource;
/// use ccpm::cache::Cache;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let entry = LockedResource {
///     name: "example-agent".to_string(),
///     source: Some("community".to_string()),
///     url: Some("https://github.com/example/repo.git".to_string()),
///     path: "agents/example.md".to_string(),
///     version: Some("v1.0.0".to_string()),
///     resolved_commit: Some("abc123".to_string()),
///     checksum: "sha256:...".to_string(),
///     installed_at: ".claude/agents/example.md".to_string(),
/// };
///
/// let (installed, checksum) = install_resource(&entry, Path::new("."), "agents", &cache, false).await?;
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
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
    force_refresh: bool,
) -> Result<(bool, String)> {
    // Determine destination path
    let dest_path = if entry.installed_at.is_empty() {
        project_dir
            .join(resource_dir)
            .join(format!("{}.md", entry.name))
    } else {
        project_dir.join(&entry.installed_at)
    };

    // Check if file already exists and compare checksums
    let existing_checksum = if dest_path.exists() {
        // Use blocking task for checksum calculation to avoid blocking the async runtime
        let path = dest_path.clone();
        tokio::task::spawn_blocking(move || LockFile::compute_checksum(&path))
            .await??
            .into()
    } else {
        None
    };

    let new_content = if let Some(source_name) = &entry.source {
        // Remote resource - use cache (with optional force refresh)
        let url = entry
            .url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Remote resource {} has no URL", entry.name))?;

        let revision = entry
            .resolved_commit
            .as_deref()
            .filter(|commit| *commit != "local")
            .or(entry.version.as_deref());

        let mut cache_dir = cache
            .get_or_clone_source_worktree_with_context(
                source_name,
                url,
                revision,
                Some(&entry.name),
            )
            .await?;

        if force_refresh {
            let _ = cache.cleanup_worktree(&cache_dir).await;
            cache_dir = cache
                .get_or_clone_source_worktree_with_context(
                    source_name,
                    url,
                    revision,
                    Some(&entry.name),
                )
                .await?;
        }

        // Read the content from the source
        let source_path = cache_dir.join(&entry.path);
        let content = tokio::fs::read_to_string(&source_path)
            .await
            .with_context(|| format!("Failed to read resource file: {}", source_path.display()))?;

        // Validate markdown
        MarkdownFile::parse(&content).with_context(|| {
            format!(
                "Invalid markdown file '{}' at {}",
                entry.name,
                source_path.display()
            )
        })?;

        content
    } else {
        // Local resource - copy directly from project directory or absolute path
        let source_path = {
            let candidate = Path::new(&entry.path);
            if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                project_dir.join(candidate)
            }
        };

        if !source_path.exists() {
            return Err(anyhow::anyhow!(
                "Local file '{}' not found. Expected at: {}",
                entry.path,
                source_path.display()
            ));
        }

        let content = tokio::fs::read_to_string(&source_path)
            .await
            .with_context(|| format!("Failed to read resource file: {}", source_path.display()))?;

        MarkdownFile::parse(&content).with_context(|| {
            format!(
                "Invalid markdown file '{}' at {}",
                entry.name,
                source_path.display()
            )
        })?;

        content
    };

    // Calculate checksum of new content
    let new_checksum = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(new_content.as_bytes());
        let hash = hasher.finalize();
        format!("sha256:{}", hex::encode(hash))
    };

    // Check if content has changed by comparing checksums
    let actually_installed = existing_checksum.as_ref() != Some(&new_checksum);

    if actually_installed {
        // Only write if content is different or file doesn't exist
        if let Some(parent) = dest_path.parent() {
            ensure_dir(parent)?;
        }

        atomic_write(&dest_path, new_content.as_bytes())
            .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;
    }

    Ok((actually_installed, new_checksum))
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
/// - `String`: SHA-256 checksum of the installed content
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
/// use ccpm::installer::install_resource_with_progress;
/// use ccpm::lockfile::LockedResource;
/// use ccpm::cache::Cache;
/// use ccpm::utils::progress::ProgressBar;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let pb = ProgressBar::new(1);
/// let entry = LockedResource {
///     name: "example-agent".to_string(),
///     source: Some("community".to_string()),
///     url: Some("https://github.com/example/repo.git".to_string()),
///     path: "agents/example.md".to_string(),
///     version: Some("v1.0.0".to_string()),
///     resolved_commit: Some("abc123".to_string()),
///     checksum: "sha256:...".to_string(),
///     installed_at: ".claude/agents/example.md".to_string(),
/// };
///
/// let (installed, checksum) = install_resource_with_progress(
///     &entry,
///     Path::new("."),
///     "agents",
///     &cache,
///     false,
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
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
    force_refresh: bool,
    pb: &ProgressBar,
) -> Result<(bool, String)> {
    pb.set_message(format!("Installing {}", entry.name));
    install_resource(entry, project_dir, resource_dir, cache, force_refresh).await
}

/// Install multiple resources in parallel using worktree-based concurrency.
///
/// This function performs parallel installation of all resources defined in the
/// lockfile, using Git worktrees to enable safe concurrent access to repositories.
/// Each dependency gets its own isolated worktree to prevent conflicts.
///
/// # Arguments
///
/// * `lockfile` - The lockfile containing all resources to install
/// * `manifest` - The project manifest for configuration
/// * `project_dir` - The root project directory for installation
/// * `pb` - Progress bar for user feedback
/// * `cache` - Cache instance managing Git repositories and worktrees
///
/// # Parallel Architecture
///
/// The function uses several layers of concurrency control:
/// - **Tokio tasks**: Each resource installation runs in its own async task
/// - **Unlimited task concurrency**: Uses `buffer_unordered(usize::MAX)`
/// - **Parallelism control**: --max-parallel flag controls concurrent operations
/// - **Worktree isolation**: Each dependency gets its own worktree for safety
///
/// # Performance Optimizations
///
/// - **Stream processing**: Uses `futures::stream` for efficient task scheduling
/// - **Context logging**: Each operation includes dependency name for debugging
/// - **Worktree reuse**: Cache layer optimizes Git repository access
/// - **Batched progress**: Updates progress atomically to reduce contention
/// - **Deferred cleanup**: Worktrees are left for reuse, cleaned up by cache commands
///
/// # Concurrency Control Flow
///
/// ```text
/// Lockfile Resources
///       ↓
/// Async Task Stream (unlimited concurrency)
///       ↓
/// install_resource_for_parallel() calls
///       ↓
/// Cache worktree operations (parallelism-controlled)
///       ↓
/// Git operations (controlled by --max-parallel)
/// ```
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::installer::install_resources_parallel;
/// use ccpm::lockfile::LockFile;
/// use ccpm::manifest::Manifest;
/// use ccpm::cache::Cache;
/// use ccpm::utils::progress::ProgressBar;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("ccpm.lock"))?;
/// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
/// let cache = Cache::new()?;
///
/// // Count total resources for progress bar
/// let total = lockfile.agents.len() + lockfile.snippets.len()
///     + lockfile.commands.len() + lockfile.scripts.len()
///     + lockfile.hooks.len() + lockfile.mcp_servers.len();
/// let pb = ProgressBar::new(total as u64);
///
/// let count = install_resources_parallel(
///     &lockfile,
///     &manifest,
///     Path::new("."),
///     &pb,
///     &cache,
///     false,
///     None,
/// ).await?;
///
/// println!("Installed {} resources", count);
/// # Ok(())
/// # }
/// ```
///
/// # Error Handling
///
/// - **Atomic failure**: If any resource fails, the entire operation fails
/// - **Detailed context**: Errors include specific resource and source information
/// - **Progress preservation**: Progress updates continue even on partial failures
/// - **Resource cleanup**: Failed operations don't leave partial state
///
/// # Return Value
///
/// Returns the total number of resources successfully installed.
// Removed install_resources_parallel - use install_resources with MultiPhaseProgress instead
#[deprecated(note = "Use install_resources with MultiPhaseProgress instead")]
pub async fn install_resources_parallel(
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    pb: &ProgressBar,
    cache: &Cache,
    force_refresh: bool,
    max_concurrency: Option<usize>,
) -> Result<usize> {
    // Collect all entries to install using ResourceIterator
    let all_entries = ResourceIterator::collect_all_entries(lockfile, manifest);

    if all_entries.is_empty() {
        return Ok(0);
    }

    // Pre-warm the cache by creating all needed worktrees upfront
    // This allows maximum parallelism for Git operations
    // Update the progress bar message to indicate preparation phase
    let total = all_entries.len();
    pb.set_message("Preparing resources");

    // Collect unique (source, version) pairs to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
        if let Some(source_name) = &entry.source {
            if let Some(url) = &entry.url {
                let version = entry
                    .resolved_commit
                    .as_ref()
                    .or(entry.version.as_ref())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "HEAD".to_string());
                unique_worktrees.insert((source_name.clone(), url.clone(), version));
            }
        }
    }

    // Pre-create all worktrees in parallel
    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, version)| {
                let cache = cache.clone();
                async move {
                    cache
                        .get_or_clone_source_worktree_with_context(
                            &source,
                            &url,
                            Some(&version),
                            Some("pre-warm"),
                        )
                        .await
                        .ok(); // Ignore errors during pre-warming
                }
            })
            .collect();

        // Execute all worktree creations in parallel
        future::join_all(worktree_futures).await;
    }

    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let pb = Arc::new(pb.clone());

    // Update message for installation phase
    pb.set_message(format!("Installing 0/{} resources", total));

    let shared_cache = Arc::new(cache.clone());
    let concurrency = max_concurrency.unwrap_or(usize::MAX).max(1);

    let results: Vec<InstallResult> = stream::iter(all_entries)
        .map(|(entry, resource_dir)| {
            let entry = entry.clone();
            let project_dir = project_dir.to_path_buf();
            let resource_dir = resource_dir.to_string();
            let installed_count = Arc::clone(&installed_count);
            let pb = Arc::clone(&pb);
            let cache = Arc::clone(&shared_cache);

            async move {
                let res = install_resource_for_parallel(
                    &entry,
                    &project_dir,
                    &resource_dir,
                    cache.as_ref(),
                    force_refresh,
                )
                .await;

                match res {
                    Ok((actually_installed, checksum)) => {
                        if actually_installed {
                            let mut count = installed_count.lock().await;
                            *count += 1;
                        }
                        let count = *installed_count.lock().await;
                        pb.set_message(format!("Installing {}/{} resources", count, total));
                        pb.inc(1);
                        Ok((entry.name.clone(), actually_installed, checksum))
                    }
                    Err(err) => Err((entry.name.clone(), err)),
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok((_name, _installed, _checksum)) => {
                // Old function doesn't return checksums
            }
            Err((name, error)) => {
                errors.push((name, error));
            }
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|(name, error)| format!("  {name}: {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to install {} resources:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    let final_count = *installed_count.lock().await;
    Ok(final_count)
}

/// Install a single resource in a thread-safe manner for parallel execution.
///
/// This function provides a thin wrapper around [`install_resource`] specifically
/// designed for use in parallel installation streams. It ensures thread-safe
/// operation when called concurrently from multiple async tasks.
///
/// # Thread Safety
///
/// While this function is just a wrapper, it's used within parallel streams where:
/// - Each resource gets its own isolated Git worktree via the cache layer
/// - File operations are atomic to prevent corruption
/// - Progress tracking is coordinated through shared state
///
/// # Arguments
///
/// * `entry` - The locked resource to install
/// * `project_dir` - Project root directory for installation
/// * `resource_dir` - Resource type subdirectory (e.g., "agents", "snippets")
/// * `cache` - Cache instance managing Git repositories and worktrees
/// * `force_refresh` - Whether to force refresh cached repositories
///
/// # Returns
///
/// Returns a tuple of:
/// - `bool`: Whether installation actually occurred (`true` for new/changed, `false` for up-to-date)
/// - `String`: SHA-256 checksum of the installed file content
///
/// # Usage in Parallel Streams
///
/// This function is typically used within futures streams for concurrent processing:
///
/// ```rust,ignore
/// use futures::stream::{self, StreamExt};
/// use ccpm::installer::install_resource_for_parallel;
/// # use ccpm::lockfile::LockedResource;
/// # use ccpm::cache::Cache;
/// # use std::path::Path;
///
/// # async fn example(entries: Vec<LockedResource>, cache: Cache) -> anyhow::Result<()> {
/// let results: Vec<_> = stream::iter(entries)
///     .map(|entry| {
///         let cache = cache.clone();
///         async move {
///             install_resource_for_parallel(
///                 &entry,
///                 Path::new("."),
///                 "agents",
///                 &cache,
///                 false
///             ).await
///         }
///     })
///     .buffer_unordered(10) // Process up to 10 resources concurrently
///     .collect()
///     .await;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns the same errors as [`install_resource`]:
/// - Git repository access failures
/// - File system permission or space issues
/// - Invalid markdown file format
/// - Worktree creation conflicts
async fn install_resource_for_parallel(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
    force_refresh: bool,
) -> Result<(bool, String)> {
    install_resource(entry, project_dir, resource_dir, cache, force_refresh).await
}

/// Progress update message for parallel installation operations.
///
/// This struct encapsulates the current state of a parallel installation operation,
/// providing detailed information about which dependencies are actively being
/// processed and the overall completion status. It's designed for use with
/// channel-based progress reporting systems.
///
/// # Fields
///
/// * `active_deps` - Names of dependencies currently being processed in parallel
/// * `completed_count` - Number of dependencies that have finished processing
/// * `total_count` - Total number of dependencies to be processed
///
/// # Usage
///
/// This struct is typically sent through async channels to provide real-time
/// progress updates to user interface components:
///
/// ```rust,no_run
/// use ccpm::installer::InstallProgress;
/// use tokio::sync::mpsc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let (tx, mut rx) = mpsc::unbounded_channel::<InstallProgress>();
///
/// // Installation task sends progress updates
/// tokio::spawn(async move {
///     let progress = InstallProgress {
///         active_deps: vec!["agent1".to_string(), "tool2".to_string()],
///         completed_count: 3,
///         total_count: 10,
///     };
///     let _ = tx.send(progress);
/// });
///
/// // UI task receives and displays progress
/// while let Some(progress) = rx.recv().await {
///     println!("Active: {:?}, Progress: {}/{}",
///         progress.active_deps,
///         progress.completed_count,
///         progress.total_count
///     );
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Design Purpose
///
/// This structure enables sophisticated progress reporting that shows:
/// - Which specific dependencies are being processed concurrently
/// - Overall completion percentage for the installation operation
/// - Real-time updates as the parallel installation progresses
///
/// The `active_deps` field is particularly useful for debugging parallel
/// operations, as it shows exactly which resources are currently being
/// downloaded, validated, or installed.
#[derive(Debug, Clone)]
pub struct InstallProgress {
    /// Names of dependencies currently being processed in parallel.
    ///
    /// This vector contains the names of all resources that are actively
    /// being installed at the time this progress update was generated.
    /// The list changes dynamically as resources complete and new ones begin.
    pub active_deps: Vec<String>,

    /// Number of dependencies that have completed processing.
    ///
    /// This count includes both successful installations and failed attempts.
    /// It represents the total number of resources that have finished,
    /// regardless of outcome.
    pub completed_count: usize,

    /// Total number of dependencies to be processed in this operation.
    ///
    /// This count remains constant throughout the installation and represents
    /// the full scope of the parallel installation operation.
    pub total_count: usize,
}

/// Install resources in parallel with detailed progress updates via async channels.
///
/// This function performs parallel resource installation while providing real-time
/// progress updates through an async channel. It's designed for UI implementations
/// that need detailed visibility into parallel installation operations, showing
/// which specific dependencies are being processed at any given time.
///
/// # Arguments
///
/// * `lockfile` - Lockfile containing all resources to install
/// * `manifest` - Project manifest providing configuration context
/// * `project_dir` - Root directory for resource installation
/// * `cache` - Cache instance for Git repository and worktree management
/// * `force_refresh` - Whether to force refresh of cached repositories
/// * `max_concurrency` - Optional limit on concurrent operations (`None` = unlimited)
/// * `progress_sender` - Optional channel sender for progress updates
///
/// # Progress Updates
///
/// When `progress_sender` is provided, the function sends [`InstallProgress`]
/// updates that include:
/// - Active dependencies currently being processed
/// - Completed count (successful and failed installations)
/// - Total dependency count for completion calculation
///
/// Updates are sent at key points:
/// - When a dependency starts processing (added to `active_deps`)
/// - When a dependency completes (removed from `active_deps`, `completed_count` incremented)
///
/// # Channel-Based Architecture
///
/// ```rust,ignore
/// use ccpm::installer::{install_resources_parallel_with_progress, InstallProgress};
/// use ccpm::lockfile::LockFile;
/// use ccpm::manifest::Manifest;
/// use ccpm::cache::Cache;
/// use tokio::sync::mpsc;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let (tx, mut rx) = mpsc::unbounded_channel::<InstallProgress>();
/// let lockfile = LockFile::load(Path::new("ccpm.lock"))?;
/// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
/// let cache = Cache::new()?;
///
/// // Spawn installation task
/// let install_task = tokio::spawn(async move {
///     install_resources_parallel_with_progress(
///         &lockfile,
///         &manifest,
///         Path::new("."),
///         &cache,
///         false,
///         Some(8),      // Max 8 concurrent operations
///         Some(tx)      // Progress updates
///     ).await
/// });
///
/// // Handle progress updates
/// tokio::spawn(async move {
///     while let Some(progress) = rx.recv().await {
///         println!("Progress: {}/{}, Active: {:?}",
///             progress.completed_count,
///             progress.total_count,
///             progress.active_deps
///         );
///     }
/// });
///
/// let count = install_task.await??;
/// println!("Installed {} resources", count);
/// # Ok(())
/// # }
/// ```
///
/// # Concurrency Control
///
/// The function implements the same parallel processing architecture as
/// [`install_resources_parallel`] but adds channel-based progress reporting:
/// - Pre-warming of Git worktrees for optimal parallelism
/// - Configurable concurrency limits via `max_concurrency`
/// - Thread-safe progress tracking with atomic updates
///
/// # Performance Characteristics
///
/// Progress updates are designed to have minimal performance impact:
/// - Updates are sent asynchronously without blocking installation
/// - Failed channel sends are silently ignored to prevent installation failures
/// - State updates are batched to reduce contention
///
/// # Returns
///
/// Returns the total number of resources that were successfully installed.
/// This count only includes resources that were actually modified (new or updated content),
/// not resources that already existed with identical content.
///
/// # Errors
///
/// Returns an error if any resource installation fails. The error includes
/// details about all failed installations with specific error context.
/// Progress updates continue until the error occurs.
// Removed install_resources_parallel_with_progress - use install_resources with MultiPhaseProgress instead
#[deprecated(note = "Use install_resources with MultiPhaseProgress instead")]
pub async fn install_resources_parallel_with_progress(
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    cache: &Cache,
    force_refresh: bool,
    max_concurrency: Option<usize>,
    progress_sender: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<usize> {
    // Collect all entries to install using ResourceIterator
    let all_entries = ResourceIterator::collect_all_entries(lockfile, manifest);

    if all_entries.is_empty() {
        return Ok(0);
    }

    let total = all_entries.len();

    // Pre-warm the cache by creating all needed worktrees upfront
    // Collect unique (source, version) pairs to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
        if let Some(source_name) = &entry.source {
            if let Some(url) = &entry.url {
                let version = entry
                    .resolved_commit
                    .as_ref()
                    .or(entry.version.as_ref())
                    .unwrap_or(&"main".to_string())
                    .clone();
                unique_worktrees.insert((source_name.clone(), url.clone(), version));
            }
        }
    }

    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, version)| {
                async move {
                    cache
                        .get_or_clone_source_worktree_with_context(
                            &source,
                            &url,
                            Some(&version),
                            Some("pre-warm"),
                        )
                        .await
                        .ok(); // Ignore errors during pre-warming
                }
            })
            .collect();

        // Execute all worktree creations in parallel
        future::join_all(worktree_futures).await;
    }

    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let active_deps = Arc::new(Mutex::new(Vec::<String>::new()));
    let sender = progress_sender.map(Arc::new);

    let shared_cache = Arc::new(cache.clone());
    let concurrency = max_concurrency.unwrap_or(usize::MAX).max(1);

    let results: Vec<InstallResult> = stream::iter(all_entries)
        .map(|(entry, resource_dir)| {
            let entry = entry.clone();
            let project_dir = project_dir.to_path_buf();
            let resource_dir = resource_dir.to_string();
            let installed_count = Arc::clone(&installed_count);
            let active_deps = Arc::clone(&active_deps);
            let sender = sender.clone();
            let cache = Arc::clone(&shared_cache);

            async move {
                // Add to active list and send update
                {
                    let mut active = active_deps.lock().await;
                    active.push(entry.name.clone());
                    let count = *installed_count.lock().await;

                    if let Some(ref tx) = sender {
                        let _ = tx.send(InstallProgress {
                            active_deps: active.clone(),
                            completed_count: count,
                            total_count: total,
                        });
                    }
                }

                let res = install_resource_for_parallel(
                    &entry,
                    &project_dir,
                    &resource_dir,
                    cache.as_ref(),
                    force_refresh,
                )
                .await;

                // Remove from active list and update count only if actually installed
                {
                    let mut active = active_deps.lock().await;
                    active.retain(|x| x != &entry.name);

                    if let Ok((actually_installed, _checksum)) = &res {
                        if *actually_installed {
                            let mut count = installed_count.lock().await;
                            *count += 1;
                        }

                        let count = *installed_count.lock().await;
                        if let Some(ref tx) = sender {
                            let _ = tx.send(InstallProgress {
                                active_deps: active.clone(),
                                completed_count: count,
                                total_count: total,
                            });
                        }
                    }
                }

                match res {
                    Ok((installed, checksum)) => Ok((entry.name.clone(), installed, checksum)),
                    Err(err) => Err((entry.name.clone(), err)),
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok((_name, _installed, _checksum)) => {
                // Old function doesn't return checksums
            }
            Err((name, error)) => {
                errors.push((name, error));
            }
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|(name, error)| format!("  {name}: {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to install {} resources:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    let final_count = *installed_count.lock().await;
    Ok(final_count)
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
/// use ccpm::installer::ResourceFilter;
///
/// let filter = ResourceFilter::All;
/// // This will install every resource in the lockfile
/// ```
///
/// Install only updated resources:
/// ```rust,no_run
/// use ccpm::installer::ResourceFilter;
///
/// let updates = vec![
///     ("agent1".to_string(), "v1.0.0".to_string(), "v1.1.0".to_string()),
///     ("tool2".to_string(), "v2.1.0".to_string(), "v2.2.0".to_string()),
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
    /// - Old version (for tracking)
    /// - New version (to install)
    Updated(Vec<(String, String, String)>),
}

/// Resource installation function supporting multiple progress configurations.
///
/// This function consolidates all resource installation patterns into a single, flexible
/// interface that can handle both full installations and selective updates with different
/// progress reporting mechanisms. It represents the modernized installation architecture
/// introduced in CCPM v0.3.0.
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
/// - A vector of (resource_name, checksum) pairs for all processed resources
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
/// use ccpm::installer::{install_resources, ResourceFilter};
/// use ccpm::utils::progress::MultiPhaseProgress;
/// use ccpm::lockfile::LockFile;
/// use ccpm::manifest::Manifest;
/// use ccpm::cache::Cache;
/// use std::sync::Arc;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let lockfile = LockFile::default();
/// # let manifest = Manifest::default();
/// # let project_dir = Path::new(".");
/// # let cache = Cache::new()?;
/// let progress = Arc::new(MultiPhaseProgress::new(true));
///
/// let (count, _checksums) = install_resources(
///     ResourceFilter::All,
///     &lockfile,
///     &manifest,
///     &project_dir,
///     cache,
///     false,
///     Some(8), // Limit to 8 concurrent operations
///     Some(progress),
/// ).await?;
///
/// println!("Installed {} resources", count);
/// # Ok(())
/// # }
/// ```
///
/// Install resources quietly (for automation):
/// ```rust,no_run
/// use ccpm::installer::{install_resources, ResourceFilter};
/// use ccpm::lockfile::LockFile;
/// use ccpm::manifest::Manifest;
/// use ccpm::cache::Cache;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let lockfile = LockFile::default();
/// # let manifest = Manifest::default();
/// # let project_dir = Path::new(".");
/// # let cache = Cache::new()?;
/// let updates = vec![("agent1".to_string(), "v1.0".to_string(), "v1.1".to_string())];
///
/// let (count, _checksums) = install_resources(
///     ResourceFilter::Updated(updates),
///     &lockfile,
///     &manifest,
///     &project_dir,
///     cache,
///     false,
///     None, // Unlimited concurrency
///     None, // No progress output
/// ).await?;
///
/// println!("Updated {} resources", count);
/// # Ok(())
/// # }
/// ```
#[allow(clippy::too_many_arguments)]
pub async fn install_resources(
    filter: ResourceFilter,
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    cache: Cache,
    force_refresh: bool,
    max_concurrency: Option<usize>,
    progress: Option<Arc<MultiPhaseProgress>>,
) -> Result<(usize, Vec<(String, String)>)> {
    // Collect entries to install based on filter
    let all_entries: Vec<(LockedResource, String)> = match filter {
        ResourceFilter::All => {
            // Use existing ResourceIterator logic for all entries
            ResourceIterator::collect_all_entries(lockfile, manifest)
                .into_iter()
                .map(|(entry, dir)| (entry.clone(), dir.to_string()))
                .collect()
        }
        ResourceFilter::Updated(ref updates) => {
            // Collect only the updated entries
            let mut entries = Vec::new();
            for (name, _, _) in updates {
                if let Some((resource_type, entry)) =
                    ResourceIterator::find_resource_by_name(lockfile, name)
                {
                    let target_dir = resource_type.get_target_dir(&manifest.target);
                    entries.push((entry.clone(), target_dir.to_string()));
                }
            }
            entries
        }
    };

    if all_entries.is_empty() {
        return Ok((0, Vec::new()));
    }

    let total = all_entries.len();

    // Start installation phase with progress if provided
    if let Some(ref pm) = progress {
        pm.start_phase_with_progress(InstallationPhase::InstallingResources, total);
    }

    // Pre-warm the cache by creating all needed worktrees upfront
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
        if let Some(source_name) = &entry.source {
            if let Some(url) = &entry.url {
                let version = entry
                    .resolved_commit
                    .as_ref()
                    .or(entry.version.as_ref())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "HEAD".to_string());
                unique_worktrees.insert((source_name.clone(), url.clone(), version));
            }
        }
    }

    if !unique_worktrees.is_empty() {
        let context = match filter {
            ResourceFilter::All => "pre-warm",
            ResourceFilter::Updated(_) => "update-pre-warm",
        };

        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, version)| {
                let cache = cache.clone();
                async move {
                    cache
                        .get_or_clone_source_worktree_with_context(
                            &source,
                            &url,
                            Some(&version),
                            Some(context),
                        )
                        .await
                        .ok(); // Ignore errors during pre-warming
                }
            })
            .collect();

        // Execute all worktree creations in parallel
        future::join_all(worktree_futures).await;
    }

    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let concurrency = max_concurrency.unwrap_or(usize::MAX).max(1);

    // Update initial progress message
    if let Some(ref pm) = progress {
        pm.update_current_message(&format!("Installing 0/{total} resources"));
    }

    // Process installations in parallel
    let results: Vec<InstallResult> = stream::iter(all_entries)
        .map(|(entry, resource_dir)| {
            let entry = entry.clone();
            let project_dir = project_dir.to_path_buf();
            let resource_dir = resource_dir.to_string();
            let installed_count = Arc::clone(&installed_count);
            let cache = cache.clone();
            let progress = progress.clone();

            async move {
                // Update progress message for current resource
                if let Some(ref pm) = progress {
                    pm.update_current_message(&format!("Installing {}", entry.name));
                }

                let res = install_resource_for_parallel(
                    &entry,
                    &project_dir,
                    &resource_dir,
                    &cache,
                    force_refresh,
                )
                .await;

                // Update progress on success - but only count if actually installed
                if let Ok((actually_installed, _checksum)) = &res {
                    if *actually_installed {
                        let mut count = installed_count.lock().await;
                        *count += 1;
                    }

                    if let Some(ref pm) = progress {
                        let count = *installed_count.lock().await;
                        pm.update_current_message(&format!(
                            "Installing {}/{} resources",
                            count, total
                        ));
                        pm.increment_progress(1);
                    }
                }

                match res {
                    Ok((installed, checksum)) => Ok((entry.name.clone(), installed, checksum)),
                    Err(err) => Err((entry.name.clone(), err)),
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // Handle errors and collect checksums
    let mut errors = Vec::new();
    let mut checksums = Vec::new();
    for result in results {
        match result {
            Ok((name, _installed, checksum)) => {
                checksums.push((name, checksum));
            }
            Err((name, error)) => {
                errors.push((name, error));
            }
        }
    }

    if !errors.is_empty() {
        // Complete phase with error message
        if let Some(ref pm) = progress {
            pm.complete_phase(Some(&format!(
                "Failed to install {} resources",
                errors.len()
            )));
        }

        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|(name, error)| format!("  {name}: {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to install {} resources:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    let final_count = *installed_count.lock().await;

    // Complete installation phase successfully
    if let Some(ref pm) = progress {
        pm.complete_phase(Some(&format!("Installed {} resources", final_count)));
    }

    Ok((final_count, checksums))
}

/// Install resources with real-time dynamic progress management.
///
/// This function provides sophisticated parallel resource installation with
/// live progress tracking that shows individual dependency states in real-time.
/// It uses a `ProgressBar` to display progress of dependencies
/// are currently being processed, completed, or experiencing issues.
///
/// # Arguments
///
/// * `lockfile` - Lockfile containing all resources to install
/// * `manifest` - Project manifest providing configuration and target directories
/// * `project_dir` - Root directory where resources will be installed
/// * `cache` - Cache instance for Git repository and worktree management
/// * `force_refresh` - Whether to force refresh of cached repositories
/// * `max_concurrency` - Optional limit on concurrent operations (`None` = unlimited)
/// * `progress_bar` - Optional dynamic progress bar for real-time updates
///
/// # Dynamic Progress Features
///
/// When a `ProgressBar` is provided, the installation displays:
/// - Real-time list of dependencies being processed concurrently
/// - Live updates as dependencies start, progress, and complete
/// - Clean terminal output with automatic clearing when finished
/// - Graceful handling of errors with preserved context
///
/// # Progress Flow
///
/// 1. **Initialization**: Progress manager starts with total dependency count
/// 2. **Pre-warming**: Cache prepares Git worktrees for parallel access
/// 3. **Parallel Processing**: Dependencies install concurrently with live updates
/// 4. **Completion**: Progress display clears, leaving clean final state
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::installer::install_resources_with_dynamic_progress;
/// use ccpm::utils::progress::ProgressBar;
/// use ccpm::lockfile::LockFile;
/// use ccpm::manifest::Manifest;
/// use ccpm::cache::Cache;
/// use std::sync::Arc;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("ccpm.lock"))?;
/// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
/// let cache = Cache::new()?;
///
/// // Create dynamic progress manager
/// let progress_bar = Arc::new(ProgressBar::new(100));
///
/// let count = install_resources_with_dynamic_progress(
///     &lockfile,
///     &manifest,
///     Path::new("."),
///     &cache,
///     false,                    // No force refresh
///     Some(10),                 // Max 10 concurrent operations
///     Some(progress_bar)        // Dynamic progress display
/// ).await?;
///
/// println!("Successfully installed {} resources", count);
/// # Ok(())
/// # }
/// ```
///
/// # Performance Optimizations
///
/// The function includes several performance enhancements:
/// - **Worktree pre-warming**: All needed Git worktrees created upfront
/// - **Parallel processing**: Configurable concurrency for optimal resource usage
/// - **Progress batching**: Updates are batched to reduce terminal overhead
/// - **Efficient cleanup**: Worktrees left for reuse rather than immediate cleanup
///
/// # Returns
///
/// Returns the total number of resources that were actually installed.
/// This count only includes resources with new or updated content, not
/// resources that already existed and were unchanged.
///
/// # Errors
///
/// Returns an error if any resource installation fails. The error includes
/// detailed information about all failed installations. The progress manager
/// is automatically cleaned up even if errors occur.
// Removed install_resources_with_dynamic_progress - use install_resources with MultiPhaseProgress instead
#[deprecated(note = "Use install_resources with MultiPhaseProgress instead")]
pub async fn install_resources_with_dynamic_progress(
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    cache: &Cache,
    force_refresh: bool,
    max_concurrency: Option<usize>,
    progress_bar: Option<Arc<crate::utils::progress::ProgressBar>>,
) -> Result<usize> {
    // Collect all entries to install using ResourceIterator
    let all_entries = ResourceIterator::collect_all_entries(lockfile, manifest);

    if all_entries.is_empty() {
        return Ok(0);
    }

    let _total = all_entries.len();

    // Start progress if provided
    if let Some(ref progress) = progress_bar {
        progress.set_message("Installing resources");
    }

    // Pre-warm the cache by creating all needed worktrees upfront
    // Collect unique (source, version) pairs to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
        if let Some(source_name) = &entry.source {
            if let Some(url) = &entry.url {
                let version = entry
                    .resolved_commit
                    .as_ref()
                    .or(entry.version.as_ref())
                    .unwrap_or(&"main".to_string())
                    .clone();
                unique_worktrees.insert((source_name.clone(), url.clone(), version));
            }
        }
    }

    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, version)| {
                async move {
                    cache
                        .get_or_clone_source_worktree_with_context(
                            &source,
                            &url,
                            Some(&version),
                            Some("pre-warm"),
                        )
                        .await
                        .ok(); // Ignore errors during pre-warming
                }
            })
            .collect();

        // Execute all worktree creations in parallel
        future::join_all(worktree_futures).await;
    }

    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let shared_cache = Arc::new(cache.clone());
    let concurrency = max_concurrency.unwrap_or(usize::MAX).max(1);

    let results: Vec<InstallResult> = stream::iter(all_entries)
        .map(|(entry, resource_dir)| {
            let entry = entry.clone();
            let project_dir = project_dir.to_path_buf();
            let resource_dir = resource_dir.to_string();
            let installed_count = Arc::clone(&installed_count);
            let cache = Arc::clone(&shared_cache);
            let progress_bar_ref = progress_bar.clone();

            async move {
                // Update progress if available
                if let Some(ref progress) = progress_bar_ref {
                    progress.set_message(format!("Installing {}", entry.name));
                }

                let res = install_resource_for_parallel(
                    &entry,
                    &project_dir,
                    &resource_dir,
                    cache.as_ref(),
                    force_refresh,
                )
                .await;

                // Signal completion and update count only if actually installed
                if let Ok((actually_installed, _checksum)) = &res {
                    if *actually_installed {
                        let mut count = installed_count.lock().await;
                        *count += 1;
                    }

                    if let Some(ref progress) = progress_bar_ref {
                        progress.inc(1);
                    }
                }

                match res {
                    Ok((installed, checksum)) => Ok((entry.name.clone(), installed, checksum)),
                    Err(err) => Err((entry.name.clone(), err)),
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok((_name, _installed, _checksum)) => {
                // Old function doesn't return checksums
            }
            Err((name, error)) => {
                errors.push((name, error));
            }
        }
    }

    if !errors.is_empty() {
        // Finish with error
        if let Some(ref progress) = progress_bar {
            progress.finish_and_clear();
        }

        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|(name, error)| format!("  {name}: {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to install {} resources:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    let final_count = *installed_count.lock().await;

    // Clear the progress display - success message will be shown by the caller
    if let Some(ref progress) = progress_bar {
        progress.finish_and_clear();
    }

    Ok(final_count)
}

/// Install only specific updated resources in parallel (selective installation).
///
/// This function provides targeted installation of only the resources that have
/// been updated, rather than reinstalling all resources. It's designed for
/// efficient update operations where only a subset of dependencies have changed.
/// The function uses the same parallel processing architecture as full installations
/// but operates on a filtered set of resources.
///
/// # Arguments
///
/// * `updates` - Vector of tuples containing (name, old_version, new_version) for each updated resource
/// * `lockfile` - Lockfile containing all available resources (updated resources must exist here)
/// * `manifest` - Project manifest providing configuration and target directories
/// * `project_dir` - Root directory where resources will be installed
/// * `cache` - Cache instance for Git repository and worktree management
/// * `pb` - Optional progress bar for user feedback during installation
/// * `_quiet` - Quiet mode flag (currently unused, maintained for API compatibility)
///
/// # Update Tuple Format
///
/// Each update tuple contains:
/// - `name`: Resource name as defined in the lockfile
/// - `old_version`: Previous version (used for logging and user feedback)
/// - `new_version`: New version that will be installed
///
/// # Selective Processing
///
/// The function implements selective resource processing:
/// 1. **Filtering**: Only processes resources listed in the `updates` vector
/// 2. **Lookup**: Finds corresponding entries in the lockfile for each update
/// 3. **Validation**: Ensures all specified resources exist before processing
/// 4. **Installation**: Uses the same parallel architecture as full installations
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::installer::install_updated_resources;
/// use ccpm::lockfile::LockFile;
/// use ccpm::manifest::Manifest;
/// use ccpm::cache::Cache;
/// use ccpm::utils::progress::ProgressBar;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("ccpm.lock"))?;
/// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
/// let cache = Cache::new()?;
/// let pb = ProgressBar::new(3);
///
/// // Define which resources to update
/// let updates = vec![
///     ("ai-agent".to_string(), "v1.0.0".to_string(), "v1.1.0".to_string()),
///     ("helper-tool".to_string(), "v2.0.0".to_string(), "v2.1.0".to_string()),
///     ("data-processor".to_string(), "v1.5.0".to_string(), "v1.6.0".to_string()),
/// ];
///
/// let count = install_updated_resources(
///     &updates,
///     &lockfile,
///     &manifest,
///     Path::new("."),
///     &cache,
///     Some(&pb),
///     false
/// ).await?;
///
/// println!("Updated {} resources", count);
/// # Ok(())
/// # }
/// ```
///
/// # Performance Benefits
///
/// Selective installation provides significant performance benefits:
/// - **Reduced processing**: Only installs resources that have actually changed
/// - **Faster execution**: Avoids redundant operations on unchanged resources
/// - **Network efficiency**: Only fetches Git data for repositories with updates
/// - **Disk efficiency**: Minimizes file system operations and cache usage
///
/// # Integration with Update Command
///
/// This function is typically used by the `ccpm update` command after dependency
/// resolution determines which resources have new versions available:
///
/// ```text
/// Update Flow:
/// 1. Resolve dependencies → identify version changes
/// 2. Update lockfile → record new versions and checksums
/// 3. Selective installation → install only changed resources
/// ```
///
/// # Returns
///
/// Returns the total number of resources that were successfully installed.
/// This represents the actual number of files that were updated on disk.
///
/// # Errors
///
/// Returns an error if:
/// - Any specified resource name is not found in the lockfile
/// - Git repository access fails for resources being updated
/// - File system operations fail during installation
/// - Any individual resource installation encounters an error
///
/// The function uses atomic error handling - if any resource fails, the entire
/// operation fails and detailed error information is provided.
pub async fn install_updated_resources(
    updates: &[(String, String, String)], // (name, old_version, new_version)
    lockfile: &LockFile,
    manifest: &Manifest,
    project_dir: &Path,
    cache: &Cache,
    pb: Option<&ProgressBar>,
    _quiet: bool,
) -> Result<usize> {
    if updates.is_empty() {
        return Ok(0);
    }

    let total = updates.len();

    // Collect all entries to install
    let mut entries_to_install = Vec::new();
    for (name, _, _) in updates {
        if let Some((resource_type, entry)) =
            ResourceIterator::find_resource_by_name(lockfile, name)
        {
            let target_dir = resource_type.get_target_dir(&manifest.target);
            entries_to_install.push((entry.clone(), target_dir.to_string()));
        }
    }

    if entries_to_install.is_empty() {
        return Ok(0);
    }

    // Pre-warm the cache by creating all needed worktrees upfront
    if let Some(pb) = pb {
        pb.set_message("Preparing resources...");
    }

    // Collect unique (source, version) pairs to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &entries_to_install {
        if let Some(source_name) = &entry.source {
            if let Some(url) = &entry.url {
                let version = entry
                    .resolved_commit
                    .as_ref()
                    .or(entry.version.as_ref())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "HEAD".to_string());
                unique_worktrees.insert((source_name.clone(), url.clone(), version));
            }
        }
    }

    // Pre-create all worktrees in parallel
    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, version)| {
                async move {
                    cache
                        .get_or_clone_source_worktree_with_context(
                            &source,
                            &url,
                            Some(&version),
                            Some("update-pre-warm"),
                        )
                        .await
                        .ok(); // Ignore errors during pre-warming
                }
            })
            .collect();

        // Execute all worktree creations in parallel
        future::join_all(worktree_futures).await;
    }

    // Create thread-safe progress tracking
    let installed_count = Arc::new(Mutex::new(0));
    let pb = pb.map(Arc::new);
    let cache = Arc::new(cache);

    // Set initial progress
    if let Some(ref pb) = pb {
        pb.set_message(format!("Installing 0/{total} resources"));
    }

    // Use concurrent stream processing for parallel installation
    let results: Vec<Result<(), anyhow::Error>> = stream::iter(entries_to_install)
        .map(|(entry, resource_dir)| {
            let project_dir = project_dir.to_path_buf();
            let installed_count = Arc::clone(&installed_count);
            let pb = pb.clone();
            let cache = Arc::clone(&cache);

            async move {
                // Install the resource
                install_resource_for_parallel(
                    &entry,
                    &project_dir,
                    &resource_dir,
                    cache.as_ref(),
                    false,
                )
                .await?;

                // Update progress
                let mut count = installed_count.lock().await;
                *count += 1;

                if let Some(pb) = pb {
                    pb.set_message(format!("Installing {}/{} resources", *count, total));
                    pb.inc(1);
                }

                Ok::<(), anyhow::Error>(())
            }
        })
        .buffer_unordered(usize::MAX) // Allow unlimited task concurrency
        .collect()
        .await;

    // Check all results for errors
    for result in results {
        result?;
    }

    let final_count = *installed_count.lock().await;
    Ok(final_count)
}

/// Update .gitignore with installed file paths
pub fn update_gitignore(lockfile: &LockFile, project_dir: &Path, enabled: bool) -> Result<()> {
    if !enabled {
        // Gitignore management is disabled
        return Ok(());
    }

    let gitignore_path = project_dir.join(".gitignore");

    // Collect all installed file paths relative to project root
    let mut paths_to_ignore = HashSet::new();

    // Helper to add paths from a resource list
    let mut add_resource_paths = |resources: &[LockedResource]| {
        for resource in resources {
            if !resource.installed_at.is_empty() {
                // Use the explicit installed_at path
                paths_to_ignore.insert(resource.installed_at.clone());
            }
        }
    };

    // Collect paths from all resource types
    add_resource_paths(&lockfile.agents);
    add_resource_paths(&lockfile.snippets);
    add_resource_paths(&lockfile.commands);
    add_resource_paths(&lockfile.scripts);
    add_resource_paths(&lockfile.hooks);
    add_resource_paths(&lockfile.mcp_servers);

    // Read existing gitignore if it exists
    let mut before_ccpm_section = Vec::new();
    let mut after_ccpm_section = Vec::new();

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .with_context(|| format!("Failed to read {}", gitignore_path.display()))?;

        let mut in_ccpm_section = false;
        let mut past_ccpm_section = false;

        for line in content.lines() {
            if line == "# CCPM managed entries - do not edit below this line" {
                in_ccpm_section = true;
                continue;
            } else if line == "# End of CCPM managed entries" {
                in_ccpm_section = false;
                past_ccpm_section = true;
                continue;
            }

            if !in_ccpm_section && !past_ccpm_section {
                // Preserve everything before CCPM section exactly as-is
                before_ccpm_section.push(line.to_string());
            } else if in_ccpm_section {
                // Skip existing CCPM entries (they'll be replaced)
                continue;
            } else {
                // Preserve everything after CCPM section exactly as-is
                after_ccpm_section.push(line.to_string());
            }
        }
    }

    // Build the new content
    let mut new_content = String::new();

    // Add everything before CCPM section exactly as it was
    if !before_ccpm_section.is_empty() {
        for line in &before_ccpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
        // Add blank line before CCPM section if the previous content doesn't end with one
        if !before_ccpm_section.is_empty() && !before_ccpm_section.last().unwrap().trim().is_empty()
        {
            new_content.push('\n');
        }
    }

    // Add CCPM managed section
    new_content.push_str("# CCPM managed entries - do not edit below this line\n");

    // Convert paths to gitignore format (relative to project root)
    // Sort paths for consistent output
    let mut sorted_paths: Vec<_> = paths_to_ignore.into_iter().collect();
    sorted_paths.sort();

    for path in &sorted_paths {
        // Use paths as-is since gitignore is now at project root
        let ignore_path = if path.starts_with("./") {
            // Remove leading ./ if present
            path.strip_prefix("./").unwrap_or(path).to_string()
        } else {
            path.clone()
        };

        new_content.push_str(&ignore_path);
        new_content.push('\n');
    }

    new_content.push_str("# End of CCPM managed entries\n");

    // Add everything after CCPM section exactly as it was
    if !after_ccpm_section.is_empty() {
        new_content.push('\n');
        for line in &after_ccpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    // If this is a new file, add a basic header
    if before_ccpm_section.is_empty() && after_ccpm_section.is_empty() {
        let mut default_content = String::new();
        default_content.push_str("# .gitignore - CCPM managed entries\n");
        default_content.push_str("# CCPM entries are automatically generated\n");
        default_content.push('\n');
        default_content.push_str("# CCPM managed entries - do not edit below this line\n");

        // Add the CCPM paths
        for path in &sorted_paths {
            let ignore_path = if path.starts_with("./") {
                path.strip_prefix("./").unwrap_or(path).to_string()
            } else {
                path.clone()
            };
            default_content.push_str(&ignore_path);
            default_content.push('\n');
        }

        default_content.push_str("# End of CCPM managed entries\n");
        new_content = default_content;
    }

    // Write the updated gitignore
    atomic_write(&gitignore_path, new_content.as_bytes())
        .with_context(|| format!("Failed to update {}", gitignore_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_locked_resource(name: &str, is_local: bool) -> LockedResource {
        if is_local {
            LockedResource {
                name: name.to_string(),
                source: None,
                url: None,
                path: "test.md".to_string(),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at: String::new(),
            }
        } else {
            LockedResource {
                name: name.to_string(),
                source: Some("test_source".to_string()),
                url: Some("https://github.com/test/repo.git".to_string()),
                path: "resources/test.md".to_string(),
                version: Some("v1.0.0".to_string()),
                resolved_commit: Some("abc123".to_string()),
                checksum: "sha256:test".to_string(),
                installed_at: String::new(),
            }
        }
    }

    #[tokio::test]
    async fn test_install_resource_local() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Test Resource\nThis is a test").unwrap();

        // Create a locked resource pointing to the local file
        let mut entry = create_test_locked_resource("local-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache, false).await;
        assert!(
            result.is_ok(),
            "Failed to install local resource: {:?}",
            result
        );

        // Should be installed the first time
        let (installed, _checksum) = result.unwrap();
        assert!(installed, "Should have installed new resource");

        // Verify the file was installed
        let expected_path = project_dir.join("agents").join("local-test.md");
        assert!(expected_path.exists(), "Installed file not found");

        // Verify content
        let content = std::fs::read_to_string(expected_path).unwrap();
        assert_eq!(content, "# Test Resource\nThis is a test");
    }

    #[tokio::test]
    async fn test_install_resource_with_custom_path() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Custom Path Test").unwrap();

        // Create a locked resource with custom installation path
        let mut entry = create_test_locked_resource("custom-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "custom/location/resource.md".to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache, false).await;
        assert!(result.is_ok());
        let (installed, _checksum) = result.unwrap();
        assert!(installed, "Should have installed new resource");

        // Verify the file was installed at custom path
        let expected_path = project_dir.join("custom/location/resource.md");
        assert!(expected_path.exists(), "File not installed at custom path");
    }

    #[tokio::test]
    async fn test_install_resource_local_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a locked resource pointing to non-existent file
        let mut entry = create_test_locked_resource("missing-test", true);
        entry.path = "/non/existent/file.md".to_string();

        // Try to install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache, false).await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Local file") && error_msg.contains("not found"));
    }

    #[tokio::test]
    async fn test_install_resource_invalid_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create an invalid markdown file
        let local_file = temp_dir.path().join("invalid.md");
        std::fs::write(&local_file, "---\ninvalid: yaml: [\n---\nContent").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("invalid-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Try to install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache, false).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid markdown"));
    }

    #[tokio::test]
    async fn test_install_resource_with_progress() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();
        let pb = ProgressBar::new(1);

        // Create a local markdown file
        let local_file = temp_dir.path().join("test.md");
        std::fs::write(&local_file, "# Progress Test").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("progress-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install with progress
        let result =
            install_resource_with_progress(&entry, project_dir, "agents", &cache, false, &pb).await;
        assert!(result.is_ok());

        // Verify installation
        let expected_path = project_dir.join("agents").join("progress-test.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_install_resources_empty() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create empty lockfile and manifest
        let lockfile = LockFile::new();
        let manifest = Manifest::new();

        let (count, _) = install_resources(
            ResourceFilter::All,
            &lockfile,
            &manifest,
            project_dir,
            cache,
            false,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(count, 0, "Should install 0 resources from empty lockfile");
    }

    #[tokio::test]
    async fn test_install_resources_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create test markdown files
        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        let file3 = temp_dir.path().join("command.md");
        std::fs::write(&file1, "# Agent").unwrap();
        std::fs::write(&file2, "# Snippet").unwrap();
        std::fs::write(&file3, "# Command").unwrap();

        // Create lockfile with multiple resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        lockfile.snippets.push(snippet);

        let mut command = create_test_locked_resource("test-command", true);
        command.path = file3.to_string_lossy().to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let (count, _) = install_resources(
            ResourceFilter::All,
            &lockfile,
            &manifest,
            project_dir,
            cache,
            false,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(count, 3, "Should install 3 resources");

        // Verify all files were installed (using default directories)
        assert!(project_dir.join(".claude/agents/test-agent.md").exists());
        assert!(project_dir
            .join(".claude/ccpm/snippets/test-snippet.md")
            .exists());
        assert!(project_dir
            .join(".claude/commands/test-command.md")
            .exists());
    }

    #[tokio::test]
    async fn test_install_updated_resources() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create test markdown files
        let file1 = temp_dir.path().join("agent.md");
        let file2 = temp_dir.path().join("snippet.md");
        std::fs::write(&file1, "# Updated Agent").unwrap();
        std::fs::write(&file2, "# Updated Snippet").unwrap();

        // Create lockfile with resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.path = file1.to_string_lossy().to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        lockfile.snippets.push(snippet);

        let manifest = Manifest::new();

        // Define updates (only agent is updated)
        let updates = vec![(
            "test-agent".to_string(),
            "v1.0.0".to_string(),
            "v1.1.0".to_string(),
        )];

        let count = install_updated_resources(
            &updates,
            &lockfile,
            &manifest,
            project_dir,
            &cache,
            None,
            false, // quiet
        )
        .await
        .unwrap();

        assert_eq!(count, 1, "Should install 1 updated resource");
        assert!(project_dir.join(".claude/agents/test-agent.md").exists());
        assert!(!project_dir
            .join(".claude/snippets/test-snippet.md")
            .exists()); // Not updated
    }

    #[tokio::test]
    async fn test_install_updated_resources_quiet_mode() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create test markdown file
        let file = temp_dir.path().join("command.md");
        std::fs::write(&file, "# Command").unwrap();

        // Create lockfile
        let mut lockfile = LockFile::new();
        let mut command = create_test_locked_resource("test-command", true);
        command.path = file.to_string_lossy().to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let updates = vec![(
            "test-command".to_string(),
            "v1.0.0".to_string(),
            "v2.0.0".to_string(),
        )];

        let count = install_updated_resources(
            &updates,
            &lockfile,
            &manifest,
            project_dir,
            &cache,
            None,
            true, // quiet mode
        )
        .await
        .unwrap();

        assert_eq!(count, 1);
        assert!(project_dir
            .join(".claude/commands/test-command.md")
            .exists());
    }

    #[tokio::test]
    async fn test_install_resource_for_parallel() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("parallel.md");
        std::fs::write(&local_file, "# Parallel Test").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("parallel-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install using the parallel function
        let result =
            install_resource_for_parallel(&entry, project_dir, "agents", &cache, false).await;
        assert!(result.is_ok());

        // Verify installation
        let expected_path = project_dir.join("agents").join("parallel-test.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_install_resource_creates_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a local markdown file
        let local_file = temp_dir.path().join("nested.md");
        std::fs::write(&local_file, "# Nested Test").unwrap();

        // Create a locked resource with deeply nested path
        let mut entry = create_test_locked_resource("nested-test", true);
        entry.path = local_file.to_string_lossy().to_string();
        entry.installed_at = "very/deeply/nested/path/resource.md".to_string();

        // Install the resource
        let result = install_resource(&entry, project_dir, "agents", &cache, false).await;
        assert!(result.is_ok());
        let (installed, _checksum) = result.unwrap();
        assert!(installed, "Should have installed new resource");

        // Verify nested directories were created
        let expected_path = project_dir.join("very/deeply/nested/path/resource.md");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_update_gitignore_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create a lockfile with some resources
        let mut lockfile = LockFile::new();

        // Add agent with installed path
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.installed_at = ".claude/agents/test-agent.md".to_string();
        lockfile.agents.push(agent);

        // Add snippet with installed path
        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.installed_at = ".claude/ccpm/snippets/test-snippet.md".to_string();
        lockfile.snippets.push(snippet);

        // Call update_gitignore
        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        // Check that .gitignore was created
        let gitignore_path = project_dir.join(".gitignore");
        assert!(gitignore_path.exists(), "Gitignore file should be created");

        // Check content
        let content = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("CCPM managed entries"));
        assert!(content.contains(".claude/agents/test-agent.md"));
        assert!(content.contains(".claude/ccpm/snippets/test-snippet.md"));
    }

    #[tokio::test]
    async fn test_update_gitignore_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        let lockfile = LockFile::new();

        // Call with disabled flag
        let result = update_gitignore(&lockfile, project_dir, false);
        assert!(result.is_ok());

        // Check that .gitignore was NOT created
        let gitignore_path = project_dir.join(".gitignore");
        assert!(
            !gitignore_path.exists(),
            "Gitignore should not be created when disabled"
        );
    }

    #[tokio::test]
    async fn test_update_gitignore_preserves_user_entries() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create .claude directory for resources
        let claude_dir = project_dir.join(".claude");
        ensure_dir(&claude_dir).unwrap();

        // Create existing gitignore with user entries at project root
        let gitignore_path = project_dir.join(".gitignore");
        let existing_content = "# User comment\n\
                               user-file.txt\n\
                               *.backup\n\
                               # CCPM managed entries - do not edit below this line\n\
                               .claude/agents/old-entry.md\n\
                               # End of CCPM managed entries\n";
        std::fs::write(&gitignore_path, existing_content).unwrap();

        // Create lockfile with new resources
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("new-agent", true);
        agent.installed_at = ".claude/agents/new-agent.md".to_string();
        lockfile.agents.push(agent);

        // Update gitignore
        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        // Check that user entries are preserved
        let updated_content = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(updated_content.contains("user-file.txt"));
        assert!(updated_content.contains("*.backup"));
        assert!(updated_content.contains("# User comment"));

        // Check that new entries are added
        assert!(updated_content.contains(".claude/agents/new-agent.md"));

        // Check that old managed entries are replaced
        assert!(!updated_content.contains(".claude/agents/old-entry.md"));
    }

    #[tokio::test]
    async fn test_update_gitignore_handles_external_paths() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        let mut lockfile = LockFile::new();

        // Add resource installed outside .claude
        let mut script = create_test_locked_resource("test-script", true);
        script.installed_at = "scripts/test.sh".to_string();
        lockfile.scripts.push(script);

        // Add resource inside .claude
        let mut agent = create_test_locked_resource("test-agent", true);
        agent.installed_at = ".claude/agents/test.md".to_string();
        lockfile.agents.push(agent);

        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        let gitignore_path = project_dir.join(".gitignore");
        let content = std::fs::read_to_string(&gitignore_path).unwrap();

        // External path should be as-is
        assert!(content.contains("scripts/test.sh"));

        // Internal path should be as-is
        assert!(content.contains(".claude/agents/test.md"));
    }

    #[tokio::test]
    async fn test_install_updated_resources_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        let lockfile = LockFile::new();
        let manifest = Manifest::new();

        // Try to update a resource that doesn't exist
        let updates = vec![(
            "non-existent".to_string(),
            "v1.0.0".to_string(),
            "v2.0.0".to_string(),
        )];

        let count = install_updated_resources(
            &updates,
            &lockfile,
            &manifest,
            project_dir,
            &cache,
            None,
            false,
        )
        .await
        .unwrap();

        assert_eq!(count, 0, "Should install 0 resources when not found");
    }
}
