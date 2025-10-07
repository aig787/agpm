//! Install Claude Code resources from manifest dependencies.
//!
//! This module provides the `install` command which reads dependencies from the
//! `agpm.toml` manifest file, resolves them, and installs the resource files
//! to the project directory. The command supports both fresh installations and
//! updates to existing installations with advanced parallel processing capabilities.
//!
//! # Features
//!
//! - **Dependency Resolution**: Resolves all dependencies defined in the manifest
//! - **Transitive Dependencies**: Automatically discovers and installs dependencies declared in resource files
//! - **Lockfile Management**: Generates and maintains `agpm.lock` for reproducible builds
//! - **Worktree-Based Parallel Installation**: Uses Git worktrees for safe concurrent resource installation
//! - **Multi-Phase Progress Tracking**: Shows detailed progress with phase transitions and real-time updates
//! - **Resource Validation**: Validates markdown files and content during installation
//! - **Cache Support**: Advanced cache with instance-level optimizations and worktree management
//! - **Concurrency Control**: User-configurable parallelism via `--max-parallel` flag
//! - **Cycle Detection**: Prevents circular dependency loops in transitive dependency graphs
//!
//! # Examples
//!
//! Install all dependencies from manifest:
//! ```bash
//! agpm install
//! ```
//!
//! Force reinstall all dependencies:
//! ```bash
//! agpm install --force
//! ```
//!
//! Install without creating lockfile:
//! ```bash
//! agpm install --no-lock
//! ```
//!
//! Use frozen lockfile (CI/production):
//! ```bash
//! agpm install --frozen
//! ```
//!
//! Disable cache and clone fresh:
//! ```bash
//! agpm install --no-cache
//! ```
//!
//! Install only direct dependencies (skip transitive):
//! ```bash
//! agpm install --no-transitive
//! ```
//!
//! Preview installation without making changes:
//! ```bash
//! agpm install --dry-run
//! ```
//!
//! # Installation Process
//!
//! 1. **Manifest Loading**: Reads `agpm.toml` to understand dependencies
//! 2. **Source Synchronization**: Clones/fetches Git repositories for all sources
//! 3. **Dependency Resolution**: Resolves versions and creates dependency graph
//! 4. **Transitive Discovery**: Extracts dependencies from resource files (YAML/JSON metadata)
//! 5. **Cycle Detection**: Validates dependency graph for circular references
//! 6. **Worktree Preparation**: Pre-creates Git worktrees for optimal parallel access
//! 7. **Parallel Resource Installation**: Installs resources concurrently using isolated worktrees
//! 8. **Progress Coordination**: Updates multi-phase progress tracking throughout installation
//! 9. **Configuration Updates**: Updates hooks and MCP server configurations as needed
//! 10. **Lockfile Generation**: Creates or updates `agpm.lock` with checksums and metadata
//! 11. **Artifact Cleanup**: Removes old artifacts from removed or relocated dependencies
//!
//! # Error Conditions
//!
//! - No manifest file found in project
//! - Invalid manifest syntax or structure
//! - Dependency resolution conflicts
//! - Circular dependency loops detected
//! - Invalid transitive dependency metadata (malformed YAML/JSON)
//! - Network or Git access issues
//! - File system permissions or disk space issues
//! - Invalid resource file format
//!
//! # Performance
//!
//! The install command is optimized for maximum performance:
//! - **Worktree-based parallelism**: Each dependency gets its own isolated Git worktree
//! - **Instance-level caching**: Optimized worktree reuse within command execution
//! - **Configurable concurrency**: `--max-parallel` flag controls dependency-level parallelism
//! - **Pre-warming strategy**: Creates all needed worktrees upfront for optimal parallel access
//! - **Atomic file operations**: Safe, corruption-resistant file installation
//! - **Multi-phase progress**: Real-time progress updates with phase transitions

use anyhow::{Context, Result};
use clap::Args;
use std::path::{Path, PathBuf};

use crate::cache::Cache;
use crate::core::ResourceIterator;
use crate::installer::update_gitignore;
use crate::lockfile::LockFile;
use crate::manifest::{ResourceDependency, find_manifest_with_optional};
use crate::resolver::DependencyResolver;

