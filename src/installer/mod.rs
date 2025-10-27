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
mod selective;

#[cfg(test)]
mod tests;

pub use cleanup::cleanup_removed_artifacts;
pub use context::InstallContext;
pub use gitignore::{add_path_to_gitignore, update_gitignore};
pub use selective::*;

use context::read_with_cache_retry;

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

use futures::{
    future,
    stream::{self, StreamExt},
};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

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
/// let context = InstallContext::new(Path::new("."), &cache, false, false, None, None, None, None, None, None, None);
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

    // Check if file already exists and compare checksums
    let existing_checksum = if dest_path.exists() {
        // Use blocking task for checksum calculation to avoid blocking the async runtime
        let path = dest_path.clone();
        tokio::task::spawn_blocking(move || LockFile::compute_checksum(&path)).await??.into()
    } else {
        None
    };

    // Early-exit optimization: Skip processing if nothing changed
    // This dramatically speeds up subsequent installations when resources are unchanged
    if !context.force_refresh {
        if let Some(old_lockfile) = context.old_lockfile {
            if let Some(old_entry) = old_lockfile.find_resource(&entry.name, &entry.resource_type) {
                // Check if all inputs that affect the final content are unchanged
                let resolved_commit_unchanged = old_entry.resolved_commit == entry.resolved_commit;
                let variant_inputs_unchanged = old_entry.variant_inputs == entry.variant_inputs;
                let patches_unchanged = old_entry.applied_patches == entry.applied_patches;

                let all_inputs_unchanged =
                    resolved_commit_unchanged && variant_inputs_unchanged && patches_unchanged;

                if all_inputs_unchanged && dest_path.exists() {
                    // File exists and all inputs match - verify checksum matches
                    if existing_checksum.as_ref() == Some(&old_entry.checksum) {
                        tracing::debug!(
                            "⏭️  Skipping unchanged resource: {} (checksum matches)",
                            entry.name
                        );
                        return Ok((
                            false, // not installed (already up to date)
                            old_entry.checksum.clone(),
                            old_entry.context_checksum.clone(),
                            crate::manifest::patches::AppliedPatches::from_lockfile_patches(
                                &old_entry.applied_patches,
                            ),
                        ));
                    } else {
                        tracing::debug!(
                            "Checksum mismatch for {}: existing={:?}, expected={}",
                            entry.name,
                            existing_checksum,
                            old_entry.checksum
                        );
                    }
                }
            }
        }
    }

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
    let empty_patches = std::collections::BTreeMap::new();
    let (patched_content, applied_patches) = if context.project_patches.is_some()
        || context.private_patches.is_some()
    {
        use crate::manifest::patches::apply_patches_to_content_with_origin;

        // Look up patches for this specific resource
        let resource_type = entry.resource_type.to_plural();
        let lookup_name = entry.lookup_name();

        tracing::debug!(
            "Installer patch lookup: resource_type={}, lookup_name={}, name={}, manifest_alias={:?}",
            resource_type,
            lookup_name,
            entry.name,
            entry.manifest_alias
        );

        let project_patch_data = context
            .project_patches
            .and_then(|patches| patches.get(resource_type, lookup_name))
            .unwrap_or(&empty_patches);

        tracing::debug!("Found {} project patches for {}", project_patch_data.len(), lookup_name);

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

    // Apply templating to markdown files (after patching)
    // Strategy: Always render first, then check agpm.templating flag in rendered frontmatter
    // Track whether templating was applied and capture context checksum from rendering
    let (final_content, templating_was_applied, captured_checksum) = if entry.path.ends_with(".md")
    {
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

            let resource_id = crate::lockfile::ResourceId::new(
                entry.name.clone(),
                entry.source.clone(),
                entry.tool.clone(),
                resource_type,
                entry.variant_inputs.hash().to_string(),
            );
            let (template_context, captured_context_checksum) = template_context_builder
                .build_context(&resource_id, entry.variant_inputs.json())
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

            // Debug: Check if agpm.deps.snippets exists in context
            if let Some(agpm) = template_context.get("agpm") {
                if let Some(deps) = agpm.get("deps") {
                    if let Some(snippets) = deps.get("snippets") {
                        tracing::debug!(
                            "Template context has agpm.deps.snippets with {} entries",
                            snippets.as_object().map(|o| o.len()).unwrap_or(0)
                        );
                        if let Some(best_practices) = snippets.get("best_practices") {
                            tracing::debug!(
                                "✓ Found best_practices in context: has content field = {}",
                                best_practices.get("content").is_some()
                            );
                        } else {
                            tracing::warn!("✗ best_practices NOT found in agpm.deps.snippets");
                        }
                    } else {
                        tracing::warn!("✗ agpm.deps.snippets NOT found in context");
                    }
                } else {
                    tracing::warn!("✗ agpm.deps NOT found in context");
                }
            } else {
                tracing::warn!("✗ agpm NOT found in context");
            }

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

            // Parse the RENDERED content to check agpm.templating flag
            let use_rendered = if let Ok(md_file) = MarkdownFile::parse(&rendered_content) {
                md_file
                    .metadata
                    .as_ref()
                    .and_then(|m| m.extra.get("agpm"))
                    .and_then(|agpm| agpm.get("templating"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            } else {
                tracing::warn!(
                    "Failed to parse rendered frontmatter for {}, using original content",
                    entry.name
                );
                false
            };

            if use_rendered {
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
                (rendered_content, true, captured_context_checksum)
            } else {
                tracing::debug!(
                    "agpm.templating not enabled for {}, using original content",
                    entry.name
                );
                (patched_content, false, None)
            }
        } else {
            tracing::debug!("No template context builder available for {}", entry.name);
            (patched_content, false, None)
        }
    } else {
        tracing::debug!("Not a markdown file: {}", entry.path);
        (patched_content, false, None)
    };

    // Calculate file checksum of final content (after patching and templating)
    let file_checksum = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(final_content.as_bytes());
        let hash = hasher.finalize();
        format!("sha256:{}", hex::encode(hash))
    };

    // Use captured context checksum from rendering (avoid rebuilding context)
    let context_checksum = if templating_was_applied {
        captured_checksum
    } else {
        None
    };

    // Reinstall decision: trust file checksum, ignore context checksum
    let content_changed = existing_checksum.as_ref() != Some(&file_checksum);

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
/// use agpm_cli::utils::progress::ProgressBar;
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
/// let context = InstallContext::new(Path::new("."), &cache, false, false, None, None, None, None, None, None, None);
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
/// let (count, _checksums, _old_checksums, _patches) = install_resources(
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
/// let (count, _checksums, _old_checksums, _patches) = install_resources(
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
    old_lockfile: Option<&LockFile>,
) -> Result<(
    usize,
    Vec<(crate::lockfile::ResourceId, String)>,
    Vec<(crate::lockfile::ResourceId, Option<String>)>,
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
        return Ok((0, Vec::new(), Vec::new(), Vec::new()));
    }

    // Sort entries for deterministic processing order
    // This ensures context checksums are deterministic even when lockfile isn't normalized yet
    let mut all_entries = all_entries;
    all_entries.sort_by(|(a, _), (b, _)| {
        a.resource_type.cmp(&b.resource_type).then_with(|| a.name.cmp(&b.name))
    });

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
                    old_lockfile, // Pass old_lockfile for early-exit optimization
                    Some(&manifest.project_patches),
                    Some(&manifest.private_patches),
                    Some(&gitignore_lock),
                    None, // max_content_file_size - not available in install_resources context
                );

                let res =
                    install_resource_for_parallel(&entry, &resource_dir, &install_context).await;

                // Update progress on success - but only count if actually installed
                if let Ok((
                    actually_installed,
                    _file_checksum,
                    _context_checksum,
                    _applied_patches,
                )) = &res
                {
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
                    Ok((installed, file_checksum, context_checksum, applied_patches)) => Ok((
                        entry.id(),
                        installed,
                        file_checksum,
                        context_checksum,
                        applied_patches,
                    )),
                    Err(err) => Err((entry.id(), err)),
                }
            }
        })
        .buffered(concurrency) // Use buffered instead of buffer_unordered to preserve input order for deterministic checksums
        .collect()
        .await;

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

    Ok((final_count, checksums, context_checksums, applied_patches_list))
}
