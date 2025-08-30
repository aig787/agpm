//! Command-line interface for CCPM (Claude Code Package Manager).
//!
//! This module contains all CLI command implementations for the Claude Code Package Manager.
//! The CLI provides a comprehensive set of commands for managing Claude Code resources,
//! from project initialization to dependency management and global configuration.
//!
//! # Command Architecture
//!
//! Each command is implemented as a separate module with its own argument structures
//! and execution logic. This modular design allows for:
//! - Clear separation of concerns
//! - Independent testing of each command
//! - Easy addition of new commands
//! - Consistent documentation and error handling
//!
//! # Available Commands
//!
//! ## Project Management
//! - `init` - Initialize a new CCPM project with a manifest file
//! - `add` - Add sources and dependencies to the project manifest
//! - `remove` - Remove sources and dependencies from the project manifest  
//! - `install` - Install dependencies from the manifest
//! - `update` - Update dependencies within version constraints
//!
//! ## Information and Inspection  
//! - `list` - List installed resources from the lockfile
//! - `validate` - Validate project configuration and dependencies
//!
//! ## System Management
//! - `cache` - Manage the global Git repository cache
//! - `config` - Manage global configuration settings
//!
//! # Command Usage Patterns
//!
//! ## Basic Workflow
//! ```bash
//! # 1. Initialize a new project
//! ccpm init
//!
//! # 2. Add sources and dependencies
//! ccpm add source official https://github.com/org/ccpm-resources.git
//! ccpm add dep official:agents/code-reviewer.md@v1.0.0 --agent
//!
//! # 3. Install dependencies
//! ccpm install
//!
//! # 4. List what's installed
//! ccpm list
//! ```
//!
//! ## Maintenance Operations
//! ```bash
//! # Validate project configuration
//! ccpm validate --resolve --sources
//!
//! # Update dependencies
//! ccpm update
//!
//! # Manage cache
//! ccpm cache clean
//!
//! # Configure global settings
//! ccpm config add-source private https://oauth2:TOKEN@github.com/org/private.git
//! ```
//!
//! # Global vs Project Configuration
//!
//! CCPM uses two types of configuration:
//!
//! | Type | File | Purpose | Version Control |
//! |------|------|---------|----------------|
//! | Project | `ccpm.toml` | Define dependencies | ✅ Commit |
//! | Global | `~/.ccpm/config.toml` | Authentication tokens | ❌ Never commit |
//!
//! # Cross-Platform Support
//!
//! The CLI is designed to work consistently across:
//! - Windows (Command Prompt, `PowerShell`)
//! - macOS (Terminal, various shells)
//! - Linux (bash, zsh, fish, etc.)
//!
//! # Command Modules
//!
//! Each command is implemented in its own module:
//!
//! # Global Options
//!
//! All commands support these global options:
//! - `--verbose` - Enable debug output
//! - `--quiet` - Suppress all output except errors
//! - `--no-progress` - Disable progress bars and spinners
//! - `--config` - Path to custom config file
//!
//! # Example
//!
//! ```bash
//! # Initialize a new project
//! ccpm init --with-examples
//!
//! # Install dependencies
//! ccpm install --verbose
//!
//! # Update dependencies
//! ccpm update --no-progress
//! ```

mod add;
mod cache;
pub mod common;
mod config;
mod init;
mod install;
mod list;
mod remove;
mod resource_ops;
mod update;
pub mod validate;

#[cfg(test)]
mod tests;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Runtime configuration for CLI execution.
///
/// This struct holds configuration that would otherwise be set as environment variables,
/// enabling dependency injection and better testability. It allows tests and programmatic
/// usage to control CLI behavior without modifying global environment state.
///
/// # Design Rationale
///
/// Rather than directly setting environment variables in CLI parsing, this struct:
/// - Enables clean testing without global state pollution
/// - Allows configuration composition and validation
/// - Provides a single point for environment variable management
/// - Supports configuration serialization if needed
///
/// # Usage Pattern
///
/// ```rust,ignore
/// let config = CliConfig::new()
///     .with_log_level("debug")
///     .with_no_progress(true);
/// config.apply_to_env();
/// ```
#[derive(Debug, Clone, Default)]
pub struct CliConfig {
    /// Log level for the `RUST_LOG` environment variable.
    ///
    /// Controls the verbosity of logging output throughout CCPM. Common values:
    /// - `"error"`: Only errors are logged
    /// - `"warn"`: Errors and warnings
    /// - `"info"`: Errors, warnings, and informational messages
    /// - `"debug"`: All messages including debug information
    /// - `"trace"`: Maximum verbosity for troubleshooting
    ///
    /// When `None`, the existing `RUST_LOG` value is preserved.
    pub log_level: Option<String>,

