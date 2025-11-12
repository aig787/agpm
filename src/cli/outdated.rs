//! Check for available updates to installed dependencies.
//!
//! The `outdated` command analyzes the current project's lockfile against available
//! versions in Git repositories to identify dependencies that have newer versions
//! available. It provides both compatible updates (within version constraints) and
//! major updates (beyond current constraints).
//!
//! # Overview
//!
//! This module implements AGPM's update checking functionality, which:
//! - Compares installed versions against repository tags
//! - Respects semantic version constraints from the manifest
//! - Distinguishes between compatible and major updates
//! - Supports both table and JSON output formats
//! - Provides exit codes for CI/CD integration
//! - Handles Git repository caching and fetching
//!
//! # Command Usage
//!
//! ## Basic Usage
//!
//! ```bash
//! # Check all dependencies for updates
//! agpm outdated
//!
//! # Check specific dependencies
//! agpm outdated my-agent other-agent
//!
//! # Use cached data without fetching
//! agpm outdated --no-fetch
//!
//! # Exit with error code if updates available
//! agpm outdated --check
//!
//! # JSON output for scripting
//! agpm outdated --format json
//!
//! # Control parallelism
//! agpm outdated --max-parallel 5
//! ```
//!
//! ## Output Formats
//!
//! ### Table Format (Default)
//!
//! ```text
//! Package                        Current      Latest       Available    Tool
//! ─────────────────────────────────────────────────────────────────────────────
//! my-agent                       v1.0.0       v1.2.0       v2.0.0       claude-code
//! helper-agent                   v2.1.0       v2.1.0       v3.0.0       claude-code
//!
//! Summary:
//!   Total dependencies: 5
//!   2 dependencies have compatible updates
//!   2 dependencies have major updates available
//!   3 dependencies are up to date
//! ```
//!
//! ### JSON Format
//!
//! ```json
//! {
//!   "outdated": [
//!     {
//!       "name": "my-agent",
//!       "type": "agent",
//!       "source": "official",
//!       "tool": "claude-code",
//!       "current": "v1.0.0",
//!       "latest": "v1.2.0",
//!       "latest_available": "v2.0.0",
//!       "constraint": "^1.0.0",
//!       "has_update": true,
//!       "has_major_update": true
//!     }
//!   ],
//!   "summary": {
//!     "total": 5,
//!     "outdated": 2,
//!     "with_updates": 2,
//!     "with_major_updates": 2,
//!     "up_to_date": 3
//!   }
//! }
//! ```
//!
//! # Version Comparison Logic
//!
//! The outdated command performs sophisticated version analysis:
//!
//! 1. **Current Version**: The version installed and locked in `agpm.lock`
//! 2. **Latest Compatible**: The newest version that satisfies the manifest's version constraint
//! 3. **Latest Available**: The absolute newest version in the repository
//!
//! ## Update Types
//!
//! - **Compatible Update**: A newer version that satisfies the current constraint
//!   - `^1.0.0` constraint with `v1.2.0` available
//!   - Indicated by `has_update: true`
//!
//! - **Major Update**: A newer version that requires constraint changes
//!   - `^1.0.0` constraint with `v2.0.0` available
//!   - Indicated by `has_major_update: true`
//!
//! # Requirements
//!
//! - Requires an existing `agpm.lock` file (run `agpm install` first)
//! - Accesses Git repositories to fetch latest version information
//! - Supports both local cache and remote fetching modes
//!
//! # Integration with Other Commands
//!
//! The outdated command complements other AGPM commands:
//!
//! - [`crate::cli::install`] - Creates the required lockfile
//! - [`crate::cli::update`] - Updates dependencies to newer versions
//! - [`crate::cli::validate`] - Validates manifest and lockfile consistency
//!
//! # Error Handling
//!
//! Common error scenarios and their handling:
//!
//! - **Missing lockfile**: Returns error suggesting `agpm install`
//! - **Repository not cached**: Skips dependency or fails if `--no-fetch` used
//! - **Invalid version constraints**: Reports parsing errors with context
//! - **Network issues**: Graceful degradation with appropriate error messages
//!
//! # Examples
//!
//! ```rust,ignore
//! use agpm_cli::cli::outdated::OutdatedCommand;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Check all dependencies with default settings
//! let cmd = OutdatedCommand {
//!     dependencies: vec![],
//!     format: "table".to_string(),
//!     check: false,
//!     no_fetch: false,
//!     max_parallel: None,
//!     no_progress: false,
//! };
//!
//! cmd.execute_with_manifest_path(None).await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

use crate::cache::Cache;
use crate::core::OperationContext;
use crate::git::parse_git_url;
use crate::lockfile::LockedResource;
use crate::manifest::{Manifest, find_manifest_with_optional};
use crate::resolver::DependencyResolver;
use crate::utils::progress::{InstallationPhase, MultiPhaseProgress};

