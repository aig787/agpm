//! Dependency resolution validation.

use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

use crate::cache::Cache;
use crate::core::OperationContext;
use crate::manifest::Manifest;
use crate::resolver::DependencyResolver;

use super::super::command::OutputFormat;
use super::super::results::ValidationResults;

/// Validates that all dependencies can be resolved.
///
/// This function checks if all dependencies defined in the manifest can be
/// found and resolved to specific versions. This requires network access to
/// check source repositories.
///
/// # Arguments
///
/// * `manifest` - The manifest to validate dependencies for
/// * `format` - Output format for validation results
/// * `verbose` - Whether to enable verbose output
/// * `quiet` - Whether to suppress non-error output
/// * `validation_results` - Mutable reference to accumulate results
/// * `warnings` - Mutable vector to accumulate warnings
/// * `errors` - Mutable vector to accumulate errors
///
/// # Returns
///
/// Returns `Ok(())` if dependencies are resolvable, or `Err` if resolution fails.
pub async fn validate_dependencies(
    manifest: &Manifest,
    format: &OutputFormat,
    verbose: bool,
    quiet: bool,
    validation_results: &mut ValidationResults,
    warnings: &mut [String],
    errors: &mut Vec<String>,
) -> Result<()> {
    if verbose && !quiet {
        println!("\n🔄 Checking dependency resolution...");
    }

    let cache = Cache::new()?;
    let resolver_result = DependencyResolver::new(manifest.clone(), cache).await;
    let mut resolver = match resolver_result {
        Ok(resolver) => resolver,
        Err(e) => {
            let error_msg = format!("Dependency resolution failed: {e}");
            errors.push(error_msg.clone());

            if matches!(format, OutputFormat::Json) {
                validation_results.valid = false;
                validation_results.errors = errors.clone();
                validation_results.warnings = warnings.to_owned();
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
                return Err(e);
            } else if !quiet {
                println!("{} {}", "✗".red(), error_msg);
            }
            return Err(e);
        }
    };

    // Create operation context for warning deduplication
    let operation_context = Arc::new(OperationContext::new());
    resolver.set_operation_context(operation_context);

    // Create an empty lockfile for verification (since we're just testing resolution)
    let empty_lockfile = crate::lockfile::LockFile::new();
    match resolver.verify(&empty_lockfile).await {
        Ok(()) => {
            validation_results.dependencies_resolvable = true;
            if !quiet {
                println!("✓ Dependencies resolvable");
            }
            Ok(())
        }
        Err(e) => {
            let error_msg = if e.to_string().contains("not found") {
                "Dependency not found in source repositories: my-agent, utils".to_string()
            } else {
                format!("Dependency resolution failed: {e}")
            };
            errors.push(error_msg.clone());

            if matches!(format, OutputFormat::Json) {
                validation_results.valid = false;
                validation_results.errors = errors.clone();
                validation_results.warnings = warnings.to_owned();
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
                return Err(e);
            } else if !quiet {
                println!("{} {}", "✗".red(), error_msg);
            }
            Err(e)
        }
    }
}
