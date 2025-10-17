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

pub mod handlers;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl ClaudeSettings {
    /// Load an existing `.claude/settings.local.json` file or create a new configuration.
    ///
    /// This method preserves all existing configurations.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            crate::utils::read_json_file(path).with_context(|| {
                format!(
                    "Failed to parse settings file: {}\n\
                     The file may be malformed or contain invalid JSON.",
                    path.display()
                )
            })
        } else {
            Ok(Self::default())
        }
    }

    /// Save the settings to `.claude/settings.local.json` file.
    ///
    /// The file is written atomically to prevent corruption.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Create a backup if the file exists
        if path.exists() {
            // Generate backup path at project root: .agpm/backups/claude-code/settings.local.json
            let backup_path = crate::utils::generate_backup_path(path, "claude-code")?;

            // Ensure backup directory exists
            if let Some(backup_dir) = backup_path.parent() {
                if !backup_dir.exists() {
                    std::fs::create_dir_all(backup_dir).with_context(|| {
                        format!("Failed to create directory: {}", backup_dir.display())
                    })?;
                }
            }

            std::fs::copy(path, &backup_path).with_context(|| {
                format!("Failed to create backup of settings at: {}", backup_path.display())
            })?;
        }

        // Write with pretty formatting for readability
        crate::utils::write_json_file(path, self, true)
            .with_context(|| format!("Failed to write settings to: {}", path.display()))?;

        Ok(())
    }

    /// Update MCP servers from stored configurations.
    ///
    /// This method loads all MCP server configurations from the specified directory
    /// and merges them into the settings, preserving user-managed servers.
    pub fn update_mcp_servers(&mut self, mcp_servers_dir: &Path) -> Result<()> {
        if !mcp_servers_dir.exists() {
            return Ok(());
        }

        let mut agpm_servers = HashMap::new();

        // Read all .json files from the mcp-servers directory
        for entry in std::fs::read_dir(mcp_servers_dir).with_context(|| {
            format!("Failed to read MCP servers directory: {}", mcp_servers_dir.display())
        })? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "json") {
                let server_config: McpServerConfig = crate::utils::read_json_file(&path)
                    .with_context(|| {
                        format!("Failed to parse MCP server file: {}", path.display())
                    })?;

                // Use the filename without extension as the server name
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    agpm_servers.insert(name.to_string(), server_config);
                }
            }
        }

        // Initialize mcp_servers if None
        if self.mcp_servers.is_none() {
            self.mcp_servers = Some(HashMap::new());
        }

        // Update MCP servers, preserving user-managed ones
        if let Some(servers) = &mut self.mcp_servers {
            // Remove old AGPM-managed servers
            servers
                .retain(|_, config| config.agpm_metadata.as_ref().is_none_or(|meta| !meta.managed));

            // Add all AGPM-managed servers
            servers.extend(agpm_servers);
        }

        Ok(())
    }
}

impl McpConfig {
    /// Load an existing `.mcp.json` file or create a new empty configuration.
    ///
    /// This method preserves all existing server configurations, including
    /// user-managed ones.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            // Parse with lenient error handling to preserve user configurations
            crate::utils::read_json_file(path).with_context(|| {
                format!(
                    "Failed to parse MCP configuration file: {}\n\
                     The file may be malformed or contain invalid JSON.",
                    path.display()
                )
            })
        } else {
            Ok(Self::default())
        }
    }

    /// Save the configuration to a `.mcp.json` file.
    ///
    /// The file is written atomically to prevent corruption.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Create a backup if the file exists
        if path.exists() {
            // Generate backup path at project root: .agpm/backups/claude-code/.mcp.json
            let backup_path = crate::utils::generate_backup_path(path, "claude-code")?;

            // Ensure backup directory exists
            if let Some(backup_dir) = backup_path.parent() {
                if !backup_dir.exists() {
                    std::fs::create_dir_all(backup_dir).with_context(|| {
                        format!("Failed to create directory: {}", backup_dir.display())
                    })?;
                }
            }

            std::fs::copy(path, &backup_path).with_context(|| {
                format!(
                    "Failed to create backup of MCP configuration at: {}",
                    backup_path.display()
                )
            })?;
        }

        // Write with pretty formatting for readability
        crate::utils::write_json_file(path, self, true)
            .with_context(|| format!("Failed to write MCP configuration to: {}", path.display()))?;

        Ok(())
    }

    /// Update only AGPM-managed servers, preserving user configurations.
    ///
    /// This method:
    /// 1. Removes old AGPM-managed servers not in the update set
    /// 2. Adds or updates AGPM-managed servers from the update set
    /// 3. Preserves all user-managed servers (those without AGPM metadata)
    pub fn update_managed_servers(
        &mut self,
        updates: HashMap<String, McpServerConfig>,
    ) -> Result<()> {
        // Build set of server names being updated
        let updating_names: std::collections::HashSet<_> = updates.keys().cloned().collect();

        // Remove old AGPM-managed servers not being updated
        self.mcp_servers.retain(|name, config| {
            // Keep if:
            // 1. It's not managed by AGPM (user server), OR
            // 2. It's being updated in this operation
            config
                .agpm_metadata
                .as_ref()
                .is_none_or(|meta| !meta.managed || updating_names.contains(name))
        });

        // Add/update AGPM-managed servers
        for (name, config) in updates {
            self.mcp_servers.insert(name, config);
        }

        Ok(())
    }

    /// Check for conflicts with user-managed servers.
    ///
    /// Returns a list of server names that would conflict with existing
    /// user-managed servers.
    #[must_use]
    pub fn check_conflicts(&self, new_servers: &HashMap<String, McpServerConfig>) -> Vec<String> {
        let mut conflicts = Vec::new();

        for name in new_servers.keys() {
            if let Some(existing) = self.mcp_servers.get(name) {
                // Conflict if the existing server is not managed by AGPM
                if existing.agpm_metadata.is_none()
                    || !existing.agpm_metadata.as_ref().unwrap().managed
                {
                    conflicts.push(name.clone());
                }
            }
        }

        conflicts
    }

    /// Remove all AGPM-managed servers.
    ///
    /// This is useful for cleanup operations.
    pub fn remove_all_managed(&mut self) {
        self.mcp_servers
            .retain(|_, config| config.agpm_metadata.as_ref().is_none_or(|meta| !meta.managed));
    }

    /// Get all AGPM-managed servers.
    #[must_use]
    pub fn get_managed_servers(&self) -> HashMap<String, &McpServerConfig> {
        self.mcp_servers
            .iter()
            .filter(|(_, config)| config.agpm_metadata.as_ref().is_some_and(|meta| meta.managed))
            .map(|(name, config)| (name.clone(), config))
            .collect()
    }
}