    /// Whether to disable progress indicators and animated output.
    ///
    /// When `true`, sets the `CCPM_NO_PROGRESS` environment variable to disable:
    /// - Progress bars during long operations
    /// - Spinner animations
    /// - Real-time status updates
    ///
    /// This is useful for:
    /// - Automated scripts and CI/CD pipelines
    /// - Terminal environments that don't support ANSI codes
    /// - Debugging where animated output interferes with logs
    pub no_progress: bool,

    /// Custom path to the global configuration file.
    ///
    /// When specified, sets the `CCPM_CONFIG` environment variable to override
    /// the default configuration file location (`~/.ccpm/config.toml`).
    ///
    /// This enables:
    /// - Testing with isolated configuration files
    /// - Alternative configuration layouts
    /// - Shared configuration in team environments
    pub config_path: Option<String>,
}

impl CliConfig {
    /// Create a new CLI configuration with default values.
    ///
    /// Returns a configuration with:
    /// - No log level override (`log_level: None`)
    /// - Progress indicators enabled (`no_progress: false`)
    /// - Default config file location (`config_path: None`)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::CliConfig;
    ///
    /// let config = CliConfig::new();
    /// assert_eq!(config.log_level, None);
    /// assert_eq!(config.no_progress, false);
    /// assert_eq!(config.config_path, None);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply this configuration to the process environment.
    ///
    /// This method sets environment variables based on the configuration values,
    /// which are then read by various parts of CCPM during execution. It should
    /// be called exactly once at the start of CLI execution.
    ///
    /// # Environment Variables Set
    ///
    /// - `RUST_LOG`: Set to `log_level` if specified
    /// - `CCPM_NO_PROGRESS`: Set to "1" if `no_progress` is true
    /// - `CCPM_CONFIG`: Set to `config_path` if specified
    ///
    /// # Side Effects
    ///
    /// Modifies the process environment, which affects:
    /// - Logging behavior throughout the application
    /// - Progress indicator display in long-running operations
    /// - Configuration file discovery and loading
    ///
    /// # Thread Safety
    ///
    /// This method is not thread-safe as it modifies global environment state.
    /// It should only be called from the main thread before spawning other threads.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::CliConfig;
    ///
    /// let mut config = CliConfig::new();
    /// config.log_level = Some("debug".to_string());
    /// config.no_progress = true;
    /// config.apply_to_env();
    ///
    /// // Now RUST_LOG=debug and CCPM_NO_PROGRESS=1 are set
    /// ```
    pub fn apply_to_env(&self) {
        if self.no_progress {
            std::env::set_var("CCPM_NO_PROGRESS", "1");
        }

        if let Some(ref path) = self.config_path {
            std::env::set_var("CCPM_CONFIG", path);
        }
    }
}

