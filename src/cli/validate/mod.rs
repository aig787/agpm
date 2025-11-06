//! Validate AGPM project configuration and dependencies.
//!
//! This module provides the `validate` command which performs comprehensive
//! validation of a AGPM project's manifest file, dependencies, sources, and
//! overall configuration. The command can check various aspects of the project
//! setup and report issues or warnings.
//!
//! # Features
//!
//! - **Manifest Validation**: Checks `agpm.toml` syntax and structure
//! - **Dependency Resolution**: Verifies all dependencies can be resolved
//! - **Source Accessibility**: Tests if source repositories are reachable
//! - **Path Validation**: Checks if local file dependencies exist
//! - **Lockfile Consistency**: Compares manifest and lockfile for consistency
//! - **Multiple Output Formats**: Text and JSON output formats
//! - **Strict Mode**: Treats warnings as errors for CI environments
//!
//! # Examples
//!
//! Basic validation:
//! ```bash
//! agpm validate
//! ```
//!
//! Comprehensive validation with all checks:
//! ```bash
//! agpm validate --resolve --sources --paths --check-lock
//! ```
//!
//! JSON output for automation:
//! ```bash
//! agpm validate --format json
//! ```
//!
//! Strict mode for CI:
//! ```bash
//! agpm validate --strict --quiet
//! ```
//!
//! Validate specific manifest file:
//! ```bash
//! agpm validate ./projects/my-project/agpm.toml
//! ```
//!
//! # Validation Levels
//!
//! ## Basic Validation (Default)
//! - Manifest file syntax and structure
//! - Required field presence
//! - Basic consistency checks
//!
//! ## Extended Validation (Flags Required)
//! - `--resolve`: Dependency resolution verification
//! - `--sources`: Source repository accessibility
//! - `--paths`: Local file path existence
//! - `--check-lock`: Lockfile consistency with manifest
//!
//! # Output Formats
//!
//! ## Text Format (Default)
//! ```text
//! ✓ Valid agpm.toml
//! ✓ Dependencies resolvable
//! ⚠ Warning: No dependencies defined
//! ```
//!
//! ## JSON Format
//! ```json
//! {
//!   "valid": true,
//!   "manifest_valid": true,
//!   "dependencies_resolvable": true,
//!   "sources_accessible": false,
//!   "errors": [],
//!   "warnings": ["No dependencies defined"]
//! }
//! ```
//!
//! # Error Categories
//!
//! - **Syntax Errors**: Invalid TOML format or structure
//! - **Semantic Errors**: Missing required fields, invalid references
//! - **Resolution Errors**: Dependencies cannot be found or resolved
//! - **Network Errors**: Sources are not accessible
//! - **File System Errors**: Local paths do not exist
//! - **Consistency Errors**: Manifest and lockfile are out of sync

mod command;
mod executor;
mod results;
mod validators;

#[cfg(test)]
mod tests;

// Re-export public API
pub use command::{OutputFormat, ValidateCommand};
pub use results::ValidationResults;
