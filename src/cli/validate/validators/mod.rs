//! Validation functions for different aspects of AGPM projects.
//!
//! This module provides specialized validators for different aspects of project
//! validation, organized by concern.

use crate::cli::validate::command::OutputFormat;
use crate::cli::validate::results::ValidationResults;
use crate::manifest::Manifest;

/// Common context for validation operations to reduce parameter count.
pub struct ValidationContext<'a> {
    /// The manifest being validated
    pub manifest: &'a Manifest,
    /// Output format for results
    pub format: &'a OutputFormat,
    /// Whether to enable verbose output
    pub verbose: bool,
    /// Whether to suppress non-error output
    pub quiet: bool,
    /// Mutable reference to accumulate validation results
    pub validation_results: &'a mut ValidationResults,
    /// Mutable vector to accumulate warnings
    pub warnings: &'a mut Vec<String>,
    /// Mutable vector to accumulate errors
    pub errors: &'a mut Vec<String>,
}

impl<'a> ValidationContext<'a> {
    /// Create a new validation context.
    pub fn new(
        manifest: &'a Manifest,
        format: &'a OutputFormat,
        verbose: bool,
        quiet: bool,
        validation_results: &'a mut ValidationResults,
        warnings: &'a mut Vec<String>,
        errors: &'a mut Vec<String>,
    ) -> Self {
        Self {
            manifest,
            format,
            verbose,
            quiet,
            validation_results,
            warnings,
            errors,
        }
    }

    /// Print a message if verbose and not quiet.
    pub fn print_verbose(&self, message: &str) {
        if self.verbose && !self.quiet {
            println!("{}", message);
        }
    }

    /// Print a message if not quiet.
    pub fn print(&self, message: &str) {
        if !self.quiet {
            println!("{}", message);
        }
    }
}

pub mod dependencies;
pub mod lockfile;
pub mod manifest;
pub mod paths;
pub mod sources;
pub mod templates;

// Re-export validation functions for convenience
pub use dependencies::validate_dependencies;
pub use lockfile::validate_lockfile;
pub use manifest::validate_manifest;
pub use paths::validate_paths;
pub use sources::validate_sources;
pub use templates::validate_templates;
