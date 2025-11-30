//! Skill installation support for directory-based resources.
//!
//! This module handles the installation of skill resources, which are directories
//! containing a `SKILL.md` file and potentially other supporting files. Skills are
//! treated differently from regular file-based resources because they:
//!
//! - Are complete directory structures rather than single files
//! - Require validation of the directory structure (must contain SKILL.md)
//! - Support size and file count limits for security
//! - Use directory checksums rather than file checksums

use crate::installer::context::InstallContext;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::patches::AppliedPatches;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Collect patches for a skill resource from project and private patch sources.
///
/// This function gathers patches from both the project-level patches (defined in
/// `agpm.toml`) and private patches (defined in `agpm.private.toml`) for a skill
/// resource.
///
/// # Arguments
///
/// * `entry` - The locked resource representing the skill
/// * `context` - Installation context containing patch sources
///
/// # Returns
///
/// An `AppliedPatches` struct containing both project and private patches
pub fn collect_skill_patches(
    entry: &LockedResource,
    context: &InstallContext<'_>,
) -> AppliedPatches {
    let resource_type = entry.resource_type.to_plural();
    let lookup_name = entry.lookup_name();

    tracing::debug!(
        "Collecting skill patches: resource_type={}, lookup_name={}, name={}, manifest_alias={:?}",
        resource_type,
        lookup_name,
        entry.name,
        entry.manifest_alias
    );

    let project_patches = context
        .project_patches
        .and_then(|patches| patches.get(resource_type, lookup_name))
        .cloned()
        .unwrap_or_default();

    tracing::debug!("Found {} project patches for skill {}", project_patches.len(), lookup_name);

    let private_patches = context
        .private_patches
        .and_then(|patches| patches.get(resource_type, lookup_name))
        .cloned()
        .unwrap_or_default();

    tracing::debug!("Found {} private patches for skill {}", private_patches.len(), lookup_name);

    AppliedPatches {
        project: project_patches,
        private: private_patches,
    }
}

/// Install a skill directory (directory-based resource).
///
/// This function handles the special case of skill resources, which are directories
/// containing a SKILL.md file and potentially other supporting files. The installation
/// process includes:
///
/// 1. Validating that installation should proceed
/// 2. Resolving the source directory
/// 3. Validating skill size limits
/// 4. Validating skill structure and metadata
/// 5. Creating the destination directory
/// 6. Copying the entire skill directory
/// 7. Applying patches to SKILL.md if any
///
/// # Arguments
///
/// * `entry` - The locked resource representing the skill
/// * `dest_path` - The destination path for the skill directory
/// * `applied_patches` - Patches to apply to the SKILL.md file
/// * `should_install` - Whether installation should proceed (from `install` field)
/// * `content_changed` - Whether the content has changed since last installation
/// * `context` - Installation context containing project configuration
///
/// # Returns
///
/// Returns `Ok(true)` if the skill was installed, `Ok(false)` if skipped
pub async fn install_skill_directory(
    entry: &LockedResource,
    dest_path: &Path,
    applied_patches: &AppliedPatches,
    should_install: bool,
    content_changed: bool,
    context: &InstallContext<'_>,
) -> Result<bool> {
    use crate::installer::gitignore::add_path_to_gitignore;
    use crate::utils::fs::ensure_dir;
    use anyhow::Context;

    if !should_install {
        tracing::debug!("Skipping skill directory installation (install=false)");
        return Ok(false);
    }

    if !content_changed {
        tracing::debug!("Skipping skill directory installation (content unchanged)");
        return Ok(false);
    }

    // Determine the source directory for the skill
    let source_dir = get_skill_source_directory(entry, context).await?;

    // Ensure source is a directory and validate skill structure
    if !source_dir.is_dir() {
        return Err(anyhow::anyhow!("Skill source is not a directory: {}", source_dir.display()));
    }

    // Validate skill size and collect directory info in a single pass
    // This runs blocking I/O in spawn_blocking to avoid blocking the async runtime
    tracing::debug!("Validating skill size limits: {}", source_dir.display());
    let dir_info = crate::skills::validate_skill_size(&source_dir)
        .await
        .with_context(|| format!("Skill size validation failed: {}", source_dir.display()))?;

    // Extract metadata from the already-collected directory info (no re-iteration)
    let (skill_frontmatter, skill_files) =
        crate::skills::extract_skill_metadata_from_info(&dir_info, &source_dir)
            .with_context(|| format!("Invalid skill directory: {}", source_dir.display()))?;

    tracing::debug!(
        "Installing skill '{}' with {} files: {}",
        skill_frontmatter.name,
        skill_files.len(),
        source_dir.display()
    );

    // For skills, dest_path should be a directory, not a file
    let skill_dest_dir = if dest_path.extension().and_then(|s| s.to_str()) == Some("md") {
        dest_path.with_extension("")
    } else {
        dest_path.to_path_buf()
    };

    // Ensure parent directory exists
    if let Some(parent) = skill_dest_dir.parent() {
        ensure_dir(parent)?;
    }

    // Add to .gitignore BEFORE copying directory
    let relative_path = skill_dest_dir
        .strip_prefix(context.project_dir)
        .unwrap_or(&skill_dest_dir)
        .to_string_lossy()
        .to_string();

    add_path_to_gitignore(context.project_dir, &relative_path)
        .await
        .with_context(|| format!("Failed to add {} to .gitignore", relative_path))?;

    // Remove existing skill directory first to ensure clean installation
    // Note: We skip the exists() check to avoid TOCTOU race conditions with concurrent processes
    // Instead, we handle NotFound gracefully during removal
    tracing::debug!("Removing existing skill directory if present: {}", skill_dest_dir.display());
    let skill_dest_dir_clone = skill_dest_dir.clone();
    let removal_result =
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&skill_dest_dir_clone))
            .await
            .map_err(|e| anyhow::anyhow!("Task join error during directory cleanup: {}", e))?;

    // Handle removal result - NotFound is acceptable (directory may not exist)
    match removal_result {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("Skill directory did not exist, nothing to remove");
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to remove existing skill directory {}: {}",
                skill_dest_dir.display(),
                e
            ));
        }
    }

    // Copy entire skill directory
    tracing::debug!(
        "Installing skill directory from {} to {}",
        source_dir.display(),
        skill_dest_dir.display()
    );

    let source_dir_clone = source_dir.clone();
    let skill_dest_dir_clone = skill_dest_dir.clone();
    tokio::task::spawn_blocking(move || {
        crate::utils::fs::copy_dir(&source_dir_clone, &skill_dest_dir_clone)
    })
    .await
    .with_context(|| {
        format!(
            "Failed to copy skill directory from {} to {}",
            source_dir.display(),
            skill_dest_dir.display()
        )
    })??;

    // Apply patches to SKILL.md if any were applied
    if !applied_patches.is_empty() {
        tracing::debug!(
            "Applying {} patches to skill SKILL.md file",
            applied_patches.total_count()
        );

        let installed_skill_md = skill_dest_dir.join("SKILL.md");
        let skill_md_content = tokio::fs::read_to_string(&installed_skill_md).await?;

        // Re-apply patches to the installed content
        let (patched_content, _) = crate::skills::patches::apply_skill_patches_preview(
            &skill_md_content,
            &applied_patches.project,
            &applied_patches.private,
        )?;

        // Write patched content back
        tokio::fs::write(&installed_skill_md, patched_content).await?;
    }

    Ok(true)
}