/// Command to install Claude Code resources from manifest dependencies.
///
/// This command reads the project's `agpm.toml` manifest file, resolves all dependencies,
/// and installs the resource files to the appropriate directories. It generates or updates
/// a `agpm.lock` lockfile to ensure reproducible installations.
///
/// # Behavior
///
/// 1. Locates and loads the project manifest (`agpm.toml`)
/// 2. Resolves dependencies using the dependency resolver
/// 3. Downloads or updates Git repository sources as needed
/// 4. Installs resource files to target directories
/// 5. Generates or updates the lockfile (`agpm.lock`)
/// 6. Provides progress feedback during installation
///
/// # Examples
///
/// ```rust,ignore
/// use agpm::cli::install::InstallCommand;
///
/// // Standard installation
/// let cmd = InstallCommand {
///     no_lock: false,
///     frozen: false,
///     no_cache: false,
///     max_parallel: None,
///     quiet: false,
/// };
///
/// // CI/Production installation (frozen lockfile)
/// let cmd = InstallCommand {
///     no_lock: false,
///     frozen: true,
///     no_cache: false,
///     max_parallel: Some(2),
///     quiet: false,
/// };
/// ```
#[derive(Args)]
pub struct InstallCommand {
    /// Don't write lockfile after installation
    ///
    /// Prevents the command from creating or updating the `agpm.lock` file.
    /// This is useful for development scenarios where you don't want to
    /// commit lockfile changes.
    #[arg(long)]
    no_lock: bool,

    /// Verify checksums from existing lockfile
    ///
    /// Uses the existing lockfile as-is without updating dependencies.
    /// This mode ensures reproducible installations and is recommended
    /// for CI/CD pipelines and production deployments.
    #[arg(long)]
    frozen: bool,

    /// Don't use cache, clone fresh repositories
    ///
    /// Disables the local Git repository cache and clones repositories
    /// to temporary locations. This increases installation time but ensures
    /// completely fresh downloads.
    #[arg(long)]
    no_cache: bool,

    /// Maximum number of parallel operations (default: max(10, 2 × CPU cores))
    ///
    /// Controls the level of parallelism during installation. The default value
    /// is calculated as `max(10, 2 × CPU cores)` to provide good performance
    /// while avoiding resource exhaustion. Higher values can speed up installation
    /// of many dependencies but may strain system resources or hit API rate limits.
    ///
    /// # Performance Impact
    ///
    /// - **Low values (1-4)**: Conservative approach, slower but more reliable
    /// - **Default values (10-16)**: Balanced performance for most systems
    /// - **High values (>20)**: May overwhelm system resources or trigger rate limits
    ///
    /// # Examples
    ///
    /// - `--max-parallel 1`: Sequential installation (debugging)
    /// - `--max-parallel 4`: Conservative parallel installation
    /// - `--max-parallel 20`: Aggressive parallel installation (powerful systems)
    #[arg(long, value_name = "NUM")]
    max_parallel: Option<usize>,

    /// Suppress non-essential output
    ///
    /// When enabled, only errors and essential information will be printed.
    /// Progress bars and status messages will be hidden.
    #[arg(short, long)]
    quiet: bool,

    /// Disable progress bars (for programmatic use, not exposed as CLI arg)
    #[arg(skip)]
    pub no_progress: bool,

    /// Don't resolve transitive dependencies
    ///
    /// When enabled, only direct dependencies from the manifest will be installed.
    /// Transitive dependencies declared within resource files (via YAML frontmatter
    /// or JSON fields) will be ignored. This can be useful for faster installations
    /// when you know transitive dependencies are already satisfied or for debugging
    /// dependency issues.
    #[arg(long)]
    no_transitive: bool,

    /// Preview installation without making changes
    ///
    /// Shows what would be installed, including new dependencies and lockfile changes,
    /// but doesn't modify any files. Useful for reviewing changes before applying them,
    /// especially in CI/CD pipelines to detect when dependencies would change.
    ///
    /// When enabled:
    /// - Resolves all dependencies normally
    /// - Shows what resources would be installed
    /// - Shows lockfile changes (new entries, version updates)
    /// - Does NOT write the lockfile
    /// - Does NOT install any resources
    /// - Does NOT update .gitignore
    ///
    /// Exit codes:
    /// - 0: No changes would be made
    /// - 1: Changes would be made (useful for CI checks)
    #[arg(long)]
    dry_run: bool,
}

impl InstallCommand {
    /// Creates a default `InstallCommand` for programmatic use.
    ///
    /// This constructor creates an `InstallCommand` with standard settings:
    /// - Lockfile generation enabled
    /// - Fresh dependency resolution (not frozen)
    /// - Cache enabled for performance
    /// - Default parallelism (max(10, 2 × CPU cores))
    /// - Progress output enabled
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::cli::install::InstallCommand;
    ///
    /// let cmd = InstallCommand::new();
    /// // cmd can now be executed with execute_from_path()
    /// ```
    #[allow(dead_code)]
    pub const fn new() -> Self {
        Self {
            no_lock: false,
            frozen: false,
            no_cache: false,
            max_parallel: None,
            quiet: false,
            no_progress: false,
            no_transitive: false,
            dry_run: false,
        }
    }

