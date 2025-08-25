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
//! - Windows (Command Prompt, PowerShell)
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
mod config;
mod init;
mod install;
mod list;
mod update;
pub mod validate;

#[cfg(test)]
mod tests;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Runtime configuration for CLI execution
/// This struct holds configuration that would otherwise be set as environment variables,
/// allowing for dependency injection and better testability.
#[derive(Debug, Clone, Default)]
pub struct CliConfig {
    /// Log level (e.g., "debug", "info", "warn", "error")
    pub log_level: Option<String>,
    /// Whether to disable progress indicators
    pub no_progress: bool,
    /// Custom config file path
    pub config_path: Option<String>,
}

impl CliConfig {
    /// Create a new config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply this configuration to the environment
    /// This is called once at the start of execution
    pub fn apply_to_env(&self) {
        if let Some(ref level) = self.log_level {
            std::env::set_var("RUST_LOG", level);
        }

        if self.no_progress {
            std::env::set_var("CCPM_NO_PROGRESS", "1");
        }

        if let Some(ref path) = self.config_path {
            std::env::set_var("CCPM_CONFIG", path);
        }
    }
}

#[derive(Parser)]
#[command(
    name = "ccpm",
    about = "Claude Code Package Manager - Manage Claude Code resources",
    version,
    author,
    long_about = "CCPM is a Git-based package manager for Claude Code resources including agents, snippets, and more."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Path to config file
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Disable progress bars and spinners
    #[arg(long, global = true)]
    no_progress: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new ccpm.toml manifest file
    Init(init::InitCommand),

    /// Add sources or dependencies to ccpm.toml
    Add(add::AddCommand),

    /// Install resources from ccpm.toml
    Install(install::InstallCommand),

    /// Update installed resources
    Update(update::UpdateCommand),

    /// List installed resources
    List(list::ListCommand),

    /// Validate ccpm.toml configuration
    Validate(validate::ValidateCommand),

    /// Manage the global cache
    Cache(cache::CacheCommand),

    /// Manage global configuration
    Config(config::ConfigCommand),
}

impl Cli {
    /// Execute the CLI with default configuration
    pub async fn execute(self) -> Result<()> {
        let config = self.build_config();
        self.execute_with_config(config).await
    }

    /// Build a CliConfig from the CLI arguments
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

    /// Execute the CLI with a specific configuration
    /// This method allows for dependency injection in tests
    pub async fn execute_with_config(self, config: CliConfig) -> Result<()> {
        // Apply configuration to environment once at the start
        config.apply_to_env();

        match self.command {
            Commands::Init(cmd) => cmd.execute().await,
            Commands::Add(cmd) => cmd.execute().await,
            Commands::Install(cmd) => cmd.execute().await,
            Commands::Update(cmd) => cmd.execute().await,
            Commands::List(cmd) => cmd.execute().await,
            Commands::Validate(cmd) => cmd.execute().await,
            Commands::Cache(cmd) => cmd.execute().await,
            Commands::Config(cmd) => cmd.execute().await,
        }
    }
}