/// Compute directory checksum for a skill resource.
///
/// This function computes a SHA-256 checksum for the entire skill directory
/// by hashing all files within it in a deterministic order.
///
/// # Arguments
///
/// * `entry` - The locked resource representing the skill
/// * `context` - Installation context containing cache and configuration
///
/// # Returns
///
/// A SHA-256 checksum string in the format "sha256:..."
pub async fn compute_skill_directory_checksum(
    entry: &LockedResource,
    context: &InstallContext<'_>,
) -> Result<String> {
    let checksum_path = get_skill_source_directory(entry, context).await?;

    tracing::debug!(
        "Computing directory checksum for skill '{}' from path: {}",
        entry.name,
        checksum_path.display()
    );

    let checksum = LockFile::compute_directory_checksum(&checksum_path)?;
    tracing::debug!(
        "Calculated directory checksum for skill {}: {} (from: {})",
        entry.name,
        checksum,
        checksum_path.display()
    );

    Ok(checksum)
}

/// Get the source directory path for a skill resource.
///
/// This function resolves the source directory for a skill, handling both:
/// - Git-based sources (using SHA-based worktrees)
/// - Local sources (resolved relative to the manifest directory)
///
/// # Arguments
///
/// * `entry` - The locked resource representing the skill
/// * `context` - Installation context containing cache and manifest info
///
/// # Returns
///
/// The absolute path to the skill's source directory
pub async fn get_skill_source_directory(
    entry: &LockedResource,
    context: &InstallContext<'_>,
) -> Result<PathBuf> {
    use crate::core::file_error::{FileOperation, FileResultExt};

    if let Some(source_name) = &entry.source {
        let is_local_source = entry.resolved_commit.as_deref().is_none_or(str::is_empty);

        if is_local_source {
            // Local directory source - resolve the path relative to manifest directory
            let manifest = context
                .manifest
                .ok_or_else(|| anyhow::anyhow!("Manifest not available for local skill"))?;
            let manifest_dir = manifest.manifest_dir.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Manifest directory not available for local skill")
            })?;

            let skill_path = manifest_dir.join(&entry.path);
            Ok(skill_path.canonicalize().with_file_context(
                FileOperation::Canonicalize,
                &skill_path,
                format!("resolving local skill path for {}", entry.name),
                "get_skill_source_directory",
            )?)
        } else {
            // Git-based resource - use SHA-based worktree
            let url = entry
                .url
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Resource {} has no URL", entry.name))?;

            let sha = entry.resolved_commit.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Resource {} missing resolved commit SHA. Run 'agpm update' to regenerate lockfile.",
                    entry.name
                )
            })?;

            // Validate SHA format
            if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(anyhow::anyhow!(
                    "Invalid SHA '{}' for resource {}. Expected 40 hex characters.",
                    sha,
                    entry.name
                ));
            }

            let cache_dir = context
                .cache
                .get_or_create_worktree_for_sha(source_name, url, sha, Some(&entry.name))
                .await?;

            Ok(cache_dir.join(&entry.path))
        }
    } else {
        // Local skill - no source defined
        tracing::debug!("Processing local skill with no source: path='{}'", entry.path);
        let candidate = Path::new(&entry.path);
        Ok(if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            let manifest = context
                .manifest
                .ok_or_else(|| anyhow::anyhow!("Manifest not available for local skill"))?;
            let manifest_dir = manifest.manifest_dir.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Manifest directory not available for local skill")
            })?;

            manifest_dir.join(&entry.path)
        })
    }
}