    /// Creates an `InstallCommand` configured for quiet operation.
    ///
    /// This constructor creates an `InstallCommand` with quiet mode enabled,
    /// which suppresses progress bars and non-essential output. Useful for
    /// automated scripts or CI/CD environments where minimal output is desired.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::cli::install::InstallCommand;
    ///
    /// let cmd = InstallCommand::new_quiet();
    /// // cmd will execute without progress bars or status messages
    /// ```
    #[allow(dead_code)]
    pub const fn new_quiet() -> Self {
        Self {
            no_lock: false,
            frozen: false,
            no_cache: false,
            max_parallel: None,
            quiet: true,
            no_progress: true,
            no_transitive: false,
            dry_run: false,
        }
    }

    /// Executes the install command with automatic manifest discovery.
    ///
    /// This method provides convenient manifest file discovery, searching for
    /// `agpm.toml` in the current directory and parent directories if no specific
    /// path is provided. It's the standard entry point for CLI usage.
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Optional explicit path to `agpm.toml`. If `None`,
    ///   the method searches for `agpm.toml` starting from the current directory
    ///   and walking up the directory tree.
    ///
    /// # Manifest Discovery
    ///
    /// When `manifest_path` is `None`, the search process:
    /// 1. Checks current directory for `agpm.toml`
    /// 2. Walks up parent directories until `agpm.toml` is found
    /// 3. Stops at filesystem root if no manifest found
    /// 4. Returns an error with helpful guidance if no manifest exists
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm::cli::install::InstallCommand;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cmd = InstallCommand::new();
    ///
    /// // Auto-discover manifest in current directory or parents
    /// cmd.execute_with_manifest_path(None).await?;
    ///
    /// // Use specific manifest file
    /// cmd.execute_with_manifest_path(Some(PathBuf::from("./my-project/agpm.toml"))).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No `agpm.toml` file found in search path
    /// - Specified manifest path doesn't exist
    /// - Manifest file contains invalid TOML syntax
    /// - Dependencies cannot be resolved
    /// - Installation process fails
    ///
    /// # Error Messages
    ///
    /// When no manifest is found, the error includes helpful guidance:
    /// ```text
    /// No agpm.toml found in current directory or any parent directory.
    ///
    /// To get started, create a agpm.toml file with your dependencies:
    ///
    /// [sources]
    /// official = "https://github.com/example-org/agpm-official.git"
    ///
    /// [agents]
    /// my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
    /// ```
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        // Find manifest file
        let manifest_path = find_manifest_with_optional(manifest_path).with_context(|| {
            "No agpm.toml found in current directory or any parent directory.\n\n\
            To get started, create a agpm.toml file with your dependencies:\n\n\
            [sources]\n\
            official = \"https://github.com/example-org/agpm-official.git\"\n\n\
            [agents]\n\
            my-agent = { source = \"official\", path = \"agents/my-agent.md\", version = \"v1.0.0\" }"
        })?;

