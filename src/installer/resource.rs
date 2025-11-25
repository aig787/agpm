//! Resource installation helper functions.
//!
//! This module contains extracted helper functions for resource installation,
//! breaking down the complex installation process into manageable, testable units.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::core::file_error::{FileOperation, FileResultExt};
use crate::installer::context::InstallContext;
use crate::lockfile::LockedResource;
use crate::markdown::MarkdownFile;
use crate::templating::RenderingMetadata;
use crate::utils::fs::{atomic_write, ensure_dir};

/// Read source content from Git repository or local file.
///
/// This function handles the complexity of reading content from either:
/// - Git-based sources (using SHA-based worktrees)
/// - Local directory sources
/// - Local file paths (relative or absolute)
///
/// # Arguments
///
/// * `entry` - The locked resource containing source information
/// * `context` - Installation context with cache and project directory
///
/// # Returns
///
/// Returns the file content as a String if successful.
pub async fn read_source_content(
    entry: &LockedResource,
    context: &InstallContext<'_>,
) -> Result<String> {
    if let Some(source_name) = &entry.source {
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

        // Read the content from the source
        // Use retry for Git worktree files - they can have brief visibility
        // delays after creation, especially under high parallel I/O load
        let source_path = cache_dir.join(&entry.path);
        crate::utils::fs::read_text_file_with_retry(&source_path).await
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

        tokio::fs::read_to_string(&source_path)
            .await
            .with_file_context(
                FileOperation::Read,
                &source_path,
                "reading resource file",
                "installer_resource",
            )
            .map_err(Into::into)
    }
}

/// Validate markdown content format.
///
/// This function checks that the content is valid markdown and can be parsed.
/// It silently accepts invalid frontmatter (warnings are handled by MetadataExtractor).
///
/// # Arguments
///
/// * `content` - The markdown content to validate
///
/// # Returns
///
/// Returns Ok(()) if valid, or an error with details if invalid.
pub fn validate_markdown_content(content: &str) -> Result<()> {
    MarkdownFile::parse(content)?;
    Ok(())
}

/// Apply patches to resource content.
///
/// This function applies both project-level and private patches to the content
/// before templating occurs. Patches are looked up by resource type and name.
///
/// # Arguments
///
/// * `content` - The original content to patch
/// * `entry` - The locked resource containing metadata
/// * `context` - Installation context with patch data
///
/// # Returns
///
/// Returns a tuple of (patched_content, applied_patches).
pub fn apply_resource_patches(
    content: &str,
    entry: &LockedResource,
    context: &InstallContext<'_>,
) -> Result<(String, crate::manifest::patches::AppliedPatches)> {
    let empty_patches = std::collections::BTreeMap::new();

    if context.project_patches.is_some() || context.private_patches.is_some() {
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
            content,
            file_path,
            project_patch_data,
            private_patch_data,
        )
        .with_context(|| format!("Failed to apply patches to resource {}", entry.name))
    } else {
        Ok((content.to_string(), crate::manifest::patches::AppliedPatches::default()))
    }
}

