use crate::mcp::models::{ClaudeSettings, McpServerConfig};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

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
