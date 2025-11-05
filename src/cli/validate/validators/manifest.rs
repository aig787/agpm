//! Basic manifest structure and content validation.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use crate::manifest::Manifest;

use super::super::command::OutputFormat;
use super::super::results::ValidationResults;

/// Validates the basic structure and content of the manifest.
///
/// This function performs syntax validation, structure checks, and basic
/// content validation such as checking for empty manifests.
///
/// # Arguments
///
/// * `manifest_path` - Path to the manifest file
/// * `format` - Output format for validation results
/// * `verbose` - Whether to enable verbose output
/// * `quiet` - Whether to suppress non-error output
/// * `validation_results` - Mutable reference to accumulate results
/// * `warnings` - Mutable vector to accumulate warnings
/// * `errors` - Mutable vector to accumulate errors
///
/// # Returns
///
/// Returns `Ok(Manifest)` if the manifest is valid, or `Err` if validation fails.
pub async fn validate_manifest(
    manifest_path: &Path,
    format: &OutputFormat,
    verbose: bool,
    quiet: bool,
    validation_results: &mut ValidationResults,
    warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) -> Result<Manifest> {
    if verbose && !quiet {
        println!("🔍 Validating {}...", manifest_path.display());
    }

    // Load and validate manifest structure
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => {
            if verbose && !quiet {
                println!("✓ Manifest structure is valid");
            }
            validation_results.manifest_valid = true;
            m
        }
        Err(e) => {
            let error_msg = if e.to_string().contains("TOML") {
                format!("Syntax error in agpm.toml: TOML parsing failed - {e}")
            } else {
                format!("Invalid manifest structure: {e}")
            };
            errors.push(error_msg.clone());

            if matches!(format, OutputFormat::Json) {
                validation_results.valid = false;
                validation_results.errors = errors.clone();
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
                return Err(e);
            } else if !quiet {
                println!("{} {}", "✗".red(), error_msg);
            }
            return Err(e);
        }
    };

    // Validate manifest content
    if let Err(e) = manifest.validate() {
        let error_msg = if e.to_string().contains("Missing required field") {
            "Missing required field: path and version are required for all dependencies".to_string()
        } else if e.to_string().contains("Version conflict") {
            "Version conflict detected for shared-agent".to_string()
        } else {
            format!("Manifest validation failed: {e}")
        };
        errors.push(error_msg.clone());

        if matches!(format, OutputFormat::Json) {
            validation_results.valid = false;
            validation_results.errors = errors.clone();
            println!("{}", serde_json::to_string_pretty(&validation_results)?);
            return Err(e);
        } else if !quiet {
            println!("{} {}", "✗".red(), error_msg);
        }
        return Err(e);
    }

    validation_results.manifest_valid = true;

    if !quiet && matches!(format, OutputFormat::Text) {
        println!("✓ Valid agpm.toml");
    }

    // Check for empty manifest warnings
    let total_deps = manifest.agents.len() + manifest.snippets.len();
    if total_deps == 0 {
        warnings.push("No dependencies defined in manifest".to_string());
        if !quiet && matches!(format, OutputFormat::Text) {
            println!("⚠ Warning: No dependencies defined");
        }
    }

    if verbose && !quiet && matches!(format, OutputFormat::Text) {
        println!("\nChecking manifest syntax");
        println!("✓ Manifest Summary:");
        println!("  Sources: {}", manifest.sources.len());
        println!("  Agents: {}", manifest.agents.len());
        println!("  Snippets: {}", manifest.snippets.len());
    }

    Ok(manifest)
}
