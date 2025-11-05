//! Template rendering and file reference validation.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;
use std::sync::Arc;

use crate::cache::Cache;
use crate::cli::common::CommandContext;
use crate::core::ResourceType;
use crate::markdown::reference_extractor::{extract_file_references, validate_file_references};
use crate::templating::{RenderingMetadata, TemplateContextBuilder, TemplateRenderer};

use super::{OutputFormat, ValidationContext};

/// Validates template rendering and file references in markdown resources.
///
/// This function performs two validations:
/// 1. **Template Rendering**: Checks that all markdown resources can be successfully
///    rendered with their template syntax.
/// 2. **File References**: Validates that all file references within markdown content
///    point to existing files.
///
/// Requires a lockfile to build the template context.
///
/// # Arguments
///
/// * `ctx` - Validation context containing all necessary parameters
/// * `project_dir` - Path to the project directory
///
/// # Returns
///
/// Returns `Ok(())` if all templates render successfully and file references are valid,
/// or `Err` if validation fails.
pub async fn validate_templates(ctx: &mut ValidationContext<'_>, project_dir: &Path) -> Result<()> {
    ctx.print_verbose("\n🔍 Validating template rendering...");

    // Load lockfile - required for template context
    let lockfile_path = project_dir.join("agpm.lock");

    if !lockfile_path.exists() {
        let error_msg = "Lockfile required for template rendering (run 'agpm install' first)";
        ctx.errors.push(error_msg.to_string());

        if matches!(ctx.format, OutputFormat::Json) {
            ctx.validation_results.valid = false;
            ctx.validation_results.errors = ctx.errors.clone();
            ctx.validation_results.warnings = ctx.warnings.to_owned();
            println!("{}", serde_json::to_string_pretty(&ctx.validation_results)?);
            return Err(anyhow::anyhow!("{}", error_msg));
        } else {
            ctx.print(&format!("{} {}", "✗".red(), error_msg));
        }
        return Err(anyhow::anyhow!("{}", error_msg));
    }

    // Create command context for enhanced lockfile loading
    let command_context = CommandContext::new(ctx.manifest.clone(), project_dir.to_path_buf())?;

    // Use enhanced lockfile loading with automatic regeneration
    let lockfile = match command_context.load_lockfile_with_regeneration(true, "validate")? {
        Some(lockfile) => Arc::new(lockfile),
        None => {
            return Err(anyhow::anyhow!(
                "Lockfile was invalid and has been removed. Run 'agpm install' to regenerate it first."
            ));
        }
    };
    let cache = Arc::new(Cache::new()?);

    // Load global config for template rendering settings
    let global_config = crate::config::GlobalConfig::load().await.unwrap_or_default();
    let max_content_file_size = Some(global_config.max_content_file_size);

    // Collect all markdown resources from manifest
    let mut template_results = Vec::new();
    let mut templates_found = 0;
    let mut templates_rendered = 0;

    // Helper macro to validate template rendering
    macro_rules! validate_resource_template {
        ($name:expr, $entry:expr, $resource_type:expr) => {{
            // Read the resource content
            let content = if $entry.source.is_some() && $entry.resolved_commit.is_some() {
                // Git resource - read from worktree
                let source_name = $entry.source.as_ref().unwrap();
                let sha = $entry.resolved_commit.as_ref().unwrap();
                let url = match $entry.url.as_ref() {
                    Some(u) => u,
                    None => {
                        template_results.push(format!("{}: Missing URL for Git resource", $name));
                        continue;
                    }
                };

                let cache_dir = match cache
                    .get_or_create_worktree_for_sha(source_name, url, sha, Some($name))
                    .await
                {
                    Ok(dir) => dir,
                    Err(e) => {
                        template_results.push(format!("{}: {}", $name, e));
                        continue;
                    }
                };

                let source_path = cache_dir.join(&$entry.path);
                match tokio::fs::read_to_string(&source_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        template_results.push(format!(
                            "{}: Failed to read file '{}': {}",
                            $name,
                            source_path.display(),
                            e
                        ));
                        continue;
                    }
                }
            } else {
                // Local resource - read from project directory
                let source_path = {
                    let candidate = Path::new(&$entry.path);
                    if candidate.is_absolute() {
                        candidate.to_path_buf()
                    } else {
                        project_dir.join(candidate)
                    }
                };

                match tokio::fs::read_to_string(&source_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        template_results.push(format!(
                            "{}: Failed to read file '{}': {}",
                            $name,
                            source_path.display(),
                            e
                        ));
                        continue;
                    }
                }
            };

            // Check if it contains template syntax
            let has_template_syntax =
                content.contains("{{") || content.contains("{%") || content.contains("{#");

            if !has_template_syntax {
                continue; // Not a template
            }

            templates_found += 1;

            // Build template context
            let project_config = ctx.manifest.project.clone();
            let context_builder = TemplateContextBuilder::new(
                Arc::clone(&lockfile),
                project_config,
                Arc::clone(&cache),
                project_dir.to_path_buf(),
            );
            // Use canonical name from lockfile entry, not manifest key
            let resource_id = crate::lockfile::ResourceId::new(
                $entry.name.clone(),
                $entry.source.clone(),
                $entry.tool.clone(),
                $resource_type,
                $entry.variant_inputs.hash().to_string(),
            );
            let context = match context_builder
                .build_context(&resource_id, $entry.variant_inputs.json())
                .await
            {
                Ok((c, _checksum)) => c,
                Err(e) => {
                    template_results.push(format!("{}: {}", $name, e));
                    continue;
                }
            };

            // Try to render
            let mut renderer =
                match TemplateRenderer::new(true, project_dir.to_path_buf(), max_content_file_size)
                {
                    Ok(r) => r,
                    Err(e) => {
                        template_results.push(format!("{}: {}", $name, e));
                        continue;
                    }
                };

            // Create rendering metadata for better error messages
            let rendering_metadata = RenderingMetadata {
                resource_name: $entry.name.clone(),
                resource_type: $resource_type,
                dependency_chain: vec![], // Could be enhanced to include parent info
                source_path: Some($entry.path.clone().into()),
                depth: 0,
            };

            match renderer.render_template(&content, &context, Some(&rendering_metadata)) {
                Ok(_) => {
                    templates_rendered += 1;
                }
                Err(e) => {
                    template_results.push(format!("{}: {}", $name, e));
                }
            }
        }};
    }

    // Process each resource type
    // Use manifest_alias (if present) when matching manifest keys to lockfile entries
    for resource_type in
        &[ResourceType::Agent, ResourceType::Snippet, ResourceType::Command, ResourceType::Script]
    {
        let manifest_resources = ctx.manifest.get_resources(resource_type);
        let lockfile_resources = lockfile.get_resources(resource_type);

        for name in manifest_resources.keys() {
            if let Some(entry) = lockfile_resources
                .iter()
                .find(|e| e.manifest_alias.as_ref().unwrap_or(&e.name) == name)
            {
                validate_resource_template!(name, entry, *resource_type);
            }
        }
    }

    // Update validation results
    ctx.validation_results.templates_total = templates_found;
    ctx.validation_results.templates_rendered = templates_rendered;
    ctx.validation_results.templates_valid = template_results.is_empty();

    // Report results (only for text output, not JSON)
    if template_results.is_empty() {
        if templates_found > 0 {
            if !ctx.quiet && *ctx.format == OutputFormat::Text {
                println!("✓ All {} templates rendered successfully", templates_found);
            }
        } else if !ctx.quiet && *ctx.format == OutputFormat::Text {
            println!("⚠ No templates found in resources");
        }
    } else {
        let error_msg =
            format!("Template rendering failed for {} resource(s)", template_results.len());
        ctx.errors.push(error_msg.clone());

        if matches!(ctx.format, OutputFormat::Json) {
            ctx.validation_results.valid = false;
            ctx.validation_results.errors.extend(template_results);
            ctx.validation_results.errors.push(error_msg);
            ctx.validation_results.warnings = ctx.warnings.to_owned();
            println!("{}", serde_json::to_string_pretty(&ctx.validation_results)?);
            return Err(anyhow::anyhow!("Template rendering failed"));
        } else if !ctx.quiet {
            println!("{} {}", "✗".red(), error_msg);
            for error in &template_results {
                println!("  {}", error);
            }
        }
        return Err(anyhow::anyhow!("Template rendering failed"));
    }

    // Validate file references in markdown content
    if ctx.verbose && !ctx.quiet {
        println!("\n🔍 Validating file references in markdown content...");
    }

    let mut file_reference_errors = Vec::new();
    let mut total_references_checked = 0;

    // Helper macro to validate file references in markdown resources
    macro_rules! validate_file_references_in_resource {
        ($name:expr, $entry:expr) => {{
            // Read the resource content
            let content = if $entry.source.is_some() && $entry.resolved_commit.is_some() {
                // Git resource - read from worktree
                let source_name = $entry.source.as_ref().unwrap();
                let sha = $entry.resolved_commit.as_ref().unwrap();
                let url = match $entry.url.as_ref() {
                    Some(u) => u,
                    None => {
                        continue;
                    }
                };

                let cache_dir = match cache
                    .get_or_create_worktree_for_sha(source_name, url, sha, Some($name))
                    .await
                {
                    Ok(dir) => dir,
                    Err(_) => {
                        continue;
                    }
                };

                let source_path = cache_dir.join(&$entry.path);
                match tokio::fs::read_to_string(&source_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::debug!(
                            "Failed to read source file '{}' for reference validation: {}",
                            source_path.display(),
                            e
                        );
                        continue;
                    }
                }
            } else {
                // Local resource - read from installed location
                let installed_path = project_dir.join(&$entry.installed_at);

                match tokio::fs::read_to_string(&installed_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::debug!(
                            "Failed to read installed file '{}' for reference validation: {}",
                            installed_path.display(),
                            e
                        );
                        continue;
                    }
                }
            };

            // Extract file references from markdown content
            let references = extract_file_references(&content);

            if !references.is_empty() {
                total_references_checked += references.len();

                // Validate each reference exists
                match validate_file_references(&references, project_dir) {
                    Ok(missing) => {
                        for missing_ref in missing {
                            file_reference_errors.push(format!(
                                "{}: references non-existent file '{}'",
                                $entry.installed_at, missing_ref
                            ));
                        }
                    }
                    Err(e) => {
                        file_reference_errors.push(format!(
                            "{}: failed to validate references: {}",
                            $entry.installed_at, e
                        ));
                    }
                }
            }
        }};
    }

    // Process each markdown resource type from lockfile
    for entry in &lockfile.agents {
        validate_file_references_in_resource!(&entry.name, entry);
    }

    for entry in &lockfile.snippets {
        validate_file_references_in_resource!(&entry.name, entry);
    }

    for entry in &lockfile.commands {
        validate_file_references_in_resource!(&entry.name, entry);
    }

    for entry in &lockfile.scripts {
        validate_file_references_in_resource!(&entry.name, entry);
    }

    // Report file reference validation results
    if file_reference_errors.is_empty() {
        if total_references_checked > 0 {
            if !ctx.quiet && *ctx.format == OutputFormat::Text {
                println!(
                    "✓ All {} file references validated successfully",
                    total_references_checked
                );
            }
        } else if ctx.verbose && !ctx.quiet && *ctx.format == OutputFormat::Text {
            println!("⚠ No file references found in resources");
        }
        Ok(())
    } else {
        let error_msg = format!(
            "File reference validation failed: {} broken reference(s) found",
            file_reference_errors.len()
        );
        ctx.errors.push(error_msg.clone());

        if matches!(ctx.format, OutputFormat::Json) {
            ctx.validation_results.valid = false;
            ctx.validation_results.errors.extend(file_reference_errors);
            ctx.validation_results.errors.push(error_msg);
            ctx.validation_results.warnings = ctx.warnings.to_owned();
            println!("{}", serde_json::to_string_pretty(&ctx.validation_results)?);
            return Err(anyhow::anyhow!("File reference validation failed"));
        } else if !ctx.quiet {
            println!("{} {}", "✗".red(), error_msg);
            for error in &file_reference_errors {
                println!("  {}", error);
            }
        }
        Err(anyhow::anyhow!("File reference validation failed"))
    }
}
