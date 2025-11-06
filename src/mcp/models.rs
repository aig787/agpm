use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Settings structure for `.claude/settings.local.json`.
/// This represents the complete settings file that may contain various configurations.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ClaudeSettings {
    /// Map of server names to their configurations
    #[serde(rename = "mcpServers", skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,

    /// Hook configurations for event-based automation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Value>,

    /// Permissions configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Value>,

    /// Other settings preserved from the original file
    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}

/// The main MCP configuration file structure for `.mcp.json`.
///
/// This represents the complete MCP configuration file that Claude Code reads
/// to connect to MCP servers. The file may contain both AGPM-managed and
/// user-managed server configurations.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct McpConfig {
    /// Map of server names to their configurations
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// Individual MCP server configuration.
///
/// This structure represents a single MCP server entry in the `.mcp.json` file.
/// It supports both command-based and HTTP transport configurations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    /// The command to execute to start the server (command-based servers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments to pass to the command (command-based servers)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables to set when running the server (command-based servers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, Value>>,

    /// Transport type (HTTP-based servers) - Claude Code uses "type" field
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,

    /// Server URL (HTTP-based servers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// HTTP headers (HTTP-based servers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, Value>>,

    /// AGPM management metadata (only present for AGPM-managed servers)
    #[serde(rename = "_agpm", skip_serializing_if = "Option::is_none")]
    pub agpm_metadata: Option<AgpmMetadata>,
}

/// AGPM management metadata for tracking managed servers.
///
/// This metadata is added to server configurations that are managed by AGPM,
/// allowing us to distinguish between AGPM-managed and user-managed servers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgpmMetadata {
    /// Indicates this server is managed by AGPM
    pub managed: bool,

    /// Source repository
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Version or git reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Timestamp when the server was installed/updated
    pub installed_at: String,

    /// Original manifest dependency name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_name: Option<String>,
}
