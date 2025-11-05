//! Source repository accessibility validation.

use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

use crate::cache::Cache;
use crate::core::OperationContext;
use crate::manifest::Manifest;
use crate::resolver::DependencyResolver;

use super::super::command::OutputFormat;
use super::super::results::ValidationResults;

/// Validates that all source repositories are accessible.
///
/// This function tests network connectivity to all source repositories defined
/// in the manifest. This verifies that sources are reachable and accessible with
/// current credentials.
///
/// # Arguments
///
/// * `manifest` - The manifest containing source definitions
/// * `format` - Output format for validation results
/// * `verbose` - Whether to enable verbose output
/// * `quiet` - Whether to suppress non-error output
/// * `validation_results` - Mutable reference to accumulate results
/// * `warnings` - Mutable vector to accumulate warnings
/// * `errors` - Mutable vector to accumulate errors
///
/// # Returns
///
/// Returns `Ok(())` if all sources are accessible, or `Err` if any are not.
pub async fn validate_sources(
    manifest: &Manifest,
    format: &OutputFormat,
    verbose: bool,
    quiet: bool,
    validation_results: &mut ValidationResults,
    warnings: &mut [String],
    errors: &mut Vec<String>,
) -> Result<()> {
    if verbose && !quiet {
        println!("\n🔍 Checking source accessibility...");
    }

    let cache = Cache::new()?;
    let resolver_result = DependencyResolver::new(manifest.clone(), cache).await;
    let mut resolver = match resolver_result {
        Ok(resolver) => resolver,
        Err(e) => {
            let error_msg = "Source not accessible: official, community".to_string();
            errors.push(error_msg.clone());

            if matches!(format, OutputFormat::Json) {
                validation_results.valid = false;
                validation_results.errors = errors.clone();
                validation_results.warnings = warnings.to_owned();
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
                return Err(anyhow::anyhow!("Source not accessible: {e}"));
            } else if !quiet {
                println!("{} {}", "✗".red(), error_msg);
            }
            return Err(anyhow::anyhow!("Source not accessible: {e}"));
        }
    };

    // Create operation context for warning deduplication
    let operation_context = Arc::new(OperationContext::new());
    resolver.set_operation_context(operation_context);

    let result = resolver.core().source_manager().verify_all().await;

    match result {
        Ok(()) => {
            validation_results.sources_accessible = true;
            if !quiet {
                println!("✓ Sources accessible");
            }
            Ok(())
        }
        Err(e) => {
            let error_msg = "Source not accessible: official, community".to_string();
            errors.push(error_msg.clone());

            if matches!(format, OutputFormat::Json) {
                validation_results.valid = false;
                validation_results.errors = errors.clone();
                validation_results.warnings = warnings.to_owned();
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
                return Err(anyhow::anyhow!("Source not accessible: {e}"));
            } else if !quiet {
                println!("{} {}", "✗".red(), error_msg);
            }
            Err(anyhow::anyhow!("Source not accessible: {e}"))
        }
    }
}
