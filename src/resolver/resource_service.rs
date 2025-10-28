//! Resource fetching service for dependency resolution.
//!
//! This service handles fetching resource content from local files or Git worktrees
//! and resolving canonical paths for dependencies.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::manifest::ResourceDependency;

use super::types::ResolutionCore;
use super::version_resolver::VersionResolutionService;

/// Service for fetching resource content and resolving paths.
pub struct ResourceFetchingService;

impl ResourceFetchingService {
    /// Create a new resource fetching service.
    pub fn new() -> Self {
        Self
    }

    /// Fetch the content of a resource for metadata extraction.
    ///
    /// This method retrieves the file content from either:
    /// - Local filesystem (for path-only dependencies)
    /// - Git worktree (for Git-backed dependencies with version)
    ///
    /// This method can prepare versions on-demand if they haven't been prepared yet,
    /// which is necessary for transitive dependencies discovered during resolution.
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with manifest and cache
    /// * `dep` - The resource dependency to fetch
    /// * `version_service` - Version service to get/prepare worktree paths
    ///
    /// # Returns
    ///
    /// The file content as a string
    pub async fn fetch_content(
        core: &ResolutionCore,
        dep: &ResourceDependency,
        version_service: &mut VersionResolutionService,
    ) -> Result<String> {
        match dep {
            ResourceDependency::Simple(path) => {
                // Local file - resolve relative to manifest directory
                let manifest_dir = core
                    .manifest
                    .manifest_dir
                    .as_ref()
                    .context("Manifest directory not available for local dependency")?;

                let full_path = manifest_dir.join(path);
                let canonical_path = full_path
                    .canonicalize()
                    .with_context(|| format!("Failed to resolve local path: {}", path))?;

                Self::read_with_cache_retry(&canonical_path).await
            }
            ResourceDependency::Detailed(detailed) => {
                if let Some(source) = &detailed.source {
                    // Git-backed dependency
                    // Use dep.get_version() to handle branch/rev/version precedence
                    let version_key = dep.get_version().unwrap_or("HEAD");
                    let group_key = format!("{}::{}", source, version_key);

                    // Check if version is already prepared, if not prepare it on-demand
                    if version_service.get_prepared_version(&group_key).is_none() {
                        // Prepare this version on-demand (common with transitive dependencies)
                        // Use dep.get_version() to properly handle branch/rev/version precedence
                        version_service
                            .prepare_additional_version(core, source, dep.get_version())
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to prepare version on-demand for source '{}' @ '{}'",
                                    source, version_key
                                )
                            })?;
                    }

                    let prepared = version_service.get_prepared_version(&group_key).unwrap();
                    let worktree_path = &prepared.worktree_path;
                    let file_path = worktree_path.join(&detailed.path);

                    // Don't canonicalize Git-backed files - worktrees may have coherency delays
                    Self::read_with_cache_retry(&file_path).await
                } else {
                    // Local path-only dependency
                    let manifest_dir = core
                        .manifest
                        .manifest_dir
                        .as_ref()
                        .context("Manifest directory not available")?;

                    let full_path = manifest_dir.join(&detailed.path);
                    let canonical_path = full_path.canonicalize().with_context(|| {
                        format!("Failed to resolve local path: {}", detailed.path)
                    })?;

                    Self::read_with_cache_retry(&canonical_path).await
                }
            }
        }
    }

    /// Get the canonical path for a dependency.
    ///
    /// Resolves dependency path to its canonical form on the filesystem.
    /// Can prepare versions on-demand if needed.
    ///
    /// # Arguments
    ///
    /// * `core` - The resolution core with manifest and cache
    /// * `dep` - The resource dependency
    /// * `version_service` - Version service to get/prepare worktree paths
    ///
    /// # Returns
    ///
    /// The canonical absolute path to the resource
    pub async fn get_canonical_path(
        core: &ResolutionCore,
        dep: &ResourceDependency,
        version_service: &mut VersionResolutionService,
    ) -> Result<PathBuf> {
        match dep {
            ResourceDependency::Simple(path) => {
                let manifest_dir = core
                    .manifest
                    .manifest_dir
                    .as_ref()
                    .context("Manifest directory not available")?;

                let full_path = manifest_dir.join(path);
                full_path
                    .canonicalize()
                    .with_context(|| format!("Failed to canonicalize path: {}", path))
            }
            ResourceDependency::Detailed(detailed) => {
                if let Some(source) = &detailed.source {
                    // Git-backed dependency
                    // Use dep.get_version() to handle branch/rev/version precedence
                    let version_key = dep.get_version().unwrap_or("HEAD");
                    let group_key = format!("{}::{}", source, version_key);

                    // Check if version is already prepared, if not prepare it on-demand
                    if version_service.get_prepared_version(&group_key).is_none() {
                        version_service
                            .prepare_additional_version(core, source, detailed.version.as_deref())
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to prepare version on-demand for source '{}' @ '{}'",
                                    source, version_key
                                )
                            })?;
                    }

                    let prepared = version_service.get_prepared_version(&group_key).unwrap();

                    let worktree_path = &prepared.worktree_path;
                    let file_path = worktree_path.join(&detailed.path);

                    // Return the path without canonicalizing - Git worktrees may have coherency delays
                    Ok(file_path)
                } else {
                    // Local path-only dependency
                    let manifest_dir = core
                        .manifest
                        .manifest_dir
                        .as_ref()
                        .context("Manifest directory not available")?;

                    let full_path = manifest_dir.join(&detailed.path);
                    full_path
                        .canonicalize()
                        .with_context(|| format!("Failed to canonicalize path: {}", detailed.path))
                }
            }
        }
    }

    /// Read file with retry logic for cache coherency issues.
    ///
    /// Git worktrees can have filesystem coherency delays after creation.
    /// This method retries up to 10 times with 100ms delays between attempts.
    async fn read_with_cache_retry(path: &Path) -> Result<String> {
        use tokio::time::{Duration, sleep};

        const MAX_ATTEMPTS: u32 = 10;
        const RETRY_DELAY_MS: u64 = 100;

        for attempt in 0..MAX_ATTEMPTS {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => return Ok(content),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound && attempt < MAX_ATTEMPTS - 1 =>
                {
                    // File not found, but we have retries left
                    tracing::debug!(
                        "File not found at {}, retrying ({}/{})",
                        path.display(),
                        attempt + 1,
                        MAX_ATTEMPTS
                    );
                    sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    continue;
                }
                Err(e) => {
                    // Other error or final attempt
                    return Err(e)
                        .with_context(|| format!("Failed to read file: {}", path.display()));
                }
            }
        }

        // This should never be reached, but provide a fallback
        anyhow::bail!("Failed to read file after {} attempts: {}", MAX_ATTEMPTS, path.display())
    }
}

impl Default for ResourceFetchingService {
    fn default() -> Self {
        Self::new()
    }
}