        self.execute_from_path(Some(&manifest_path)).await
    }

    pub async fn execute_from_path(&self, path: Option<&Path>) -> Result<()> {
        use crate::installer::{ResourceFilter, install_resources};
        use crate::manifest::Manifest;
        use crate::utils::progress::{InstallationPhase, MultiPhaseProgress};
        use std::sync::Arc;

        let manifest_path = if let Some(p) = path {
            p.to_path_buf()
        } else {
            std::env::current_dir()?.join("agpm.toml")
        };

        if !manifest_path.exists() {
            return Err(anyhow::anyhow!("No agpm.toml found at {}", manifest_path.display()));
        }

        let manifest = Manifest::load(&manifest_path)?;

        // In --frozen mode, check for corruption and security issues only
        let lockfile_path =
            manifest_path.parent().unwrap_or_else(|| Path::new(".")).join("agpm.lock");

        if self.frozen && lockfile_path.exists() {
            // Check for critical issues (corruption/security) but not version/path changes
            let lockfile = LockFile::load(&lockfile_path)?;
            if let Some(reason) = lockfile.validate_against_manifest(&manifest, false)? {
                return Err(anyhow::anyhow!(
                    "Lockfile has critical issues in --frozen mode:\n\n\
                     {reason}\n\n\
                     Hint: Fix the issue or remove --frozen flag."
                ));
            }
        }
        let total_deps = manifest.all_dependencies().len();

        // Initialize multi-phase progress for all progress tracking
        let multi_phase = Arc::new(MultiPhaseProgress::new(!self.quiet && !self.no_progress));

        // Show initial status

        let actual_project_dir =
            manifest_path.parent().ok_or_else(|| anyhow::anyhow!("Invalid manifest path"))?;

        // Check for existing lockfile
        let lockfile_path = actual_project_dir.join("agpm.lock");

        let existing_lockfile = if lockfile_path.exists() {
            Some(LockFile::load(&lockfile_path)?)
        } else {
            None
        };

        // Initialize cache (always needed now, even with --no-cache)
        let cache = Cache::new()?;

        // Resolution phase
        let mut resolver =
            DependencyResolver::new_with_global(manifest.clone(), cache.clone()).await?;

        // Pre-sync sources phase (if not frozen and we have remote deps)
        let has_remote_deps =
            manifest.all_dependencies().iter().any(|(_, dep)| dep.get_source().is_some());

        if !self.frozen && has_remote_deps {
            // Start syncing sources phase
            if !self.quiet && !self.no_progress {
                multi_phase.start_phase(InstallationPhase::SyncingSources, None);
            }

            // Get all dependencies for pre-syncing
            let deps: Vec<(String, ResourceDependency)> = manifest
                .all_dependencies_with_mcp()
                .into_iter()
                .map(|(name, dep)| (name.to_string(), dep.into_owned()))
                .collect();

            // Pre-sync all required sources (performs actual Git operations)
            resolver.pre_sync_sources(&deps).await?;

            // Complete syncing sources phase
            if !self.quiet && !self.no_progress {
                multi_phase.complete_phase(Some("Sources synced"));
            }
        }

        let mut lockfile = if let Some(existing) = existing_lockfile {
            if self.frozen {
                // Use existing lockfile as-is
                if !self.quiet {
                    println!("✓ Using frozen lockfile ({total_deps} dependencies)");
                }
                existing
            } else {
                // Start resolving phase
                if !self.quiet && !self.no_progress && total_deps > 0 {
                    multi_phase.start_phase(InstallationPhase::ResolvingDependencies, None);
                }

                // Update lockfile with any new dependencies
                let result = resolver.update(&existing, None).await?;

                // Complete resolving phase
                if !self.quiet && !self.no_progress && total_deps > 0 {
                    multi_phase
                        .complete_phase(Some(&format!("Resolved {total_deps} dependencies")));
                }

                result
            }
        } else {
            // Start resolving phase
            if !self.quiet && !self.no_progress && total_deps > 0 {
                multi_phase.start_phase(InstallationPhase::ResolvingDependencies, None);
            }

            // Fresh resolution
            let result = resolver.resolve_with_options(!self.no_transitive).await?;

            // Complete resolving phase
            if !self.quiet && !self.no_progress && total_deps > 0 {
                multi_phase.complete_phase(Some(&format!("Resolved {total_deps} dependencies")));
            }

            result
        };

        // Check for tag movement if we have both old and new lockfiles (skip in frozen mode)
        let old_lockfile = if !self.frozen && lockfile_path.exists() {
            // Load the old lockfile for comparison
            if let Ok(old) = LockFile::load(&lockfile_path) {
                detect_tag_movement(&old, &lockfile, self.quiet);
                Some(old)
            } else {
                None
            }
        } else {
            None
        };

        // Handle dry-run mode: show what would be installed without making changes
        if self.dry_run {
            return self.handle_dry_run(&lockfile, &lockfile_path, &multi_phase);
        }

        let total_resources = ResourceIterator::count_total_resources(&lockfile);

        // Track installation error to return later
        let mut installation_error = None;

        // Track counts for finalizing phase
        let mut hook_count = 0;
        let mut server_count = 0;

        let installed_count = if total_resources == 0 {
            0
        } else {
            // Start installation phase
            if !self.quiet && !self.no_progress {
                multi_phase.start_phase(
                    InstallationPhase::Installing,
                    Some(&format!("({total_resources} resources)")),
                );
            }

            let max_concurrency = self.max_parallel.unwrap_or_else(|| {
                let cores =
                    std::thread::available_parallelism().map(std::num::NonZero::get).unwrap_or(4);
                std::cmp::max(10, cores * 2)
            });

            // Install resources using the main installation function
            match install_resources(
                ResourceFilter::All,
                &lockfile,
                &manifest,
                actual_project_dir,
                cache,
                self.no_cache,
                Some(max_concurrency),
                Some(multi_phase.clone()),
            )
            .await
            {
                Ok((count, checksums)) => {
                    // Update lockfile with checksums
                    for (name, checksum) in checksums {
                        lockfile.update_resource_checksum(&name, &checksum);
                    }

                    // Complete installation phase
                    if count > 0 && !self.quiet && !self.no_progress {
                        multi_phase.complete_phase(Some(&format!("Installed {count} resources")));
                    }
                    count
                }
                Err(e) => {
                    // Save the error to return immediately - don't continue with hooks/mcp/gitignore
                    installation_error = Some(e);
                    0
                }
            }
        };

        // Only proceed with hooks, MCP, and finalization if installation succeeded
        if installation_error.is_none() {
            // Handle hooks if present
            if !lockfile.hooks.is_empty() {
                // Actually install and configure hooks
                crate::hooks::install_hooks(&manifest, actual_project_dir).await?;
                hook_count = lockfile.hooks.len();
            }

            // Handle MCP servers if present
            if !lockfile.mcp_servers.is_empty() {
                // Configure MCP servers by updating .mcp.json
                let mcp_servers_dir = actual_project_dir.join(&manifest.target.mcp_servers);
                crate::mcp::configure_mcp_servers(actual_project_dir, &mcp_servers_dir).await?;

                server_count = lockfile.mcp_servers.len();
                if !self.quiet {
                    if server_count == 1 {
                        println!("✓ Configured 1 MCP server");
                    } else {
                        println!("✓ Configured {server_count} MCP servers");
                    }
                }
            }

            // Clean up removed or moved artifacts AFTER successful installation
            // This ensures we don't delete old files if installation fails
            if installation_error.is_none()
                && let Some(old) = old_lockfile
                && let Ok(removed) =
                    crate::installer::cleanup_removed_artifacts(&old, &lockfile, actual_project_dir)
                        .await
                && !removed.is_empty()
                && !self.quiet
            {
                println!("🗑️  Cleaned up {} moved or removed artifact(s)", removed.len());
            }

            // Start finalizing phase
            if !self.quiet
                && !self.no_progress
                && (installed_count > 0 || hook_count > 0 || server_count > 0)
            {
                multi_phase.start_phase(InstallationPhase::Finalizing, None);
            }

            if !self.no_lock {
                // Save lockfile with checksums (no progress needed for this quick operation)
                lockfile.save(&lockfile_path).with_context(|| {
                    format!("Failed to save lockfile to {}", lockfile_path.display())
                })?;
            }

            // Update .gitignore only if installation succeeded
            if manifest.target.gitignore {
                update_gitignore(&lockfile, actual_project_dir, true)?;
            }

            // Complete finalizing phase
            if !self.quiet
                && !self.no_progress
                && (installed_count > 0 || hook_count > 0 || server_count > 0)
            {
                multi_phase.complete_phase(Some("Installation complete!"));
            }
        }

        // Return the installation error if there was one
        if let Some(error) = installation_error {
            return Err(error);
        }

        // Clear the multi-phase display
        if !self.quiet && !self.no_progress {
            multi_phase.clear();
        }

        // Only show message if no progress was shown and there's nothing installed
        if self.no_progress
            && !self.quiet
            && installed_count == 0
            && hook_count == 0
            && server_count == 0
        {
            println!("\nNo dependencies to install");
        }

        Ok(())
    }

    /// Handles dry-run mode by comparing lockfiles and displaying what would change.
    ///
    /// This method compares the resolved lockfile with the existing lockfile (if any)
    /// to determine what would be installed, updated, or added. It displays a summary
    /// of the changes without modifying any files.
    ///
    /// # Arguments
    ///
    /// * `new_lockfile` - The newly resolved lockfile
    /// * `lockfile_path` - Path to the lockfile on disk
    /// * `multi_phase` - Progress tracker for clearing display
    ///
    /// # Returns
    ///
    /// - `Ok(())` if no changes would be made (exit code 0)
    /// - `Err(anyhow::Error)` with exit code 1 if changes would be made
    ///
    /// # Exit Codes
    ///
    /// - 0: No changes would be made
    /// - 1: Changes would be made (useful for CI pipelines)
    fn handle_dry_run(
        &self,
        new_lockfile: &LockFile,
        lockfile_path: &Path,
        multi_phase: &std::sync::Arc<crate::utils::progress::MultiPhaseProgress>,
    ) -> Result<()> {
        use colored::Colorize;

        // Clear progress display
        if !self.quiet && !self.no_progress {
            multi_phase.clear();
        }

        // Track changes
        let mut new_resources = Vec::new();
        let mut updated_resources = Vec::new();
        let mut unchanged_count = 0;

        // Load existing lockfile if it exists
        let existing_lockfile = if lockfile_path.exists() {
            LockFile::load(lockfile_path).ok()
        } else {
            None
        };

        // Compare lockfiles to find changes
        if let Some(existing) = existing_lockfile.as_ref() {
            ResourceIterator::for_each_resource(new_lockfile, |resource_type, new_entry| {
                // Find corresponding entry in existing lockfile
                if let Some((_, old_entry)) = ResourceIterator::find_resource_by_name_and_source(
                    existing,
                    &new_entry.name,
                    new_entry.source.as_deref(),
                ) {
                    // Check if it was updated
                    if old_entry.resolved_commit == new_entry.resolved_commit {
                        unchanged_count += 1;
                    } else {
                        let old_version =
                            old_entry.version.clone().unwrap_or_else(|| "latest".to_string());
                        let new_version =
                            new_entry.version.clone().unwrap_or_else(|| "latest".to_string());
                        updated_resources.push((
                            resource_type.to_string(),
                            new_entry.name.clone(),
                            old_version,
                            new_version,
                        ));
                    }
                } else {
                    // New resource
                    new_resources.push((
                        resource_type.to_string(),
                        new_entry.name.clone(),
                        new_entry.version.clone().unwrap_or_else(|| "latest".to_string()),
                    ));
                }
            });
        } else {
            // No existing lockfile, everything is new
            ResourceIterator::for_each_resource(new_lockfile, |resource_type, new_entry| {
                new_resources.push((
                    resource_type.to_string(),
                    new_entry.name.clone(),
                    new_entry.version.clone().unwrap_or_else(|| "latest".to_string()),
                ));
            });
        }

        // Display results
        let has_changes = !new_resources.is_empty() || !updated_resources.is_empty();

        if !self.quiet {
            if has_changes {
                println!("{}", "Dry run - the following changes would be made:".yellow());
                println!();

                if !new_resources.is_empty() {
                    println!("{}", "New resources:".green().bold());
                    for (resource_type, name, version) in &new_resources {
                        println!(
                            "  {} {} ({})",
                            "+".green(),
                            name.cyan(),
                            format!("{resource_type} {version}").dimmed()
                        );
                    }
                    println!();
                }

                if !updated_resources.is_empty() {
                    println!("{}", "Updated resources:".yellow().bold());
                    for (resource_type, name, old_ver, new_ver) in &updated_resources {
                        print!("  {} {} {} → ", "~".yellow(), name.cyan(), old_ver.yellow());
                        println!("{} ({})", new_ver.green(), resource_type.dimmed());
                    }
                    println!();
                }

                if unchanged_count > 0 {
                    println!("{}", format!("{unchanged_count} unchanged resources").dimmed());
                }

                println!();
                println!(
                    "{}",
                    format!(
                        "Total: {} new, {} updated, {} unchanged",
                        new_resources.len(),
                        updated_resources.len(),
                        unchanged_count
                    )
                    .bold()
                );
                println!();
                println!("{}", "No files were modified (dry-run mode)".yellow());
            } else {
                println!("✓ {}", "No changes would be made".green());
            }
        }

        // Return error with special exit code if there are changes (useful for CI)
        if has_changes {
            return Err(anyhow::anyhow!("Dry-run detected changes (exit 1)"));
        }

        Ok(())
    }
}

