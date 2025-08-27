//! MCP (Model Context Protocol) server configuration management for CCPM.
//!
//! This module handles the integration of MCP servers with CCPM, including:
//! - Storing raw MCP server configurations in `.claude/ccpm/mcp-servers/`
//! - Merging configurations into `.claude/settings.local.json`
//! - Managing CCPM-controlled MCP server configurations
//! - Preserving user-managed server configurations
//! - Safe atomic updates to shared configuration files

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
/// to connect to MCP servers. The file may contain both CCPM-managed and
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
/// It includes the command to run, arguments, environment variables, and
/// optional CCPM management metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// The command to execute to start the server
    pub command: String,

    /// Arguments to pass to the command
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables to set when running the server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, Value>>,

    /// CCPM management metadata (only present for CCPM-managed servers)
    #[serde(rename = "_ccpm", skip_serializing_if = "Option::is_none")]
    pub ccpm_metadata: Option<CcpmMetadata>,
}

/// CCPM management metadata for tracking managed servers.
///
/// This metadata is added to server configurations that are managed by CCPM,
/// allowing us to distinguish between CCPM-managed and user-managed servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcpmMetadata {
    /// Indicates this server is managed by CCPM
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
            let backup_path = path.with_extension("json.backup");
            std::fs::copy(path, &backup_path).with_context(|| {
                format!(
                    "Failed to create backup of settings at: {}",
                    backup_path.display()
                )
            })?;
        }

        // Write with pretty formatting for readability
        crate::utils::write_json_file(path, self, true)
            .with_context(|| format!("Failed to write settings to: {}", path.display()))?;

        Ok(())
    }

    /// Update MCP servers from stored configurations.
    ///
    /// This method loads all MCP server configurations from `.claude/ccpm/mcp-servers/`
    /// and merges them into the settings, preserving user-managed servers.
    pub fn update_mcp_servers(&mut self, mcp_servers_dir: &Path) -> Result<()> {
        if !mcp_servers_dir.exists() {
            return Ok(());
        }

        let mut ccpm_servers = HashMap::new();

        // Read all .json files from the mcp-servers directory
        for entry in std::fs::read_dir(mcp_servers_dir).with_context(|| {
            format!(
                "Failed to read MCP servers directory: {}",
                mcp_servers_dir.display()
            )
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
                    ccpm_servers.insert(name.to_string(), server_config);
                }
            }
        }

        // Initialize mcp_servers if None
        if self.mcp_servers.is_none() {
            self.mcp_servers = Some(HashMap::new());
        }

        // Update MCP servers, preserving user-managed ones
        if let Some(servers) = &mut self.mcp_servers {
            // Remove old CCPM-managed servers
            servers.retain(|_, config| {
                config
                    .ccpm_metadata
                    .as_ref()
                    .is_none_or(|meta| !meta.managed)
            });

            // Add all CCPM-managed servers
            servers.extend(ccpm_servers);
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
            let backup_path = path.with_extension("json.backup");
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

    /// Update only CCPM-managed servers, preserving user configurations.
    ///
    /// This method:
    /// 1. Removes old CCPM-managed servers not in the update set
    /// 2. Adds or updates CCPM-managed servers from the update set
    /// 3. Preserves all user-managed servers (those without CCPM metadata)
    pub fn update_managed_servers(
        &mut self,
        updates: HashMap<String, McpServerConfig>,
    ) -> Result<()> {
        // Build set of server names being updated
        let updating_names: std::collections::HashSet<_> = updates.keys().cloned().collect();

        // Remove old CCPM-managed servers not being updated
        self.mcp_servers.retain(|name, config| {
            // Keep if:
            // 1. It's not managed by CCPM (user server), OR
            // 2. It's being updated in this operation
            config
                .ccpm_metadata
                .as_ref()
                .is_none_or(|meta| !meta.managed || updating_names.contains(name))
        });

        // Add/update CCPM-managed servers
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
                // Conflict if the existing server is not managed by CCPM
                if existing.ccpm_metadata.is_none()
                    || !existing.ccpm_metadata.as_ref().unwrap().managed
                {
                    conflicts.push(name.clone());
                }
            }
        }

        conflicts
    }

    /// Remove all CCPM-managed servers.
    ///
    /// This is useful for cleanup operations.
    pub fn remove_all_managed(&mut self) {
        self.mcp_servers.retain(|_, config| {
            config
                .ccpm_metadata
                .as_ref()
                .is_none_or(|meta| !meta.managed)
        });
    }

    /// Get all CCPM-managed servers.
    #[must_use]
    pub fn get_managed_servers(&self) -> HashMap<String, &McpServerConfig> {
        self.mcp_servers
            .iter()
            .filter(|(_, config)| {
                config
                    .ccpm_metadata
                    .as_ref()
                    .is_some_and(|meta| meta.managed)
            })
            .map(|(name, config)| (name.clone(), config))
            .collect()
    }
}

