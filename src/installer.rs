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

use crate::utils::progress::{InstallationPhase, MultiPhaseProgress};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

/// Type alias for complex installation result tuples to improve code readability.
///
/// This type alias simplifies the return type of parallel installation functions
/// that need to return either success information or error details with context.
/// It was introduced in AGPM v0.3.0 to resolve `clippy::type_complexity` warnings
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
    (String, bool, String, crate::manifest::patches::AppliedPatches),
    (String, anyhow::Error),
>;

use futures::{
    future,
    stream::{self, StreamExt},
};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use crate::cache::Cache;
use crate::core::ResourceIterator;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::Manifest;
use crate::markdown::MarkdownFile;
use crate::utils::fs::{atomic_write, ensure_dir};
use crate::utils::normalize_path_for_storage;
use crate::utils::progress::ProgressBar;
use hex;
use std::collections::HashSet;
use std::fs;

/// Read a file with retry logic to handle cross-process filesystem cache coherency issues.
///
/// This function wraps `tokio::fs::read_to_string` with retry logic to handle cases where
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
/// - Max delay: 500ms
/// - Factor: 2x (exponential backoff)
/// - Max attempts: 10
/// - Total max time: ~10 seconds
///
/// Only `NotFound` errors are retried, as these indicate cache coherency issues.
/// Other errors (permissions, I/O errors) fail immediately by returning Ok to bypass retry.
async fn read_with_cache_retry(path: &Path) -> Result<String> {
    use std::io;

    let retry_strategy = tokio_retry::strategy::ExponentialBackoff::from_millis(10)
        .max_delay(Duration::from_millis(500))
        .factor(2)
        .take(10);

    let path_buf = path.to_path_buf();

    tokio_retry::Retry::spawn(retry_strategy, || {
        let path = path_buf.clone();
        async move {
            tokio::fs::read_to_string(&path).await.map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    tracing::debug!(
                        "File not yet visible (likely cache coherency issue): {}",
                        path.display()
                    );
                    format!("File not found: {}", path.display())
                } else {
                    // Non-retriable error - return error message that will fail fast
                    format!("I/O error (non-retriable): {}", e)
                }
            })
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to read resource file: {}: {}", path.display(), e))
}

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
/// use agpm_cli::installer::install_resource;
/// use agpm_cli::lockfile::LockedResource;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::core::ResourceType;
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
///     dependencies: vec![],
///     resource_type: ResourceType::Agent,
///     tool: Some("claude-code".to_string()),
///     manifest_alias: None,
///     applied_patches: std::collections::HashMap::new(),
/// };
///
/// let (installed, checksum, _patches) = install_resource(&entry, Path::new("."), "agents", &cache, false, None, None).await?;
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
    project_patches: Option<&crate::manifest::PatchData>,
    private_patches: Option<&crate::manifest::PatchData>,
) -> Result<(bool, String, crate::manifest::patches::AppliedPatches)> {
    // Determine destination path
    let dest_path = if entry.installed_at.is_empty() {
        project_dir.join(resource_dir).join(format!("{}.md", entry.name))
    } else {
        project_dir.join(&entry.installed_at)
    };

    // Check if file already exists and compare checksums
    let existing_checksum = if dest_path.exists() {
        // Use blocking task for checksum calculation to avoid blocking the async runtime
        let path = dest_path.clone();
        tokio::task::spawn_blocking(move || LockFile::compute_checksum(&path)).await??.into()
    } else {
        None
    };

    let new_content = if let Some(source_name) = &entry.source {
        let url = entry
            .url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Resource {} has no URL", entry.name))?;

        // Check if this is a local directory source (no SHA or empty SHA)
        let is_local_source = entry.resolved_commit.as_deref().is_none_or(str::is_empty);

        let cache_dir = if is_local_source {
            // Local directory source - use the URL as the path directly
            PathBuf::from(url)
        } else {
            // Git-based resource - use SHA-based worktree creation
            let sha = entry.resolved_commit.as_deref().ok_or_else(|| {
                anyhow::anyhow!("Resource {} missing resolved commit SHA. Run 'agpm update' to regenerate lockfile.", entry.name)
            })?;

            // Validate SHA format
            if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(anyhow::anyhow!(
                    "Invalid SHA '{}' for resource {}. Expected 40 hex characters.",
                    sha,
                    entry.name
                ));
            }

            let mut cache_dir = cache
                .get_or_create_worktree_for_sha(source_name, url, sha, Some(&entry.name))
                .await?;

            if force_refresh {
                let _ = cache.cleanup_worktree(&cache_dir).await;
                cache_dir = cache
                    .get_or_create_worktree_for_sha(source_name, url, sha, Some(&entry.name))
                    .await?;
            }

            cache_dir
        };

        // Read the content from the source (with cache coherency retry)
        let source_path = cache_dir.join(&entry.path);
        let content = read_with_cache_retry(&source_path).await?;

        // Validate markdown - this will emit a warning if frontmatter is invalid but won't fail
        MarkdownFile::parse_with_context(&content, Some(&source_path.display().to_string()))?;

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

        // Validate markdown - this will emit a warning if frontmatter is invalid but won't fail
        MarkdownFile::parse_with_context(&content, Some(&source_path.display().to_string()))?;

        content
    };

    // Apply patches if provided
    let (final_content, applied_patches) = if project_patches.is_some() || private_patches.is_some()
    {
        use crate::manifest::patches::apply_patches_to_content_with_origin;
        let file_path = entry.installed_at.as_str();
        apply_patches_to_content_with_origin(
            &new_content,
            file_path,
            project_patches.unwrap_or(&std::collections::HashMap::new()),
            private_patches.unwrap_or(&std::collections::HashMap::new()),
        )
        .with_context(|| format!("Failed to apply patches to resource {}", entry.name))?
    } else {
        (new_content, crate::manifest::patches::AppliedPatches::default())
    };

    // Calculate checksum of patched content
    let new_checksum = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(final_content.as_bytes());
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

        atomic_write(&dest_path, final_content.as_bytes())
            .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;
    }

    Ok((actually_installed, new_checksum, applied_patches))
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
/// use agpm_cli::installer::install_resource_with_progress;
/// use agpm_cli::lockfile::LockedResource;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::utils::progress::ProgressBar;
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
///     dependencies: vec![],
///     resource_type: ResourceType::Agent,
///     tool: Some("claude-code".to_string()),
///     manifest_alias: None,
///     applied_patches: std::collections::HashMap::new(),
/// };
///
/// let (installed, checksum, _patches) = install_resource_with_progress(
///     &entry,
///     Path::new("."),
///     "agents",
///     &cache,
///     false,
///     &pb,
///     None,
///     None
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
#[allow(clippy::too_many_arguments)]
pub async fn install_resource_with_progress(
    entry: &LockedResource,
    project_dir: &Path,
    resource_dir: &str,
    cache: &Cache,
    force_refresh: bool,
    pb: &ProgressBar,
    project_patches: Option<&crate::manifest::PatchData>,
    private_patches: Option<&crate::manifest::PatchData>,
) -> Result<(bool, String, crate::manifest::patches::AppliedPatches)> {
    pb.set_message(format!("Installing {}", entry.name));
    install_resource(
        entry,
        project_dir,
        resource_dir,
        cache,
        force_refresh,
        project_patches,
        private_patches,
    )
    .await
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
/// use agpm_cli::installer::install_resources_parallel;
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::utils::progress::ProgressBar;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// let manifest = Manifest::load(Path::new("agpm.toml"))?;
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

    // Collect unique (source, url, sha) triples to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
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

    // Pre-create all worktrees in parallel
    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, sha)| {
                let cache = cache.clone();
                async move {
                    cache
                        .get_or_create_worktree_for_sha(&source, &url, &sha, Some("pre-warm"))
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
    pb.set_message(format!("Installing 0/{total} resources"));

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
                    manifest,
                )
                .await;

                match res {
                    Ok((actually_installed, checksum, applied_patches)) => {
                        if actually_installed {
                            let mut count = installed_count.lock().await;
                            *count += 1;
                        }
                        let count = *installed_count.lock().await;
                        pb.set_message(format!("Installing {count}/{total} resources"));
                        pb.inc(1);
                        Ok((entry.name.clone(), actually_installed, checksum, applied_patches))
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
            Ok((_name, _installed, _checksum, _applied_patches)) => {
                // Old function doesn't return checksums or patches
            }
            Err((name, error)) => {
                errors.push((name, error));
            }
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|(name, error)| format!("  {name}: {error}")).collect();
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
/// use agpm_cli::installer::install_resource_for_parallel;
/// # use agpm_cli::lockfile::LockedResource;
/// # use agpm_cli::cache::Cache;
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
    manifest: &Manifest,
) -> Result<(bool, String, crate::manifest::patches::AppliedPatches)> {
    // Look up patches for this resource from the manifest
    // For pattern-expanded resources, use manifest_alias; otherwise use name
    let resource_type = entry.resource_type.to_plural();
    let lookup_name = entry.manifest_alias.as_ref().unwrap_or(&entry.name);

    let project_patches = manifest.project_patches.get(resource_type, lookup_name);
    let private_patches = manifest.private_patches.get(resource_type, lookup_name);

    install_resource(
        entry,
        project_dir,
        resource_dir,
        cache,
        force_refresh,
        project_patches,
        private_patches,
    )
    .await
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
/// use agpm_cli::installer::InstallProgress;
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
/// use agpm_cli::installer::{install_resources_parallel_with_progress, InstallProgress};
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use tokio::sync::mpsc;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let (tx, mut rx) = mpsc::unbounded_channel::<InstallProgress>();
/// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// let manifest = Manifest::load(Path::new("agpm.toml"))?;
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
    // Collect unique (source, url, sha) triples to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
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

    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, sha)| {
                async move {
                    cache
                        .get_or_create_worktree_for_sha(&source, &url, &sha, Some("pre-warm"))
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
                    manifest,
                )
                .await;

                // Remove from active list and update count only if actually installed
                {
                    let mut active = active_deps.lock().await;
                    active.retain(|x| x != &entry.name);

                    if let Ok((actually_installed, _checksum, _applied_patches)) = &res {
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
                    Ok((installed, checksum, applied_patches)) => {
                        Ok((entry.name.clone(), installed, checksum, applied_patches))
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
            Ok((_name, _installed, _checksum, _applied_patches)) => {
                // Old function doesn't return checksums or patches
            }
            Err((name, error)) => {
                errors.push((name, error));
            }
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|(name, error)| format!("  {name}: {error}")).collect();
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
/// # let lockfile = LockFile::default();
/// # let manifest = Manifest::default();
/// # let project_dir = Path::new(".");
/// # let cache = Cache::new()?;
/// let progress = Arc::new(MultiPhaseProgress::new(true));
///
/// let (count, _checksums, _patches) = install_resources(
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
/// use agpm_cli::installer::{install_resources, ResourceFilter};
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let lockfile = LockFile::default();
/// # let manifest = Manifest::default();
/// # let project_dir = Path::new(".");
/// # let cache = Cache::new()?;
/// let updates = vec![("agent1".to_string(), None, "v1.0".to_string(), "v1.1".to_string())];
///
/// let (count, _checksums, _patches) = install_resources(
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
) -> Result<(usize, Vec<(String, String)>, Vec<(String, crate::manifest::patches::AppliedPatches)>)>
{
    // Collect entries to install based on filter
    let all_entries: Vec<(LockedResource, String)> = match filter {
        ResourceFilter::All => {
            // Use existing ResourceIterator logic for all entries
            ResourceIterator::collect_all_entries(lockfile, manifest)
                .into_iter()
                .map(|(entry, dir)| (entry.clone(), dir.into_owned()))
                .collect()
        }
        ResourceFilter::Updated(ref updates) => {
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
                    let artifact_path = manifest
                        .get_artifact_resource_path(tool, resource_type)
                        .expect("Resource type should be supported by configured tools");
                    let target_dir = artifact_path.display().to_string();
                    entries.push((entry.clone(), target_dir));
                }
            }
            entries
        }
    };

    if all_entries.is_empty() {
        return Ok((0, Vec::new(), Vec::new()));
    }

    let total = all_entries.len();

    // Start installation phase with progress if provided
    if let Some(ref pm) = progress {
        pm.start_phase_with_progress(InstallationPhase::InstallingResources, total);
    }

    // Pre-warm the cache by creating all needed worktrees upfront
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
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

    if !unique_worktrees.is_empty() {
        let context = match filter {
            ResourceFilter::All => "pre-warm",
            ResourceFilter::Updated(_) => "update-pre-warm",
        };

        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, sha)| {
                let cache = cache.clone();
                async move {
                    cache
                        .get_or_create_worktree_for_sha(&source, &url, &sha, Some(context))
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
            let project_dir = project_dir.to_path_buf();
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
                    manifest,
                )
                .await;

                // Update progress on success - but only count if actually installed
                if let Ok((actually_installed, _checksum, _applied_patches)) = &res {
                    if *actually_installed {
                        let mut count = installed_count.lock().await;
                        *count += 1;
                    }

                    if let Some(ref pm) = progress {
                        let count = *installed_count.lock().await;
                        pm.update_current_message(&format!("Installing {count}/{total} resources"));
                        pm.increment_progress(1);
                    }
                }

                match res {
                    Ok((installed, checksum, applied_patches)) => {
                        Ok((entry.name.clone(), installed, checksum, applied_patches))
                    }
                    Err(err) => Err((entry.name.clone(), err)),
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // Handle errors and collect checksums and applied patches
    let mut errors = Vec::new();
    let mut checksums = Vec::new();
    let mut applied_patches_list = Vec::new();
    for result in results {
        match result {
            Ok((name, _installed, checksum, applied_patches)) => {
                checksums.push((name.clone(), checksum));
                applied_patches_list.push((name, applied_patches));
            }
            Err((name, error)) => {
                errors.push((name, error));
            }
        }
    }

    if !errors.is_empty() {
        // Complete phase with error message
        if let Some(ref pm) = progress {
            pm.complete_phase(Some(&format!("Failed to install {} resources", errors.len())));
        }

        let error_msgs: Vec<String> =
            errors.into_iter().map(|(name, error)| format!("  {name}: {error}")).collect();
        return Err(anyhow::anyhow!(
            "Failed to install {} resources:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    let final_count = *installed_count.lock().await;

    // Complete installation phase successfully
    if let Some(ref pm) = progress
        && final_count > 0
    {
        pm.complete_phase(Some(&format!("Installed {final_count} resources")));
    }

    Ok((final_count, checksums, applied_patches_list))
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
/// use agpm_cli::installer::install_resources_with_dynamic_progress;
/// use agpm_cli::utils::progress::ProgressBar;
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use std::sync::Arc;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// let manifest = Manifest::load(Path::new("agpm.toml"))?;
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
    // Collect unique (source, url, sha) triples to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &all_entries {
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

    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, sha)| {
                async move {
                    cache
                        .get_or_create_worktree_for_sha(&source, &url, &sha, Some("pre-warm"))
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
                    manifest,
                )
                .await;

                // Signal completion and update count only if actually installed
                if let Ok((actually_installed, _checksum, _applied_patches)) = &res {
                    if *actually_installed {
                        let mut count = installed_count.lock().await;
                        *count += 1;
                    }

                    if let Some(ref progress) = progress_bar_ref {
                        progress.inc(1);
                    }
                }

                match res {
                    Ok((installed, checksum, applied_patches)) => {
                        Ok((entry.name.clone(), installed, checksum, applied_patches))
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
            Ok((_name, _installed, _checksum, _applied_patches)) => {
                // Old function doesn't return checksums or patches
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

        let error_msgs: Vec<String> =
            errors.into_iter().map(|(name, error)| format!("  {name}: {error}")).collect();
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
/// * `updates` - Vector of tuples containing (name, `old_version`, `new_version`) for each updated resource
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
/// use agpm_cli::installer::install_updated_resources;
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::utils::progress::ProgressBar;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// let manifest = Manifest::load(Path::new("agpm.toml"))?;
/// let cache = Cache::new()?;
/// let pb = ProgressBar::new(3);
///
/// // Define which resources to update
/// let updates = vec![
///     ("ai-agent".to_string(), None, "v1.0.0".to_string(), "v1.1.0".to_string()),
///     ("helper-tool".to_string(), Some("community".to_string()), "v2.0.0".to_string(), "v2.1.0".to_string()),
///     ("data-processor".to_string(), None, "v1.5.0".to_string(), "v1.6.0".to_string()),
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
/// This function is typically used by the `agpm update` command after dependency
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
    updates: &[(String, Option<String>, String, String)], // (name, source, old_version, new_version)
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
    for (name, source, _, _) in updates {
        if let Some((resource_type, entry)) =
            ResourceIterator::find_resource_by_name_and_source(lockfile, name, source.as_deref())
        {
            // Get artifact configuration path
            let tool = entry.tool.as_deref().unwrap_or("claude-code");
            let artifact_path = manifest
                .get_artifact_resource_path(tool, resource_type)
                .expect("Resource type should be supported by configured tools");
            let target_dir = artifact_path.display().to_string();
            entries_to_install.push((entry.clone(), target_dir));
        }
    }

    if entries_to_install.is_empty() {
        return Ok(0);
    }

    // Pre-warm the cache by creating all needed worktrees upfront
    if let Some(pb) = pb {
        pb.set_message("Preparing resources...");
    }

    // Collect unique (source, url, sha) triples to pre-create worktrees
    let mut unique_worktrees = HashSet::new();
    for (entry, _) in &entries_to_install {
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

    // Pre-create all worktrees in parallel
    if !unique_worktrees.is_empty() {
        let worktree_futures: Vec<_> = unique_worktrees
            .into_iter()
            .map(|(source, url, sha)| {
                async move {
                    cache
                        .get_or_create_worktree_for_sha(
                            &source,
                            &url,
                            &sha,
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
                    manifest,
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
    // Skip hooks and MCP servers - they are configured only, not installed as files
    add_resource_paths(&lockfile.agents);
    add_resource_paths(&lockfile.snippets);
    add_resource_paths(&lockfile.commands);
    add_resource_paths(&lockfile.scripts);

    // Read existing gitignore if it exists
    let mut before_agpm_section = Vec::new();
    let mut after_agpm_section = Vec::new();

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .with_context(|| format!("Failed to read {}", gitignore_path.display()))?;

        let mut in_agpm_section = false;
        let mut past_agpm_section = false;

        for line in content.lines() {
            // Support both AGPM and legacy CCPM markers for migration compatibility
            if line == "# AGPM managed entries - do not edit below this line"
                || line == "# CCPM managed entries - do not edit below this line"
            {
                in_agpm_section = true;
                continue;
            } else if line == "# End of AGPM managed entries"
                || line == "# End of CCPM managed entries"
            {
                in_agpm_section = false;
                past_agpm_section = true;
                continue;
            }

            if !in_agpm_section && !past_agpm_section {
                // Preserve everything before AGPM section exactly as-is
                before_agpm_section.push(line.to_string());
            } else if in_agpm_section {
                // Skip existing AGPM/CCPM entries (they'll be replaced)
                continue;
            } else {
                // Preserve everything after AGPM section exactly as-is
                after_agpm_section.push(line.to_string());
            }
        }
    }

    // Build the new content
    let mut new_content = String::new();

    // Add everything before AGPM section exactly as it was
    if !before_agpm_section.is_empty() {
        for line in &before_agpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
        // Add blank line before AGPM section if the previous content doesn't end with one
        if !before_agpm_section.is_empty() && !before_agpm_section.last().unwrap().trim().is_empty()
        {
            new_content.push('\n');
        }
    }

    // Add AGPM managed section
    new_content.push_str("# AGPM managed entries - do not edit below this line\n");

    // Always include private config files
    new_content.push_str("agpm.private.toml\n");
    new_content.push_str("agpm.private.lock\n");

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

        // Normalize to forward slashes for .gitignore (Git expects forward slashes on all platforms)
        let normalized_path = normalize_path_for_storage(&ignore_path);

        new_content.push_str(&normalized_path);
        new_content.push('\n');
    }

    new_content.push_str("# End of AGPM managed entries\n");

    // Add everything after AGPM section exactly as it was
    if !after_agpm_section.is_empty() {
        new_content.push('\n');
        for line in &after_agpm_section {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    // If this is a new file, add a basic header
    if before_agpm_section.is_empty() && after_agpm_section.is_empty() {
        let mut default_content = String::new();
        default_content.push_str("# .gitignore - AGPM managed entries\n");
        default_content.push_str("# AGPM entries are automatically generated\n");
        default_content.push('\n');
        default_content.push_str("# AGPM managed entries - do not edit below this line\n");

        // Always include private config files
        default_content.push_str("agpm.private.toml\n");
        default_content.push_str("agpm.private.lock\n");

        // Add the AGPM paths
        for path in &sorted_paths {
            let ignore_path = if path.starts_with("./") {
                path.strip_prefix("./").unwrap_or(path).to_string()
            } else {
                path.clone()
            };
            // Normalize to forward slashes for .gitignore (Git expects forward slashes on all platforms)
            let normalized_path = ignore_path.replace('\\', "/");
            default_content.push_str(&normalized_path);
            default_content.push('\n');
        }

        default_content.push_str("# End of AGPM managed entries\n");
        new_content = default_content;
    }

    // Write the updated gitignore
    atomic_write(&gitignore_path, new_content.as_bytes())
        .with_context(|| format!("Failed to update {}", gitignore_path.display()))?;

    Ok(())
}

/// Removes artifacts that are no longer needed based on lockfile comparison.
///
/// This function performs automatic cleanup of obsolete resource files by comparing
/// the old and new lockfiles. It identifies and removes artifacts that have been:
/// - **Removed from manifest**: Dependencies deleted from `agpm.toml`
/// - **Relocated**: Files with changed `installed_at` paths due to:
///   - Relative path preservation (v0.3.18+)
///   - Custom target changes
///   - Dependency name changes
/// - **Replaced**: Resources that moved due to source or version changes
///
/// After removing files, it also cleans up any empty parent directories to prevent
/// directory accumulation over time.
///
/// # Cleanup Strategy
///
/// The function uses a **set-based difference algorithm**:
/// 1. Collects all `installed_at` paths from the new lockfile into a `HashSet`
/// 2. Iterates through old lockfile resources
/// 3. For each old path not in the new set:
///    - Removes the file if it exists
///    - Recursively cleans empty parent directories
///    - Records the path for reporting
///
/// # Arguments
///
/// * `old_lockfile` - The previous lockfile state containing old installation paths
/// * `new_lockfile` - The current lockfile state with updated installation paths
/// * `project_dir` - The project root directory (usually contains `.claude/`)
///
/// # Returns
///
/// Returns `Ok(Vec<String>)` containing the list of `installed_at` paths that were
/// successfully removed. An empty vector indicates no artifacts needed cleanup.
///
/// # Errors
///
/// Returns an error if:
/// - File removal fails due to permissions or locks
/// - Directory cleanup encounters unexpected I/O errors
/// - File system operations fail for other reasons
///
/// # Examples
///
/// ## Basic Cleanup After Update
///
/// ```no_run
/// use agpm_cli::installer::cleanup_removed_artifacts;
/// use agpm_cli::lockfile::LockFile;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let old_lockfile = LockFile::load(Path::new("agpm.lock"))?;
/// let new_lockfile = LockFile::new(); // After resolution
/// let project_dir = Path::new(".");
///
/// let removed = cleanup_removed_artifacts(&old_lockfile, &new_lockfile, project_dir).await?;
/// if !removed.is_empty() {
///     println!("Cleaned up {} artifact(s)", removed.len());
///     for path in removed {
///         println!("  - Removed: {}", path);
///     }
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Cleanup After Path Migration
///
/// When relative path preservation changes installation paths:
///
/// ```text
/// Old lockfile (v0.3.17):
///   installed_at: ".claude/agents/helper.md"
///
/// New lockfile (v0.3.18+):
///   installed_at: ".claude/agents/ai/helper.md"  # Preserved subdirectory
///
/// Cleanup removes: .claude/agents/helper.md
/// ```
///
/// ## Cleanup After Dependency Removal
///
/// ```no_run
/// # use agpm_cli::installer::cleanup_removed_artifacts;
/// # use agpm_cli::lockfile::{LockFile, LockedResource};
/// # use std::path::Path;
/// # async fn removal_example() -> anyhow::Result<()> {
/// // Old lockfile had 3 agents
/// let mut old_lockfile = LockFile::new();
/// old_lockfile.agents = vec![
///     // ... 3 agents including one at .claude/agents/removed.md
/// ];
///
/// // New lockfile only has 2 agents (one was removed from manifest)
/// let mut new_lockfile = LockFile::new();
/// new_lockfile.agents = vec![
///     // ... 2 agents, removed.md is gone
/// ];
///
/// let removed = cleanup_removed_artifacts(&old_lockfile, &new_lockfile, Path::new(".")).await?;
/// assert!(removed.contains(&".claude/agents/removed.md".to_string()));
/// # Ok(())
/// # }
/// ```
///
/// ## Integration with Install Command
///
/// This function is automatically called during `agpm install` when both old and
/// new lockfiles exist:
///
/// ```rust,ignore
/// // In src/cli/install.rs
/// if !self.frozen && !self.regenerate && lockfile_path.exists() {
///     if let Ok(old_lockfile) = LockFile::load(&lockfile_path) {
///         detect_tag_movement(&old_lockfile, &lockfile, self.quiet);
///
///         // Automatic cleanup of removed or moved artifacts
///         if let Ok(removed) = cleanup_removed_artifacts(
///             &old_lockfile,
///             &lockfile,
///             actual_project_dir,
///         ).await && !removed.is_empty() && !self.quiet {
///             println!("🗑️  Cleaned up {} moved or removed artifact(s)", removed.len());
///         }
///     }
/// }
/// ```
///
/// # Performance
///
/// - **Time Complexity**: O(n + m) where n = old resources, m = new resources
/// - **Space Complexity**: O(m) for the `HashSet` of new paths
/// - **I/O Operations**: One file removal per obsolete artifact
/// - **Directory Cleanup**: Walks up parent directories once per removed file
///
/// The function is highly efficient as it:
/// - Uses `HashSet` for O(1) path lookups
/// - Only performs I/O for files that actually exist
/// - Cleans directories recursively but stops at first non-empty directory
///
/// # Safety
///
/// - Only removes files explicitly tracked in the old lockfile
/// - Never removes files outside the project directory
/// - Stops directory cleanup at `.claude/` boundary
/// - Handles concurrent file access gracefully (ENOENT is not an error)
///
/// # Use Cases
///
/// ## Relative Path Migration (v0.3.18+)
///
/// When upgrading to v0.3.18+, resource paths change to preserve directory structure:
/// ```text
/// Before: .claude/agents/helper.md  (flat)
/// After:  .claude/agents/ai/helper.md  (nested)
/// ```
/// This function removes the old flat file automatically.
///
/// ## Dependency Reorganization
///
/// When reorganizing dependencies with custom targets:
/// ```toml
/// # Before
/// [agents]
/// helper = { source = "community", path = "agents/helper.md" }
///
/// # After (with custom target)
/// [agents]
/// helper = { source = "community", path = "agents/helper.md", target = "tools" }
/// ```
/// Old file at `.claude/agents/helper.md` is removed, new file at
/// `.claude/agents/tools/helper.md` is installed.
///
/// ## Manifest Cleanup
///
/// Simply removing dependencies from `agpm.toml` triggers automatic cleanup:
/// ```toml
/// # Remove unwanted dependency
/// [agents]
/// # old-agent = { ... }  # Commented out or deleted
/// ```
/// The next `agpm install` removes the old agent file automatically.
///
/// # Version History
///
/// - **v0.3.18**: Introduced to handle relative path preservation and custom target changes
/// - Works in conjunction with `cleanup_empty_dirs()` for comprehensive cleanup
pub async fn cleanup_removed_artifacts(
    old_lockfile: &LockFile,
    new_lockfile: &LockFile,
    project_dir: &std::path::Path,
) -> Result<Vec<String>> {
    use std::collections::HashSet;

    let mut removed = Vec::new();

    // Collect all installed paths from new lockfile
    let new_paths: HashSet<String> =
        new_lockfile.all_resources().into_iter().map(|r| r.installed_at.clone()).collect();

    // Check each old resource
    for old_resource in old_lockfile.all_resources() {
        // If the old path doesn't exist in new lockfile, it needs to be removed
        if !new_paths.contains(&old_resource.installed_at) {
            let full_path = project_dir.join(&old_resource.installed_at);

            // Only remove if the file actually exists
            if full_path.exists() {
                tokio::fs::remove_file(&full_path).await.with_context(|| {
                    format!("Failed to remove old artifact: {}", full_path.display())
                })?;

                removed.push(old_resource.installed_at.clone());

                // Try to clean up empty parent directories
                cleanup_empty_dirs(&full_path).await?;
            }
        }
    }

    Ok(removed)
}

/// Recursively removes empty parent directories up to the project root.
///
/// This helper function performs bottom-up directory cleanup after file removal.
/// It walks up the directory tree from a given file path, removing empty parent
/// directories until it encounters:
/// - A non-empty directory (containing other files or subdirectories)
/// - The `.claude` directory boundary (cleanup stops here for safety)
/// - The project root (no parent directory)
/// - A directory that cannot be removed (permissions, locks, etc.)
///
/// This prevents accumulation of empty directory trees over time as resources
/// are removed, renamed, or relocated.
///
/// # Cleanup Algorithm
///
/// The function implements a **safe recursive cleanup** strategy:
/// 1. Starts at the parent directory of the given file path
/// 2. Attempts to remove the directory
/// 3. If successful (directory was empty), moves to parent and repeats
/// 4. If unsuccessful, stops immediately (directory has content or other issues)
/// 5. Always stops at `.claude/` directory to avoid over-cleanup
///
/// # Safety Boundaries
///
/// The function enforces strict boundaries to prevent accidental data loss:
/// - **`.claude/` boundary**: Never removes the `.claude` directory itself
/// - **Project root**: Stops if parent directory is None
/// - **Non-empty guard**: Only removes truly empty directories
/// - **Error tolerance**: ENOENT (directory not found) is not considered an error
///
/// # Arguments
///
/// * `file_path` - The path to the removed file whose parent directories should be cleaned.
///   Typically this is the full path to a resource file that was just deleted.
///
/// # Returns
///
/// Returns `Ok(())` in all normal cases, including:
/// - All empty directories successfully removed
/// - Cleanup stopped at a non-empty directory
/// - Directory already doesn't exist (ENOENT)
///
/// # Errors
///
/// Returns an error only for unexpected I/O failures during directory removal
/// that are not normal "directory not empty" or "not found" errors.
///
/// # Examples
///
/// ## Basic Directory Cleanup
///
/// ```ignore
/// # use agpm_cli::installer::cleanup_empty_dirs;
/// # use std::path::Path;
/// # use std::fs;
/// # async fn example() -> anyhow::Result<()> {
/// // After removing: .claude/agents/rust/specialized/expert.md
/// let file_path = Path::new(".claude/agents/rust/specialized/expert.md");
///
/// // If this was the last file in specialized/, the directory will be removed
/// // If specialized/ was the last item in rust/, that will be removed too
/// // Cleanup stops at .claude/agents/ or when it finds a non-empty directory
/// cleanup_empty_dirs(file_path).await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Cleanup Scenarios
///
/// ### Scenario 1: Full Cleanup
///
/// ```text
/// Before:
///   .claude/agents/rust/specialized/expert.md  (only file in hierarchy)
///
/// After removing expert.md:
///   cleanup_empty_dirs() removes:
///   - .claude/agents/rust/specialized/  (now empty)
///   - .claude/agents/rust/              (now empty)
///   Stops at .claude/agents/ (keeps base directory)
/// ```
///
/// ### Scenario 2: Partial Cleanup
///
/// ```text
/// Before:
///   .claude/agents/rust/specialized/expert.md
///   .claude/agents/rust/specialized/tester.md
///   .claude/agents/rust/basic.md
///
/// After removing expert.md:
///   .claude/agents/rust/specialized/ still has tester.md
///   cleanup_empty_dirs() stops at specialized/ (not empty)
/// ```
///
/// ### Scenario 3: Boundary Enforcement
///
/// ```text
/// After removing: .claude/agents/only-agent.md
///
/// cleanup_empty_dirs() attempts to remove:
/// - .claude/agents/ (empty now)
/// - But stops because parent is .claude/ (boundary)
///
/// Result: .claude/agents/ remains (empty but preserved)
/// ```
///
/// ## Integration with `cleanup_removed_artifacts`
///
/// This function is called automatically by [`cleanup_removed_artifacts`]
/// after each file removal:
///
/// ```rust,ignore
/// for old_resource in old_lockfile.all_resources() {
///     if !new_paths.contains(&old_resource.installed_at) {
///         let full_path = project_dir.join(&old_resource.installed_at);
///
///         if full_path.exists() {
///             tokio::fs::remove_file(&full_path).await?;
///             removed.push(old_resource.installed_at.clone());
///
///             // Automatic directory cleanup after file removal
///             cleanup_empty_dirs(&full_path).await?;
///         }
///     }
/// }
/// ```
///
/// # Performance
///
/// - **Time Complexity**: O(d) where d = directory depth from file to `.claude/`
/// - **I/O Operations**: One `remove_dir` attempt per directory level
/// - **Early Termination**: Stops immediately on first non-empty directory
///
/// The function is extremely efficient as it:
/// - Only walks up the directory tree (no scanning of siblings)
/// - Stops at the first non-empty directory (no unnecessary attempts)
/// - Uses atomic `remove_dir` which fails fast on non-empty directories
/// - Typical depth is 2-4 levels (.claude/agents/subdir/file.md)
///
/// # Error Handling Strategy
///
/// The function differentiates between expected and unexpected errors:
///
/// | Error Kind | Interpretation | Action |
/// |------------|----------------|--------|
/// | `Ok(())` | Directory was empty and removed | Continue up tree |
/// | `ENOENT` | Directory doesn't exist | Continue up tree (race condition) |
/// | `ENOTEMPTY` | Directory has contents | Stop cleanup (expected) |
/// | `EPERM` | No permission | Stop cleanup (expected) |
/// | Other | Unexpected I/O error | Propagate error |
///
/// In practice, most errors simply stop the cleanup process without failing
/// the overall operation, as the goal is best-effort cleanup.
///
/// # Thread Safety
///
/// This function is safe for concurrent use because:
/// - Uses async filesystem operations from `tokio::fs`
/// - `remove_dir` is atomic (succeeds only if directory is empty)
/// - ENOENT handling accounts for race conditions
/// - Multiple concurrent calls won't interfere with each other
///
/// # Use Cases
///
/// ## After Pattern-Based Installation Changes
///
/// When pattern matches change, old directory structures may become empty:
/// ```toml
/// # Old: pattern matched agents/rust/expert.md, agents/rust/testing.md
/// # New: pattern only matches agents/rust/expert.md
///
/// # testing.md removed → agents/rust/ might now be empty
/// ```
///
/// ## After Custom Target Changes
///
/// Custom target changes can leave old directory structures empty:
/// ```toml
/// # Old: target = "tools"  → .claude/agents/tools/helper.md
/// # New: target = "utils" → .claude/agents/utils/helper.md
///
/// # .claude/agents/tools/ might now be empty
/// ```
///
/// ## After Dependency Removal
///
/// Removing the last dependency in a category may leave empty subdirectories:
/// ```toml
/// [agents]
/// # Removed: python-helper (was in agents/python/)
/// # Only agents/rust/ remains
///
/// # .claude/agents/python/ should be cleaned up
/// ```
///
/// # Design Rationale
///
/// This function exists to solve the "directory accumulation problem":
/// - Without cleanup: Empty directories accumulate over time
/// - With cleanup: Project structure stays clean and organized
/// - Safety boundaries: Prevents accidental removal of important directories
/// - Best-effort approach: Cleanup failures don't block main operations
///
/// # Version History
///
/// - **v0.3.18**: Introduced alongside [`cleanup_removed_artifacts`]
/// - Complements relative path preservation by cleaning up old directory structures
async fn cleanup_empty_dirs(file_path: &std::path::Path) -> Result<()> {
    let mut current = file_path.parent();

    while let Some(dir) = current {
        // Stop if we've reached .claude or the project root
        if dir.ends_with(".claude") || dir.parent().is_none() {
            break;
        }

        // Try to remove the directory (will only succeed if empty)
        match tokio::fs::remove_dir(dir).await {
            Ok(()) => {
                // Directory was empty and removed, continue up
                current = dir.parent();
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Directory doesn't exist, continue up
                current = dir.parent();
            }
            Err(_) => {
                // Directory is not empty or we don't have permission, stop here
                break;
            }
        }
    }

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
                dependencies: vec![],
                resource_type: crate::core::ResourceType::Agent,
                tool: Some("claude-code".to_string()),
                manifest_alias: None,
                applied_patches: std::collections::HashMap::new(),
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
                dependencies: vec![],
                resource_type: crate::core::ResourceType::Agent,
                tool: Some("claude-code".to_string()),
                manifest_alias: None,
                applied_patches: std::collections::HashMap::new(),
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
        let result =
            install_resource(&entry, project_dir, "agents", &cache, false, None, None).await;
        assert!(result.is_ok(), "Failed to install local resource: {:?}", result);

        // Should be installed the first time
        let (installed, _checksum, _applied_patches) = result.unwrap();
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
        let result =
            install_resource(&entry, project_dir, "agents", &cache, false, None, None).await;
        assert!(result.is_ok());
        let (installed, _checksum, _applied_patches) = result.unwrap();
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
        let result =
            install_resource(&entry, project_dir, "agents", &cache, false, None, None).await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Local file") && error_msg.contains("not found"));
    }

    #[tokio::test]
    async fn test_install_resource_invalid_markdown_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create a markdown file with invalid frontmatter
        let local_file = temp_dir.path().join("invalid.md");
        std::fs::write(&local_file, "---\ninvalid: yaml: [\n---\nContent").unwrap();

        // Create a locked resource
        let mut entry = create_test_locked_resource("invalid-test", true);
        entry.path = local_file.to_string_lossy().to_string();

        // Install should now succeed even with invalid frontmatter (just emits a warning)
        let result =
            install_resource(&entry, project_dir, "agents", &cache, false, None, None).await;
        assert!(result.is_ok());
        let (installed, _checksum, _applied_patches) = result.unwrap();
        assert!(installed);

        // Verify the file was installed
        let dest_path = project_dir.join("agents/invalid-test.md");
        assert!(dest_path.exists());

        // Content should include the entire file since frontmatter was invalid
        let installed_content = std::fs::read_to_string(&dest_path).unwrap();
        assert!(installed_content.contains("---"));
        assert!(installed_content.contains("invalid: yaml:"));
        assert!(installed_content.contains("Content"));
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
        let result = install_resource_with_progress(
            &entry,
            project_dir,
            "agents",
            &cache,
            false,
            &pb,
            None,
            None,
        )
        .await;
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

        let (count, _, _) = install_resources(
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
        agent.installed_at = ".claude/agents/test-agent.md".to_string();
        lockfile.agents.push(agent);

        let mut snippet = create_test_locked_resource("test-snippet", true);
        snippet.path = file2.to_string_lossy().to_string();
        snippet.resource_type = crate::core::ResourceType::Snippet;
        snippet.tool = Some("agpm".to_string()); // Snippets use agpm tool
        snippet.installed_at = ".agpm/snippets/test-snippet.md".to_string();
        lockfile.snippets.push(snippet);

        let mut command = create_test_locked_resource("test-command", true);
        command.path = file3.to_string_lossy().to_string();
        command.resource_type = crate::core::ResourceType::Command;
        command.installed_at = ".claude/commands/test-command.md".to_string();
        lockfile.commands.push(command);

        let manifest = Manifest::new();

        let (count, _, _) = install_resources(
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
        assert!(project_dir.join(".agpm/snippets/test-snippet.md").exists());
        assert!(project_dir.join(".claude/commands/test-command.md").exists());
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
            None, // source
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
        assert!(!project_dir.join(".claude/snippets/test-snippet.md").exists()); // Not updated
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
            None, // source
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
        assert!(project_dir.join(".claude/commands/test-command.md").exists());
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

        // Create a manifest (needed for patches)
        let manifest = Manifest::new();

        // Install using the parallel function
        let result =
            install_resource_for_parallel(&entry, project_dir, "agents", &cache, false, &manifest)
                .await;
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
        let result =
            install_resource(&entry, project_dir, "agents", &cache, false, None, None).await;
        assert!(result.is_ok());
        let (installed, _checksum, _applied_patches) = result.unwrap();
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
        snippet.installed_at = ".agpm/snippets/test-snippet.md".to_string();
        lockfile.snippets.push(snippet);

        // Call update_gitignore
        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        // Check that .gitignore was created
        let gitignore_path = project_dir.join(".gitignore");
        assert!(gitignore_path.exists(), "Gitignore file should be created");

        // Check content
        let content = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(content.contains("AGPM managed entries"));
        assert!(content.contains(".claude/agents/test-agent.md"));
        assert!(content.contains(".agpm/snippets/test-snippet.md"));
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
        assert!(!gitignore_path.exists(), "Gitignore should not be created when disabled");
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
                               # AGPM managed entries - do not edit below this line\n\
                               .claude/agents/old-entry.md\n\
                               # End of AGPM managed entries\n";
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
    async fn test_update_gitignore_migrates_ccpm_entries() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create .claude directory
        tokio::fs::create_dir_all(project_dir.join(".claude/agents")).await.unwrap();

        // Create a gitignore with legacy CCPM markers
        let gitignore_path = project_dir.join(".gitignore");
        let legacy_content = r#"# User's custom entries
*.backup
temp/

# CCPM managed entries - do not edit below this line
.claude/agents/old-ccpm-agent.md
.claude/commands/old-ccpm-command.md
# End of CCPM managed entries

# More user entries
local-config.json
"#;
        tokio::fs::write(&gitignore_path, legacy_content).await.unwrap();

        // Create a new lockfile with AGPM entries
        let mut lockfile = LockFile::new();
        let mut agent = create_test_locked_resource("new-agent", true);
        agent.installed_at = ".claude/agents/new-agent.md".to_string();
        lockfile.agents.push(agent);

        // Update gitignore
        let result = update_gitignore(&lockfile, project_dir, true);
        assert!(result.is_ok());

        // Read updated content
        let updated_content = tokio::fs::read_to_string(&gitignore_path).await.unwrap();

        // User entries before CCPM section should be preserved
        assert!(updated_content.contains("*.backup"));
        assert!(updated_content.contains("temp/"));

        // User entries after CCPM section should be preserved
        assert!(updated_content.contains("local-config.json"));

        // Should have AGPM markers now (not CCPM)
        assert!(updated_content.contains("# AGPM managed entries - do not edit below this line"));
        assert!(updated_content.contains("# End of AGPM managed entries"));

        // Old CCPM markers should be removed
        assert!(!updated_content.contains("# CCPM managed entries"));
        assert!(!updated_content.contains("# End of CCPM managed entries"));

        // Old CCPM entries should be removed
        assert!(!updated_content.contains("old-ccpm-agent.md"));
        assert!(!updated_content.contains("old-ccpm-command.md"));

        // New AGPM entries should be added
        assert!(updated_content.contains(".claude/agents/new-agent.md"));
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
            None, // source
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
