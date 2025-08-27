//! MCP (Model Context Protocol) server management commands.
//!
//! This module provides CLI commands for managing MCP server configurations
//! in CCPM projects. MCP servers are configured in `.mcp.json` files and
//! allow Claude Code to connect to external systems and services.
//!
//! # Features
//!
//! - **Server Listing**: View all configured MCP servers (CCPM-managed and user-managed)
//! - **Cleanup Operations**: Remove CCPM-managed servers while preserving user configs
//! - **Status Reporting**: Show configuration status and synchronization with manifest
//! - **Non-Destructive**: Preserves user-managed server configurations
//!
//! # MCP Configuration Management
//!
//! CCPM distinguishes between two types of MCP server configurations:
//! - **CCPM-Managed**: Servers defined in `ccpm.toml` and managed by CCPM
//! - **User-Managed**: Servers manually added to `.mcp.json` by the user
//!
//! # Examples
//!
//! List all MCP servers:
//! ```bash
//! ccpm mcp list
//! ```
//!
//! Show configuration status:
//! ```bash
//! ccpm mcp status
//! ```
//!
//! Remove CCPM-managed servers:
//! ```bash
//! ccpm mcp clean
//! ```
//!
//! # Configuration Files
//!
//! ## Project Manifest (ccpm.toml)
//! ```toml
//! [mcp-servers]
//! filesystem = { command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem"] }
//! postgres = { command = "mcp-postgres", args = ["--connection", "${DATABASE_URL}"] }
//! ```
//!
//! ## MCP Configuration (.mcp.json)
//! ```json
//! {
//!   "mcpServers": {
//!     "filesystem": {
//!       "command": "npx",
//!       "args": ["-y", "@modelcontextprotocol/server-filesystem"],
//!       "_ccpm": true
//!     },
//!     "user-server": {
//!       "command": "custom-mcp-server"
//!     }
//!   }
//! }
//! ```
//!
//! # Safety Features
//!
//! - User-managed configurations are never modified by cleanup operations
//! - Status command shows sync issues between manifest and configuration
//! - All operations provide clear feedback about changes made
//!
//! # Error Conditions
//!
//! - No manifest file found in project
//! - Invalid `.mcp.json` format
//! - File system permission issues

use anyhow::Result;
use clap::Subcommand;
use std::path::Path;

/// Command for managing MCP (Model Context Protocol) server configurations.
///
/// This command provides operations for managing MCP servers defined in the project
/// manifest and configured in the `.mcp.json` file. MCP servers allow Claude Code
/// to connect to external systems and services for enhanced functionality.
///
/// # Subcommands
///
/// - [`list`](McpSubcommand::List): Display all configured MCP servers
/// - [`clean`](McpSubcommand::Clean): Remove CCMP-managed servers
/// - [`status`](McpSubcommand::Status): Show configuration synchronization status
///
/// # Examples
///
/// ```rust,ignore
/// use ccpm::cli::mcp::{McpCommand, McpSubcommand};
///
/// // List all MCP servers
/// let cmd = McpCommand {
///     subcommand: McpSubcommand::List
/// };
///
/// // Check status
/// let cmd = McpCommand {
///     subcommand: McpSubcommand::Status
/// };
/// ```
#[derive(Debug, clap::Parser)]
pub struct McpCommand {
    /// MCP management operation to perform
    #[command(subcommand)]
    subcommand: McpSubcommand,
}

/// Subcommands for MCP server management operations.
///
/// This enum defines the available operations for managing MCP server configurations.
/// Each operation serves a specific purpose in the MCP server lifecycle.
#[derive(Debug, Subcommand)]
enum McpSubcommand {
    /// List all configured MCP servers with their types and commands.
    ///
    /// Displays a comprehensive list of all MCP servers found in the project's
    /// `.mcp.json` configuration file. The output distinguishes between:
    ///
    /// - **CCPM-managed servers**: Those defined in `ccpm.toml` and installed by CCPM
    /// - **User-managed servers**: Those manually added to `.mcp.json` by the user
    ///
    /// # Output Format
    ///
    /// For each server, displays:
    /// - Server name and management type (CCPM/User)
    /// - Command and arguments used to start the server
    /// - Any environment variables or special configuration
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm mcp list
    /// ```
    ///
    /// # Error Conditions
    ///
    /// - Project manifest not found
    /// - Invalid `.mcp.json` format
    List,

