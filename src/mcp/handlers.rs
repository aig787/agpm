//! MCP server installation handlers for different artifact types.
//!
//! This module provides a pluggable handler system for installing MCP servers
//! into different tools' configuration formats (Claude Code, OpenCode, etc.).

use anyhow::{Context, Result};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

/// Trait for handling MCP server installation for different artifact types.
///
/// Each artifact type (claude-code, opencode, etc.) may have different ways
/// of managing MCP servers. This trait provides a common interface for:
/// - Installing MCP server configurations
/// - Determining where to install files (if applicable)
/// - Merging configurations into tool-specific formats
pub trait McpHandler: Send + Sync {
    /// Get the name of this MCP handler (e.g., "claude-code", "opencode").
    fn name(&self) -> &str;

    /// Determine whether this handler copies MCP config files to disk.
    ///
    /// Returns `true` if the handler needs MCP config files copied to a directory
    /// (e.g., Claude Code copies to `.claude/agpm/mcp-servers/`).
    ///
    /// Returns `false` if the handler merges directly without file copies
    /// (e.g., OpenCode merges directly into `opencode.json`).
    fn requires_file_installation(&self) -> bool;

    /// Get the directory path where MCP config files should be copied.
    ///
    /// Only called if `requires_file_installation()` returns `true`.
    ///
    /// # Arguments
    ///
    /// * `project_root` - The project root directory
    /// * `artifact_base` - The base directory for this artifact type (e.g., `.claude`, `.opencode`)
    ///
    /// # Returns
    ///
    /// The directory path where MCP server JSON files should be copied.
    fn get_installation_dir(&self, project_root: &Path, artifact_base: &Path)
    -> std::path::PathBuf;

    /// Install or update MCP servers into the tool's configuration.
    ///
    /// This method is called after MCP config files have been copied (if applicable).
    /// It should merge the MCP configurations into the tool's config file.
    ///
    /// # Arguments
    ///
    /// * `project_root` - The project root directory
    /// * `artifact_base` - The base directory for this artifact type
    /// * `mcp_servers_dir` - Directory containing MCP server JSON files (if applicable)
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if the installation failed.
    fn configure_mcp_servers(
        &self,
        project_root: &Path,
        artifact_base: &Path,
        mcp_servers_dir: Option<&Path>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Clean/remove all managed MCP servers for this handler.
    ///
    /// # Arguments
    ///
    /// * `project_root` - The project root directory
    /// * `artifact_base` - The base directory for this artifact type
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if the cleanup failed.
    fn clean_mcp_servers(&self, project_root: &Path, artifact_base: &Path) -> Result<()>;
}

/// MCP handler for Claude Code.
///
/// Claude Code stores MCP server configurations in two places:
/// 1. Raw config files in `.claude/agpm/mcp-servers/`
/// 2. Merged configurations in `.mcp.json` at project root
pub struct ClaudeCodeMcpHandler;

impl McpHandler for ClaudeCodeMcpHandler {
    fn name(&self) -> &str {
        "claude-code"
    }

    fn requires_file_installation(&self) -> bool {
        true
    }

    fn get_installation_dir(
        &self,
        _project_root: &Path,
        artifact_base: &Path,
    ) -> std::path::PathBuf {
        artifact_base.join("agpm").join("mcp-servers")
    }

    fn configure_mcp_servers(
        &self,
        project_root: &Path,
        _artifact_base: &Path,
        mcp_servers_dir: Option<&Path>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let project_root = project_root.to_path_buf();
        let mcp_servers_dir = mcp_servers_dir.map(|p| p.to_path_buf());

        Box::pin(async move {
            if let Some(dir) = mcp_servers_dir {
                // Use existing configure_mcp_servers function
                super::configure_mcp_servers(&project_root, &dir).await
            } else {
                Ok(())
            }
        })
    }

    fn clean_mcp_servers(&self, project_root: &Path, _artifact_base: &Path) -> Result<()> {
        // Use existing clean_mcp_servers function
        super::clean_mcp_servers(project_root)
    }
}

/// MCP handler for OpenCode.
///
/// OpenCode merges MCP server configurations directly into `.opencode/opencode.json`
/// without copying config files to a separate directory.
pub struct OpenCodeMcpHandler;

impl McpHandler for OpenCodeMcpHandler {
    fn name(&self) -> &str {
        "opencode"
    }

    fn requires_file_installation(&self) -> bool {
        true // OpenCode needs files in staging directory for merging
    }