/// Command to check for available updates to installed dependencies.
///
/// The `OutdatedCommand` analyzes the current project's lockfile against available
/// versions in Git repositories to identify dependencies that have newer versions.
/// It distinguishes between compatible updates (within version constraints) and
/// major updates (beyond current constraints).
///
/// # Fields
///
/// * `dependencies` - Specific dependencies to check. If empty, checks all dependencies
/// * `format` - Output format: "table" for human-readable or "json" for machine parsing
/// * `check` - Exit with code 1 if any updates are available (useful for CI/CD)
/// * `no_fetch` - Use cached repository data without fetching latest from remote
/// * `max_parallel` - Limit concurrent Git operations (default: 2 × CPU cores)
/// * `no_progress` - Disable progress indicators (set automatically by global flag)
///
/// # Examples
///
/// ## Check All Dependencies
///
/// ```rust,ignore
/// use agpm_cli::cli::outdated::OutdatedCommand;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cmd = OutdatedCommand {
///     dependencies: vec![], // Check all
///     format: "table".to_string(),
///     check: false,
///     no_fetch: false,
///     max_parallel: None,
///     no_progress: false,
/// };
///
/// cmd.execute_with_manifest_path(None).await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Check Specific Dependencies with JSON Output
///
/// ```rust,ignore
/// use agpm_cli::cli::outdated::OutdatedCommand;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cmd = OutdatedCommand {
///     dependencies: vec!["my-agent".to_string(), "helper".to_string()],
///     format: "json".to_string(),
///     check: true, // Exit with error if updates found
///     no_fetch: false,
///     max_parallel: Some(5),
///     no_progress: true,
/// };
///
/// cmd.execute_with_manifest_path(None).await?;
/// # Ok(())
/// # }
/// ```
///
/// ## CI/CD Integration Example
///
/// ```rust,ignore
/// use agpm_cli::cli::outdated::OutdatedCommand;
///
/// # async fn ci_check() -> anyhow::Result<()> {
/// // This will exit with code 1 if any updates are available
/// let cmd = OutdatedCommand {
///     dependencies: vec![],
///     format: "json".to_string(), // Machine-readable output
///     check: true, // Fail build if updates exist
///     no_fetch: false, // Always check latest
///     max_parallel: Some(10), // Parallel for speed
///     no_progress: true, // No TTY in CI
/// };
///
/// cmd.execute_with_manifest_path(None).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Args)]
#[command(about = "Check for available updates to installed dependencies", author, version)]
pub struct OutdatedCommand {
    /// Specific dependencies to check (checks all if omitted)
    #[arg(value_name = "DEPENDENCY")]
    pub dependencies: Vec<String>,

    /// Output format (table or json)
    #[arg(long, default_value = "table", value_parser = ["table", "json"])]
    pub format: String,

    /// Exit with non-zero code if updates are available
    #[arg(long)]
    pub check: bool,

    /// Skip fetching latest from remote (use cached data)
    #[arg(long)]
    pub no_fetch: bool,

    /// Maximum parallel operations
    #[arg(long, value_name = "NUMBER")]
    pub max_parallel: Option<usize>,

    /// Don't show progress bars (automatically set by global option)
    #[arg(skip)]
    pub no_progress: bool,
}