/// MCP server dependency specification from the manifest.
///
/// This represents an MCP server dependency as specified in `ccpm.toml`.
/// It includes the same fields as regular dependencies plus MCP-specific fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDependency {
    /// Source repository name (for remote dependencies)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Path to the resource file in the source repository
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Version constraint (e.g., "v1.0.0", "^2.0", "latest")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Git branch name (e.g., "main", "develop")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Git commit hash (revision)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,

    /// The command to execute (required for MCP servers)
    pub command: String,

    /// Arguments to pass to the command
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables (supports ${VAR} expansion)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

impl McpServerDependency {
    /// Convert to an MCP server configuration for the `.mcp.json` file.
    #[must_use]
    pub fn to_mcp_config(&self, name: &str) -> McpServerConfig {
        McpServerConfig {
            command: self.resolve_command(),
            args: self.expand_args(),
            env: self.expand_env(),
            ccpm_metadata: Some(CcpmMetadata {
                managed: true,
                source: self.source.clone(),
                version: self.version.clone(),
                installed_at: Utc::now().to_rfc3339(),
                dependency_name: Some(name.to_string()),
            }),
        }
    }

    /// Resolve the command path.
    ///
    /// If the command is a relative or absolute path, it's returned as-is.
    /// Otherwise, it's assumed to be in PATH.
    fn resolve_command(&self) -> String {
        // TODO: Could use `which` crate to validate PATH commands
        self.command.clone()
    }

    /// Expand arguments with environment variable substitution.
    fn expand_args(&self) -> Vec<String> {
        self.args.iter().map(|arg| expand_env_vars(arg)).collect()
    }

    /// Expand environment variables in the env map.
    fn expand_env(&self) -> Option<HashMap<String, Value>> {
        self.env.as_ref().map(|env| {
            env.iter()
                .map(|(key, value)| {
                    let expanded = expand_env_vars(value);
                    (key.clone(), Value::String(expanded))
                })
                .collect()
        })
    }
}

/// Expand environment variables in a string.
///
/// Supports ${VAR} and $VAR syntax.
fn expand_env_vars(s: &str) -> String {
    // For now, return as-is and let the shell handle expansion
    // TODO: Implement proper environment variable expansion
    s.to_string()
}