    fn get_installation_dir(
        &self,
        _project_root: &Path,
        artifact_base: &Path,
    ) -> std::path::PathBuf {
        artifact_base.join("agpm").join("mcp-servers")
    }

    fn configure_mcp_servers(
        &self,
        _project_root: &Path,
        artifact_base: &Path,
        mcp_servers_dir: Option<&Path>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let opencode_config_path = artifact_base.join("opencode.json");
        let mcp_servers_dir = mcp_servers_dir.map(|p| p.to_path_buf());

        Box::pin(async move {
            // If no MCP servers directory, nothing to merge
            let Some(servers_dir) = mcp_servers_dir else {
                return Ok(());
            };

            if !servers_dir.exists() {
                return Ok(());
            }

            // Read all MCP server JSON files
            let mut mcp_servers: std::collections::HashMap<String, super::McpServerConfig> =
                std::collections::HashMap::new();
            let mut entries = tokio::fs::read_dir(&servers_dir).await.with_context(|| {
                format!(
                    "Failed to read MCP servers directory: {}",
                    servers_dir.display()
                )
            })?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if path.extension().is_some_and(|ext| ext == "json")
                    && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                {
                    // Read and parse the MCP server configuration
                    let mut config: super::McpServerConfig = crate::utils::read_json_file(&path)
                        .with_context(|| {
                            format!("Failed to parse MCP server file: {}", path.display())
                        })?;

                    // Add AGPM metadata if not present (to track managed servers)
                    if config.agpm_metadata.is_none() {
                        config.agpm_metadata = Some(super::AgpmMetadata {
                            managed: true,
                            source: Some("agpm".to_string()),
                            version: None,
                            installed_at: chrono::Utc::now().to_rfc3339(),
                            dependency_name: Some(name.to_string()),
                        });
                    }

                    mcp_servers.insert(name.to_string(), config);
                }
            }

            if mcp_servers.is_empty() {
                return Ok(());
            }

            // Load or create opencode.json
            let mut opencode_config: serde_json::Value = if opencode_config_path.exists() {
                crate::utils::read_json_file(&opencode_config_path).with_context(|| {
                    format!(
                        "Failed to read OpenCode config: {}",
                        opencode_config_path.display()
                    )
                })?
            } else {
                serde_json::json!({})
            };

            // Ensure opencode_config is an object
            if !opencode_config.is_object() {
                opencode_config = serde_json::json!({});
            }

            // Get or create "mcp" section
            let config_obj = opencode_config
                .as_object_mut()
                .expect("opencode_config must be an object after is_object() check on line 225");
            let mcp_section = config_obj
                .entry("mcp")
                .or_insert_with(|| serde_json::json!({}));

            // Merge MCP servers into the mcp section
            if let Some(mcp_obj) = mcp_section.as_object_mut() {
                for (name, server_config) in mcp_servers {
                    let server_json = serde_json::to_value(&server_config)?;
                    mcp_obj.insert(name, server_json);
                }
            }

            // Save the updated configuration
            crate::utils::write_json_file(&opencode_config_path, &opencode_config, true)
                .with_context(|| {
                    format!(
                        "Failed to write OpenCode config: {}",
                        opencode_config_path.display()
                    )
                })?;

            Ok(())
        })
    }

    fn clean_mcp_servers(&self, _project_root: &Path, artifact_base: &Path) -> Result<()> {
        let opencode_config_path = artifact_base.join("opencode.json");
        let mcp_servers_dir = artifact_base.join("agpm").join("mcp-servers");

        // Remove MCP server files from the staging directory
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

        // Clean up opencode.json by removing only AGPM-managed servers
        if opencode_config_path.exists() {
            let mut opencode_config: serde_json::Value =
                crate::utils::read_json_file(&opencode_config_path).with_context(|| {
                    format!(
                        "Failed to read OpenCode config: {}",
                        opencode_config_path.display()
                    )
                })?;

            if let Some(config_obj) = opencode_config.as_object_mut()
                && let Some(mcp_section) = config_obj.get_mut("mcp")
                && let Some(mcp_obj) = mcp_section.as_object_mut()
            {
                // Remove only AGPM-managed servers
                mcp_obj.retain(|_name, server| {
                    // Try to parse as McpServerConfig to check metadata
                    if let Ok(config) =
                        serde_json::from_value::<super::McpServerConfig>(server.clone())
                    {
                        // Keep if not managed by AGPM
                        config
                            .agpm_metadata
                            .as_ref()
                            .is_none_or(|meta| !meta.managed)
                    } else {
                        // Keep if we can't parse it (preserve user data)
                        true
                    }
                });

                crate::utils::write_json_file(&opencode_config_path, &opencode_config, true)
                    .with_context(|| {
                        format!(
                            "Failed to write OpenCode config: {}",
                            opencode_config_path.display()
                        )
                    })?;
            }
        }

        if removed_count > 0 {
            println!("✓ Removed {removed_count} MCP server(s) from OpenCode");
        } else {
            println!("No MCP servers found to remove");
        }

        Ok(())
    }
}