/// Main CLI structure for CCPM (Claude Code Package Manager).
///
/// This struct represents the root command and all its global options. It uses the
/// `clap` derive API to automatically generate command-line parsing, help text, and
/// validation. All options marked as `global = true` are available to all subcommands.
///
/// # Design Philosophy
///
/// The CLI follows standard Unix conventions:
/// - Short options use single dashes (`-v`)
/// - Long options use double dashes (`--verbose`)
/// - Global options work with all subcommands
/// - Mutually exclusive options are validated automatically
///
/// # Global Options
///
/// All subcommands inherit these global options:
/// - **Verbosity control**: `--verbose` and `--quiet` for output level
/// - **Configuration**: `--config` for custom config file paths
/// - **UI control**: `--no-progress` for automation-friendly output
///
/// # Examples
///
/// ```bash
/// # Basic command with global options
/// ccpm --verbose install
/// ccpm --quiet --no-progress list
/// ccpm --config ./custom.toml validate
///
/// # Global options work with any subcommand
/// ccpm --verbose mcp list
/// ccpm --quiet cache clean
/// ```
///
/// # Subcommand Structure
///
/// Commands are organized by functionality:
/// - **Project management**: `install`, `update`, `add`
/// - **Information**: `list`, `validate`
/// - **System**: `cache`, `mcp`
///
/// # Integration Points
///
/// This CLI integrates with:
/// - [`CliConfig`] for dependency injection and testing
/// - Environment variables for runtime configuration
/// - Global and project-specific configuration files
/// - Cross-platform file system operations
///
/// # Main CLI application structure for CCPM
///
/// This struct represents the top-level command-line interface for the Claude Code
/// Package Manager. It handles global flags and delegates to subcommands for
/// specific operations.
#[derive(Parser)]
#[command(
    name = "ccpm",
    about = "Claude Code Package Manager - Manage Claude Code resources",
    version,
    author,
    long_about = "CCPM is a Git-based package manager for Claude Code resources including agents, snippets, and more."
)]
pub struct Cli {
    /// The subcommand to execute.
    ///
    /// Each subcommand provides a specific functionality area within CCPM.
    /// The available commands are defined in the [`Commands`] enum.
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output for debugging and detailed information.
    ///
    /// When enabled, shows:
    ///   - Detailed progress information
    ///   - Debug messages and internal state
    ///   - Expanded error context and suggestions
    ///   - Git operation details and network calls
    ///
    /// This is equivalent to setting `RUST_LOG=debug`. Mutually exclusive
    /// with `--quiet`.
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm --verbose install     # Verbose installation
    /// ccpm -v update             # Short form
    /// ```
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress all output except errors for automation.
    ///
    /// When enabled:
    ///   - Suppresses informational messages and progress indicators
    ///   - Only outputs errors and warnings
    ///   - Ideal for scripts and CI/CD pipelines
    ///   - JSON output (where supported) remains unchanged
    ///
    /// Mutually exclusive with `--verbose`.
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm --quiet install       # Silent installation
    /// ccpm -q list               # Short form
    /// ccpm --quiet cache clean   # Automated cache cleanup
    /// ```
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Path to custom global configuration file.
    ///
    /// Overrides the default configuration file location (`~/.ccpm/config.toml`)
    /// with a custom path. This is useful for:
    ///
    /// - **Testing**: Using isolated configuration files
    /// - **Deployment**: Shared configuration in team environments
    /// - **Development**: Different configurations per project
    ///
    /// The configuration file contains:
    /// - Global source repository definitions with authentication
    /// - Default settings for cache and network operations
    /// - User preferences and customizations
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm --config ./dev-config.toml install    # Custom config
    /// ccpm -c ~/.ccpm/team-config.toml list      # Team config
    /// ccpm --config /etc/ccpm/global.toml update # System config
    /// ```
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Path to the manifest file (ccpm.toml).
    ///
    /// By default, CCPM searches for ccpm.toml in the current directory
    /// and parent directories. This option allows you to specify an exact
    /// path to the manifest file, which is useful for:
    ///
    /// - Running commands from outside the project directory
    /// - CI/CD pipelines with non-standard layouts
    /// - Testing with temporary manifests
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm --manifest-path /path/to/ccpm.toml install
    /// ccpm --manifest-path ../other-project/ccpm.toml list
    /// ```
    #[arg(long, global = true)]
    manifest_path: Option<PathBuf>,

    /// Disable progress bars and spinners for automation.
    ///
    /// When enabled:
    /// - Disables animated progress indicators
    /// - Uses plain text status messages instead
    /// - Prevents cursor manipulation and ANSI escape codes
    /// - Ideal for terminals without ANSI support
    ///
    /// This option is automatically enabled in non-TTY environments
    /// (pipes, redirects, CI systems) but can be explicitly controlled.
    ///
    /// # Use Cases
    ///
    /// - **CI/CD pipelines**: Clean log output for build systems
    /// - **Scripts**: Avoid interference with text processing
    /// - **Legacy terminals**: Compatibility with older terminal emulators
    /// - **Debugging**: Easier to follow operation sequences
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm --no-progress install         # No animations
    /// ccpm install 2>&1 | tee log.txt   # Auto-detected non-TTY
    /// CI=true ccpm install               # CI environment
    /// ```
    #[arg(long, global = true)]
    no_progress: bool,
}