/// Install MCP servers from the manifest into `.claude/ccpm/mcp-servers/` and update settings.
///
/// This function:
/// 1. Saves individual MCP server configs to `.claude/ccpm/mcp-servers/<name>.json`
/// 2. Updates `.claude/settings.local.json` with merged MCP configurations
/// 3. Returns locked MCP server entries for the lockfile
pub async fn install_mcp_servers(
    manifest: &crate::manifest::Manifest,
    project_root: &Path,
) -> Result<Vec<crate::lockfile::LockedMcpServer>> {
    if manifest.mcp_servers.is_empty() {
        return Ok(Vec::new());
    }

    let claude_dir = project_root.join(".claude");
    let mcp_servers_dir = claude_dir.join("mcp-servers");
    let settings_path = claude_dir.join("settings.local.json");

    // Ensure directories exist
    crate::utils::fs::ensure_dir(&mcp_servers_dir)?;

    // Validate commands exist
    for (name, dep) in &manifest.mcp_servers {
        validate_command(&dep.command)
            .with_context(|| format!("Invalid command for MCP server '{name}'"))?;
    }

    // Clean up old MCP server files that are no longer in the manifest
    if mcp_servers_dir.exists() {
        for entry in std::fs::read_dir(&mcp_servers_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if !manifest.mcp_servers.contains_key(name) {
                        std::fs::remove_file(&path).with_context(|| {
                            format!("Failed to remove old MCP server file: {}", path.display())
                        })?;
                    }
                }
            }
        }
    }

    // Save each MCP server configuration to its own file
    for (name, dep) in &manifest.mcp_servers {
        let server_config = dep.to_mcp_config(name);
        let server_path = mcp_servers_dir.join(format!("{name}.json"));

        crate::utils::write_json_file(&server_path, &server_config, true).with_context(|| {
            format!(
                "Failed to write MCP server config: {}",
                server_path.display()
            )
        })?;
    }

    // Load existing settings
    let mut settings = ClaudeSettings::load_or_default(&settings_path)?;

    // Update MCP servers from the stored configurations
    settings.update_mcp_servers(&mcp_servers_dir)?;

    // Check for conflicts with user-managed servers in existing settings
    if let Some(servers) = &settings.mcp_servers {
        let mut conflicts = Vec::new();
        for name in manifest.mcp_servers.keys() {
            if let Some(existing) = servers.get(name) {
                // Conflict if the existing server is not managed by CCPM
                if existing.ccpm_metadata.is_none()
                    || !existing.ccpm_metadata.as_ref().unwrap().managed
                {
                    conflicts.push(name.clone());
                }
            }
        }

        if !conflicts.is_empty() {
            return Err(anyhow::anyhow!(
                "The following MCP servers already exist and are not managed by CCPM: {}\n\
                 Please rename your servers or remove the existing ones from .claude/settings.local.json",
                conflicts.join(", ")
            ));
        }
    }

    // Save the updated settings
    settings.save(&settings_path)?;

    println!(
        "✓ Configured {} MCP server(s) in .claude/settings.local.json",
        manifest.mcp_servers.len()
    );

    // Build locked entries for the lockfile
    let locked_servers: Vec<crate::lockfile::LockedMcpServer> = manifest
        .mcp_servers
        .iter()
        .map(|(name, dep)| crate::lockfile::LockedMcpServer {
            name: name.clone(),
            command: dep.command.clone(),
            args: dep.args.clone(),
            source: dep.source.clone(),
            version: dep.version.clone(),
            package: None, // Not using package managers, only git sources
            configured_at: Utc::now().to_rfc3339(),
        })
        .collect();

    Ok(locked_servers)
}

/// Validate that a command exists and is executable.
fn validate_command(command: &str) -> Result<()> {
    // Skip validation for common package runners that might not be installed yet
    let skip_validation = ["npx", "uvx", "bunx", "deno"];
    if skip_validation.contains(&command) {
        return Ok(());
    }

    // If it's an absolute path, check it exists
    if command.starts_with('/') || command.starts_with("./") {
        let path = Path::new(command);
        if !path.exists() {
            return Err(anyhow::anyhow!(
                "Command '{}' does not exist.\n\
                 Please ensure the file exists and has execute permissions.",
                command
            ));
        }
    }
    // For commands in PATH, we'll let the system handle it at runtime
    // TODO: Could use `which` crate for better validation

    Ok(())
}

/// Remove all CCPM-managed MCP servers from the configuration.
pub fn clean_mcp_servers(project_root: &Path) -> Result<()> {
    let claude_dir = project_root.join(".claude");
    let mcp_servers_dir = claude_dir.join("mcp-servers");
    let settings_path = claude_dir.join("settings.local.json");

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

    // Update settings to remove CCPM-managed servers
    if settings_path.exists() {
        let mut settings = ClaudeSettings::load_or_default(&settings_path)?;
        if let Some(servers) = &mut settings.mcp_servers {
            servers.retain(|_, config| {
                config
                    .ccpm_metadata
                    .as_ref()
                    .is_none_or(|meta| !meta.managed)
            });
        }
        settings.save(&settings_path)?;
    }

    if removed_count == 0 {
        println!("No CCPM-managed MCP servers found");
    } else {
        println!("✓ Removed {removed_count} CCPM-managed MCP server(s)");
    }

    Ok(())
}

