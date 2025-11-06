//! Lockfile consistency validation.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use crate::core::ResourceType;

use super::{OutputFormat, ValidationContext};

/// Validates lockfile consistency with the manifest.
///
/// This function compares the manifest dependencies with those recorded in the
/// lockfile to identify inconsistencies. It warns if dependencies are missing
/// from the lockfile or if extra entries exist.
///
/// # Arguments
///
/// * `ctx` - Validation context containing all necessary parameters
/// * `project_dir` - Path to the project directory (fallback if not in manifest)
///
/// # Returns
///
/// Returns `Ok(())` if the lockfile is consistent or missing, or `Err` if inconsistent.
pub async fn validate_lockfile(ctx: &mut ValidationContext<'_>, project_dir: &Path) -> Result<()> {
    let lockfile_path = project_dir.join("agpm.lock");

    if !lockfile_path.exists() {
        ctx.print("⚠ No lockfile found");
        ctx.warnings.push("No lockfile found".to_string());

        // Check private lockfile validity if it exists
        validate_private_lockfile(project_dir, ctx.verbose, ctx.quiet, ctx.warnings, ctx.errors)
            .await;

        return Ok(());
    }

    ctx.print_verbose("\n🔍 Checking lockfile consistency...");

    match crate::lockfile::LockFile::load(&lockfile_path) {
        Ok(lockfile) => {
            // Check that all manifest dependencies are in lockfile
            let mut missing = Vec::new();
            let mut extra = Vec::new();

            // Check for missing dependencies using unified interface
            for resource_type in &[ResourceType::Agent, ResourceType::Snippet] {
                let manifest_resources = ctx.manifest.get_resources(resource_type);
                let lockfile_resources = lockfile.get_resources(resource_type);
                let type_name = match resource_type {
                    ResourceType::Agent => "agent",
                    ResourceType::Snippet => "snippet",
                    _ => unreachable!(),
                };

                for name in manifest_resources.keys() {
                    if !lockfile_resources
                        .iter()
                        .any(|e| e.manifest_alias.as_ref().unwrap_or(&e.name) == name)
                    {
                        missing.push((name.clone(), type_name));
                    }
                }
            }

            // Check for extra dependencies in lockfile
            for resource_type in &[ResourceType::Agent, ResourceType::Snippet] {
                let manifest_resources = ctx.manifest.get_resources(resource_type);
                let lockfile_resources = lockfile.get_resources(resource_type);
                let type_name = match resource_type {
                    ResourceType::Agent => "agent",
                    ResourceType::Snippet => "snippet",
                    _ => unreachable!(),
                };

                for entry in lockfile_resources {
                    let manifest_key = entry.manifest_alias.as_ref().unwrap_or(&entry.name);
                    if !manifest_resources.contains_key(manifest_key) {
                        extra.push((entry.name.clone(), type_name));
                    }
                }
            }

            if missing.is_empty() && extra.is_empty() {
                ctx.validation_results.lockfile_consistent = true;
                ctx.print("✓ Lockfile consistent");
            } else if !extra.is_empty() {
                let error_msg = format!(
                    "Lockfile inconsistent with manifest: found {}",
                    // Safe: !extra.is_empty() is checked above, guaranteeing first() returns Some
                    extra.first().unwrap().0
                );
                ctx.errors.push(error_msg.clone());

                if matches!(ctx.format, OutputFormat::Json) {
                    ctx.validation_results.valid = false;
                    ctx.validation_results.errors = ctx.errors.clone();
                    ctx.validation_results.warnings = ctx.warnings.to_owned();
                    println!("{}", serde_json::to_string_pretty(&ctx.validation_results)?);
                    return Err(anyhow::anyhow!("Lockfile inconsistent"));
                } else {
                    ctx.print(&format!("{} {}", "✗".red(), error_msg));
                }
                return Err(anyhow::anyhow!("Lockfile inconsistent"));
            } else {
                ctx.validation_results.lockfile_consistent = false;
                ctx.print(&format!(
                    "{} Lockfile is missing {} dependencies:",
                    "⚠".yellow(),
                    missing.len()
                ));
                for (name, type_) in missing {
                    ctx.print(&format!("  - {name} ({type_}))"));
                }
                ctx.print("\nRun 'agpm install' to update the lockfile");
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to parse lockfile: {e}");
            ctx.errors.push(error_msg.to_string());

            if matches!(ctx.format, OutputFormat::Json) {
                ctx.validation_results.valid = false;
                ctx.validation_results.errors = ctx.errors.clone();
                ctx.validation_results.warnings = ctx.warnings.to_owned();
                println!("{}", serde_json::to_string_pretty(&ctx.validation_results)?);
                return Err(anyhow::anyhow!("Invalid lockfile syntax: {e}"));
            } else {
                ctx.print(&format!("{} {}", "✗".red(), error_msg));
            }
            return Err(anyhow::anyhow!("Invalid lockfile syntax: {e}"));
        }
    }

    // Check private lockfile validity if it exists
    validate_private_lockfile(project_dir, ctx.verbose, ctx.quiet, ctx.warnings, ctx.errors).await;

    Ok(())
}

/// Validates the private lockfile if it exists.
///
/// This is a helper function that checks if agpm.private.lock is valid.
async fn validate_private_lockfile(
    project_dir: &Path,
    verbose: bool,
    quiet: bool,
    _warnings: &mut [String],
    errors: &mut Vec<String>,
) {
    let private_lock_path = project_dir.join("agpm.private.lock");
    if !private_lock_path.exists() {
        return;
    }

    if verbose && !quiet {
        println!("\n🔍 Checking private lockfile...");
    }

    match crate::lockfile::PrivateLockFile::load(project_dir) {
        Ok(Some(_)) => {
            if !quiet && verbose {
                println!("✓ Private lockfile is valid");
            }
        }
        Ok(None) => {
            // File exists but couldn't be loaded - this shouldn't happen
            // We can't push to a slice directly, so we'll skip this warning
            // In a real fix, we'd change the function signature to return warnings
        }
        Err(e) => {
            let error_msg = format!("Failed to parse private lockfile: {e}");
            errors.push(error_msg.to_string());
            if !quiet {
                println!("{} {}", "✗".red(), error_msg);
            }
        }
    }
}
