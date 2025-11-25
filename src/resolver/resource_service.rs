//! Resource fetching service for dependency resolution.
//!
//! This service handles fetching resource content from local files or Git worktrees
//! and resolving canonical paths for dependencies.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::core::file_error::{FileOperation, FileResultExt};
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

    /// Helper function to canonicalize a path with proper error context.
    ///
    /// This function provides consistent error handling for path canonicalization
    /// operations throughout the resource service.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to canonicalize
    /// * `operation_desc` - Description of the operation being performed
    /// * `caller` - The function name calling this helper
    ///
    /// # Returns
    ///
    /// The canonical path with structured error context on failure
    pub fn canonicalize_with_context(
        path: &Path,
        operation_desc: String,
        caller: &str,
    ) -> Result<PathBuf> {
        path.canonicalize().map_err(|e| {
            let file_error = crate::core::file_error::FileOperationError::new(
                crate::core::file_error::FileOperationContext::new(
                    crate::core::file_error::FileOperation::Canonicalize,
                    path,
                    operation_desc,
                    caller,
                ),
                e,
            );
            anyhow::Error::from(file_error)
        })
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
        version_service: &VersionResolutionService,
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
                let canonical_path = Self::canonicalize_with_context(
                    &full_path,
                    format!("resolving local dependency path: {}", path),
                    "resource_service",
                )?;

                tokio::fs::read_to_string(&canonical_path)
                    .await
                    .with_file_context(
                        FileOperation::Read,
                        &canonical_path,
                        "reading local dependency content",
                        "resource_service",
                    )
                    .map_err(Into::into)
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

                    // Safe: prepare_additional_version was just called (if needed) which guarantees
                    // the group_key exists in prepared_versions. If preparation fails, the error
                    // propagates before reaching this unwrap.
                    let prepared = version_service.get_prepared_version(&group_key).unwrap();
                    let worktree_path = &prepared.worktree_path;
                    let file_path = worktree_path.join(&detailed.path);

                    // Use retry for Git worktree files - they can have brief visibility
                    // delays after creation, especially under high parallel I/O load
                    crate::utils::fs::read_text_file_with_retry(&file_path).await
                } else {
                    // Local path-only dependency
                    let manifest_dir = core
                        .manifest
                        .manifest_dir
                        .as_ref()
                        .context("Manifest directory not available")?;

                    let full_path = manifest_dir.join(&detailed.path);
                    let canonical_path = Self::canonicalize_with_context(
                        &full_path,
                        format!("resolving local dependency path: {}", detailed.path),
                        "resource_service::fetch_content",
                    )?;

                    tokio::fs::read_to_string(&canonical_path)
                        .await
                        .with_file_context(
                            FileOperation::Read,
                            &canonical_path,
                            "reading local dependency content",
                            "resource_service",
                        )
                        .map_err(Into::into)
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
        version_service: &VersionResolutionService,
    ) -> Result<PathBuf> {
        match dep {
            ResourceDependency::Simple(path) => {
                let manifest_dir = core
                    .manifest
                    .manifest_dir
                    .as_ref()
                    .context("Manifest directory not available")?;

                let full_path = manifest_dir.join(path);
                Self::canonicalize_with_context(
                    &full_path,
                    format!("canonicalizing local dependency path: {}", path),
                    "resource_service::get_canonical_path",
                )
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

                    // Safe: Same invariant as above - prepare_additional_version ensures the
                    // group_key exists in prepared_versions before this point is reached.
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
                    Self::canonicalize_with_context(
                        &full_path,
                        format!("canonicalizing dependency path: {}", detailed.path),
                        "resource_service::get_canonical_path",
                    )
                }
            }
        }
    }
}

impl Default for ResourceFetchingService {
    fn default() -> Self {
        Self::new()
    }
}