/// List all MCP servers, indicating which are CCPM-managed.
pub fn list_mcp_servers(project_root: &Path) -> Result<()> {
    let settings_path = project_root.join(".claude/settings.local.json");

    if !settings_path.exists() {
        println!("No .claude/settings.local.json file found");
        return Ok(());
    }

    let settings = ClaudeSettings::load_or_default(&settings_path)?;

    let servers = settings.mcp_servers.as_ref();
    if servers.is_none() || servers.as_ref().unwrap().is_empty() {
        println!("No MCP servers configured");
        return Ok(());
    }

    let servers = servers.unwrap();
    println!("MCP Servers:");
    println!("╭─────────────────────┬──────────┬───────────╮");
    println!("│ Name                │ Managed  │ Version   │");
    println!("├─────────────────────┼──────────┼───────────┤");

    for (name, server) in servers {
        let (managed, version) = if let Some(meta) = &server.ccpm_metadata {
            if meta.managed {
                ("✓ (ccpm)", meta.version.as_deref().unwrap_or("-"))
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

    #[test]
    fn test_claude_settings_load_save() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("settings.local.json");

        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();
        servers.insert(
            "test-server".to_string(),
            McpServerConfig {
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                env: None,
                ccpm_metadata: None,
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
        let settings_path = temp.path().join("settings.local.json");
        let backup_path = temp.path().join("settings.local.json.backup");

        // Create initial file
        fs::write(&settings_path, r#"{"test": "value"}"#).unwrap();

        let settings = ClaudeSettings::default();
        settings.save(&settings_path).unwrap();

        // Backup should be created
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
            command: "managed".to_string(),
            args: vec![],
            env: None,
            ccpm_metadata: Some(CcpmMetadata {
                managed: true,
                source: Some("test".to_string()),
                version: Some("v1.0.0".to_string()),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                dependency_name: Some("ccpm-server".to_string()),
            }),
        };
        let config_path = mcp_servers_dir.join("ccpm-server.json");
        let json = serde_json::to_string_pretty(&server_config).unwrap();
        std::fs::write(&config_path, json).unwrap();

        // Add a user-managed server to settings
        let mut settings = ClaudeSettings::default();
        let mut servers = HashMap::new();
        servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: "custom".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: None,
            },
        );
        settings.mcp_servers = Some(servers);

        // Update from directory
        settings.update_mcp_servers(&mcp_servers_dir).unwrap();

        // Both servers should be present
        assert!(settings.mcp_servers.is_some());
        let servers = settings.mcp_servers.as_ref().unwrap();
        assert!(servers.contains_key("user-server"));
        assert!(servers.contains_key("ccpm-server"));
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
                command: "user-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: None,
            },
        );

        // Old CCPM-managed server (should be removed)
        servers.insert(
            "old-managed".to_string(),
            McpServerConfig {
                command: "old-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
            command: "new-managed".to_string(),
            args: vec![],
            env: None,
            ccpm_metadata: Some(CcpmMetadata {
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
            command: "test".to_string(),
            args: vec![],
            env: None,
            ccpm_metadata: None,
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
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                env: Some({
                    let mut env = HashMap::new();
                    env.insert("NODE_ENV".to_string(), json!("production"));
                    env
                }),
                ccpm_metadata: None,
            },
        );

        config.save(&config_path).unwrap();

        let loaded = McpConfig::load_or_default(&config_path).unwrap();
        assert!(loaded.mcp_servers.contains_key("test-server"));
        let server = &loaded.mcp_servers["test-server"];
        assert_eq!(server.command, "node");
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
        let config_path = temp.path().join("mcp.json");
        let backup_path = temp.path().join("mcp.json.backup");

        // Create initial file
        fs::write(
            &config_path,
            r#"{"mcpServers": {"old": {"command": "old"}}}"#,
        )
        .unwrap();

        let config = McpConfig::default();
        config.save(&config_path).unwrap();

        // Backup should be created
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
                command: "user-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: None,
            },
        );

        // Add old CCPM-managed server
        config.mcp_servers.insert(
            "old-managed".to_string(),
            McpServerConfig {
                command: "old-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "new-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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

        // Add CCPM-managed server that will be updated
        config.mcp_servers.insert(
            "updating-server".to_string(),
            McpServerConfig {
                command: "old-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "new-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
        assert_eq!(server.command, "new-command");
        assert_eq!(
            server.ccpm_metadata.as_ref().unwrap().version,
            Some("v2.0.0".to_string())
        );
    }

    #[test]
    fn test_mcp_config_check_conflicts() {
        let mut config = McpConfig::default();

        // Add user-managed server
        config.mcp_servers.insert(
            "user-server".to_string(),
            McpServerConfig {
                command: "user-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: None,
            },
        );

        // Add CCPM-managed server
        config.mcp_servers.insert(
            "managed-server".to_string(),
            McpServerConfig {
                command: "managed-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "new-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "updated-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "new-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "user-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "new-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "user-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: None,
            },
        );

        config.mcp_servers.insert(
            "managed-server".to_string(),
            McpServerConfig {
                command: "managed-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "unmanaged-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "user-command".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: None,
            },
        );

        config.mcp_servers.insert(
            "managed-server1".to_string(),
            McpServerConfig {
                command: "managed-command1".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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
                command: "managed-command2".to_string(),
                args: vec![],
                env: None,
                ccpm_metadata: Some(CcpmMetadata {
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

    // McpServerDependency tests
    #[test]
    fn test_mcp_server_dependency_to_config() {
        let dep = McpServerDependency {
            source: Some("test-source".to_string()),
            path: Some("servers/test.js".to_string()),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: "node".to_string(),
            args: vec![
                "test.js".to_string(),
                "--port".to_string(),
                "3000".to_string(),
            ],
            env: Some({
                let mut env = HashMap::new();
                env.insert("NODE_ENV".to_string(), "production".to_string());
                env.insert("DATABASE_URL".to_string(), "${DATABASE_URL}".to_string());
                env
            }),
        };

        let config = dep.to_mcp_config("test-server");

        assert_eq!(config.command, "node");
        assert_eq!(config.args, vec!["test.js", "--port", "3000"]);
        assert!(config.env.is_some());

        let env = config.env.as_ref().unwrap();
        assert_eq!(env.get("NODE_ENV"), Some(&json!("production")));
        assert_eq!(env.get("DATABASE_URL"), Some(&json!("${DATABASE_URL}")));

        let metadata = config.ccpm_metadata.as_ref().unwrap();
        assert!(metadata.managed);
        assert_eq!(metadata.source, Some("test-source".to_string()));
        assert_eq!(metadata.version, Some("v1.0.0".to_string()));
        assert_eq!(metadata.dependency_name, Some("test-server".to_string()));
        assert!(!metadata.installed_at.is_empty());
    }

    #[test]
    fn test_mcp_server_dependency_minimal() {
        let dep = McpServerDependency {
            source: None,
            path: None,
            version: None,
            branch: None,
            rev: None,
            command: "simple-command".to_string(),
            args: vec![],
            env: None,
        };

        let config = dep.to_mcp_config("simple");

        assert_eq!(config.command, "simple-command");
        assert!(config.args.is_empty());
        assert!(config.env.is_none());

        let metadata = config.ccpm_metadata.as_ref().unwrap();
        assert!(metadata.managed);
        assert!(metadata.source.is_none());
        assert!(metadata.version.is_none());
        assert_eq!(metadata.dependency_name, Some("simple".to_string()));
    }

    #[test]
    fn test_expand_env_vars() {
        // Currently a placeholder implementation
        assert_eq!(expand_env_vars("${HOME}"), "${HOME}");
        assert_eq!(expand_env_vars("$USER"), "$USER");
        assert_eq!(expand_env_vars("literal"), "literal");
        assert_eq!(expand_env_vars(""), "");
    }

    #[test]
    fn test_validate_command_skip_validation() {
        // These should pass without validation
        assert!(validate_command("npx").is_ok());
        assert!(validate_command("uvx").is_ok());
        assert!(validate_command("bunx").is_ok());
        assert!(validate_command("deno").is_ok());
    }

    #[test]
    fn test_validate_command_absolute_path_nonexistent() {
        let result = validate_command("/nonexistent/path/to/command");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_validate_command_relative_path_nonexistent() {
        let result = validate_command("./nonexistent_command");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_validate_command_regular_command() {
        // Regular commands in PATH should pass (we don't validate them)
        assert!(validate_command("node").is_ok());
        assert!(validate_command("python").is_ok());
        assert!(validate_command("custom-command").is_ok());
    }

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
                command: "test-cmd".to_string(),
                args: vec!["arg1".to_string()],
                env: Some({
                    let mut env = HashMap::new();
                    env.insert("VAR".to_string(), json!("value"));
                    env
                }),
                ccpm_metadata: Some(CcpmMetadata {
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
        assert_eq!(
            deserialized.other.get("custom"),
            settings.other.get("custom")
        );
    }

    #[test]
    fn test_mcp_config_serialization() {
        let mut config = McpConfig::default();

        config.mcp_servers.insert(
            "test".to_string(),
            McpServerConfig {
                command: "test-cmd".to_string(),
                args: vec!["arg1".to_string(), "arg2".to_string()],
                env: Some({
                    let mut env = HashMap::new();
                    env.insert("TEST_VAR".to_string(), json!("test_value"));
                    env
                }),
                ccpm_metadata: Some(CcpmMetadata {
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
        assert_eq!(server.command, "test-cmd");
        assert_eq!(server.args.len(), 2);
        assert!(server.env.is_some());
        assert!(server.ccpm_metadata.is_some());

        let metadata = server.ccpm_metadata.as_ref().unwrap();
        assert!(metadata.managed);
        assert_eq!(metadata.source, Some("github.com/test/repo".to_string()));
        assert_eq!(metadata.version, Some("v2.0.0".to_string()));
    }

    #[test]
    fn test_mcp_server_config_minimal_serialization() {
        let config = McpServerConfig {
            command: "minimal".to_string(),
            args: vec![],
            env: None,
            ccpm_metadata: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: McpServerConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.command, "minimal");
        assert!(deserialized.args.is_empty());
        assert!(deserialized.env.is_none());
        assert!(deserialized.ccpm_metadata.is_none());

        // Check that empty args are skipped in serialization
        assert!(!json.contains(r#""args":[]"#));
    }

    #[test]
    fn test_ccpm_metadata_serialization() {
        let metadata = CcpmMetadata {
            managed: true,
            source: Some("test-source".to_string()),
            version: None,
            installed_at: "2024-01-01T00:00:00Z".to_string(),
            dependency_name: Some("test-dep".to_string()),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: CcpmMetadata = serde_json::from_str(&json).unwrap();

        assert!(deserialized.managed);
        assert_eq!(deserialized.source, Some("test-source".to_string()));
        assert_eq!(deserialized.version, None);
        assert_eq!(deserialized.installed_at, "2024-01-01T00:00:00Z");
        assert_eq!(deserialized.dependency_name, Some("test-dep".to_string()));

        // Check that None version is skipped in serialization
        assert!(!json.contains(r#""version""#));
    }

    #[test]
    fn test_mcp_server_dependency_serialization() {
        let dep = McpServerDependency {
            source: Some("test-source".to_string()),
            path: None,
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: "test-command".to_string(),
            args: vec!["arg1".to_string()],
            env: Some({
                let mut env = HashMap::new();
                env.insert("VAR".to_string(), "value".to_string());
                env
            }),
        };

        let json = serde_json::to_string(&dep).unwrap();
        let deserialized: McpServerDependency = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.source, Some("test-source".to_string()));
        assert_eq!(deserialized.path, None);
        assert_eq!(deserialized.version, Some("v1.0.0".to_string()));
        assert_eq!(deserialized.command, "test-command");
        assert_eq!(deserialized.args, vec!["arg1"]);
        assert!(deserialized.env.is_some());

        // Check that None fields are skipped in serialization
        assert!(!json.contains(r#""path""#));
        assert!(!json.contains(r#""git""#));
    }
}