/// Concrete MCP handler enum for different artifact types.
///
/// This enum wraps all supported MCP handlers and provides a unified interface.
pub enum ConcreteMcpHandler {
    /// Claude Code MCP handler
    ClaudeCode(ClaudeCodeMcpHandler),
    /// OpenCode MCP handler
    OpenCode(OpenCodeMcpHandler),
}

impl McpHandler for ConcreteMcpHandler {
    fn name(&self) -> &str {
        match self {
            Self::ClaudeCode(h) => h.name(),
            Self::OpenCode(h) => h.name(),
        }
    }

    fn requires_file_installation(&self) -> bool {
        match self {
            Self::ClaudeCode(h) => h.requires_file_installation(),
            Self::OpenCode(h) => h.requires_file_installation(),
        }
    }

    fn get_installation_dir(
        &self,
        project_root: &Path,
        artifact_base: &Path,
    ) -> std::path::PathBuf {
        match self {
            Self::ClaudeCode(h) => h.get_installation_dir(project_root, artifact_base),
            Self::OpenCode(h) => h.get_installation_dir(project_root, artifact_base),
        }
    }

    fn configure_mcp_servers(
        &self,
        project_root: &Path,
        artifact_base: &Path,
        mcp_servers_dir: Option<&Path>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        match self {
            Self::ClaudeCode(h) => {
                h.configure_mcp_servers(project_root, artifact_base, mcp_servers_dir)
            }
            Self::OpenCode(h) => {
                h.configure_mcp_servers(project_root, artifact_base, mcp_servers_dir)
            }
        }
    }

    fn clean_mcp_servers(&self, project_root: &Path, artifact_base: &Path) -> Result<()> {
        match self {
            Self::ClaudeCode(h) => h.clean_mcp_servers(project_root, artifact_base),
            Self::OpenCode(h) => h.clean_mcp_servers(project_root, artifact_base),
        }
    }
}

/// Get the appropriate MCP handler for an artifact type.
///
/// # Arguments
///
/// * `artifact_type` - The artifact type name (e.g., "claude-code", "opencode")
///
/// # Returns
///
/// An MCP handler for the given artifact type, or None if no handler exists.
pub fn get_mcp_handler(artifact_type: &str) -> Option<ConcreteMcpHandler> {
    match artifact_type {
        "claude-code" => Some(ConcreteMcpHandler::ClaudeCode(ClaudeCodeMcpHandler)),
        "opencode" => Some(ConcreteMcpHandler::OpenCode(OpenCodeMcpHandler)),
        _ => None, // Other artifact types don't have MCP support
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_mcp_handler_claude_code() {
        let handler = get_mcp_handler("claude-code");
        assert!(handler.is_some());
        let handler = handler.unwrap();
        assert_eq!(handler.name(), "claude-code");
        assert!(handler.requires_file_installation());
    }

    #[test]
    fn test_get_mcp_handler_opencode() {
        let handler = get_mcp_handler("opencode");
        assert!(handler.is_some());
        let handler = handler.unwrap();
        assert_eq!(handler.name(), "opencode");
        assert!(handler.requires_file_installation());
    }

    #[test]
    fn test_get_mcp_handler_unknown() {
        let handler = get_mcp_handler("unknown");
        assert!(handler.is_none());
    }

    #[test]
    fn test_get_mcp_handler_agpm() {
        // AGPM doesn't support MCP servers
        let handler = get_mcp_handler("agpm");
        assert!(handler.is_none());
    }

    #[test]
    fn test_claude_code_handler_installation_dir() {
        let handler = ClaudeCodeMcpHandler;
        let project_root = Path::new("/project");
        let artifact_base = Path::new("/project/.claude");

        let dir = handler.get_installation_dir(project_root, artifact_base);
        assert_eq!(dir, Path::new("/project/.claude/agpm/mcp-servers"));
    }
}