/// Merge MCP server configurations into the config file.
///
/// This is a helper function used by MCP handlers to merge server configurations
/// that have already been read from source files.
pub async fn merge_mcp_servers(
    mcp_config_path: &Path,
    agpm_servers: HashMap<String, McpServerConfig>,
) -> Result<()> {
    if agpm_servers.is_empty() {
        return Ok(());
    }

    // Load existing MCP configuration
    let mut mcp_config = McpConfig::load_or_default(mcp_config_path)?;

    // Check for conflicts with user-managed servers
    let conflicts = mcp_config.check_conflicts(&agpm_servers);
    if !conflicts.is_empty() {
        return Err(anyhow::anyhow!(
            "The following MCP servers already exist and are not managed by AGPM: {}\n\
             Please rename your servers or remove the existing ones from .mcp.json",
            conflicts.join(", ")
        ));
    }

    // Update MCP configuration with AGPM-managed servers
    mcp_config.update_managed_servers(agpm_servers)?;

    // Save the updated MCP configuration
    mcp_config.save(mcp_config_path)?;

    Ok(())
}

pub async fn configure_mcp_servers(project_root: &Path, mcp_servers_dir: &Path) -> Result<()> {
    if !mcp_servers_dir.exists() {
        return Ok(());
    }

    let mcp_config_path = project_root.join(".mcp.json");

    // Read all MCP server JSON files
    let mut agpm_servers = HashMap::new();
    let mut entries = tokio::fs::read_dir(mcp_servers_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "json")
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
        {
            // Read and parse the MCP server configuration
            let config: McpServerConfig = crate::utils::read_json_file(&path)
                .with_context(|| format!("Failed to parse MCP server file: {}", path.display()))?;

            // Add AGPM metadata
            let mut config_with_metadata = config;
            if config_with_metadata.agpm_metadata.is_none() {
                config_with_metadata.agpm_metadata = Some(AgpmMetadata {
                    managed: true,
                    source: Some("agpm".to_string()),
                    version: None,
                    installed_at: Utc::now().to_rfc3339(),
                    dependency_name: Some(name.to_string()),
                });
            }

            agpm_servers.insert(name.to_string(), config_with_metadata);
        }
    }

    // Use the helper function to merge servers
    merge_mcp_servers(&mcp_config_path, agpm_servers).await
}

/// Remove all AGPM-managed MCP servers from the configuration.
pub fn clean_mcp_servers(project_root: &Path) -> Result<()> {
    let claude_dir = project_root.join(".claude");
    let agpm_dir = claude_dir.join("agpm");
    let mcp_servers_dir = agpm_dir.join("mcp-servers");
    let mcp_config_path = project_root.join(".mcp.json");

    // Remove all files from mcp-servers directory
    let mut removed_count = 0;
    if mcp_servers_dir.exists() {
        for entry in std::fs::read_dir(&mcp_servers_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                std::fs::remove_file(&path).with_context(|| {
                    format!("Failed to remove MCP server file: {}", path.display())
                })?;
                removed_count += 1;
            }
        }
    }

    // Update MCP config to remove AGPM-managed servers
    if mcp_config_path.exists() {
        let mut mcp_config = McpConfig::load_or_default(&mcp_config_path)?;
        mcp_config.remove_all_managed();
        mcp_config.save(&mcp_config_path)?;
    }

    if removed_count == 0 {
        println!("No AGPM-managed MCP servers found");
    } else {
        println!("✓ Removed {removed_count} AGPM-managed MCP server(s)");
    }

    Ok(())
}