/// Detects if any tags have moved between the old and new lockfiles.
///
/// Tags in Git are supposed to be immutable, so if a tag points to a different
/// commit than before, this is potentially problematic and worth warning about.
///
/// Branches are expected to move, so we don't warn about those.
fn detect_tag_movement(old_lockfile: &LockFile, new_lockfile: &LockFile, quiet: bool) {
    use crate::core::ResourceType;

    // Helper function to check if a version looks like a tag (not a branch or SHA)
    fn is_tag_like(version: &str) -> bool {
        // Skip if it looks like a SHA
        if version.len() >= 7 && version.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }

        // Skip if it's a known branch name
        if matches!(
            version,
            "main" | "master" | "develop" | "dev" | "staging" | "production" | "HEAD"
        ) || version.starts_with("release/")
            || version.starts_with("feature/")
            || version.starts_with("hotfix/")
            || version.starts_with("bugfix/")
        {
            return false;
        }

        // Likely a tag if it starts with 'v' or looks like a version
        version.starts_with('v')
            || version.starts_with("release-")
            || version.parse::<semver::Version>().is_ok()
            || version.contains('.') // Likely a version number
    }

    // Helper to check resources of a specific type
    fn check_resources(
        old_resources: &[crate::lockfile::LockedResource],
        new_resources: &[crate::lockfile::LockedResource],
        resource_type: ResourceType,
        quiet: bool,
    ) {
        for new_resource in new_resources {
            // Skip if no version or resolved commit
            let Some(ref new_version) = new_resource.version else {
                continue;
            };
            let Some(ref new_commit) = new_resource.resolved_commit else {
                continue;
            };

            // Skip if not a tag
            if !is_tag_like(new_version) {
                continue;
            }

            // Find the corresponding old resource
            if let Some(old_resource) = old_resources.iter().find(|r| r.name == new_resource.name)
                && let (Some(old_version), Some(old_commit)) =
                    (&old_resource.version, &old_resource.resolved_commit)
            {
                // Check if the same tag now points to a different commit
                if old_version == new_version && old_commit != new_commit && !quiet {
                    eprintln!(
                        "⚠️  Warning: Tag '{}' for {} '{}' has moved from {} to {}",
                        new_version,
                        resource_type,
                        new_resource.name,
                        &old_commit[..8.min(old_commit.len())],
                        &new_commit[..8.min(new_commit.len())]
                    );
                    eprintln!(
                        "   Tags should be immutable. This may indicate the upstream repository force-pushed the tag."
                    );
                }
            }
        }
    }

    // Check all resource types
    check_resources(&old_lockfile.agents, &new_lockfile.agents, ResourceType::Agent, quiet);
    check_resources(&old_lockfile.snippets, &new_lockfile.snippets, ResourceType::Snippet, quiet);
    check_resources(&old_lockfile.commands, &new_lockfile.commands, ResourceType::Command, quiet);
    check_resources(&old_lockfile.scripts, &new_lockfile.scripts, ResourceType::Script, quiet);
    check_resources(&old_lockfile.hooks, &new_lockfile.hooks, ResourceType::Hook, quiet);
    check_resources(
        &old_lockfile.mcp_servers,
        &new_lockfile.mcp_servers,
        ResourceType::McpServer,
        quiet,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{LockFile, LockedResource};
    use crate::manifest::{DetailedDependency, Manifest, ResourceDependency};
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_install_command_no_manifest() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        let cmd = InstallCommand::new();
        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("agpm.toml"));
    }

    #[tokio::test]
    async fn test_install_with_empty_manifest() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        Manifest::new().save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_ok());

        let lockfile_path = temp.path().join("agpm.lock");
        assert!(lockfile_path.exists());
        let lockfile = LockFile::load(&lockfile_path).unwrap();
        assert!(lockfile.agents.is_empty());
        assert!(lockfile.snippets.is_empty());
    }

    #[tokio::test]
    async fn test_install_command_new_defaults() {
        let cmd = InstallCommand::new();
        assert!(!cmd.no_lock);
        assert!(!cmd.frozen);
        assert!(!cmd.no_cache);
        assert!(cmd.max_parallel.is_none());
        assert!(!cmd.quiet);
    }

    #[tokio::test]
    async fn test_install_respects_no_lock_flag() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        Manifest::new().save(&manifest_path).unwrap();

        let cmd = InstallCommand {
            no_lock: true,
            frozen: false,
            no_cache: false,
            max_parallel: None,
            quiet: false,
            no_progress: false,
            no_transitive: false,
            dry_run: false,
        };

        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_ok());
        assert!(!temp.path().join("agpm.lock").exists());
    }

    #[tokio::test]
    async fn test_install_with_local_dependency() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let local_file = temp.path().join("local-agent.md");
        fs::write(
            &local_file,
            "# Local Agent
This is a test agent.",
        )
        .unwrap();

        let mut manifest = Manifest::new();
        manifest.agents.insert(
            "local-agent".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "local-agent.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_ok());
        assert!(temp.path().join(".claude/agents/local-agent.md").exists());
    }

    #[tokio::test]
    async fn test_install_with_invalid_manifest_syntax() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        fs::write(&manifest_path, "[invalid toml").unwrap();

        let cmd = InstallCommand::new();
        let err = cmd.execute_from_path(Some(temp.path())).await.unwrap_err();
        // The actual error will be about parsing the invalid TOML
        let err_str = err.to_string();
        assert!(
            err_str.contains("Cannot read manifest")
                || err_str.contains("unclosed")
                || err_str.contains("parse")
                || err_str.contains("expected")
                || err_str.contains("invalid"),
            "Unexpected error message: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_install_uses_existing_lockfile_when_frozen() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let lockfile_path = temp.path().join("agpm.lock");

        let local_file = temp.path().join("test-agent.md");
        fs::write(
            &local_file,
            "# Test Agent
Body",
        )
        .unwrap();

        let mut manifest = Manifest::new();
        manifest.agents.insert(
            "test-agent".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "test-agent.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.save(&manifest_path).unwrap();

        LockFile {
            version: 1,
            sources: vec![],
            commands: vec![],
            agents: vec![LockedResource {
                name: "test-agent".into(),
                source: None,
                url: None,
                path: "test-agent.md".into(),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at: ".claude/agents/test-agent.md".into(),
                dependencies: vec![],
                resource_type: crate::core::ResourceType::Agent,
            }],
            snippets: vec![],
            mcp_servers: vec![],
            scripts: vec![],
            hooks: vec![],
        }
        .save(&lockfile_path)
        .unwrap();

        let cmd = InstallCommand {
            no_lock: false,
            frozen: true,
            no_cache: false,
            max_parallel: None,
            quiet: false,
            no_progress: false,
            no_transitive: false,
            dry_run: false,
        };

        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_ok());
        assert!(temp.path().join(".claude/agents/test-agent.md").exists());
    }

    #[tokio::test]
    async fn test_install_errors_when_local_file_missing() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");

        let mut manifest = Manifest::new();
        manifest.agents.insert(
            "missing".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "missing.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.save(&manifest_path).unwrap();

        let err = InstallCommand::new().execute_from_path(Some(&manifest_path)).await.unwrap_err();
        assert!(err.to_string().contains("Local file"));
    }

    #[tokio::test]
    async fn test_install_single_resource_paths() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let snippet_file = temp.path().join("single-snippet.md");
        fs::write(
            &snippet_file,
            "# Snippet
Body",
        )
        .unwrap();

        let mut manifest = Manifest::new();
        manifest.snippets.insert(
            "single".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "single-snippet.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        assert!(cmd.execute_from_path(Some(&manifest_path)).await.is_ok());

        let lockfile = LockFile::load(&temp.path().join("agpm.lock")).unwrap();
        assert_eq!(lockfile.snippets.len(), 1);
        let installed_path = temp.path().join(&lockfile.snippets[0].installed_at);
        assert!(installed_path.exists());
    }

    #[tokio::test]
    async fn test_install_single_command_resource() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let command_file = temp.path().join("single-command.md");
        fs::write(
            &command_file,
            "# Command
Body",
        )
        .unwrap();

        let mut manifest = Manifest::new();
        manifest.commands.insert(
            "cmd".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "single-command.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        assert!(cmd.execute_from_path(Some(&manifest_path)).await.is_ok());

        let lockfile = LockFile::load(&temp.path().join("agpm.lock")).unwrap();
        assert_eq!(lockfile.commands.len(), 1);
        assert!(temp.path().join(&lockfile.commands[0].installed_at).exists());
    }

    #[tokio::test]
    async fn test_install_dry_run_mode() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let lockfile_path = temp.path().join("agpm.lock");
        let agent_file = temp.path().join("test-agent.md");

        // Create a local file for the agent
        fs::write(&agent_file, "# Test Agent\nBody").unwrap();

        let mut manifest = Manifest::new();
        manifest.agents.insert(
            "test-agent".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "test-agent.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand {
            no_lock: false,
            frozen: false,
            no_cache: false,
            max_parallel: None,
            quiet: true, // Suppress output in test
            no_progress: true,
            no_transitive: false,
            dry_run: true,
        };

        // In dry-run mode, this should return an error indicating changes would be made
        let result = cmd.execute_from_path(Some(&manifest_path)).await;

        // Should return an error because changes would be made
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Dry-run detected changes"));

        // Lockfile should NOT be created in dry-run mode
        assert!(!lockfile_path.exists());
        // Resource should NOT be installed
        assert!(!temp.path().join(".claude/agents/test-agent.md").exists());
    }

    #[tokio::test]
    async fn test_install_summary_with_mcp_servers() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("agpm.toml");
        let agent_file = temp.path().join("summary-agent.md");
        fs::write(&agent_file, "# Agent\nBody").unwrap();

        let mcp_dir = temp.path().join("mcp");
        fs::create_dir_all(&mcp_dir).unwrap();
        fs::write(mcp_dir.join("test-mcp.json"), "{\"name\":\"test\"}").unwrap();

        let mut manifest = Manifest::new();
        manifest.agents.insert(
            "summary".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "summary-agent.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.add_mcp_server(
            "test-mcp".into(),
            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: "mcp/test-mcp.json".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
                dependencies: None,
            })),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        assert!(cmd.execute_from_path(Some(&manifest_path)).await.is_ok());
    }
}