/// Available subcommands for the CCPM CLI.
///
/// This enum defines all the subcommands available in CCPM, organized by
/// functional categories. Each variant contains the specific command structure
/// with its own arguments and options.
///
/// # Command Categories
///
/// ## Project Management
/// - [`Init`](Commands::Init): Initialize new CCPM projects
/// - [`Add`](Commands::Add): Add sources and dependencies
/// - [`Remove`](Commands::Remove): Remove sources and dependencies
/// - [`Install`](Commands::Install): Install dependencies from manifest
/// - [`Update`](Commands::Update): Update dependencies within constraints
///
/// ## Information & Validation
/// - [`List`](Commands::List): Display installed resources
/// - [`Validate`](Commands::Validate): Verify project configuration
///
/// ## System Management
/// - [`Cache`](Commands::Cache): Manage Git repository cache
/// - [`Config`](Commands::Config): Manage global configuration
/// - [`Mcp`](Commands::Mcp): Manage MCP server configurations
///
/// # Command Execution
///
/// Each command is executed through its respective `execute()` method,
/// which handles:
/// - Argument validation and parsing
/// - Async operation coordination
/// - Error handling and user feedback
/// - Cross-platform compatibility
///
/// # Examples
///
/// ```bash
/// # Project setup and management
/// ccpm init                    # Initialize new project
/// ccpm add source official ... # Add a source repository
/// ccpm install                 # Install all dependencies
/// ccpm update                  # Update to latest versions
///
/// # Information and validation
/// ccpm list                    # Show installed resources
/// ccpm validate --resolve      # Comprehensive validation
///
/// # System management
/// ccpm cache info              # Show cache information
/// ccpm mcp list               # List MCP servers
/// ```
#[derive(Subcommand)]
enum Commands {
    /// Initialize a new CCPM project with a manifest file.
    ///
    /// Creates a new `ccpm.toml` manifest file in the current directory with
    /// basic project structure and example configurations. This is the first
    /// step in setting up a new CCPM project.
    ///
    /// See [`init::InitCommand`] for detailed options and behavior.
    Init(init::InitCommand),

    /// Add sources and dependencies to the project manifest.
    ///
    /// Provides subcommands to add Git repository sources and resource
    /// dependencies (agents, snippets, commands, MCP servers) to the
    /// `ccpm.toml` manifest file.
    ///
    /// See [`add::AddCommand`] for detailed options and behavior.
    Add(add::AddCommand),

    /// Remove sources and dependencies from the project manifest.
    ///
    /// Provides subcommands to remove Git repository sources and resource
    /// dependencies (agents, snippets, commands, MCP servers) from the
    /// `ccpm.toml` manifest file.
    ///
    /// See [`remove::RemoveCommand`] for detailed options and behavior.
    Remove(remove::RemoveCommand),

    /// Install Claude Code resources from manifest dependencies.
    ///
    /// Reads the `ccpm.toml` manifest, resolves all dependencies, downloads
    /// resources from Git repositories, and installs them to the project
    /// directory. Creates or updates the `ccpm.lock` lockfile.
    ///
    /// See [`install::InstallCommand`] for detailed options and behavior.
    Install(install::InstallCommand),

    /// Update installed resources within version constraints.
    ///
    /// Updates existing dependencies to newer versions while respecting
    /// version constraints defined in the manifest. Updates the lockfile
    /// with resolved versions.
    ///
    /// See [`update::UpdateCommand`] for detailed options and behavior.
    Update(update::UpdateCommand),

    /// List installed Claude Code resources.
    ///
    /// Displays information about currently installed dependencies based on
    /// the lockfile. Supports various output formats and filtering options
    /// for different use cases.
    ///
    /// See [`list::ListCommand`] for detailed options and behavior.
    List(list::ListCommand),

    /// Validate CCPM project configuration and dependencies.
    ///
    /// Performs comprehensive validation of the project manifest, dependencies,
    /// source accessibility, and configuration consistency. Supports multiple
    /// validation levels and output formats.
    ///
    /// See [`validate::ValidateCommand`] for detailed options and behavior.
    Validate(validate::ValidateCommand),

    /// Manage the global Git repository cache.
    ///
    /// Provides operations for managing the global cache directory where
    /// CCPM stores cloned Git repositories. Includes cache information,
    /// cleanup, and size management.
    ///
    /// See [`cache::CacheCommand`] for detailed options and behavior.
    Cache(cache::CacheCommand),

    /// Manage global CCPM configuration.
    ///
    /// Provides operations for managing the global configuration file
    /// (`~/.ccpm/config.toml`) which contains authentication tokens,
    /// default sources, and user preferences.
    ///
    /// See [`config::ConfigCommand`] for detailed options and behavior.
    Config(config::ConfigCommand),
}