/// Render resource content with templating support.
///
/// This is the main orchestrator for template rendering. It handles the corrected flow:
/// 1. Always render frontmatter (may contain template variables)
/// 2. Check `agpm.templating` flag in rendered frontmatter
/// 3. If true: render full file (frontmatter + body)
/// 4. If false: use rendered frontmatter + original body (via boundary replacement)
///
/// # Arguments
///
/// * `content` - The patched content to render
/// * `entry` - The locked resource containing metadata
/// * `context` - Installation context with template builder
///
/// # Returns
///
/// Returns a tuple of (final_content, templating_was_applied, context_checksum).
pub async fn render_resource_content(
    content: &str,
    entry: &LockedResource,
    context: &InstallContext<'_>,
) -> Result<(String, bool, Option<String>)> {
    // Only process markdown files
    if !entry.path.ends_with(".md") {
        tracing::debug!("Not a markdown file: {}", entry.path);
        return Ok((content.to_string(), false, None));
    }

    // Step 1: Parse frontmatter boundaries from original content
    let frontmatter_parser = crate::markdown::frontmatter::FrontmatterParser::new();
    let raw_frontmatter = frontmatter_parser.extract_raw_frontmatter(content);
    let boundaries = frontmatter_parser.get_frontmatter_boundaries(content);

    // If no frontmatter, return original content
    let Some(raw_fm) = raw_frontmatter else {
        return Ok((content.to_string(), false, None));
    };

    // Step 2: Build template context for frontmatter rendering
    let template_context_builder = &context.template_context_builder;
    let resource_id = crate::lockfile::ResourceId::new(
        entry.name.clone(),
        entry.source.clone(),
        entry.tool.clone(),
        entry.resource_type,
        entry.variant_inputs.hash().to_string(),
    );

    // Try to build template context - if it fails, just use original content
    let template_context = match template_context_builder
        .build_context(&resource_id, entry.variant_inputs.json())
        .await
    {
        Ok((ctx, _)) => ctx,
        Err(e) => {
            tracing::debug!(
                "Failed to build template context for resource '{}', using original content: {}",
                entry.name,
                e
            );
            return Ok((content.to_string(), false, None));
        }
    };

    // Step 3: Render frontmatter to resolve template variables
    let rendered_frontmatter = {
        use crate::templating::TemplateRenderer;
        let mut renderer = match TemplateRenderer::new(
            true,
            context.project_dir.to_path_buf(),
            context.max_content_file_size,
        ) {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(
                    "Failed to create template renderer for resource '{}', using original content: {}",
                    entry.name,
                    e
                );
                return Ok((content.to_string(), false, None));
            }
        };

        match renderer.render_template(&raw_fm, &template_context, None) {
            Ok(rendered) => rendered,
            Err(e) => {
                tracing::debug!(
                    "Failed to render frontmatter template for resource '{}', using original content: {}",
                    entry.name,
                    e
                );
                return Ok((content.to_string(), false, None));
            }
        }
    };

    // Step 4: Parse rendered frontmatter to check agpm.templating flag
    let parsed: serde_json::Value = match serde_yaml::from_str(&rendered_frontmatter) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(
                "Failed to parse rendered frontmatter for resource '{}' as valid YAML, using original content: {}",
                entry.name,
                e
            );
            return Ok((content.to_string(), false, None));
        }
    };

    let templating_enabled = parsed
        .get("agpm")
        .and_then(|agpm| agpm.get("templating"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Step 5: Based on templating flag, either render full file or use boundary replacement
    if !templating_enabled {
        // No body templating - use rendered frontmatter + original body via boundary replacement
        if let Some(bounds) = boundaries {
            let final_content =
                frontmatter_parser.replace_frontmatter(content, &rendered_frontmatter, bounds);
            Ok((final_content, false, None))
        } else {
            // No boundaries found, use original content
            Ok((content.to_string(), false, None))
        }
    } else {
        // Full file templating enabled - render everything
        render_full_file(content, entry, context, template_context_builder).await
    }
}

/// Render the full file (frontmatter + body) as one unit.
///
/// This function is called when `agpm.templating: true` is set in the frontmatter.
/// It renders the entire file content including both frontmatter and body.
///
/// # Arguments
///
/// * `content` - The patched content to render
/// * `entry` - The locked resource containing metadata
/// * `context` - Installation context
/// * `template_context_builder` - Builder for template context
///
/// # Returns
///
/// Returns a tuple of (rendered_content, true, context_checksum).
async fn render_full_file(
    content: &str,
    entry: &LockedResource,
    context: &InstallContext<'_>,
    template_context_builder: &crate::templating::TemplateContextBuilder,
) -> Result<(String, bool, Option<String>)> {
    use crate::templating::TemplateRenderer;

    // Compute context digest for cache invalidation
    let context_digest = template_context_builder.compute_context_digest().with_context(|| {
        format!("Failed to compute context digest for resource '{}'", entry.name)
    })?;

    let resource_id = crate::lockfile::ResourceId::new(
        entry.name.clone(),
        entry.source.clone(),
        entry.tool.clone(),
        entry.resource_type,
        entry.variant_inputs.hash().to_string(),
    );

    // Build template context
    let (template_context, captured_context_checksum) = template_context_builder
        .build_context(&resource_id, entry.variant_inputs.json())
        .await
        .with_context(|| {
            format!("Failed to build template context for resource '{}'", entry.name)
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

    // Render the entire file content as one unit
    let rendering_metadata = RenderingMetadata {
        resource_name: entry.name.clone(),
        resource_type: entry.resource_type,
        dependency_chain: vec![],
        source_path: Some(entry.path.clone().into()),
        depth: 0,
    };

    let mut renderer = TemplateRenderer::new(
        true,
        context.project_dir.to_path_buf(),
        context.max_content_file_size,
    )
    .with_context(|| "Failed to create template renderer")?;

    let rendered_content = renderer
        .render_template(content, &template_context, Some(&rendering_metadata))
        .map_err(|e| {
            tracing::error!("Template rendering failed for resource '{}': {}", entry.name, e);
            anyhow::Error::from(e)
        })?;

    if context.verbose {
        // Show verbose output after rendering
        let size_bytes = rendered_content.len();
        let size_str = if size_bytes < 1024 {
            format!("{} B", size_bytes)
        } else if size_bytes < 1024 * 1024 {
            format!("{:.1} KB", size_bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", size_bytes as f64 / (1024.0 * 1024.0))
        };
        let dest_path = if entry.installed_at.is_empty() {
            context
                .project_dir
                .join(entry.resource_type.to_plural())
                .join(format!("{}.md", entry.name))
        } else {
            context.project_dir.join(&entry.installed_at)
        };
        tracing::info!("   Output: {} ({})", dest_path.display(), size_str);
        tracing::info!("✅ Template rendered successfully");
    }

    Ok((rendered_content, true, captured_context_checksum))
}

/// Compute SHA-256 checksum of file content.
///
/// # Arguments
///
/// * `content` - The content to checksum
///
/// # Returns
///
/// Returns the checksum as a hex string with "sha256:" prefix.
pub fn compute_file_checksum(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    format!("sha256:{}", hex::encode(hash))
}

/// Check if installation should be skipped (early-exit optimization).
///
/// This function implements the early-exit optimization for Git-based dependencies
/// where we can reliably detect changes via the resolved_commit SHA.
///
/// # Arguments
///
/// * `entry` - The locked resource to check
/// * `dest_path` - The destination file path
/// * `existing_checksum` - The checksum of the existing file (if any)
/// * `context` - Installation context with old lockfile
///
/// # Returns
///
/// Returns Some((file_checksum, context_checksum, applied_patches)) if we should skip,
/// None if we should proceed with installation.
pub fn should_skip_installation(
    entry: &LockedResource,
    dest_path: &Path,
    existing_checksum: Option<&String>,
    context: &InstallContext<'_>,
) -> Option<(String, Option<String>, crate::manifest::patches::AppliedPatches)> {
    // Only optimize for Git dependencies
    let is_local_dependency = entry.resolved_commit.as_deref().is_none_or(str::is_empty);
    if context.force_refresh || is_local_dependency {
        return None;
    }

    let old_lockfile = context.old_lockfile?;
    let old_entry = old_lockfile.find_resource(&entry.name, &entry.resource_type)?;

    // Check if all inputs that affect the final content are unchanged
    let resolved_commit_unchanged = old_entry.resolved_commit == entry.resolved_commit;
    let variant_inputs_unchanged = old_entry.variant_inputs == entry.variant_inputs;
    let patches_unchanged = old_entry.applied_patches == entry.applied_patches;

    let all_inputs_unchanged =
        resolved_commit_unchanged && variant_inputs_unchanged && patches_unchanged;

    if all_inputs_unchanged && dest_path.exists() {
        // File exists and all inputs match - verify checksum matches
        if existing_checksum == Some(&old_entry.checksum) {
            tracing::debug!(
                "⏭️  Skipping unchanged Git resource: {} (checksum matches)",
                entry.name
            );
            return Some((
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

    None
}

/// Write resource content to disk with atomic operations.
///
/// This function handles the final installation step, writing the content to disk
/// atomically and updating the .gitignore file if needed.
///
/// # Arguments
///
/// * `dest_path` - The destination file path
/// * `content` - The final content to write
/// * `should_install` - Whether to actually write (install=true in manifest)
/// * `content_changed` - Whether the content has changed from existing file
/// * `context` - Installation context with gitignore lock
///
/// # Returns
///
/// Returns true if the file was actually written, false otherwise.
pub async fn write_resource_to_disk(
    dest_path: &Path,
    content: &str,
    should_install: bool,
    content_changed: bool,
    context: &InstallContext<'_>,
) -> Result<bool> {
    if !should_install {
        // install=false: content-only dependency, don't write file
        tracing::debug!("Skipping file write for content-only dependency (install=false)");
        return Ok(false);
    }

    if !content_changed {
        // install=true but content unchanged
        return Ok(false);
    }

    // Create parent directory if needed
    if let Some(parent) = dest_path.parent() {
        ensure_dir(parent)?;
    }

    // Add to .gitignore BEFORE writing file to prevent accidental commits
    if let Some(lock) = context.gitignore_lock {
        // Calculate relative path for gitignore
        let relative_path = dest_path
            .strip_prefix(context.project_dir)
            .unwrap_or(dest_path)
            .to_string_lossy()
            .to_string();

        crate::installer::gitignore::add_path_to_gitignore(
            context.project_dir,
            &relative_path,
            lock,
        )
        .await
        .with_context(|| format!("Failed to add {} to .gitignore", relative_path))?;
    }

    // Write file atomically
    atomic_write(dest_path, content.as_bytes())
        .with_context(|| format!("Failed to install resource to {}", dest_path.display()))?;

    Ok(true)
}
