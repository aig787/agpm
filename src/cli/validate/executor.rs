//! Validation execution logic and orchestration.

use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

use crate::manifest::find_manifest_with_optional;

use super::command::{OutputFormat, ValidateCommand};
use super::results::ValidationResults;
use super::validators;

impl ValidateCommand {
    /// Execute the validate command to check project configuration.
    ///
    /// This method orchestrates the complete validation process, performing
    /// checks according to the specified options and outputting results in
    /// the requested format.
    ///
    /// # Validation Process
    ///
    /// 1. **Manifest Loading**: Locates and loads the manifest file
    /// 2. **Basic Validation**: Checks syntax and required fields
    /// 3. **Extended Checks**: Performs optional network and dependency checks
    /// 4. **Result Compilation**: Aggregates all validation results
    /// 5. **Output Generation**: Formats and displays results
    /// 6. **Exit Code**: Returns success/failure based on results and strict mode
    ///
    /// # Validation Ordering
    ///
    /// Validations are performed in this order to provide early feedback:
    /// 1. Manifest structure and syntax
    /// 2. Dependency resolution (if `--resolve`)
    /// 3. Source accessibility (if `--sources`)
    /// 4. Local path validation (if `--paths`)
    /// 5. Lockfile consistency (if `--check-lock`)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if validation passes (or in strict mode, no warnings)
    /// - `Err(anyhow::Error)` if:
    ///   - Manifest file is not found
    ///   - Manifest has syntax errors
    ///   - Critical validation failures occur
    ///   - Strict mode is enabled and warnings are present
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use agpm_cli::cli::validate::{ValidateCommand, OutputFormat};
    ///
    /// let cmd = ValidateCommand {
    ///     file: None,
    ///     resolve: true,
    ///     check_lock: true,
    ///     sources: false,
    ///     paths: true,
    ///     format: OutputFormat::Text,
    ///     verbose: true,
    ///     quiet: false,
    ///     strict: false,
    ///     render: false,
    /// };
    /// // cmd.execute().await?;
    /// ```
    pub async fn execute(self) -> Result<()> {
        self.execute_with_manifest_path(None).await
    }

    /// Execute the validate command with an optional manifest path.
    ///
    /// This method performs validation of the agpm.toml manifest file and optionally
    /// the associated lockfile. It can validate manifest syntax, source availability,
    /// and dependency resolution consistency.
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Optional path to the agpm.toml file. If None, searches
    ///   for agpm.toml in current directory and parent directories. If the command
    ///   has a `file` field set, that takes precedence.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if validation passes
    /// - `Err(anyhow::Error)` if validation fails or manifest is invalid
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use agpm_cli::cli::validate::ValidateCommand;
    /// use std::path::PathBuf;
    ///
    /// let cmd = ValidateCommand {
    ///     file: None,
    ///     check_lock: false,
    ///     resolve: false,
    ///     format: OutputFormat::Text,
    ///     json: false,
    ///     paths: false,
    ///     fix: false,
    /// };
    ///
    /// cmd.execute_with_manifest_path(Some(PathBuf::from("./agpm.toml"))).await?;
    /// ```
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        // Find or use specified manifest file
        let manifest_path = if let Some(ref path) = self.file {
            PathBuf::from(path)
        } else {
            match find_manifest_with_optional(manifest_path) {
                Ok(path) => path,
                Err(e) => {
                    let error_msg =
                        "No agpm.toml found in current directory or any parent directory";

                    if matches!(self.format, OutputFormat::Json) {
                        let validation_results = ValidationResults {
                            valid: false,
                            errors: vec![error_msg.to_string()],
                            ..Default::default()
                        };
                        println!("{}", serde_json::to_string_pretty(&validation_results)?);
                        return Err(e);
                    } else if !self.quiet {
                        println!("{} {}", "✗".red(), error_msg);
                    }
                    return Err(e);
                }
            }
        };

