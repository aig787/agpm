//! Local file path validation.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use super::{OutputFormat, ValidationContext};

/// Validates that all local file dependencies exist.
///
/// This function validates that all local file dependencies (those without a
/// source) point to existing files on the file system.
///
/// # Arguments
///
/// * `ctx` - Validation context containing all necessary parameters
/// * `manifest_path` - Path to the manifest file (used to resolve relative paths)
///
/// # Returns
///
/// Returns `Ok(())` if all local paths exist, or `Err` if any are missing.
pub async fn validate_paths(ctx: &mut ValidationContext<'_>, manifest_path: &Path) -> Result<()> {
    ctx.print_verbose("\n🔍 Checking local file paths...");

    let mut missing_paths = Vec::new();

    // Check local dependencies (those without source field)
    for (_name, dep) in ctx.manifest.agents.iter().chain(ctx.manifest.snippets.iter()) {
        if dep.get_source().is_none() {
            // This is a local dependency
            let path = dep.get_path();
            let full_path = if path.starts_with("./") || path.starts_with("../") {
                manifest_path.parent().unwrap().join(path)
            } else {
                std::path::PathBuf::from(path)
            };

            if !full_path.exists() {
                missing_paths.push(path.to_string());
            }
        }
    }

    if missing_paths.is_empty() {
        ctx.validation_results.local_paths_exist = true;
        ctx.print("✓ Local paths exist");
        Ok(())
    } else {
        let error_msg = format!("Local path not found: {}", missing_paths.join(", "));
        ctx.errors.push(error_msg.clone());

        if matches!(ctx.format, OutputFormat::Json) {
            ctx.validation_results.valid = false;
            ctx.validation_results.errors = ctx.errors.clone();
            ctx.validation_results.warnings = ctx.warnings.to_owned();
            println!("{}", serde_json::to_string_pretty(&ctx.validation_results)?);
            return Err(anyhow::anyhow!("{}", error_msg));
        } else {
            ctx.print(&format!("{} {}", "✗".red(), error_msg));
        }
        Err(anyhow::anyhow!("{}", error_msg))
    }
}
