//! MCP (Model Context Protocol) server configuration management for AGPM.
//!
//! This module handles the integration of MCP servers with AGPM, including:
//! - Directly merging MCP server configurations into `.mcp.json` (no staging directory)
//! - Writing MCP server configurations to `.mcp.json` for Claude Code
//! - Managing AGPM-controlled MCP server configurations
//! - Preserving user-managed server configurations
//! - Safe atomic updates to MCP configuration files
//! - Multi-tool support via pluggable MCP handlers
//!
//! Note: Hooks and permissions are handled separately and stored in `.claude/settings.local.json`

// Module declarations
mod config;
pub mod handlers;
mod models;
mod operations;
mod settings;

#[cfg(test)]
mod tests;

// Re-export public types and functions
pub use models::{AgpmMetadata, ClaudeSettings, McpConfig, McpServerConfig};
pub use operations::{
    clean_mcp_servers, configure_mcp_servers, list_mcp_servers, merge_mcp_servers,
};
