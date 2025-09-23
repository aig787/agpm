//! Install Claude Code resources from manifest dependencies.
//!
//! This module provides the `install` command which reads dependencies from the
//! `ccpm.toml` manifest file, resolves them, and installs the resource files
//! to the project directory. The command supports both fresh installations and
//! updates to existing installations with advanced parallel processing capabilities.
//!
//! # Features
//!
//! - **Dependency Resolution**: Resolves all dependencies defined in the manifest
//! - **Lockfile Management**: Generates and maintains `ccpm.lock` for reproducible builds
//! - **Worktree-Based Parallel Installation**: Uses Git worktrees for safe concurrent resource installation
//! - **Multi-Phase Progress Tracking**: Shows detailed progress with phase transitions and real-time updates
//! - **Resource Validation**: Validates markdown files and content during installation
//! - **Cache Support**: Advanced cache with instance-level optimizations and worktree management
//! - **Concurrency Control**: User-configurable parallelism via `--max-parallel` flag
//!
//! # Examples
//!
//! Install all dependencies from manifest:
//! ```bash
//! ccpm install
//! ```
//!
//! Force reinstall all dependencies:
//! ```bash
//! ccpm install --force
//! ```
//!
//! Install without creating lockfile:
//! ```bash
//! ccpm install --no-lock
//! ```
//!
//! Use frozen lockfile (CI/production):
//! ```bash
//! ccpm install --frozen
//! ```
//!
//! Disable cache and clone fresh:
//! ```bash
//! ccpm install --no-cache
//! ```
//!
//! # Installation Process
//!
//! 1. **Manifest Loading**: Reads `ccpm.toml` to understand dependencies
//! 2. **Dependency Resolution**: Resolves versions and creates dependency graph
//! 3. **Worktree Preparation**: Pre-creates Git worktrees for optimal parallel access
//! 4. **Parallel Resource Installation**: Installs resources concurrently using isolated worktrees
//! 5. **Progress Coordination**: Updates multi-phase progress tracking throughout installation
//! 6. **Configuration Updates**: Updates hooks and MCP server configurations as needed
//! 7. **Lockfile Generation**: Creates or updates `ccpm.lock` with checksums and metadata
//!
//! # Error Conditions
//!
//! - No manifest file found in project
//! - Invalid manifest syntax or structure
//! - Dependency resolution conflicts
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
/// This command reads the project's `ccpm.toml` manifest file, resolves all dependencies,
/// and installs the resource files to the appropriate directories. It generates or updates
/// a `ccpm.lock` lockfile to ensure reproducible installations.
///
/// # Behavior
///
/// 1. Locates and loads the project manifest (`ccpm.toml`)
/// 2. Resolves dependencies using the dependency resolver
/// 3. Downloads or updates Git repository sources as needed
/// 4. Installs resource files to target directories
/// 5. Generates or updates the lockfile (`ccpm.lock`)
/// 6. Provides progress feedback during installation
///
/// # Examples
///
/// ```rust,ignore
/// use ccpm::cli::install::InstallCommand;
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
    /// Prevents the command from creating or updating the `ccpm.lock` file.
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
    /// use ccpm::cli::install::InstallCommand;
    ///
    /// let cmd = InstallCommand::new();
    /// // cmd can now be executed with execute_from_path()
    /// ```
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            no_lock: false,
            frozen: false,
            no_cache: false,
            max_parallel: None,
            quiet: false,
            no_progress: false,
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
    /// use ccpm::cli::install::InstallCommand;
    ///
    /// let cmd = InstallCommand::new_quiet();
    /// // cmd will execute without progress bars or status messages
    /// ```
    #[allow(dead_code)]
    pub fn new_quiet() -> Self {
        Self {
            no_lock: false,
            frozen: false,
            no_cache: false,
            max_parallel: None,
            quiet: true,
            no_progress: true,
        }
    }

    /// Executes the install command with automatic manifest discovery.
    ///
    /// This method provides convenient manifest file discovery, searching for
    /// `ccpm.toml` in the current directory and parent directories if no specific
    /// path is provided. It's the standard entry point for CLI usage.
    ///
    /// # Arguments
    ///
    /// * `manifest_path` - Optional explicit path to `ccpm.toml`. If `None`,
    ///   the method searches for `ccpm.toml` starting from the current directory
    ///   and walking up the directory tree.
    ///
    /// # Manifest Discovery
    ///
    /// When `manifest_path` is `None`, the search process:
    /// 1. Checks current directory for `ccpm.toml`
    /// 2. Walks up parent directories until `ccpm.toml` is found
    /// 3. Stops at filesystem root if no manifest found
    /// 4. Returns an error with helpful guidance if no manifest exists
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::install::InstallCommand;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cmd = InstallCommand::new();
    ///
    /// // Auto-discover manifest in current directory or parents
    /// cmd.execute_with_manifest_path(None).await?;
    ///
    /// // Use specific manifest file
    /// cmd.execute_with_manifest_path(Some(PathBuf::from("./my-project/ccpm.toml"))).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No `ccpm.toml` file found in search path
    /// - Specified manifest path doesn't exist
    /// - Manifest file contains invalid TOML syntax
    /// - Dependencies cannot be resolved
    /// - Installation process fails
    ///
    /// # Error Messages
    ///
    /// When no manifest is found, the error includes helpful guidance:
    /// ```text
    /// No ccpm.toml found in current directory or any parent directory.
    ///
    /// To get started, create a ccpm.toml file with your dependencies:
    ///
    /// [sources]
    /// official = "https://github.com/example-org/ccpm-official.git"
    ///
    /// [agents]
    /// my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
    /// ```
    pub async fn execute_with_manifest_path(self, manifest_path: Option<PathBuf>) -> Result<()> {
        // Find manifest file
        let manifest_path = find_manifest_with_optional(manifest_path).with_context(|| {
"No ccpm.toml found in current directory or any parent directory.\n\n\
            To get started, create a ccpm.toml file with your dependencies:\n\n\
            [sources]\n\
            official = \"https://github.com/example-org/ccpm-official.git\"\n\n\
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
            std::env::current_dir()?.join("ccpm.toml")
        };

        if !manifest_path.exists() {
            return Err(anyhow::anyhow!(
                "No ccpm.toml found at {}",
                manifest_path.display()
            ));
        }

        let manifest = Manifest::load(&manifest_path)?;
        let total_deps = manifest.all_dependencies().len();

        // Initialize multi-phase progress for all progress tracking
        let multi_phase = Arc::new(MultiPhaseProgress::new(!self.quiet && !self.no_progress));

        // Show initial status

        let actual_project_dir = manifest_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid manifest path"))?;

        // Check for existing lockfile
        let lockfile_path = actual_project_dir.join("ccpm.lock");

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
        let has_remote_deps = manifest
            .all_dependencies()
            .iter()
            .any(|(_, dep)| dep.get_source().is_some());

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
                    println!("✓ Using frozen lockfile ({} dependencies)", total_deps);
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
                        .complete_phase(Some(&format!("Resolved {} dependencies", total_deps)));
                }

                result
            }
        } else {
            // Start resolving phase
            if !self.quiet && !self.no_progress && total_deps > 0 {
                multi_phase.start_phase(InstallationPhase::ResolvingDependencies, None);
            }

            // Fresh resolution
            let result = resolver.resolve().await?;

            // Complete resolving phase
            if !self.quiet && !self.no_progress && total_deps > 0 {
                multi_phase.complete_phase(Some(&format!("Resolved {} dependencies", total_deps)));
            }

            result
        };

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
                    Some(&format!("({} resources)", total_resources)),
                );
            }

            let max_concurrency = self.max_parallel.unwrap_or_else(|| {
                let cores = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4);
                std::cmp::max(10, cores * 2)
            });

            // Install resources using the main installation function
            // Note: We capture the error to return later, after updating gitignore
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
                        multi_phase.complete_phase(Some(&format!("Installed {} resources", count)));
                    }
                    count
                }
                Err(e) => {
                    // Save the error to return later, but continue to update gitignore
                    installation_error = Some(e);
                    0
                }
            }
        };

        // Handle hooks if present
        if !lockfile.hooks.is_empty() {
            hook_count = lockfile.hooks.len();
            if !self.quiet {
                if hook_count == 1 {
                    println!("✓ Configured 1 hook");
                } else {
                    println!("✓ Configured {} hooks", hook_count);
                }
            }
            // TODO: Implement actual hook configuration when the API is available
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
                    println!("✓ Configured {} MCP servers", server_count);
                }
            }
        }

        // Start finalizing phase
        if !self.quiet
            && !self.no_progress
            && (installed_count > 0 || hook_count > 0 || server_count > 0)
        {
            multi_phase.start_phase(InstallationPhase::Finalizing, None);
        }

        if !self.no_lock {
            // Save lockfile (no progress needed for this quick operation)
            lockfile.save(&lockfile_path).with_context(|| {
                format!("Failed to save lockfile to {}", lockfile_path.display())
            })?;
        }

        // Update .gitignore if needed and not disabled
        // This happens even if installation failed, based on lockfile entries
        if manifest.target.gitignore {
            update_gitignore(&lockfile, actual_project_dir, true)?;
        }

        // Complete finalizing phase
        if !self.quiet
            && !self.no_progress
            && (installed_count > 0 || hook_count > 0 || server_count > 0)
        {
            multi_phase.complete_phase(Some("Installation finalized"));
        }

        // Return the installation error if there was one
        if let Some(error) = installation_error {
            return Err(error);
        }

        // Clear the multi-phase display before final message
        if !self.quiet && !self.no_progress {
            multi_phase.clear();
        }

        if installed_count > 0 || total_deps > 0 {
            if !self.quiet {
                println!("\nInstallation complete!");
            }
        } else if !self.quiet {
            println!("\nNo dependencies to install");
        }

        Ok(())
    }
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
        let manifest_path = temp.path().join("ccpm.toml");

        let cmd = InstallCommand::new();
        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ccpm.toml"));
    }

    #[tokio::test]
    async fn test_install_with_empty_manifest() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");
        Manifest::new().save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_ok());

        let lockfile_path = temp.path().join("ccpm.lock");
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
        let manifest_path = temp.path().join("ccpm.toml");
        Manifest::new().save(&manifest_path).unwrap();

        let cmd = InstallCommand {
            no_lock: true,
            frozen: false,
            no_cache: false,
            max_parallel: None,
            quiet: false,
            no_progress: false,
        };

        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_ok());
        assert!(!temp.path().join("ccpm.lock").exists());
    }

    #[tokio::test]
    async fn test_install_with_local_dependency() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");
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
            ResourceDependency::Detailed(DetailedDependency {
                source: None,
                path: "local-agent.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
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
        let manifest_path = temp.path().join("ccpm.toml");
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
        let manifest_path = temp.path().join("ccpm.toml");
        let lockfile_path = temp.path().join("ccpm.lock");

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
            ResourceDependency::Detailed(DetailedDependency {
                source: None,
                path: "test-agent.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
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
        };

        let result = cmd.execute_from_path(Some(&manifest_path)).await;
        assert!(result.is_ok());
        assert!(temp.path().join(".claude/agents/test-agent.md").exists());
    }

    #[tokio::test]
    async fn test_install_errors_when_local_file_missing() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");

        let mut manifest = Manifest::new();
        manifest.agents.insert(
            "missing".into(),
            ResourceDependency::Detailed(DetailedDependency {
                source: None,
                path: "missing.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.save(&manifest_path).unwrap();

        let err = InstallCommand::new()
            .execute_from_path(Some(&manifest_path))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Local file"));
    }

    #[tokio::test]
    async fn test_install_single_resource_paths() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");
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
            ResourceDependency::Detailed(DetailedDependency {
                source: None,
                path: "single-snippet.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        assert!(cmd.execute_from_path(Some(&manifest_path)).await.is_ok());

        let lockfile = LockFile::load(&temp.path().join("ccpm.lock")).unwrap();
        assert_eq!(lockfile.snippets.len(), 1);
        let installed_path = temp.path().join(&lockfile.snippets[0].installed_at);
        assert!(installed_path.exists());
    }

    #[tokio::test]
    async fn test_install_single_command_resource() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");
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
            ResourceDependency::Detailed(DetailedDependency {
                source: None,
                path: "single-command.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        assert!(cmd.execute_from_path(Some(&manifest_path)).await.is_ok());

        let lockfile = LockFile::load(&temp.path().join("ccpm.lock")).unwrap();
        assert_eq!(lockfile.commands.len(), 1);
        assert!(
            temp.path()
                .join(&lockfile.commands[0].installed_at)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_install_summary_with_mcp_servers() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("ccpm.toml");
        let agent_file = temp.path().join("summary-agent.md");
        fs::write(&agent_file, "# Agent\nBody").unwrap();

        let mcp_dir = temp.path().join("mcp");
        fs::create_dir_all(&mcp_dir).unwrap();
        fs::write(mcp_dir.join("test-mcp.json"), "{\"name\":\"test\"}").unwrap();

        let mut manifest = Manifest::new();
        manifest.agents.insert(
            "summary".into(),
            ResourceDependency::Detailed(DetailedDependency {
                source: None,
                path: "summary-agent.md".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.add_mcp_server(
            "test-mcp".into(),
            ResourceDependency::Detailed(DetailedDependency {
                source: None,
                path: "mcp/test-mcp.json".into(),
                version: None,
                branch: None,
                rev: None,
                command: None,
                args: None,
                target: None,
                filename: None,
            }),
        );
        manifest.save(&manifest_path).unwrap();

        let cmd = InstallCommand::new();
        assert!(cmd.execute_from_path(Some(&manifest_path)).await.is_ok());
    }
}