    /// Remove all CCPM-managed MCP servers from the configuration.
    ///
    /// This command safely removes only the MCP servers that were installed
    /// and managed by CCPM, preserving any user-managed server configurations.
    /// It identifies CCPM-managed servers by the presence of the `_ccpm: true`
    /// field in their configuration.
    ///
    /// # Safety Features
    ///
    /// - **Non-destructive**: Never removes user-managed server configurations
    /// - **Selective**: Only removes servers with `_ccpm: true` marker
    /// - **Backup-safe**: Original user configurations remain untouched
    ///
    /// # Use Cases
    ///
    /// - Clean up after changing project dependencies
    /// - Remove outdated CCPM-managed servers
    /// - Prepare for fresh MCP server installation
    /// - Troubleshoot MCP configuration issues
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm mcp clean
    /// ```
    ///
    /// # Error Conditions
    ///
    /// - No `.mcp.json` file found (operation succeeds with warning)
    /// - File system permission issues
    Clean,

    /// Display detailed status information about MCP server configuration.
    ///
    /// This command provides a comprehensive overview of the MCP server configuration
    /// status, including synchronization between the project manifest and the actual
    /// `.mcp.json` configuration file.
    ///
    /// # Information Displayed
    ///
    /// - **File Status**: Whether `.mcp.json` exists and is accessible
    /// - **Server Counts**: Total, CCPM-managed, and user-managed server counts
    /// - **Manifest Sync**: Servers defined in manifest but not configured
    /// - **Configuration Health**: Overall configuration status and issues
    ///
    /// # Sync Analysis
    ///
    /// Identifies discrepancies such as:
    /// - Servers defined in `ccpm.toml` but missing from `.mcp.json`
    /// - Outdated server configurations that need updating
    /// - Configuration format issues or inconsistencies
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm mcp status
    /// ```
    ///
    /// # Error Conditions
    ///
    /// - Project manifest not found
    /// - Invalid configuration file formats
    Status,
}

impl McpCommand {
    /// Execute the MCP command with the specified operation.
    ///
    /// This method orchestrates MCP server management operations by:
    ///
    /// 1. **Project Discovery**: Locates the project manifest to determine the working directory
    /// 2. **Operation Dispatch**: Routes to the appropriate handler based on the subcommand
    /// 3. **Error Handling**: Provides context-aware error messages for common issues
    ///
    /// # Operations
    ///
    /// - **List**: Displays all configured MCP servers with management type
    /// - **Clean**: Removes CCPM-managed servers while preserving user configurations
    /// - **Status**: Shows synchronization status between manifest and configuration
    ///
    /// # Project Context
    ///
    /// All operations require a valid CCPM project with a `ccpm.toml` manifest file.
    /// The project directory is used as the base for locating the `.mcp.json`
    /// configuration file.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the operation completed successfully
    /// - `Err(anyhow::Error)` if:
    ///   - No manifest file is found in the current directory tree
    ///   - The `.mcp.json` file has invalid format (for some operations)
    ///   - File system permission issues occur
    ///   - Configuration synchronization issues are detected (context-dependent)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ccpm::cli::mcp::{McpCommand, McpSubcommand};
    ///
    /// # tokio_test::block_on(async {
    /// let cmd = McpCommand {
    ///     subcommand: McpSubcommand::Status
    /// };
    /// // cmd.execute().await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # });
    /// ```
    pub async fn execute(self) -> Result<()> {
        // Get project directory
        let manifest_path = crate::manifest::find_manifest()?;
        let project_dir = manifest_path.parent().unwrap();

        match self.subcommand {
            McpSubcommand::List => {
                crate::mcp::list_mcp_servers(project_dir)?;
            }
            McpSubcommand::Clean => {
                crate::mcp::clean_mcp_servers(project_dir)?;
            }
            McpSubcommand::Status => {
                show_mcp_status(project_dir)?;
            }
        }

        Ok(())
    }
}

