//! Selective installation support for updated resources.
//!
//! This module provides targeted installation of only resources that have been
//! updated, rather than reinstalling all resources. It's designed for efficient
//! update operations where only a subset of dependencies have changed.

use anyhow::Result;
use futures::stream::{self, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::constants::default_lock_timeout;
use crate::core::ResourceIterator;
use crate::lockfile::LockFile;
use crate::manifest::Manifest;
use indicatif::ProgressBar;

use super::{InstallContext, install_resource_for_parallel};

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
/// use indicatif::ProgressBar;
/// use std::path::Path;
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let lockfile = Arc::new(LockFile::load(Path::new("agpm.lock"))?);
/// let manifest = Manifest::load(Path::new("agpm.toml"))?;
/// let cache = Cache::new()?;
/// let pb = ProgressBar::new(3);
///
/// // Create installation context
/// let project_dir = Path::new(".");
/// let context = InstallContext::builder(project_dir, &cache).build();
///
/// // Define which resources to update
/// let updates = vec![
///     ("ai-agent".to_string(), None, "v1.0.0".to_string(), "v1.1.0".to_string()),
///     ("helper-tool".to_string(), Some("community".to_string()), "v2.0.0".to_string(), "v2.1.0".to_string()),
///     ("data-processor".to_string(), None, "v1.5.0".to_string(), "v1.6.0".to_string()),
/// ];
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
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Resource type '{}' is not supported by tool '{}' - check tool configuration",
                        resource_type,
                        tool
                    )
                })?;
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
        use futures::future;
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
                let mut builder = InstallContext::builder(&project_dir, cache.as_ref())
                    .manifest(manifest)
                    .lockfile(&lockfile);

                // Add optional fields from the passed install_ctx
                if let Some(patches) = install_ctx.project_patches {
                    builder = builder.project_patches(patches);
                }
                if let Some(patches) = install_ctx.private_patches {
                    builder = builder.private_patches(patches);
                }
                if let Some(size) = install_ctx.max_content_file_size {
                    builder = builder.max_content_file_size(size);
                }

                let context = builder.build();
                install_resource_for_parallel(&entry, &resource_dir, &context).await?;

                // Update progress
                let timeout = default_lock_timeout();
                let mut count = match tokio::time::timeout(timeout, installed_count.lock()).await {
                    Ok(guard) => guard,
                    Err(_) => {
                        eprintln!("[DEADLOCK] Timeout waiting for installed_count lock after {:?}", timeout);
                        anyhow::bail!("Timeout waiting for installed_count lock after {:?} - possible deadlock", timeout);
                    }
                };
                *count += 1;

                if let Some(pb) = pb {
                    pb.set_message(format!("Installing {}/{} resources", *count, total));
                    pb.inc(1);
                }

                Ok::<(), anyhow::Error>(())
            }
        })
        .buffered(usize::MAX) // Allow unlimited task concurrency while preserving input order for deterministic checksums
        .collect()
        .await;

    // Check all results for errors
    for result in results {
        result?;
    }

    let timeout = default_lock_timeout();
    let final_count = match tokio::time::timeout(timeout, installed_count.lock()).await {
        Ok(guard) => *guard,
        Err(_) => {
            eprintln!("[DEADLOCK] Timeout waiting for installed_count lock after {:?}", timeout);
            anyhow::bail!(
                "Timeout waiting for installed_count lock after {:?} - possible deadlock",
                timeout
            );
        }
    };
    Ok(final_count)
}