/// Information about a dependency's update status.
///
/// This structure represents the complete update analysis for a single dependency,
/// comparing the currently installed version against available versions in the
/// Git repository while respecting semantic version constraints.
///
/// # Fields
///
/// * `name` - The dependency name as specified in the manifest
/// * `resource_type` - Type of resource: "agent", "snippet", "command", "script", "hook", or "mcp-server"
/// * `source` - Source repository name from the manifest's `[sources]` section
/// * `tool` - The target tool: "claude-code", "opencode", "agpm", or custom
/// * `current` - Currently installed version from the lockfile
/// * `latest` - Latest version that satisfies the manifest's version constraint
/// * `latest_available` - Absolute latest version available in the repository
/// * `constraint` - Version constraint from the manifest (e.g., "^1.0.0", "latest")
/// * `has_update` - True if a compatible update is available within the constraint
/// * `has_major_update` - True if a major update is available beyond the constraint
///
/// # JSON Schema
///
/// When serialized to JSON, the structure follows this schema:
///
/// ```json
/// {
///   "name": "string",
///   "type": "agent|snippet|command|script|hook|mcp-server",
///   "source": "string",
///   "tool": "claude-code|opencode|agpm|custom",
///   "current": "string (semver)",
///   "latest": "string (semver)",
///   "latest_available": "string (semver)",
///   "constraint": "string (version constraint)",
///   "has_update": "boolean",
///   "has_major_update": "boolean"
/// }
/// ```
///
/// # Examples
///
/// ## Compatible Update Available
///
/// ```json
/// {
///   "name": "code-reviewer",
///   "type": "agent",
///   "source": "official",
///   "tool": "claude-code",
///   "current": "v1.0.0",
///   "latest": "v1.2.0",
///   "latest_available": "v1.2.0",
///   "constraint": "^1.0.0",
///   "has_update": true,
///   "has_major_update": false
/// }
/// ```
///
/// ## Major Update Available
///
/// ```json
/// {
///   "name": "helper-agent",
///   "type": "agent",
///   "source": "community",
///   "tool": "claude-code",
///   "current": "v1.5.0",
///   "latest": "v1.5.0",
///   "latest_available": "v2.1.0",
///   "constraint": "^1.0.0",
///   "has_update": false,
///   "has_major_update": true
/// }
/// ```
///
/// ## Up to Date
///
/// ```json
/// {
///   "name": "build-script",
///   "type": "script",
///   "source": "tools",
///   "tool": "claude-code",
///   "current": "v2.1.0",
///   "latest": "v2.1.0",
///   "latest_available": "v2.1.0",
///   "constraint": "^2.0.0",
///   "has_update": false,
///   "has_major_update": false
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutdatedInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    pub source: String,
    pub tool: String,
    pub current: String,
    pub latest: String,           // Latest within constraint
    pub latest_available: String, // Absolute latest
    pub constraint: String,
    pub has_update: bool,       // Has update within constraint
    pub has_major_update: bool, // Has update beyond constraint
}

/// Summary statistics for the outdated analysis.
///
/// Provides aggregate information about the update status across all
/// dependencies in the project, useful for understanding the overall
/// maintenance status and planning update strategies.
///
/// # Fields
///
/// * `total` - Total number of dependencies analyzed
/// * `outdated` - Number of dependencies that were included in the analysis
/// * `with_updates` - Number of dependencies with compatible updates available
/// * `with_major_updates` - Number of dependencies with major updates available
/// * `up_to_date` - Number of dependencies that are current (total - outdated)
///
/// # Calculations
///
/// - `up_to_date = total - outdated`
/// - A dependency can have both `has_update` and `has_major_update` true
/// - `outdated` only includes dependencies that were actually analyzed (excludes local deps)
///
/// # JSON Schema
///
/// ```json
/// {
///   "total": "number (integer)",
///   "outdated": "number (integer)",
///   "with_updates": "number (integer)",
///   "with_major_updates": "number (integer)",
///   "up_to_date": "number (integer)"
/// }
/// ```
///
/// # Example
///
/// ```json
/// {
///   "total": 10,
///   "outdated": 3,
///   "with_updates": 2,
///   "with_major_updates": 1,
///   "up_to_date": 7
/// }
/// ```
///
/// This example shows:
/// - 10 total dependencies in the lockfile
/// - 3 dependencies were analyzed for updates (7 were skipped, likely local)
/// - 2 dependencies have compatible updates within their constraints
/// - 1 dependency has a major update available beyond its constraint
/// - 7 dependencies are up to date or were skipped
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutdatedSummary {
    pub total: usize,
    pub outdated: usize,
    pub with_updates: usize,
    pub with_major_updates: usize,
    pub up_to_date: usize,
}

/// Complete result of the outdated analysis.
///
/// This is the top-level structure returned by the outdated command when
/// using JSON output format. It combines the detailed per-dependency
/// information with aggregate summary statistics.
///
/// # Fields
///
/// * `outdated` - Detailed information for each dependency analyzed
/// * `summary` - Aggregate statistics across all dependencies
///
/// # JSON Schema
///
/// ```json
/// {
///   "outdated": [
///     {
///       "name": "string",
///       "type": "string",
///       "source": "string",
///       "tool": "string",
///       "current": "string",
///       "latest": "string",
///       "latest_available": "string",
///       "constraint": "string",
///       "has_update": "boolean",
///       "has_major_update": "boolean"
///     }
///   ],
///   "summary": {
///     "total": "number",
///     "outdated": "number",
///     "with_updates": "number",
///     "with_major_updates": "number",
///     "up_to_date": "number"
///   }
/// }
/// ```
///
/// # Usage
///
/// This structure is primarily used for JSON serialization when the
/// `--format json` flag is specified. It allows external tools and
/// scripts to programmatically analyze dependency update status.
///
/// ```rust,ignore
/// use agpm_cli::cli::outdated::{OutdatedResult, OutdatedInfo, OutdatedSummary};
/// use serde_json;
///
/// # fn example() -> anyhow::Result<()> {
/// let result = OutdatedResult {
///     outdated: vec![
///         // ... OutdatedInfo instances
///     ],
///     summary: OutdatedSummary {
///         total: 5,
///         outdated: 2,
///         with_updates: 1,
///         with_major_updates: 1,
///         up_to_date: 3,
///     },
/// };
///
/// let json = serde_json::to_string_pretty(&result)?;
/// println!("{}", json);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct OutdatedResult {
    pub outdated: Vec<OutdatedInfo>,
    pub summary: OutdatedSummary,
}