/// List all MCP servers, indicating which are AGPM-managed.
pub fn list_mcp_servers(project_root: &Path) -> Result<()> {
    let mcp_config_path = project_root.join(".mcp.json");

    if !mcp_config_path.exists() {
        println!("No .mcp.json file found");
        return Ok(());
    }

    let mcp_config = McpConfig::load_or_default(&mcp_config_path)?;

    if mcp_config.mcp_servers.is_empty() {
        println!("No MCP servers configured");
        return Ok(());
    }

    let servers = &mcp_config.mcp_servers;
    println!("MCP Servers:");
    println!("╭─────────────────────┬──────────┬───────────╮");
    println!("│ Name                │ Managed  │ Version   │");
    println!("├─────────────────────┼──────────┼───────────┤");

    for (name, server) in servers {
        let (managed, version) = if let Some(meta) = &server.agpm_metadata {
            if meta.managed {
                ("✓ (agpm)", meta.version.as_deref().unwrap_or("-"))
            } else {
                ("✗", "-")
            }
        } else {
            ("✗", "-")
        };

        println!("│ {name:<19} │ {managed:<8} │ {version:<9} │");
    }

    println!("╰─────────────────────┴──────────┴───────────╯");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    /// Test helper: Creates agpm.toml in temp directory so find_project_root works
    fn setup_project_root(temp_path: &std::path::Path) {
        fs::write(temp_path.join("agpm.toml"), "[dependencies]\n").unwrap();
    }

    #[test]
    fn test_claude_settings_load_save() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("settings.local.json");

        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();
        servers.insert(
            "test-server".to_string(),
            McpServerConfig {
                command: Some("node".to_string()),
                args: vec!["server.js".to_string()],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );
        settings.mcp_servers = Some(servers);

        settings.save(&settings_path).unwrap();

        let loaded = ClaudeSettings::load_or_default(&settings_path).unwrap();
        assert!(loaded.mcp_servers.is_some());
        let servers = loaded.mcp_servers.unwrap();
        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("test-server"));
    }

    #[test]
    fn test_claude_settings_load_nonexistent_file() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("nonexistent.json");

        let settings = ClaudeSettings::load_or_default(&settings_path).unwrap();
        assert!(settings.mcp_servers.is_none());
        assert!(settings.permissions.is_none());
        assert!(settings.other.is_empty());
    }

    #[test]
    fn test_claude_settings_load_invalid_json() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("invalid.json");
        fs::write(&settings_path, "invalid json {").unwrap();

        let result = ClaudeSettings::load_or_default(&settings_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse"));
    }

    #[test]
    fn test_claude_settings_save_creates_backup() {
        let temp = tempdir().unwrap();
        setup_project_root(temp.path());

        let settings_path = temp.path().join("settings.local.json");
        let backup_path = temp
            .path()
            .join(".agpm")
            .join("backups")
            .join("claude-code")
            .join("settings.local.json");

        // Create initial file
        fs::write(&settings_path, r#"{"test": "value"}"#).unwrap();

        let settings = ClaudeSettings::default();
        settings.save(&settings_path).unwrap();

        // Backup should be created in .agpm/backups/claude-code directory
        assert!(backup_path.exists());
        let backup_content = fs::read_to_string(backup_path).unwrap();
        assert_eq!(backup_content, r#"{"test": "value"}"#);
    }

    #[test]
    fn test_claude_settings_update_mcp_servers_empty_dir() {
        let temp = tempdir().unwrap();
        let nonexistent_dir = temp.path().join("nonexistent");

        let mut settings = ClaudeSettings::default();
        // Should not error on nonexistent directory
        settings.update_mcp_servers(&nonexistent_dir).unwrap();
    }

    #[test]
    fn test_update_mcp_servers_from_directory() {
        let temp = tempdir().unwrap();
        let mcp_servers_dir = temp.path().join("mcp-servers");
        std::fs::create_dir(&mcp_servers_dir).unwrap();

        // Create a server config file
        let server_config = McpServerConfig {
            command: Some("managed".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: Some("agpm-server".to_string()),
            }),
        };
        let config_path = mcp_servers_dir.join("agpm-server.json");
        let json = serde_json::to_string_pretty(&server_config).unwrap();
        std::fs::write(&config_path, json).unwrap();

        // Add a user-managed server to settings
        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();
        servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("custom".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );
        settings.mcp_servers = Some(servers);

        // Update from directory
        settings.update_mcp_servers(&mcp_servers_dir).unwrap();

        // Both servers should be present
        assert!(settings.mcp_servers.is_some());
        let servers = settings.mcp_servers.as_ref().unwrap();
        assert!(servers.contains_key("user-server"));
        assert!(servers.contains_key("agpm-server"));
        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn test_update_mcp_servers_replaces_old_managed() {
        let temp = tempdir().unwrap();
        let mcp_servers_dir = temp.path().join("mcp-servers");
        std::fs::create_dir(&mcp_servers_dir).unwrap();

        // Start with existing managed and user servers
        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();

        // User-managed server (should be preserved)
        servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        // Old AGPM-managed server (should be removed)
        servers.insert(
            "old-managed".to_string(),
            McpServerConfig {
                command: Some("old-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: Some("old-source".to_string()),
                    version: Some("v0.1.0".to_string()),
                    installed_at: "2023-01-01T00:00:00Z".to_string(),
                    dependency_name: Some("old-managed".to_string()),
                }),
            },
        );

        settings.mcp_servers = Some(servers);

        // Create new managed server config file
        let server_config = McpServerConfig {
            command: Some("new-managed".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("new-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: Some("new-managed".to_string()),
            }),
        };
        let config_path = mcp_servers_dir.join("new-managed.json");
        let json = serde_json::to_string_pretty(&server_config).unwrap();
        std::fs::write(&config_path, json).unwrap();

        // Update from directory
        settings.update_mcp_servers(&mcp_servers_dir).unwrap();

        let servers = settings.mcp_servers.as_ref().unwrap();
        assert!(servers.contains_key("user-server")); // User server preserved
        assert!(servers.contains_key("new-managed")); // New managed server added
        assert!(!servers.contains_key("old-managed")); // Old managed server removed
        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn test_update_mcp_servers_invalid_json_file() {
        let temp = tempdir().unwrap();
        let mcp_servers_dir = temp.path().join("mcp-servers");
        std::fs::create_dir(&mcp_servers_dir).unwrap();

        // Create invalid JSON file
        let invalid_path = mcp_servers_dir.join("invalid.json");
        std::fs::write(&invalid_path, "invalid json {").unwrap();

        let mut settings = ClaudeSettings::default();
        let result = settings.update_mcp_servers(&mcp_servers_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse"));
    }

    #[test]
    fn test_update_mcp_servers_ignores_non_json_files() {
        let temp = tempdir().unwrap();
        let mcp_servers_dir = temp.path().join("mcp-servers");
        std::fs::create_dir(&mcp_servers_dir).unwrap();

        // Create non-JSON file
        let txt_path = mcp_servers_dir.join("readme.txt");
        std::fs::write(&txt_path, "This is not a JSON file").unwrap();

        // Create valid JSON file
        let server_config = McpServerConfig {
            command: Some("test".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        };
        let json_path = mcp_servers_dir.join("valid.json");
        let json = serde_json::to_string_pretty(&server_config).unwrap();
        std::fs::write(&json_path, json).unwrap();

        let mut settings = ClaudeSettings::default();
        settings.update_mcp_servers(&mcp_servers_dir).unwrap();

        let servers = settings.mcp_servers.as_ref().unwrap();
        assert!(servers.contains_key("valid"));
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn test_settings_preserves_other_fields() {
        let temp = tempdir().unwrap();
        setup_project_root(temp.path());

        let settings_path = temp.path().join("settings.local.json");

        // Create a settings file with various fields
        let json = r#"{
            "permissions": {
                "allow": ["Bash(ls)"],
                "deny": []
            },
            "customField": "value",
            "mcpServers": {
                "test": {
                    "command": "node",
                    "args": []
                }
            }
        }"#;
        std::fs::write(&settings_path, json).unwrap();

        // Load and save
        let settings = ClaudeSettings::load_or_default(&settings_path).unwrap();
        assert!(settings.permissions.is_some());
        assert!(settings.mcp_servers.is_some());
        assert!(settings.other.contains_key("customField"));

        settings.save(&settings_path).unwrap();

        // Reload and verify all fields preserved
        let reloaded = ClaudeSettings::load_or_default(&settings_path).unwrap();
        assert!(reloaded.permissions.is_some());
        assert!(reloaded.mcp_servers.is_some());
        assert!(reloaded.other.contains_key("customField"));
    }

    // McpConfig tests
    #[test]
    fn test_mcp_config_load_save() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("mcp.json");

        let mut config = McpConfig::default();
        config.mcp_servers.insert(
            "test-server".to_string(),
            McpServerConfig {
                command: Some("node".to_string()),
                args: vec!["server.js".to_string()],
                env: Some({
                    let mut env = HashMap::new();
                    env.insert("NODE_ENV".to_string(), json!("production"));
                    env
                }),
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        config.save(&config_path).unwrap();

        let loaded = McpConfig::load_or_default(&config_path).unwrap();
        assert!(loaded.mcp_servers.contains_key("test-server"));
        let server = &loaded.mcp_servers["test-server"];
        assert_eq!(server.command, Some("node".to_string()));
        assert_eq!(server.args, vec!["server.js"]);
        assert!(server.env.is_some());
    }

    #[test]
    fn test_mcp_config_load_nonexistent() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("nonexistent.json");

        let config = McpConfig::load_or_default(&config_path).unwrap();
        assert!(config.mcp_servers.is_empty());
    }

    #[test]
    fn test_mcp_config_load_invalid_json() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("invalid.json");
        fs::write(&config_path, "invalid json {").unwrap();

        let result = McpConfig::load_or_default(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_mcp_config_save_creates_backup() {
        let temp = tempdir().unwrap();
        setup_project_root(temp.path());

        let config_path = temp.path().join("mcp.json");
        let backup_path =
            temp.path().join(".agpm").join("backups").join("claude-code").join("mcp.json");

        // Create initial file
        fs::write(&config_path, r#"{"mcpServers": {"old": {"command": "old"}}}"#).unwrap();

        let config = McpConfig::default();
        config.save(&config_path).unwrap();

        // Backup should be created in .agpm/backups/claude-code directory
        assert!(backup_path.exists());
        let backup_content = fs::read_to_string(backup_path).unwrap();
        assert!(backup_content.contains("old"));
    }

    #[test]
    fn test_mcp_config_update_managed_servers() {
        let mut config = McpConfig::default();

        // Add user-managed server
        config.mcp_servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        // Add old AGPM-managed server
        config.mcp_servers.insert(
            "old-managed".to_string(),
            McpServerConfig {
                command: Some("old-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "old-time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        // Update with new managed servers
        let mut updates = HashMap::new();
        updates.insert(
            "new-managed".to_string(),
            McpServerConfig {
                command: Some("new-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "new-time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        config.update_managed_servers(updates).unwrap();

        // User server should be preserved, old managed should be removed, new managed added
        assert!(config.mcp_servers.contains_key("user-server"));
        assert!(config.mcp_servers.contains_key("new-managed"));
        assert!(!config.mcp_servers.contains_key("old-managed"));
        assert_eq!(config.mcp_servers.len(), 2);
    }

    #[test]
    fn test_mcp_config_update_managed_servers_preserves_updating_servers() {
        let mut config = McpConfig::default();

        // Add AGPM-managed server that will be updated
        config.mcp_servers.insert(
            "updating-server".to_string(),
            McpServerConfig {
                command: Some("old-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: Some("v1.0.0".to_string()),
                    installed_at: "old-time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        // Update with new version of the same server
        let mut updates = HashMap::new();
        updates.insert(
            "updating-server".to_string(),
            McpServerConfig {
                command: Some("new-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: Some("v2.0.0".to_string()),
                    installed_at: "new-time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        config.update_managed_servers(updates).unwrap();

        assert!(config.mcp_servers.contains_key("updating-server"));
        let server = &config.mcp_servers["updating-server"];
        assert_eq!(server.command, Some("new-command".to_string()));
        assert_eq!(server.agpm_metadata.as_ref().unwrap().version, Some("v2.0.0".to_string()));
    }

    #[test]
    fn test_mcp_config_check_conflicts() {
        let mut config = McpConfig::default();

        // Add user-managed server
        config.mcp_servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        // Add AGPM-managed server
        config.mcp_servers.insert(
            "managed-server".to_string(),
            McpServerConfig {
                command: Some("managed-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        let mut new_servers = HashMap::new();
        new_servers.insert(
            "user-server".to_string(), // This conflicts
            McpServerConfig {
                command: Some("new-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );
        new_servers.insert(
            "managed-server".to_string(), // This doesn't conflict (already managed)
            McpServerConfig {
                command: Some("updated-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );
        new_servers.insert(
            "new-server".to_string(), // This doesn't conflict (new)
            McpServerConfig {
                command: Some("new-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        let conflicts = config.check_conflicts(&new_servers);
        assert_eq!(conflicts, vec!["user-server"]);
    }

    #[test]
    fn test_mcp_config_check_conflicts_unmanaged_metadata() {
        let mut config = McpConfig::default();

        // Add server with metadata but managed=false
        config.mcp_servers.insert(
            "unmanaged-server".to_string(),
            McpServerConfig {
                command: Some("user-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: false,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        let mut new_servers = HashMap::new();
        new_servers.insert(
            "unmanaged-server".to_string(),
            McpServerConfig {
                command: Some("new-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        let conflicts = config.check_conflicts(&new_servers);
        assert_eq!(conflicts, vec!["unmanaged-server"]);
    }

    #[test]
    fn test_mcp_config_remove_all_managed() {
        let mut config = McpConfig::default();

        // Add mixed servers
        config.mcp_servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        config.mcp_servers.insert(
            "managed-server".to_string(),
            McpServerConfig {
                command: Some("managed-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        config.mcp_servers.insert(
            "unmanaged-with-metadata".to_string(),
            McpServerConfig {
                command: Some("unmanaged-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: false,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        config.remove_all_managed();

        assert!(config.mcp_servers.contains_key("user-server"));
        assert!(config.mcp_servers.contains_key("unmanaged-with-metadata"));
        assert!(!config.mcp_servers.contains_key("managed-server"));
        assert_eq!(config.mcp_servers.len(), 2);
    }

    #[test]
    fn test_mcp_config_get_managed_servers() {
        let mut config = McpConfig::default();

        // Add mixed servers
        config.mcp_servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user-command".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        config.mcp_servers.insert(
            "managed-server1".to_string(),
            McpServerConfig {
                command: Some("managed-command1".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: None,
                    version: None,
                    installed_at: "time".to_string(),
                    dependency_name: None,
                }),
            },
        );

        config.mcp_servers.insert(
            "managed-server2".to_string(),
            McpServerConfig {
                command: Some("managed-command2".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: Some("source".to_string()),
                    version: Some("v1.0.0".to_string()),
                    installed_at: "time".to_string(),
                    dependency_name: Some("dep".to_string()),
                }),
            },
        );

        let managed = config.get_managed_servers();
        assert_eq!(managed.len(), 2);
        assert!(managed.contains_key("managed-server1"));
        assert!(managed.contains_key("managed-server2"));
        assert!(!managed.contains_key("user-server"));
    }

    // Tests for configure_mcp_servers function would go here
    // Since MCP servers now use standard ResourceDependency and file-based approach,
    // the old McpServerDependency tests are no longer applicable

    // Serialization tests
    #[test]
    fn test_claude_settings_serialization() {
        // Add various fields
        let mut settings = ClaudeSettings {
            permissions: Some(json!({"allow": ["test"], "deny": []})),
            ..Default::default()
        };

        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            McpServerConfig {
                command: Some("test-cmd".to_string()),
                args: vec!["arg1".to_string()],
                env: Some({
                    let mut env = HashMap::new();
                    env.insert("VAR".to_string(), json!("value"));
                    env
                }),
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: Some("test-source".to_string()),
                    version: Some("v1.0.0".to_string()),
                    installed_at: "2024-01-01T00:00:00Z".to_string(),
                    dependency_name: Some("test".to_string()),
                }),
            },
        );
        settings.mcp_servers = Some(servers);

        settings.other.insert("custom".to_string(), json!("value"));

        // Serialize and deserialize
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: ClaudeSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.permissions, settings.permissions);
        assert_eq!(deserialized.mcp_servers.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.other.get("custom"), settings.other.get("custom"));
    }

    #[test]
    fn test_mcp_config_serialization() {
        let mut config = McpConfig::default();

        config.mcp_servers.insert(
            "test".to_string(),
            McpServerConfig {
                command: Some("test-cmd".to_string()),
                args: vec!["arg1".to_string(), "arg2".to_string()],
                env: Some({
                    let mut env = HashMap::new();
                    env.insert("TEST_VAR".to_string(), json!("test_value"));
                    env
                }),
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: Some("github.com/test/repo".to_string()),
                    version: Some("v2.0.0".to_string()),
                    installed_at: "2024-01-01T12:00:00Z".to_string(),
                    dependency_name: Some("test-dep".to_string()),
                }),
            },
        );

        // Serialize and deserialize
        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: McpConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.mcp_servers.len(), 1);
        let server = &deserialized.mcp_servers["test"];
        assert_eq!(server.command, Some("test-cmd".to_string()));
        assert_eq!(server.args.len(), 2);
        assert!(server.env.is_some());
        assert!(server.agpm_metadata.is_some());

        let metadata = server.agpm_metadata.as_ref().unwrap();
        assert!(metadata.managed);
        assert_eq!(metadata.source, Some("github.com/test/repo".to_string()));
        assert_eq!(metadata.version, Some("v2.0.0".to_string()));
    }

    #[test]
    fn test_mcp_server_config_minimal_serialization() {
        let config = McpServerConfig {
            command: Some("minimal".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: McpServerConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.command, Some("minimal".to_string()));
        assert!(deserialized.args.is_empty());
        assert!(deserialized.env.is_none());
        assert!(deserialized.agpm_metadata.is_none());

        // Check that empty args are skipped in serialization
        assert!(!json.contains(r#""args":[]"#));
    }

    #[test]
    fn test_agpm_metadata_serialization() {
        let metadata = AgpmMetadata {
            managed: true,
            source: Some("test-source".to_string()),
            version: None,
            installed_at: "2024-01-01T00:00:00Z".to_string(),
            dependency_name: Some("test-dep".to_string()),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: AgpmMetadata = serde_json::from_str(&json).unwrap();

        assert!(deserialized.managed);
        assert_eq!(deserialized.source, Some("test-source".to_string()));
        assert_eq!(deserialized.version, None);
        assert_eq!(deserialized.installed_at, "2024-01-01T00:00:00Z");
        assert_eq!(deserialized.dependency_name, Some("test-dep".to_string()));

        // Check that None version is skipped in serialization
        assert!(!json.contains(r#""version""#));
    }

    #[test]
    fn test_clean_mcp_servers() {
        let temp = tempfile::TempDir::new().unwrap();
        setup_project_root(temp.path());

        let project_root = temp.path();
        let claude_dir = project_root.join(".claude");
        let agpm_dir = claude_dir.join("agpm");
        let mcp_servers_dir = agpm_dir.join("mcp-servers");
        let settings_path = claude_dir.join("settings.local.json");
        let mcp_config_path = project_root.join(".mcp.json");

        // Create directory structure
        std::fs::create_dir_all(&mcp_servers_dir).unwrap();

        // Create MCP server files
        let server1_path = mcp_servers_dir.join("server1.json");
        let server2_path = mcp_servers_dir.join("server2.json");
        let server_config = McpServerConfig {
            command: Some("test".to_string()),
            args: vec![],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("test-source".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: Some("test-server".to_string()),
            }),
        };
        crate::utils::write_json_file(&server1_path, &server_config, true).unwrap();
        crate::utils::write_json_file(&server2_path, &server_config, true).unwrap();

        // Create settings with both AGPM-managed and user-managed servers
        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();

        // AGPM-managed server
        servers.insert(
            "agpm-server".to_string(),
            McpServerConfig {
                command: Some("agpm-cmd".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: Some("test".to_string()),
                    version: Some("v1.0.0".to_string()),
                    installed_at: "2024-01-01T00:00:00Z".to_string(),
                    dependency_name: None,
                }),
            },
        );

        // User-managed server
        servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user-cmd".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        settings.mcp_servers = Some(servers);
        settings.save(&settings_path).unwrap();

        // Create .mcp.json file with the same servers
        let mut mcp_config = McpConfig::default();
        mcp_config.mcp_servers.insert(
            "agpm-server".to_string(),
            McpServerConfig {
                command: Some("agpm-cmd".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: Some("test".to_string()),
                    version: Some("v1.0.0".to_string()),
                    installed_at: "2024-01-01T00:00:00Z".to_string(),
                    dependency_name: None,
                }),
            },
        );
        mcp_config.mcp_servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user-cmd".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );
        mcp_config.save(&mcp_config_path).unwrap();

        // Run clean_mcp_servers
        clean_mcp_servers(project_root).unwrap();

        // Verify MCP server files are deleted
        assert!(!server1_path.exists());
        assert!(!server2_path.exists());

        // Verify .mcp.json only contains user-managed servers
        let updated_mcp_config = McpConfig::load_or_default(&mcp_config_path).unwrap();
        assert_eq!(updated_mcp_config.mcp_servers.len(), 1);
        assert!(updated_mcp_config.mcp_servers.contains_key("user-server"));
        assert!(!updated_mcp_config.mcp_servers.contains_key("agpm-server"));
    }

    #[test]
    fn test_clean_mcp_servers_no_servers() {
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Run clean_mcp_servers on empty project
        let result = clean_mcp_servers(project_root);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_mcp_servers() {
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();
        let claude_dir = project_root.join(".claude");
        let settings_path = claude_dir.join("settings.local.json");

        std::fs::create_dir_all(&claude_dir).unwrap();

        // Create settings with mixed servers
        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();

        servers.insert(
            "managed-server".to_string(),
            McpServerConfig {
                command: Some("managed".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: Some(AgpmMetadata {
                    managed: true,
                    source: Some("test".to_string()),
                    version: Some("v2.0.0".to_string()),
                    installed_at: "2024-01-01T00:00:00Z".to_string(),
                    dependency_name: None,
                }),
            },
        );

        servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );

        settings.mcp_servers = Some(servers);
        settings.save(&settings_path).unwrap();

        // Run list_mcp_servers - just verify it doesn't error
        let result = list_mcp_servers(project_root);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_mcp_servers_no_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Run list_mcp_servers with no settings file
        let result = list_mcp_servers(project_root);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_mcp_servers_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();
        let claude_dir = project_root.join(".claude");
        let settings_path = claude_dir.join("settings.local.json");

        std::fs::create_dir_all(&claude_dir).unwrap();

        // Create settings with no servers
        let settings = ClaudeSettings::default();
        settings.save(&settings_path).unwrap();

        // Run list_mcp_servers
        let result = list_mcp_servers(project_root);
        assert!(result.is_ok());
    }

    #[test]
    fn test_claude_settings_save_backup() {
        let temp = tempfile::TempDir::new().unwrap();
        setup_project_root(temp.path());

        let settings_path = temp.path().join("settings.local.json");
        let backup_path = temp
            .path()
            .join(".agpm")
            .join("backups")
            .join("claude-code")
            .join("settings.local.json");

        // Create initial settings
        let settings1 = ClaudeSettings::default();
        settings1.save(&settings_path).unwrap();
        assert!(settings_path.exists());
        assert!(!backup_path.exists());

        // Save again to trigger backup
        let settings2 = ClaudeSettings {
            hooks: Some(serde_json::json!({"test": "hook"})),
            ..Default::default()
        };
        settings2.save(&settings_path).unwrap();

        // Verify backup was created in agpm directory
        assert!(backup_path.exists());

        // Verify backup contains original content
        let backup_content: ClaudeSettings = crate::utils::read_json_file(&backup_path).unwrap();
        assert!(backup_content.hooks.is_none());

        // Verify main file has new content
        let main_content: ClaudeSettings = crate::utils::read_json_file(&settings_path).unwrap();
        assert!(main_content.hooks.is_some());
    }

    #[test]
    fn test_mcp_config_save_backup() {
        let temp = tempfile::TempDir::new().unwrap();
        setup_project_root(temp.path());

        let config_path = temp.path().join(".mcp.json");
        let backup_path =
            temp.path().join(".agpm").join("backups").join("claude-code").join(".mcp.json");

        // Create initial config
        let config1 = McpConfig::default();
        config1.save(&config_path).unwrap();
        assert!(config_path.exists());
        assert!(!backup_path.exists());

        // Save again with changes to trigger backup
        let mut config2 = McpConfig::default();
        config2.mcp_servers.insert(
            "test".to_string(),
            McpServerConfig {
                command: Some("test-cmd".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );
        config2.save(&config_path).unwrap();

        // Verify backup was created in .agpm/backups directory
        assert!(backup_path.exists());

        // Verify backup contains original content
        let backup_content: McpConfig = crate::utils::read_json_file(&backup_path).unwrap();
        assert!(backup_content.mcp_servers.is_empty());

        // Verify main file has new content
        let main_content: McpConfig = crate::utils::read_json_file(&config_path).unwrap();
        assert_eq!(main_content.mcp_servers.len(), 1);
    }

    #[test]
    fn test_backup_fails_without_project_root() {
        // Test that backup creation fails gracefully when no agpm.toml exists
        let temp = tempfile::TempDir::new().unwrap();
        // Deliberately NOT calling setup_project_root here

        let settings_path = temp.path().join("settings.local.json");

        // Create initial file
        fs::write(&settings_path, r#"{"test": "value"}"#).unwrap();

        let settings = ClaudeSettings::default();
        let result = settings.save(&settings_path);

        // Should fail with helpful error message about missing project root
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Failed to find project root") || error_msg.contains("agpm.toml")
        );
    }

    #[test]
    fn test_update_mcp_servers_preserves_user_servers() {
        let temp = tempfile::TempDir::new().unwrap();
        let agpm_dir = temp.path().join(".claude").join("agpm");
        let mcp_servers_dir = agpm_dir.join("mcp-servers");
        std::fs::create_dir_all(&mcp_servers_dir).unwrap();

        // Create server config files
        let server1 = McpServerConfig {
            command: Some("server1".to_string()),
            args: vec!["arg1".to_string()],
            env: None,
            r#type: None,
            url: None,
            headers: None,
            agpm_metadata: Some(AgpmMetadata {
                managed: true,
                source: Some("source1".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: None,
            }),
        };
        crate::utils::write_json_file(&mcp_servers_dir.join("server1.json"), &server1, true)
            .unwrap();

        // Create settings with existing user server
        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();
        servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: Some("user".to_string()),
                args: vec![],
                env: None,
                r#type: None,
                url: None,
                headers: None,
                agpm_metadata: None,
            },
        );
        settings.mcp_servers = Some(servers);

        // Update from directory
        settings.update_mcp_servers(&mcp_servers_dir).unwrap();

        // Verify both servers are present
        let servers = settings.mcp_servers.as_ref().unwrap();
        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("user-server"));
        assert!(servers.contains_key("server1"));

        // Verify server1 config matches
        let server1_config = servers.get("server1").unwrap();
        assert_eq!(server1_config.command, Some("server1".to_string()));
        assert_eq!(server1_config.args, vec!["arg1"]);
    }

    #[test]
    fn test_update_mcp_servers_nonexistent_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let nonexistent_dir = temp.path().join("nonexistent");

        let mut settings = ClaudeSettings::default();
        let result = settings.update_mcp_servers(&nonexistent_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mcp_config_handles_extra_fields() {
        // McpConfig doesn't preserve other fields, but it should parse files with extra fields
        let json_str = r#"{
            "mcpServers": {
                "test": {
                    "command": "test",
                    "args": []
                }
            },
            "customField": "value",
            "anotherField": {
                "nested": true
            }
        }"#;

        let temp = tempdir().unwrap();
        let config_path = temp.path().join(".mcp.json");
        std::fs::write(&config_path, json_str).unwrap();

        // Should parse successfully ignoring extra fields
        let config = McpConfig::load_or_default(&config_path).unwrap();
        assert!(config.mcp_servers.contains_key("test"));
        assert_eq!(config.mcp_servers.len(), 1);
    }
}
