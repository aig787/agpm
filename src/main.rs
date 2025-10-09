//! AGPM CLI entry point
//!
//! This is the main executable for the AGent Package Manager.
//! It handles command-line argument parsing, error display, and command execution.
//!
//! The CLI supports various commands for managing Claude Code resources:
//! - `init` - Initialize a new agpm.toml manifest
//! - `install` - Install dependencies from agpm.toml
//! - `update` - Update dependencies within version constraints
//! - `list` - List installed resources
//! - `validate` - Validate manifest and lockfile
//! - `cache` - Manage the global Git cache
//! - `config` - Manage global configuration
//! - `add` - Add sources or dependencies to manifest
//! - `remove` - Remove sources or dependencies from manifest

use agpm_cli::cli;
use agpm_cli::core::error::user_friendly_error;
use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = cli::Cli::parse();

    // Build configuration from CLI args
    let config = cli.build_config();

    // Determine logging level based on the rules:
    // 1. If RUST_LOG is specified and verbose flag isn't set - use RUST_LOG level
    // 2. If RUST_LOG is specified and verbose flag is set - use RUST_LOG level
    // 3. If RUST_LOG is NOT specified and verbose flag is set - use INFO level
    // 4. If RUST_LOG is NOT specified and verbose flag is NOT set - no logging

    let rust_log_exists = std::env::var("RUST_LOG").is_ok();
    let is_verbose = config.log_level == Some("debug".to_string());

    let filter = if rust_log_exists {
        // Rules 1 & 2: RUST_LOG is set, use it regardless of verbose flag
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("error"))
    } else if is_verbose {
        // Rule 3: No RUST_LOG but verbose flag is set, use DEBUG level
        // Verbose flag should surface detailed debug logs without requiring RUST_LOG
        EnvFilter::new("debug")
    } else {
        // Rule 4: No RUST_LOG and no verbose flag, no logging
        EnvFilter::new("off")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false) // Don't show the module path in logs
        .with_thread_ids(false) // Don't show thread IDs
        .init();

    // Set up colored output for Windows
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).ok();

    // Execute the command (execute_with_config will apply the rest of the config)
    match cli.execute_with_config(config).await {
        Ok(()) => Ok(()),
        Err(e) => {
            // Convert to user-friendly error with context and suggestions
            let error_ctx = user_friendly_error(e);
            error_ctx.display();
            std::process::exit(1);
        }
    }
}
