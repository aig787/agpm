use crate::mcp::models::{McpConfig, McpServerConfig};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

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