impl Default for OutdatedCommand {
    fn default() -> Self {
        Self {
            dependencies: vec![],
            format: "table".to_string(),
            check: false,
            no_fetch: false,
            max_parallel: None,
            no_progress: false,
        }
    }
}

impl OutdatedCommand {
    /// Execute the outdated command with an optional manifest path.
    ///
    /// This is the main entry point for the outdated command. It locates the
    /// manifest file (either from the provided path or by searching the current
    /// directory hierarchy) and delegates to [`Self::execute_from_path`].
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Optional path to the `agpm.toml` file. If `None`,
    ///   searches the current directory and parent directories for the manifest.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the analysis completes successfully, or an error if:
    /// - The manifest file cannot be found or parsed
    /// - The lockfile is missing or invalid
    /// - Git repositories cannot be accessed
    /// - Version parsing fails
    ///
    /// # Exit Codes
    ///
    /// When `self.check` is true, the process will exit with code 1 if any
    /// updates are available (either compatible or major updates).
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::cli::outdated::OutdatedCommand;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // Use default manifest discovery
    /// let cmd = OutdatedCommand::default();
    /// cmd.execute_with_manifest_path(None).await?;
    ///
    /// // Use specific manifest path
    /// let manifest_path = Some(PathBuf::from("/path/to/agpm.toml"));
    /// cmd.execute_with_manifest_path(manifest_path).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`anyhow::Error`] for various failure conditions:
    /// - `"No manifest found"` - No `agpm.toml` in current or parent directories
    /// - `"Failed to load manifest"` - Manifest exists but has syntax errors
    /// - `"No lockfile found"` - Missing `agpm.lock` file (run `agpm install` first)
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        let manifest_path = find_manifest_with_optional(manifest_path)?;
        self.execute_from_path(manifest_path).await
    }

    /// Execute the outdated command with a specific manifest path.
    ///
    /// Performs the complete outdated analysis workflow:
    /// 1. Loads the manifest and validates the lockfile exists
    /// 2. Initializes the cache and dependency resolver
    /// 3. Optionally syncs Git repositories to fetch latest versions
    /// 4. Analyzes each dependency for available updates
    /// 5. Generates summary statistics and displays results
    /// 6. Exits with appropriate code if `--check` is enabled
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Absolute path to the `agpm.toml` manifest file
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful completion. If `self.check` is true and
    /// updates are available, the process will exit with code 1 instead of
    /// returning normally.
    ///
    /// # Errors
    ///
    /// Returns errors for:
    /// - Invalid or missing manifest/lockfile
    /// - Cache initialization failures
    /// - Git repository access issues
    /// - Version parsing or constraint resolution errors
    ///
    /// # Progress Reporting
    ///
    /// Shows progress through multiple phases when `!self.no_progress`:
    /// 1. "Syncing sources" - Fetching latest repository data
    /// 2. "Checking versions" - Analyzing each dependency
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::cli::outdated::OutdatedCommand;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cmd = OutdatedCommand {
    ///     dependencies: vec![],
    ///     format: "table".to_string(),
    ///     check: false,
    ///     no_fetch: false,
    ///     max_parallel: None,
    ///     no_progress: false,
    /// };
    ///
    /// let manifest_path = PathBuf::from("./agpm.toml");
    /// cmd.execute_from_path(manifest_path).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_from_path(self, manifest_path: PathBuf) -> Result<()> {
        info!("Checking for outdated dependencies");

        // 1. Load manifest and lockfile
        let manifest = Manifest::load(&manifest_path)
            .with_context(|| format!("Failed to load manifest from {manifest_path:?}"))?;

        let project_dir =
            manifest_path.parent().ok_or_else(|| anyhow::anyhow!("Invalid manifest path"))?;
        let lockfile_path = manifest_path.with_file_name("agpm.lock");

        // Check if lockfile exists first - the outdated command requires it
        if !lockfile_path.exists() {
            return Err(anyhow::anyhow!(
                "No lockfile found at {lockfile_path:?}. Run 'agpm install' first to create a lockfile."
            ));
        }

        // Create command context for enhanced lockfile loading
        let command_context =
            crate::cli::common::CommandContext::new(manifest.clone(), project_dir.to_path_buf())?;

        // Use enhanced lockfile loading with automatic regeneration
        let lockfile = match command_context.load_lockfile_with_regeneration(true, "outdated")? {
            Some(lockfile) => lockfile,
            None => {
                return Err(anyhow::anyhow!(
                    "Lockfile was invalid and has been removed. Run 'agpm install' to regenerate it first."
                ));
            }
        };

        // 2. Initialize cache and resolver
        let cache = Cache::new().context("Failed to initialize cache")?;

        // 3. Create resolver for version resolution
        let mut resolver = DependencyResolver::new(manifest.clone(), cache.clone())
            .await
            .context("Failed to create dependency resolver")?;

        // Create operation context for warning deduplication
        let operation_context = Arc::new(OperationContext::new());
        resolver.set_operation_context(operation_context);

        // 4. Pre-sync sources if not skipped
        let progress = if self.no_progress {
            None
        } else {
            Some(MultiPhaseProgress::new(!self.no_progress))
        };

        if !self.no_fetch {
            if let Some(ref progress) = progress {
                progress.start_phase(InstallationPhase::SyncingSources, Some("Syncing sources"));
            }

            // Convert dependencies to the format expected by pre_sync_sources
            let deps: Vec<(String, crate::manifest::ResourceDependency)> = manifest
                .all_dependencies()
                .into_iter()
                .map(|(name, dep)| (name.to_string(), dep.clone()))
                .collect();

            // TODO: Thread progress parameter through outdated command
            resolver.pre_sync_sources(&deps, None).await.context("Failed to sync sources")?;

            // Progress is automatically handled by MultiPhaseProgress
        }

        // 5. Check each dependency for updates using the same resolution path as `update`
        if let Some(ref progress) = progress {
            progress
                .start_phase(InstallationPhase::ResolvingDependencies, Some("Checking versions"));
        }

        // Use DependencyResolver.update() to get what would be updated
        // This ensures consistent behavior with the `update` command
        let deps_to_check = if self.dependencies.is_empty() {
            None
        } else {
            Some(self.dependencies.clone())
        };

        let updated_lockfile = resolver.update(&lockfile, deps_to_check.clone(), None).await?;

        // Progress is automatically handled by MultiPhaseProgress

        // 6. Compare lockfiles to detect what changed and analyze versions
        let mut outdated_deps = Vec::new();

        for new_entry in updated_lockfile.all_resources() {
            // Filter by specific dependencies if requested
            if !self.dependencies.is_empty() && !self.dependencies.contains(&new_entry.name) {
                continue;
            }

            // Find corresponding old entry using display_name for backward compatibility
            if let Some((_, old_entry)) =
                crate::core::ResourceIterator::find_resource_by_name_and_source(
                    &lockfile,
                    new_entry.display_name(),
                    new_entry.source.as_deref(),
                )
            {
                if let Some(outdated_info) = self
                    .analyze_update(
                        &new_entry.name,
                        old_entry,
                        new_entry,
                        &manifest,
                        &cache,
                        &resolver,
                    )
                    .await?
                {
                    // Only add to list if there's actually an update or major update available
                    // This ensures "All dependencies are up to date!" shows when everything is current
                    if outdated_info.has_update || outdated_info.has_major_update {
                        outdated_deps.push(outdated_info);
                    }
                }
            }
        }

        // 7. Calculate summary
        let summary = self.calculate_summary(&outdated_deps, lockfile.all_resources().len());

        // 8. Display results
        self.display_results(&outdated_deps, &summary)?;

        // 9. Exit with appropriate code
        if self.check && outdated_deps.iter().any(|d| d.has_update || d.has_major_update) {
            std::process::exit(1);
        }

        Ok(())
    }

    /// Analyze a single dependency update by comparing old and new lockfile entries.
    ///
    /// Performs the core update analysis for one dependency by:
    /// 1. Finding the dependency specification in the manifest
    /// 2. Getting version information from both old and new lockfile entries
    /// 3. Retrieving available versions to find the absolute latest
    /// 4. Comparing current, latest compatible, and latest available versions
    /// 5. Determining update availability within and beyond constraints
    ///
    /// # Arguments
    ///
    /// * `name` - The dependency name from the lockfile
    /// * `old_entry` - The current locked resource from existing lockfile
    /// * `new_entry` - The updated locked resource from resolver.update()
    /// * `manifest` - The project manifest containing dependency specifications
    /// * `cache` - Cache instance for accessing Git repositories
    /// * `resolver` - Dependency resolver for version operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(OutdatedInfo))` if the dependency was analyzed successfully,
    /// `Ok(None)` if the dependency should be skipped (local dependency, not found
    /// in manifest, or repository not cached), or an error if analysis fails.
    ///
    /// # Skipped Dependencies
    ///
    /// Dependencies are skipped when:
    /// - Not found in the manifest (orphaned lockfile entry)
    /// - Local path dependency (no version to check)
    /// - Repository not present in cache and `--no-fetch` is enabled
    ///
    /// # Version Analysis
    ///
    /// The method uses the resolver.update() result for the latest compatible version,
    /// then queries the repository to find the absolute latest version available.
    /// This ensures consistent behavior with the `update` command.
    ///
    /// # Errors
    ///
    /// Returns errors for:
    /// - Git repository access failures
    /// - Version parsing errors
    /// - Missing source repository in manifest
    async fn analyze_update(
        &self,
        name: &str,
        old_entry: &LockedResource,
        new_entry: &LockedResource,
        manifest: &Manifest,
        cache: &Cache,
        resolver: &DependencyResolver,
    ) -> Result<Option<OutdatedInfo>> {
        // Find the dependency in the manifest
        let dep = manifest.find_dependency(name);
        if dep.is_none() {
            debug!("Dependency {} not found in manifest", name);
            return Ok(None);
        }
        let dep = dep.unwrap();

        // Skip local dependencies
        if dep.is_local() {
            debug!("Skipping local dependency: {}", name);
            return Ok(None);
        }

        // Get the source
        let source_name =
            dep.get_source().ok_or_else(|| anyhow::anyhow!("Dependency {name} has no source"))?;

        // Get the version constraint
        let constraint_str = dep
            .get_version()
            .map_or_else(|| "latest".to_string(), std::string::ToString::to_string);

        // The new_entry version is the latest compatible (resolved by DependencyResolver.update())
        // The old_entry version is the currently installed version

        // Now we need to find the absolute latest available version (may be beyond constraints)
        let source_url = manifest
            .sources
            .get(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source {source_name} not found in manifest"))?;

        let (owner, repo) = parse_git_url(source_url)
            .unwrap_or_else(|_| ("unknown".to_string(), source_name.to_string()));

        let bare_repo_path =
            cache.get_cache_location().join("sources").join(format!("{owner}_{repo}.git"));

        if !bare_repo_path.exists() {
            debug!("Repository not found in cache at {:?}, skipping", bare_repo_path);
            return Ok(None);
        }

        let available_versions = resolver.get_available_versions(&bare_repo_path).await?;

        // Filter to semantic versions
        let mut semver_versions: Vec<semver::Version> = available_versions
            .iter()
            .filter_map(|v| {
                let version_str = v.trim_start_matches('v');
                semver::Version::parse(version_str).ok()
            })
            .collect();

        // Sort versions (latest first)
        semver_versions.sort_by(|a, b| b.cmp(a));

        if semver_versions.is_empty() {
            debug!("No semantic versions found for {}", name);
            return Ok(None);
        }

        // Determine resource type from manifest
        let resource_type = if manifest.agents.contains_key(name) {
            "agent"
        } else if manifest.snippets.contains_key(name) {
            "snippet"
        } else if manifest.commands.contains_key(name) {
            "command"
        } else if manifest.scripts.contains_key(name) {
            "script"
        } else if manifest.hooks.contains_key(name) {
            "hook"
        } else if manifest.mcp_servers.contains_key(name) {
            "mcp-server"
        } else {
            "unknown"
        };

        // Compare old vs new to detect updates
        let current_version = old_entry.version.clone().unwrap_or_else(|| "unknown".to_string());
        let latest_compatible = new_entry.version.clone().unwrap_or_else(|| "unknown".to_string());

        // Format absolute latest with 'v' prefix if original had it
        let latest_available = if old_entry.version.as_ref().is_some_and(|s| s.starts_with('v')) {
            format!("v{}", semver_versions[0])
        } else {
            semver_versions[0].to_string()
        };

        // Determine if updates are available
        let has_update = old_entry.resolved_commit != new_entry.resolved_commit;
        let has_major_update = latest_compatible != latest_available;

        Ok(Some(OutdatedInfo {
            name: name.to_string(),
            resource_type: resource_type.to_string(),
            source: source_name.to_string(),
            tool: old_entry.tool.clone().unwrap_or_else(|| "claude-code".to_string()),
            current: current_version,
            latest: latest_compatible,
            latest_available,
            constraint: constraint_str,
            has_update,
            has_major_update,
        }))
    }

    /// Calculate aggregate summary statistics from outdated analysis results.
    ///
    /// Processes the list of analyzed dependencies to generate summary statistics
    /// about update availability across the entire project. This provides a
    /// high-level view of the project's maintenance status.
    ///
    /// # Arguments
    ///
    /// * `outdated` - List of dependencies that were analyzed for updates
    /// * `total` - Total number of dependencies in the lockfile
    ///
    /// # Returns
    ///
    /// Returns an [`OutdatedSummary`] containing:
    /// - Total dependencies in the lockfile
    /// - Number of dependencies analyzed (excluding local/skipped)
    /// - Number with compatible updates available
    /// - Number with major updates available
    /// - Number that are up to date
    ///
    /// # Calculation Logic
    ///
    /// - `outdated = outdated.len()` (dependencies that were analyzed)
    /// - `with_updates = count(has_update == true)`
    /// - `with_major_updates = count(has_major_update == true)`
    /// - `up_to_date = total - outdated` (includes skipped dependencies)
    ///
    /// Note that a dependency can have both `has_update` and `has_major_update`
    /// set to true when both compatible and major updates are available.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use agpm_cli::cli::outdated::{OutdatedCommand, OutdatedInfo};
    /// # fn example() {
    /// let cmd = OutdatedCommand::default();
    /// let outdated_deps = vec![
    ///     // Dependencies with various update statuses...
    /// ];
    ///
    /// let summary = cmd.calculate_summary(&outdated_deps, 10);
    /// println!("Total: {}, Outdated: {}, Updates: {}",
    ///     summary.total, summary.outdated, summary.with_updates);
    /// # }
    /// ```
    fn calculate_summary(&self, outdated: &[OutdatedInfo], total: usize) -> OutdatedSummary {
        let with_updates = outdated.iter().filter(|d| d.has_update).count();
        let with_major_updates = outdated.iter().filter(|d| d.has_major_update).count();
        let outdated_count = outdated.len();
        let up_to_date = total - outdated_count;

        OutdatedSummary {
            total,
            outdated: outdated_count,
            with_updates,
            with_major_updates,
            up_to_date,
        }
    }

    /// Display the outdated analysis results in the requested format.
    ///
    /// Routes the output to either table or JSON format based on the
    /// `self.format` setting. This is the final step in the outdated
    /// command workflow.
    ///
    /// # Arguments
    ///
    /// * `outdated` - List of dependencies with update information
    /// * `summary` - Aggregate statistics across all dependencies
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful output, or an error if JSON
    /// serialization fails.
    ///
    /// # Output Formats
    ///
    /// - `"table"` - Human-readable table with colored output
    /// - `"json"` - Machine-readable JSON for scripting
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use agpm_cli::cli::outdated::{OutdatedCommand, OutdatedInfo, OutdatedSummary};
    /// # fn example() -> anyhow::Result<()> {
    /// let cmd = OutdatedCommand {
    ///     format: "json".to_string(),
    ///     // ... other fields
    ///     # dependencies: vec![],
    ///     # check: false,
    ///     # no_fetch: false,
    ///     # max_parallel: None,
    ///     # no_progress: false,
    /// };
    ///
    /// let outdated = vec![];
    /// let summary = OutdatedSummary {
    ///     total: 5,
    ///     outdated: 0,
    ///     with_updates: 0,
    ///     with_major_updates: 0,
    ///     up_to_date: 5,
    /// };
    ///
    /// cmd.display_results(&outdated, &summary)?;
    /// # Ok(())
    /// # }
    /// ```
    fn display_results(&self, outdated: &[OutdatedInfo], summary: &OutdatedSummary) -> Result<()> {
        match self.format.as_str() {
            "json" => self.display_json(outdated, summary),
            _ => self.display_table(outdated, summary),
        }
    }

    /// Display results in JSON format for machine consumption.
    ///
    /// Serializes the complete analysis results as pretty-printed JSON
    /// to stdout. This format is ideal for CI/CD pipelines, scripts,
    /// and other automated tooling that needs to process update information.
    ///
    /// # Arguments
    ///
    /// * `outdated` - List of dependencies with detailed update information
    /// * `summary` - Aggregate summary statistics
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful serialization and output, or an
    /// error if JSON serialization fails.
    ///
    /// # Output Format
    ///
    /// The JSON output follows the [`OutdatedResult`] schema with
    /// pretty-printing for readability:
    ///
    /// ```json
    /// {
    ///   "outdated": [
    ///     {
    ///       "name": "my-agent",
    ///       "type": "agent",
    ///       "source": "official",
    ///       "tool": "claude-code",
    ///       "current": "v1.0.0",
    ///       "latest": "v1.2.0",
    ///       "latest_available": "v2.0.0",
    ///       "constraint": "^1.0.0",
    ///       "has_update": true,
    ///       "has_major_update": true
    ///     }
    ///   ],
    ///   "summary": {
    ///     "total": 5,
    ///     "outdated": 1,
    ///     "with_updates": 1,
    ///     "with_major_updates": 1,
    ///     "up_to_date": 4
    ///   }
    /// }
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use agpm_cli::cli::outdated::{OutdatedCommand, OutdatedInfo, OutdatedSummary};
    /// # fn example() -> anyhow::Result<()> {
    /// let cmd = OutdatedCommand::default();
    /// let outdated = vec![];
    /// let summary = OutdatedSummary {
    ///     total: 5,
    ///     outdated: 0,
    ///     with_updates: 0,
    ///     with_major_updates: 0,
    ///     up_to_date: 5,
    /// };
    ///
    /// cmd.display_json(&outdated, &summary)?;
    /// # Ok(())
    /// # }
    /// ```
    fn display_json(&self, outdated: &[OutdatedInfo], summary: &OutdatedSummary) -> Result<()> {
        let result = OutdatedResult {
            outdated: outdated.to_vec(),
            summary: summary.clone(),
        };

        println!("{}", serde_json::to_string_pretty(&result)?);
        Ok(())
    }

    /// Display results in human-readable table format.
    ///
    /// Renders the analysis results as a formatted table with colored output
    /// to highlight different types of updates. This format is optimized for
    /// human consumption and provides a quick visual overview of update status.
    ///
    /// # Arguments
    ///
    /// * `outdated` - List of dependencies that were analyzed for updates
    /// * `summary` - Aggregate statistics for the summary section
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful display.
    ///
    /// # Color Coding
    ///
    /// The table uses colors to indicate update status:
    /// - **Yellow** package names: Updates available
    /// - **Green** latest versions: Compatible updates within constraints
    /// - **Cyan** available versions: Major updates beyond constraints
    /// - **Normal** text: No updates or up-to-date
    ///
    /// # Output Format
    ///
    /// ## With Updates Available
    ///
    /// ```text
    /// Package                        Current      Latest       Available    Tool
    /// ─────────────────────────────────────────────────────────────────────────────
    /// my-agent                       v1.0.0       v1.2.0       v2.0.0       claude-code
    /// helper-script                  v2.1.0       v2.1.0       v3.0.0       claude-code
    ///
    /// Summary:
    ///   Total dependencies: 5
    ///   2 dependencies have compatible updates
    ///   2 dependencies have major updates available
    ///   3 dependencies are up to date
    /// ```
    ///
    /// ## All Up to Date
    ///
    /// ```text
    /// All dependencies are up to date!
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # use agpm_cli::cli::outdated::{OutdatedCommand, OutdatedInfo, OutdatedSummary};
    /// # fn example() -> anyhow::Result<()> {
    /// let cmd = OutdatedCommand::default();
    /// let outdated = vec![];
    /// let summary = OutdatedSummary {
    ///     total: 5,
    ///     outdated: 0,
    ///     with_updates: 0,
    ///     with_major_updates: 0,
    ///     up_to_date: 5,
    /// };
    ///
    /// cmd.display_table(&outdated, &summary)?;
    /// // Prints: "All dependencies are up to date!"
    /// # Ok(())
    /// # }
    /// ```
    fn display_table(&self, outdated: &[OutdatedInfo], summary: &OutdatedSummary) -> Result<()> {
        if outdated.is_empty() {
            println!("{}", "All dependencies are up to date!".green());
            return Ok(());
        }

        // Print header
        println!(
            "\n{:<30} {:<12} {:<12} {:<12} {:<15}",
            "Package".bold(),
            "Current".bold(),
            "Latest".bold(),
            "Available".bold(),
            "Tool".bold()
        );
        println!("{}", "─".repeat(85));

        // Print each dependency
        for dep in outdated {
            let name = if dep.has_update || dep.has_major_update {
                dep.name.yellow()
            } else {
                dep.name.normal()
            };

            let latest = if dep.has_update {
                dep.latest.green()
            } else {
                dep.latest.normal()
            };

            let available = if dep.has_major_update {
                dep.latest_available.cyan()
            } else {
                dep.latest_available.normal()
            };

            println!(
                "{:<30} {:<12} {:<12} {:<12} {:<15}",
                name,
                dep.current,
                latest,
                available,
                dep.tool.bright_black()
            );
        }

        // Print summary
        println!("\n{}", "Summary:".bold());
        println!("  Total dependencies: {}", summary.total);
        if summary.with_updates > 0 {
            println!(
                "  {} dependencies have compatible updates",
                summary.with_updates.to_string().green()
            );
        }
        if summary.with_major_updates > 0 {
            println!(
                "  {} dependencies have major updates available",
                summary.with_major_updates.to_string().cyan()
            );
        }
        println!("  {} dependencies are up to date", summary.up_to_date.to_string().green());

        Ok(())
    }
}