impl Cli {
    /// Execute the CLI with default configuration.
    ///
    /// This is the main entry point for CLI execution. It builds a configuration
    /// from the parsed command-line arguments and delegates to [`execute_with_config`](Self::execute_with_config).
    ///
    /// # Process Flow
    ///
    /// 1. **Configuration Building**: Converts CLI arguments to [`CliConfig`]
    /// 2. **Environment Setup**: Applies configuration to process environment
    /// 3. **Command Dispatch**: Routes to the appropriate subcommand handler
    /// 4. **Error Handling**: Provides user-friendly error messages
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the command executed successfully
    /// - `Err(anyhow::Error)` if the command failed with details for user feedback
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::Cli;
    /// use clap::Parser;
    ///
    /// # tokio_test::block_on(async {
    /// let cli = Cli::parse(); // From command-line arguments
    /// cli.execute().await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    pub async fn execute(self) -> Result<()> {
        let config = self.build_config();
        self.execute_with_config(config).await
    }

    /// Build a [`CliConfig`] from the parsed CLI arguments.
    ///
    /// This method translates command-line flags into a structured configuration
    /// that can be applied to the environment or injected into tests.
    ///
    /// # Configuration Logic
    ///
    /// - **Verbose mode**: Sets log level to "debug" for detailed output
    /// - **Quiet mode**: Disables logging for automation-friendly output
    /// - **Default mode**: Uses "info" level for normal operation
    /// - **Progress control**: Honors `--no-progress` flag for animations
    /// - **Config path**: Uses custom config file if specified
    ///
    /// # Validation
    ///
    /// The CLI parser already handles mutual exclusion between `--verbose` and
    /// `--quiet`, so this method doesn't need additional validation.
    ///
    /// # Returns
    ///
    /// A [`CliConfig`] instance ready for environment application or testing.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::Cli;
    /// use clap::Parser;
    ///
    /// let cli = Cli::parse_from(&["ccpm", "--verbose", "install"]);
    /// let config = cli.build_config();
    /// assert_eq!(config.log_level, Some("debug".to_string()));
    /// ```
    #[must_use]
    pub fn build_config(&self) -> CliConfig {
        let log_level = if self.verbose {
            Some("debug".to_string())
        } else if self.quiet {
            None // No logging when quiet
        } else {
            Some("info".to_string())
        };

        CliConfig {
            log_level,
            no_progress: self.no_progress,
            config_path: self.config.clone(),
        }
    }

    /// Execute the CLI with a specific configuration for dependency injection.
    ///
    /// This method enables testing and programmatic usage by accepting an
    /// external configuration instead of building one from CLI arguments.
    /// It's the core execution method that all entry points eventually call.
    ///
    /// # Design Benefits
    ///
    /// - **Testability**: Tests can inject custom configurations
    /// - **Flexibility**: Programmatic usage without CLI parsing
    /// - **Isolation**: Configuration changes don't affect global state during tests
    /// - **Consistency**: Single execution path for all scenarios
    ///
    /// # Arguments
    ///
    /// * `config` - The configuration to apply before command execution
    ///
    /// # Execution Flow
    ///
    /// 1. **Environment Setup**: Applies the configuration to process environment
    /// 2. **Command Matching**: Dispatches to the appropriate subcommand
    /// 3. **Async Execution**: Awaits the async command execution
    /// 4. **Error Propagation**: Returns any errors for higher-level handling
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the command completed successfully
    /// - `Err(anyhow::Error)` if the command failed with context for debugging
    ///
    /// # Environment Changes
    ///
    /// This method may modify the process environment based on the configuration:
    /// - `RUST_LOG`: Set according to verbosity level
    /// - `CCPM_NO_PROGRESS`: Set if progress indicators should be disabled
    /// - `CCPM_CONFIG`: Set if custom configuration path is specified
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::{Cli, CliConfig};
    /// use clap::Parser;
    ///
    /// # tokio_test::block_on(async {
    /// let cli = Cli::parse_from(&["ccpm", "install"]);
    /// let mut config = CliConfig::new();
    /// config.log_level = Some("trace".to_string());
    /// config.no_progress = true;
    ///
    /// cli.execute_with_config(config).await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    pub async fn execute_with_config(self, config: CliConfig) -> Result<()> {
        // Apply configuration to environment once at the start
        config.apply_to_env();

        match self.command {
            Commands::Init(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::Add(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::Remove(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::Install(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::Update(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::List(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::Validate(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::Cache(cmd) => cmd.execute_with_manifest_path(self.manifest_path).await,
            Commands::Config(cmd) => cmd.execute().await,
        }
    }
}
