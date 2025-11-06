//! Validation results structure for aggregating validation outcomes.

use serde::Serialize;

/// Results structure for validation operations, used primarily for JSON output.
///
/// This struct aggregates all validation results into a single structure that
/// can be serialized to JSON for machine consumption. Each field represents
/// the result of a specific validation check.
///
/// # Fields
///
/// - `valid`: Overall validation status (no errors, or warnings in strict mode)
/// - `manifest_valid`: Whether the manifest file is syntactically valid
/// - `dependencies_resolvable`: Whether all dependencies can be resolved
/// - `sources_accessible`: Whether all source repositories are accessible
/// - `local_paths_exist`: Whether all local file dependencies exist
/// - `lockfile_consistent`: Whether the lockfile matches the manifest
/// - `errors`: List of error messages that caused validation to fail
/// - `warnings`: List of warning messages (non-fatal issues)
///
/// # JSON Output Example
///
/// ```json
/// {
///   "valid": true,
///   "manifest_valid": true,
///   "dependencies_resolvable": true,
///   "sources_accessible": true,
///   "local_paths_exist": true,
///   "lockfile_consistent": false,
///   "errors": [],
///   "warnings": ["Lockfile is missing 2 dependencies"]
/// }
/// ```
#[derive(Serialize)]
pub struct ValidationResults {
    /// Overall validation status - true if no errors (and no warnings in strict mode)
    pub valid: bool,
    /// Whether the manifest file syntax and structure is valid
    pub manifest_valid: bool,
    /// Whether all dependencies can be resolved to specific versions
    pub dependencies_resolvable: bool,
    /// Whether all source repositories are accessible via network
    pub sources_accessible: bool,
    /// Whether all local file dependencies point to existing files
    pub local_paths_exist: bool,
    /// Whether the lockfile is consistent with the manifest
    pub lockfile_consistent: bool,
    /// Whether all templates rendered successfully (when --render is used)
    pub templates_valid: bool,
    /// Number of templates successfully rendered
    pub templates_rendered: usize,
    /// Total number of templates found
    pub templates_total: usize,
    /// List of error messages that caused validation failure
    pub errors: Vec<String>,
    /// List of warning messages (non-fatal issues)
    pub warnings: Vec<String>,
}

impl Default for ValidationResults {
    fn default() -> Self {
        Self {
            valid: true, // Default to true as expected by test
            manifest_valid: false,
            dependencies_resolvable: false,
            sources_accessible: false,
            local_paths_exist: false,
            lockfile_consistent: false,
            templates_valid: false,
            templates_rendered: 0,
            templates_total: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