        self.execute_from_path(manifest_path).await
    }

    /// Executes validation using a specific manifest path
    ///
    /// This method performs the same validation as `execute()` but accepts
    /// an explicit manifest path instead of searching for it.
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Path to the manifest file to validate
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if validation succeeds
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The manifest file doesn't exist
    /// - The manifest has syntax errors
    /// - Sources are invalid or unreachable (with --resolve flag)
    /// - Dependencies have conflicts
    pub async fn execute_from_path(self, manifest_path: PathBuf) -> Result<()> {
        // For consistency with execute(), require the manifest to exist
        if !manifest_path.exists() {
            let error_msg = format!("Manifest file {} not found", manifest_path.display());

            if matches!(self.format, OutputFormat::Json) {
                let validation_results = ValidationResults {
                    valid: false,
                    errors: vec![error_msg],
                    ..Default::default()
                };
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
            } else if !self.quiet {
                println!("{} {}", "✗".red(), error_msg);
            }

            return Err(anyhow::anyhow!("Manifest file {} not found", manifest_path.display()));
        }

        // Validation results for JSON output
        let mut validation_results = ValidationResults::default();
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // Load and validate manifest structure
        let manifest = validators::validate_manifest(
            &manifest_path,
            &self.format,
            self.verbose,
            self.quiet,
            &mut validation_results,
            &mut warnings,
            &mut errors,
        )
        .await?;

        // Check if dependencies can be resolved
        if self.resolve {
            validators::validate_dependencies(
                &manifest,
                &self.format,
                self.verbose,
                self.quiet,
                &mut validation_results,
                &mut warnings,
                &mut errors,
            )
            .await?;
        }

        // Check if sources are accessible
        if self.sources {
            validators::validate_sources(
                &manifest,
                &self.format,
                self.verbose,
                self.quiet,
                &mut validation_results,
                &mut warnings,
                &mut errors,
            )
            .await?;
        }

        // Check local file paths
        if self.paths {
            let mut ctx = validators::ValidationContext::new(
                &manifest,
                &self.format,
                self.verbose,
                self.quiet,
                &mut validation_results,
                &mut warnings,
                &mut errors,
            );
            validators::validate_paths(&mut ctx, &manifest_path).await?;
        }

        // Check lockfile consistency
        if self.check_lock {
            let project_dir = manifest_path.parent().unwrap();
            let mut ctx = validators::ValidationContext::new(
                &manifest,
                &self.format,
                self.verbose,
                self.quiet,
                &mut validation_results,
                &mut warnings,
                &mut errors,
            );
            validators::validate_lockfile(&mut ctx, project_dir).await?;
        }

        // Validate template rendering if requested
        if self.render {
            let project_dir = manifest_path.parent().unwrap();
            let mut ctx = validators::ValidationContext::new(
                &manifest,
                &self.format,
                self.verbose,
                self.quiet,
                &mut validation_results,
                &mut warnings,
                &mut errors,
            );
            validators::validate_templates(&mut ctx, project_dir).await?;
        }

        // Handle strict mode - treat warnings as errors
        if self.strict && !warnings.is_empty() {
            let error_msg = "Strict mode: Warnings treated as errors";
            errors.extend(warnings.clone());

            if matches!(self.format, OutputFormat::Json) {
                validation_results.valid = false;
                validation_results.errors = errors;
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
                return Err(anyhow::anyhow!("Strict mode validation failed"));
            } else if !self.quiet {
                println!("{} {}", "✗".red(), error_msg);
            }
            return Err(anyhow::anyhow!("Strict mode validation failed"));
        }

        // Set final validation status
        validation_results.valid = errors.is_empty();
        validation_results.errors = errors;
        validation_results.warnings = warnings;

        // Output results
        match self.format {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&validation_results)?);
            }
            OutputFormat::Text => {
                if !self.quiet && !validation_results.warnings.is_empty() {
                    for warning in &validation_results.warnings {
                        println!("⚠ Warning: {warning}");
                    }
                }
                // Individual validation steps already printed their success messages
            }
        }

        Ok(())
    }
}
