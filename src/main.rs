//! CCPM CLI entry point
//!
//! This is the main executable for the Claude Code Package Manager.
//! It handles command-line argument parsing, error display, and command execution.
//!
//! The CLI supports various commands for managing Claude Code resources:
//! - `init` - Initialize a new ccpm.toml manifest
//! - `install` - Install dependencies from ccpm.toml
//! - `update` - Update dependencies within version constraints
//! - `list` - List installed resources
//! - `validate` - Validate manifest and lockfile
//! - `cache` - Manage the global Git cache
//! - `config` - Manage global configuration
//! - `add` - Add sources or dependencies to manifest

use anyhow::Result;
use ccpm::cli;
use ccpm::core::error::user_friendly_error;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = cli::Cli::parse();

    // Set up colored output for Windows
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).ok();

    // Execute the command
    match cli.execute().await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Convert to user-friendly error with context and suggestions
            let error_ctx = user_friendly_error(e);
            error_ctx.display();
            std::process::exit(1);
        }
    }
}