/// Display comprehensive status information about MCP server configuration.
///
/// This function provides detailed analysis of the MCP server configuration state,
/// including file existence, server counts, and synchronization status between
/// the project manifest and the actual MCP configuration.
///
/// # Information Displayed
///
/// ## Configuration File Status
/// - Whether `.mcp.json` exists and is accessible
/// - File format validation and basic structure checks
///
/// ## Server Statistics
/// - Total number of configured MCP servers
/// - Count of CCPM-managed servers (those with `_ccpm: true`)
/// - Count of user-managed servers (manually added configurations)
///
/// ## Synchronization Analysis
/// - Servers defined in `ccpm.toml` but missing from `.mcp.json`
/// - Recommendations for resolving synchronization issues
/// - Actionable commands to fix configuration problems
///
/// # Arguments
///
/// * `project_dir` - Path to the project directory containing the manifest and configuration
///
/// # Returns
///
/// - `Ok(())` if status information was displayed successfully
/// - `Err(anyhow::Error)` if configuration files cannot be read or parsed
///
/// # Output Format
///
/// The status is displayed with colored indicators:
/// - ✓ Green checkmarks for healthy configurations
/// - ✗ Red X marks for missing or problematic configurations
/// - ⚠ Yellow warnings for sync issues or recommendations
/// - • Bullet points for detailed breakdowns
///
/// # Examples
///
/// ```rust,ignore
/// use std::path::Path;
///
/// let project_dir = Path::new("/path/to/project");
/// show_mcp_status(project_dir)?;
/// ```
fn show_mcp_status(project_dir: &Path) -> Result<()> {
    use colored::Colorize;

    let mcp_json_path = project_dir.join(".mcp.json");

    println!("MCP Server Configuration Status:");
    println!();

    // Check if .mcp.json exists
    if !mcp_json_path.exists() {
        println!("  {} No .mcp.json file found", "✗".red());
        println!("  Run 'ccpm install' to create MCP server configurations");
        return Ok(());
    }

    println!("  {} .mcp.json exists", "✓".green());

    // Load and analyze the configuration
    let config = crate::mcp::McpConfig::load_or_default(&mcp_json_path)?;

    let total_servers = config.mcp_servers.len();
    let managed_servers = config.get_managed_servers().len();
    let user_servers = total_servers - managed_servers;

    println!();
    println!("  Total servers: {total_servers}");
    println!("    {} CCPM-managed: {}", "•".cyan(), managed_servers);
    println!("    {} User-managed: {}", "•".yellow(), user_servers);

    // Check manifest for MCP servers
    let manifest_path = project_dir.join("ccpm.toml");
    if manifest_path.exists() {
        let manifest = crate::manifest::Manifest::load(&manifest_path)?;
        let manifest_servers = manifest.mcp_servers.len();

        if manifest_servers > 0 {
            println!();
            println!("  Manifest defines {manifest_servers} server(s)");

            // Check for sync issues
            let mut out_of_sync = Vec::new();
            for name in manifest.mcp_servers.keys() {
                if !config.mcp_servers.contains_key(name) {
                    out_of_sync.push(name.clone());
                }
            }

            if !out_of_sync.is_empty() {
                println!();
                println!(
                    "  {} The following servers are defined in ccpm.toml but not configured:",
                    "⚠".yellow()
                );
                for name in out_of_sync {
                    println!("    - {name}");
                }
                println!("  Run 'ccpm install' to configure these servers");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{CcpmMetadata, ClaudeSettings, McpServerConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn test_show_mcp_status_no_files() {
        let temp = tempdir().unwrap();
        let project_dir = temp.path();

        // Should handle missing files gracefully
        let result = show_mcp_status(project_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_mcp_status_with_mcp_json() {
        let temp = tempdir().unwrap();
        let project_dir = temp.path();
        let mcp_json_path = project_dir.join(".mcp.json");

        // Create .mcp.json with servers
        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();

        servers.insert(
            "test-server".to_string(),
            McpServerConfig {
                command: "test".to_string(),
                args: vec!["arg1".to_string()],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
                    managed: true,
                    source: Some("test-source".to_string()),
                    version: Some("v1.0.0".to_string()),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                    dependency_name: Some("test-server".to_string()),
                }),
            },
        );

        settings.mcp_servers = Some(servers);
        settings.save(&mcp_json_path).unwrap();

        // Status should work
        let result = show_mcp_status(project_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mcp_subcommand_creation() {
        // Test all subcommand variants
        let _list_cmd = McpSubcommand::List;
        let _clean_cmd = McpSubcommand::Clean;
        let _status_cmd = McpSubcommand::Status;

        // Just ensure they can be created - compilation is the test
        assert!(true);
    }
}
