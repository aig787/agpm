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

mod cleanup;
mod context;
mod gitignore;

#[cfg(test)]
mod tests;

pub use cleanup::cleanup_removed_artifacts;
pub use context::InstallContext;
pub use gitignore::{add_path_to_gitignore, update_gitignore};

use context::read_with_cache_retry;

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
    (crate::lockfile::ResourceId, bool, String, crate::manifest::patches::AppliedPatches),
    (crate::lockfile::ResourceId, anyhow::Error),
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
use crate::utils::progress::ProgressBar;
use hex;
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
/// Returns `Ok((installed, checksum, applied_patches))` where:
/// - `installed` is `true` if the resource was actually installed (new or updated),
///   `false` if the resource already existed and was unchanged
/// - `checksum` is the SHA-256 hash of the installed file content
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
/// use agpm_cli::lockfile::LockedResource;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::core::ResourceType;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let entry = LockedResource::new(
///     "example-agent".to_string(),
///     Some("community".to_string()),
///     Some("https://github.com/example/repo.git".to_string()),
///     "agents/example.md".to_string(),
///     Some("v1.0.0".to_string()),
///     Some("abc123".to_string()),
///     "sha256:...".to_string(),
///     ".claude/agents/example.md".to_string(),
///     vec![],
///     ResourceType::Agent,
///     Some("claude-code".to_string()),
///     None,
///     std::collections::HashMap::new(),
///     None,
///     serde_json::Value::Object(serde_json::Map::new()),
/// );
///
/// let context = InstallContext::new(Path::new("."), &cache, false, false, None, None, None, None, None, None);
/// let (installed, checksum, _patches) = install_resource(&entry, "agents", &context).await?;
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
) -> Result<(bool, String, crate::manifest::patches::AppliedPatches)> {
    // Determine destination path
    let dest_path = if entry.installed_at.is_empty() {
        context.project_dir.join(resource_dir).join(format!("{}.md", entry.name))
    } else {
        context.project_dir.join(&entry.installed_at)
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

            let mut cache_dir = context
                .cache
                .get_or_create_worktree_for_sha(source_name, url, sha, Some(&entry.name))
                .await?;

            if context.force_refresh {
                let _ = context.cache.cleanup_worktree(&cache_dir).await;
                cache_dir = context
                    .cache
                    .get_or_create_worktree_for_sha(source_name, url, sha, Some(&entry.name))
                    .await?;
            }

            cache_dir
        };

        // Read the content from the source (with cache coherency retry)
        let source_path = cache_dir.join(&entry.path);
        let file_content = read_with_cache_retry(&source_path).await?;

        // Validate markdown - silently accepts invalid frontmatter (warnings handled by MetadataExtractor)
        MarkdownFile::parse(&file_content)?;

        file_content
    } else {
        // Local resource - copy directly from project directory or absolute path
        let source_path = {
            let candidate = Path::new(&entry.path);
            if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                context.project_dir.join(candidate)
            }
        };

        if !source_path.exists() {
            return Err(anyhow::anyhow!(
                "Local file '{}' not found. Expected at: {}",
                entry.path,
                source_path.display()
            ));
        }

        let local_content = tokio::fs::read_to_string(&source_path)
            .await
            .with_context(|| format!("Failed to read resource file: {}", source_path.display()))?;

        // Validate markdown - silently accepts invalid frontmatter (warnings handled by MetadataExtractor)
        MarkdownFile::parse(&local_content)?;

        local_content
    };

    // Apply patches if provided (before templating)
    let empty_patches = std::collections::HashMap::new();
    let (patched_content, applied_patches) =
        if context.project_patches.is_some() || context.private_patches.is_some() {
            use crate::manifest::patches::apply_patches_to_content_with_origin;

            // Look up patches for this specific resource
            let resource_type = entry.resource_type.to_plural();
            let lookup_name = entry.manifest_alias.as_ref().unwrap_or(&entry.name);

            let project_patch_data = context
                .project_patches
                .and_then(|patches| patches.get(resource_type, lookup_name))
                .unwrap_or(&empty_patches);

            let private_patch_data = context
                .private_patches
                .and_then(|patches| patches.get(resource_type, lookup_name))
                .unwrap_or(&empty_patches);

            let file_path = entry.installed_at.as_str();
            apply_patches_to_content_with_origin(
                &new_content,
                file_path,
                project_patch_data,
                private_patch_data,
            )
            .with_context(|| format!("Failed to apply patches to resource {}", entry.name))?
        } else {
            (new_content.clone(), crate::manifest::patches::AppliedPatches::default())
        };

    // Apply templating to markdown files if enabled in frontmatter (after patching)
    // Track whether templating was applied and the context digest for cache invalidation
    let (final_content, template_context_digest) = if entry.path.ends_with(".md") {
        // Check for opt-in in frontmatter
        let templating_enabled = if let Ok(md_file) = MarkdownFile::parse(&patched_content) {
            md_file
                .metadata
                .as_ref()
                .and_then(|m| m.extra.get("agpm"))
                .and_then(|agpm| agpm.get("templating"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        } else {
            false
        };

        if !templating_enabled {
            tracing::debug!("Templating not enabled via frontmatter for {}", entry.name);
            (patched_content, None)
        } else if patched_content.contains("{{")
            || patched_content.contains("{%")
            || patched_content.contains("{#")
        {
            // Check if content contains template syntax
            tracing::debug!("Template syntax detected in {}, rendering...", entry.name);

            // Build template context if we have a shared builder
            if let Some(template_context_builder) = &context.template_context_builder {
                use crate::templating::TemplateRenderer;

                // Determine resource type from entry
                let resource_type = entry.resource_type;

                // Compute context digest for cache invalidation
                // This ensures that changes to dependency versions invalidate the cache
                let context_digest =
                    template_context_builder.compute_context_digest().with_context(|| {
                        format!("Failed to compute template context digest for {}", entry.name)
                    })?;

                let resource_id = crate::lockfile::ResourceId::from_serialized(
                    entry.name.clone(),
                    entry.source.clone(),
                    entry.tool.clone(),
                    resource_type,
                    entry.template_vars.clone(),
                );
                let template_context = template_context_builder
                    .build_context(&resource_id)
                    .await
                    .map_err(|e| {
                    // Preserve the full error chain for debugging
                    anyhow::anyhow!("Failed to build template context for {}: {:#}", entry.name, e)
                })?;

                // Show verbose output before rendering
                if context.verbose {
                    let num_resources = template_context
                        .get("resources")
                        .and_then(|v| v.as_object())
                        .map(|o| o.len())
                        .unwrap_or(0);
                    let num_dependencies = template_context
                        .get("dependencies")
                        .and_then(|v| v.as_object())
                        .map(|o| o.len())
                        .unwrap_or(0);

                    tracing::info!("📝 Rendering template: {}", entry.path);
                    tracing::info!(
                        "   Context: {} resources, {} dependencies",
                        num_resources,
                        num_dependencies
                    );
                    tracing::debug!("   Context digest: {}", context_digest);
                }

                // Create renderer and render template
                let mut renderer = TemplateRenderer::new(
                    true,
                    context.project_dir.to_path_buf(),
                    context.max_content_file_size,
                )
                .with_context(|| "Failed to create template renderer")?;

                let rendered_content = renderer
                    .render_template(&patched_content, &template_context)
                    .map_err(|e| {
                        // Log detailed error with full error chain
                        tracing::error!(
                            "Template rendering failed for resource '{}' ({}): {}",
                            entry.name,
                            entry.path,
                            e
                        );
                        // Log error chain if available
                        for (i, cause) in e.chain().skip(1).enumerate() {
                            tracing::error!("  Caused by [{}]: {}", i + 1, cause);
                        }
                        e
                    })
                    .with_context(|| {
                        format!(
                            "Failed to render template for '{}' (source: {}, path: {})",
                            entry.name,
                            entry.source.as_deref().unwrap_or("local"),
                            entry.path
                        )
                    })?;

                tracing::debug!("Successfully rendered template for {}", entry.name);

                // Show verbose output after rendering
                if context.verbose {
                    let size_bytes = rendered_content.len();
                    let size_str = if size_bytes < 1024 {
                        format!("{} B", size_bytes)
                    } else if size_bytes < 1024 * 1024 {
                        format!("{:.1} KB", size_bytes as f64 / 1024.0)
                    } else {
                        format!("{:.1} MB", size_bytes as f64 / (1024.0 * 1024.0))
                    };
                    tracing::info!("   Output: {} ({})", dest_path.display(), size_str);
                    tracing::info!("✅ Template rendered successfully");
                }

                (rendered_content, Some(context_digest))
            } else {
                tracing::warn!(
                    "Template syntax found in {} but manifest/lockfile not available, skipping templating",
                    entry.name
                );
                (patched_content, None)
            }
        } else {
            tracing::debug!("No template syntax in {}, skipping templating", entry.name);
            (patched_content, None)
        }
    } else {
        tracing::debug!("Not a markdown file: {}", entry.path);
        (patched_content, None)
    };

    // Calculate checksum of final content (after patching and templating)
    // Include template context digest to ensure cache invalidation when dependencies change
    let new_checksum = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(final_content.as_bytes());

        // Include context digest if templating was applied
        // This ensures that changes to dependency versions trigger re-rendering
        if let Some(ref digest) = template_context_digest {
            hasher.update(digest.as_bytes());
        }

        let hash = hasher.finalize();
        format!("sha256:{}", hex::encode(hash))
    };

    // Check if content has changed by comparing checksums
    let content_changed = existing_checksum.as_ref() != Some(&new_checksum);

    // Check if we should actually write the file to disk
    let should_install = entry.install.unwrap_or(true);

    let actually_installed = if should_install && content_changed {
        // Only write if install=true and content is different or file doesn't exist
        if let Some(parent) = dest_path.parent() {
            ensure_dir(parent)?;
        }

        // Add to .gitignore BEFORE writing file to prevent accidental commits
        if let Some(lock) = context.gitignore_lock {
            // Calculate relative path for gitignore
            let relative_path = dest_path
                .strip_prefix(context.project_dir)
                .unwrap_or(&dest_path)
                .to_string_lossy()
                .to_string();

            add_path_to_gitignore(context.project_dir, &relative_path, lock)
                .await
                .with_context(|| format!("Failed to add {} to .gitignore", relative_path))?;
        }

        atomic_write(&dest_path, final_content.as_bytes())
            .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;

        true
    } else if !should_install {
        // install=false: content-only dependency, don't write file
        tracing::debug!(
            "Skipping file write for content-only dependency: {} (install=false)",
            entry.name
        );
        false
    } else {
        // install=true but content unchanged
        false
    };

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
/// use agpm_cli::installer::{install_resource_with_progress, InstallContext};
/// use agpm_cli::lockfile::LockedResource;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::utils::progress::ProgressBar;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let pb = ProgressBar::new(1);
/// let entry = LockedResource::new(
///     "example-agent".to_string(),
///     Some("community".to_string()),
///     Some("https://github.com/example/repo.git".to_string()),
///     "agents/example.md".to_string(),
///     Some("v1.0.0".to_string()),
///     Some("abc123".to_string()),
///     "sha256:...".to_string(),
///     ".claude/agents/example.md".to_string(),
///     vec![],
///     ResourceType::Agent,
///     Some("claude-code".to_string()),
///     None,
///     std::collections::HashMap::new(),
///     None,
///     serde_json::Value::Object(serde_json::Map::new()),
/// );
///
/// let context = InstallContext::new(Path::new("."), &cache, false, false, None, None, None, None, None, None);
/// let (installed, checksum, _patches) = install_resource_with_progress(
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
) -> Result<(bool, String, crate::manifest::patches::AppliedPatches)> {
    pb.set_message(format!("Installing {}", entry.name));
    install_resource(entry, resource_dir, context).await
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
/// use agpm_cli::installer::{install_resources_parallel, InstallContext};
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::utils::progress::ProgressBar;
/// use std::path::Path;
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = Arc::new(LockFile::load(Path::new("agpm.lock"))?);
/// let manifest = Manifest::load(Path::new("agpm.toml"))?;
/// let cache = Cache::new()?;
///
/// // Count total resources for progress bar
/// let total = lockfile.agents.len() + lockfile.snippets.len()
///     + lockfile.commands.len() + lockfile.scripts.len()
///     + lockfile.hooks.len() + lockfile.mcp_servers.len();
/// let pb = ProgressBar::new(total as u64);
///
/// let context = InstallContext::new(Path::new("."), &cache, false, false, Some(&manifest), Some(&lockfile), None, None, None, None);
/// let count = install_resources_parallel(
///     &lockfile,
///     &manifest,
///     &context,
///     &pb,
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
    lockfile: &Arc<LockFile>,
    manifest: &Manifest,
    install_ctx: &InstallContext<'_>,
    pb: &ProgressBar,
    max_concurrency: Option<usize>,
) -> Result<usize> {
    let project_dir = install_ctx.project_dir;
    let cache = install_ctx.cache;
    let force_refresh = install_ctx.force_refresh;
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
        tracing::debug!(
            "Checking entry '{}' (type: {:?}): source={:?}, url={:?}, sha={:?}",
            entry.name,
            entry.resource_type,
            entry.source,
            entry.url.as_deref().map(|u| &u[..60.min(u.len())]),
            entry.resolved_commit.as_deref().map(|s| &s[..8.min(s.len())])
        );

        if let Some(source_name) = &entry.source
            && let Some(url) = &entry.url
        {
            // Only pre-warm if we have a valid SHA
            if let Some(sha) = entry.resolved_commit.as_ref().filter(|commit| {
                commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit())
            }) {
                tracing::info!(
                    "Adding worktree to pre-warm set: source={}, sha={}",
                    source_name,
                    &sha[..8]
                );
                unique_worktrees.insert((source_name.clone(), url.clone(), sha.clone()));
            } else {
                tracing::warn!(
                    "Skipping worktree pre-warm for '{}': invalid or missing SHA",
                    entry.name
                );
            }
        }
    }

    tracing::info!("Pre-warming {} unique worktrees", unique_worktrees.len());

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
                        .map_err(|e| {
                            tracing::error!(
                                "Failed to create worktree for {}/{}: {}",
                                source,
                                &sha[..8.min(sha.len())],
                                e
                            );
                            e
                        })
                }
            })
            .collect();

        // Execute all worktree creations in parallel - fail fast on first error
        future::try_join_all(worktree_futures).await.context("Failed to pre-warm worktrees")?;
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
                let context = InstallContext::new(
                    &project_dir,
                    cache.as_ref(),
                    force_refresh,
                    false, // verbose - will be threaded through from CLI
                    Some(manifest),
                    Some(lockfile),
                    install_ctx.project_patches,
                    install_ctx.private_patches,
                    install_ctx.gitignore_lock,
                    install_ctx.max_content_file_size,
                );
                let res = install_resource_for_parallel(&entry, &resource_dir, &context).await;

                match res {
                    Ok((actually_installed, checksum, applied_patches)) => {
                        if actually_installed {
                            let mut count = installed_count.lock().await;
                            *count += 1;
                        }
                        let count = *installed_count.lock().await;
                        pb.set_message(format!("Installing {count}/{total} resources"));
                        pb.inc(1);
                        Ok((entry.id(), actually_installed, checksum, applied_patches))
                    }
                    Err(err) => Err((entry.id(), err)),
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok((_id, _installed, _checksum, _applied_patches)) => {
                // Old function doesn't return checksums or patches
            }
            Err((id, error)) => {
                errors.push((id, error));
            }
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|(id, error)| format!("  {}: {error}", id.name())).collect();
        return Err(anyhow::anyhow!(
            "Failed to install {} resources:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    // Clear render cache and log statistics
    if let Some(builder) = &install_ctx.template_context_builder {
        if let Some((hits, misses, hit_rate)) = builder.render_cache_stats() {
            let total = hits + misses;
            if total > 0 {
                tracing::info!(
                    "Render cache: {} hits, {} misses ({:.1}% hit rate)",
                    hits,
                    misses,
                    hit_rate
                );
            }
        }
        builder.clear_render_cache();
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
    resource_dir: &str,
    context: &InstallContext<'_>,
) -> Result<(bool, String, crate::manifest::patches::AppliedPatches)> {
    install_resource(entry, resource_dir, context).await
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
    lockfile: &Arc<LockFile>,
    manifest: &Manifest,
    install_ctx: &InstallContext<'_>,
    max_concurrency: Option<usize>,
    progress_sender: Option<mpsc::UnboundedSender<InstallProgress>>,
) -> Result<usize> {
    let project_dir = install_ctx.project_dir;
    let cache = install_ctx.cache;
    let force_refresh = install_ctx.force_refresh;
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
            let lockfile = Arc::clone(lockfile);

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

                let context = InstallContext::new(
                    &project_dir,
                    cache.as_ref(),
                    force_refresh,
                    false, // verbose - will be threaded through from CLI
                    Some(manifest),
                    Some(&lockfile),
                    install_ctx.project_patches,
                    install_ctx.private_patches,
                    install_ctx.gitignore_lock,
                    install_ctx.max_content_file_size,
                );
                let res = install_resource_for_parallel(&entry, &resource_dir, &context).await;

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
                        Ok((entry.id(), installed, checksum, applied_patches))
                    }
                    Err(err) => Err((entry.id(), err)),
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok((_id, _installed, _checksum, _applied_patches)) => {
                // Old function doesn't return checksums or patches
            }
            Err((id, error)) => {
                errors.push((id, error));
            }
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|(id, error)| format!("  {}: {error}", id.name())).collect();
        return Err(anyhow::anyhow!(
            "Failed to install {} resources:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    // Clear render cache and log statistics
    if let Some(builder) = &install_ctx.template_context_builder {
        if let Some((hits, misses, hit_rate)) = builder.render_cache_stats() {
            let total = hits + misses;
            if total > 0 {
                tracing::info!(
                    "Render cache: {} hits, {} misses ({:.1}% hit rate)",
                    hits,
                    misses,
                    hit_rate
                );
            }
        }
        builder.clear_render_cache();
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
/// # let lockfile = Arc::new(LockFile::default());
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
///     false, // verbose
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
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let lockfile = Arc::new(LockFile::default());
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
///     false, // verbose
/// ).await?;
///
/// println!("Updated {} resources", count);
/// # Ok(())
/// # }
/// ```
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
) -> Result<(
    usize,
    Vec<(crate::lockfile::ResourceId, String)>,
    Vec<(crate::lockfile::ResourceId, crate::manifest::patches::AppliedPatches)>,
)> {
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

    // Create gitignore lock for thread-safe gitignore updates
    let gitignore_lock = Arc::new(Mutex::new(()));

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
            let gitignore_lock = Arc::clone(&gitignore_lock);

            async move {
                // Update progress message for current resource
                if let Some(ref pm) = progress {
                    pm.update_current_message(&format!("Installing {}", entry.name));
                }

                let install_context = InstallContext::new(
                    &project_dir,
                    &cache,
                    force_refresh,
                    verbose,
                    Some(manifest),
                    Some(lockfile),
                    Some(&manifest.project_patches),
                    Some(&manifest.private_patches),
                    Some(&gitignore_lock),
                    None, // max_content_file_size - not available in install_resources context
                );

                let res =
                    install_resource_for_parallel(&entry, &resource_dir, &install_context).await;

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
                        Ok((entry.id(), installed, checksum, applied_patches))
                    }
                    Err(err) => Err((entry.id(), err)),
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
            Ok((id, _installed, checksum, applied_patches)) => {
                checksums.push((id.clone(), checksum));
                applied_patches_list.push((id, applied_patches));
            }
            Err((id, error)) => {
                errors.push((id, error));
            }
        }
    }

    if !errors.is_empty() {
        // Complete phase with error message
        if let Some(ref pm) = progress {
            pm.complete_phase(Some(&format!("Failed to install {} resources", errors.len())));
        }

        let error_msgs: Vec<String> =
            errors.into_iter().map(|(id, error)| format!("  {}: {error}", id.name())).collect();
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
/// use agpm_cli::installer::{install_resources_with_dynamic_progress, InstallContext};
/// use agpm_cli::utils::progress::ProgressBar;
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use std::sync::Arc;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = Arc::new(LockFile::load(Path::new("agpm.lock"))?);
/// let manifest = Manifest::load(Path::new("agpm.toml"))?;
/// let cache = Cache::new()?;
///
/// // Create dynamic progress manager
/// let progress_bar = Arc::new(ProgressBar::new(100));
///
/// let context = InstallContext::new(Path::new("."), &cache, false, false, Some(&manifest), Some(&lockfile), None, None, None, None);
/// let count = install_resources_with_dynamic_progress(
///     &lockfile,
///     &manifest,
///     &context,
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
    lockfile: &Arc<LockFile>,
    manifest: &Manifest,
    install_ctx: &InstallContext<'_>,
    max_concurrency: Option<usize>,
    progress_bar: Option<Arc<crate::utils::progress::ProgressBar>>,
) -> Result<usize> {
    let project_dir = install_ctx.project_dir;
    let cache = install_ctx.cache;
    let force_refresh = install_ctx.force_refresh;
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
            let lockfile = Arc::clone(lockfile);

            async move {
                // Update progress if available
                if let Some(ref progress) = progress_bar_ref {
                    progress.set_message(format!("Installing {}", entry.name));
                }

                let context = InstallContext::new(
                    &project_dir,
                    cache.as_ref(),
                    force_refresh,
                    false, // verbose - will be threaded through from CLI
                    Some(manifest),
                    Some(&lockfile),
                    install_ctx.project_patches,
                    install_ctx.private_patches,
                    install_ctx.gitignore_lock,
                    install_ctx.max_content_file_size,
                );
                let res = install_resource_for_parallel(&entry, &resource_dir, &context).await;

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
                        Ok((entry.id(), installed, checksum, applied_patches))
                    }
                    Err(err) => Err((entry.id(), err)),
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
/// use agpm_cli::installer::{install_updated_resources, InstallContext};
/// use agpm_cli::lockfile::LockFile;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
/// use agpm_cli::utils::progress::ProgressBar;
/// use std::path::Path;
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = Arc::new(LockFile::load(Path::new("agpm.lock"))?);
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
/// let context = InstallContext::new(Path::new("."), &cache, false, false, Some(&manifest), Some(&lockfile), None, None, None, None);
/// let count = install_updated_resources(
///     &updates,
///     &lockfile,
///     &manifest,
///     &context,
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
    lockfile: &Arc<LockFile>,
    manifest: &Manifest,
    install_ctx: &InstallContext<'_>,
    pb: Option<&ProgressBar>,
    _quiet: bool,
) -> Result<usize> {
    let project_dir = install_ctx.project_dir;
    let cache = install_ctx.cache;
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
            let lockfile = Arc::clone(lockfile);

            async move {
                // Install the resource
                let context = InstallContext::new(
                    &project_dir,
                    cache.as_ref(),
                    false,
                    false, // verbose - will be threaded through from CLI
                    Some(manifest),
                    Some(&lockfile),
                    install_ctx.project_patches,
                    install_ctx.private_patches,
                    install_ctx.gitignore_lock,
                    install_ctx.max_content_file_size,
                );
                install_resource_for_parallel(&entry, &resource_dir, &context).await?;

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
