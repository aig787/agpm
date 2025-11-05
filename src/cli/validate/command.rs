//! Command structure and output format definitions for validation.

use clap::Args;

/// Command to validate AGPM project configuration and dependencies.
///
/// This command performs comprehensive validation of a AGPM project, checking
/// various aspects from basic manifest syntax to complex dependency resolution.
/// It supports multiple validation levels and output formats for different use cases.
///
/// # Validation Strategy
///
/// The command performs validation in layers:
/// 1. **Syntax Validation**: TOML parsing and basic structure
/// 2. **Semantic Validation**: Required fields and references
/// 3. **Extended Validation**: Network and dependency checks (opt-in)
/// 4. **Consistency Validation**: Cross-file consistency checks
///
/// # Examples
///
/// ```rust,ignore
/// use agpm_cli::cli::validate::{ValidateCommand, OutputFormat};
///
/// // Basic validation
/// let cmd = ValidateCommand {
///     file: None,
///     resolve: false,
///     check_lock: false,
///     sources: false,
///     paths: false,
///     format: OutputFormat::Text,
///     verbose: false,
///     quiet: false,
///     strict: false,
///     render: false,
/// };
///
/// // Comprehensive CI validation
/// let cmd = ValidateCommand {
///     file: None,
///     resolve: true,
///     check_lock: true,
///     sources: true,
///     paths: true,
///     format: OutputFormat::Json,
///     verbose: false,
///     quiet: true,
///     strict: true,
///     render: false,
/// };
/// ```
#[derive(Args)]
pub struct ValidateCommand {
    /// Specific manifest file path to validate
    ///
    /// If not provided, searches for `agpm.toml` in the current directory
    /// and parent directories. When specified, validates the exact file path.
    #[arg(value_name = "FILE")]
    pub file: Option<String>,

    /// Check if all dependencies can be resolved
    ///
    /// Performs dependency resolution to verify that all dependencies
    /// defined in the manifest can be found and resolved to specific
    /// versions. This requires network access to check source repositories.
    #[arg(long, alias = "dependencies")]
    pub resolve: bool,

    /// Verify lockfile matches manifest
    ///
    /// Compares the manifest dependencies with those recorded in the
    /// lockfile to identify inconsistencies. Warns if dependencies are
    /// missing from the lockfile or if extra entries exist.
    #[arg(long, alias = "lockfile")]
    pub check_lock: bool,

    /// Check if all sources are accessible
    ///
    /// Tests network connectivity to all source repositories defined
    /// in the manifest. This verifies that sources are reachable and
    /// accessible with current credentials.
    #[arg(long)]
    pub sources: bool,

    /// Check if local file paths exist
    ///
    /// Validates that all local file dependencies (those without a
    /// source) point to existing files on the file system.
    #[arg(long)]
    pub paths: bool,

    /// Output format: text or json
    ///
    /// Controls the format of validation results:
    /// - `text`: Human-readable output with colors and formatting
    /// - `json`: Structured JSON output suitable for automation
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Verbose output
    ///
    /// Enables detailed output showing individual validation steps
    /// and additional diagnostic information.
    #[arg(short, long)]
    pub verbose: bool,

    /// Quiet output (minimal messages)
    ///
    /// Suppresses informational messages, showing only errors and
    /// warnings. Useful for automated scripts and CI environments.
    #[arg(short, long)]
    pub quiet: bool,

    /// Strict mode (treat warnings as errors)
    ///
    /// In strict mode, any warnings will cause the validation to fail.
    /// This is useful for CI/CD pipelines where warnings should block
    /// deployment or integration.
    #[arg(long)]
    pub strict: bool,

    /// Pre-render markdown templates and validate file references
    ///
    /// Validates that all markdown resources can be successfully rendered
    /// with their template syntax, and that all file references within the
    /// markdown content point to existing files. This catches template errors
    /// and broken cross-references before installation. Requires a lockfile
    /// to build the template context.
    ///
    /// When enabled:
    /// - Reads all markdown resources from worktrees/local paths
    /// - Attempts to render each with the current template context
    /// - Extracts and validates file references (markdown links and direct paths)
    /// - Reports syntax errors, missing variables, and broken file references
    /// - Returns non-zero exit code on validation failures
    ///
    /// This is useful for:
    /// - Catching template errors in CI/CD before deployment
    /// - Validating template syntax during development
    /// - Ensuring referential integrity of documentation
    /// - Testing template rendering without modifying the filesystem
    #[arg(long)]
    pub render: bool,
}

/// Output format options for validation results.
///
/// This enum defines the available output formats for validation results,
/// allowing users to choose between human-readable and machine-parseable formats.
///
/// # Variants
///
/// - [`Text`](OutputFormat::Text): Human-readable output with colors and formatting
/// - [`Json`](OutputFormat::Json): Structured JSON output for automation and integration
///
/// # Examples
///
/// ```rust,ignore
/// use agpm_cli::cli::validate::OutputFormat;
///
/// // For human consumption
/// let format = OutputFormat::Text;
///
/// // For automation/CI
/// let format = OutputFormat::Json;
/// ```
#[derive(Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text output with colors and formatting.
    ///
    /// This format provides:
    /// - Colored output (✓, ✗, ⚠ symbols)
    /// - Contextual messages and suggestions
    /// - Progress indicators during validation
    /// - Formatted error and warning messages
    Text,

    /// Structured JSON output for automation.
    ///
    /// This format provides:
    /// - Machine-parseable JSON structure
    /// - Consistent field names and types
    /// - All validation results in a single object
    /// - Suitable for CI/CD pipeline integration
    Json,
}
