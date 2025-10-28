//! MCP server installation handlers for different tools.
//!
//! This module provides a pluggable handler system for installing MCP servers
//! into different tools' configuration formats (Claude Code, OpenCode, etc.).

use anyhow::{Context, Result};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

/// Trait for handling MCP server installation for different tools.
///
/// Each tool (claude-code, opencode, etc.) may have different ways
/// of managing MCP servers. This trait provides a common interface for:
/// - Reading MCP server configurations directly from source
/// - Merging configurations into tool-specific formats
pub trait McpHandler: Send + Sync {
    /// Get the name of this MCP handler (e.g., "claude-code", "opencode").
    fn name(&self) -> &str;

    /// Configure MCP servers by reading directly from source and merging into config file.
    ///
    /// This method reads MCP server configurations directly from source locations
    /// (Git worktrees or local paths) and merges them into the tool's config file.
    /// Patches from the manifest are applied to each server configuration before merging.
    ///
    /// # Arguments
    ///
    /// * `project_root` - The project root directory
    /// * `artifact_base` - The base directory for this tool
    /// * `lockfile_entries` - Locked MCP server resources with source information
    /// * `cache` - Cache for accessing Git worktrees
    /// * `manifest` - Manifest containing patch definitions
    ///
    /// # Returns
    ///
    /// `Ok((applied_patches, changed_count))` where:
    /// - `applied_patches`: Vec<(name, AppliedPatches)> for each server
    /// - `changed_count`: Number of servers that actually changed (ignoring timestamps)
    #[allow(clippy::type_complexity)]
    fn configure_mcp_servers(
        &self,
        project_root: &Path,
        artifact_base: &Path,
        lockfile_entries: &[crate::lockfile::LockedResource],
        cache: &crate::cache::Cache,
        manifest: &crate::manifest::Manifest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<(
                        Vec<(String, crate::manifest::patches::AppliedPatches)>,
                        usize,
                    )>,
                > + Send
                + '_,
        >,
    >;

    /// Clean/remove all managed MCP servers for this handler.
    ///
    /// # Arguments
    ///
    /// * `project_root` - The project root directory
    /// * `artifact_base` - The base directory for this tool
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if the cleanup failed.
    fn clean_mcp_servers(&self, project_root: &Path, artifact_base: &Path) -> Result<()>;
}

/// MCP handler for Claude Code.
///
/// Claude Code configures MCP servers directly in `.mcp.json` at project root
/// by reading from source locations (no intermediate file copying).
pub struct ClaudeCodeMcpHandler;

impl McpHandler for ClaudeCodeMcpHandler {
    fn name(&self) -> &str {
        "claude-code"
    }

    fn configure_mcp_servers(
        &self,
        project_root: &Path,
        _artifact_base: &Path,
        lockfile_entries: &[crate::lockfile::LockedResource],
        cache: &crate::cache::Cache,
        manifest: &crate::manifest::Manifest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<(
                        Vec<(String, crate::manifest::patches::AppliedPatches)>,
                        usize,
                    )>,
                > + Send
                + '_,
        >,
    > {
        let project_root = project_root.to_path_buf();
        let entries = lockfile_entries.to_vec();
        let cache = cache.clone();
        let manifest = manifest.clone();

        Box::pin(async move {
            if entries.is_empty() {
                return Ok((Vec::new(), 0));
            }

            // Read MCP server configurations directly from source files
            let mut mcp_servers: std::collections::HashMap<String, super::McpServerConfig> =
                std::collections::HashMap::new();
            let mut all_applied_patches = Vec::new();

            for entry in &entries {
                // Get the source file path
                let source_path = if let Some(source_name) = &entry.source {
                    let url = entry
                        .url
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("MCP server {} has no URL", entry.name))?;

                    // Check if this is a local directory source
                    let is_local_source =
                        entry.resolved_commit.as_deref().is_none_or(str::is_empty);

                    if is_local_source {
                        // Local directory source - use URL as path directly
                        std::path::PathBuf::from(url).join(&entry.path)
                    } else {
                        // Git-based source - get worktree
                        let sha = entry.resolved_commit.as_deref().ok_or_else(|| {
                            anyhow::anyhow!("MCP server {} missing resolved commit SHA", entry.name)
                        })?;

                        let worktree = cache
                            .get_or_create_worktree_for_sha(
                                source_name,
                                url,
                                sha,
                                Some(&entry.name),
                            )
                            .await?;
                        worktree.join(&entry.path)
                    }
                } else {
                    // Local file - resolve relative to project root
                    let candidate = Path::new(&entry.path);
                    if candidate.is_absolute() {
                        candidate.to_path_buf()
                    } else {
                        project_root.join(candidate)
                    }
                };

                // Read the MCP server configuration as string first (for patch application)
                let json_content =
                    tokio::fs::read_to_string(&source_path).await.with_context(|| {
                        format!("Failed to read MCP server file: {}", source_path.display())
                    })?;

                // Apply patches if present
                let (patched_content, applied_patches) = {
                    // Look up patches for this MCP server
                    let lookup_name = entry.lookup_name();
                    let project_patches = manifest.project_patches.get("mcp-servers", lookup_name);
                    let private_patches = manifest.private_patches.get("mcp-servers", lookup_name);

                    if project_patches.is_some() || private_patches.is_some() {
                        use crate::manifest::patches::apply_patches_to_content_with_origin;
                        apply_patches_to_content_with_origin(
                            &json_content,
                            &source_path.display().to_string(),
                            project_patches.unwrap_or(&std::collections::BTreeMap::new()),
                            private_patches.unwrap_or(&std::collections::BTreeMap::new()),
                        )?
                    } else {
                        (json_content, crate::manifest::patches::AppliedPatches::default())
                    }
                };

                // Collect applied patches for this server
                all_applied_patches.push((entry.name.clone(), applied_patches));

                // Parse the patched JSON
                let mut config: super::McpServerConfig = serde_json::from_str(&patched_content)
                    .with_context(|| {
                        format!("Failed to parse MCP server JSON from {}", source_path.display())
                    })?;

                // Add AGPM metadata
                config.agpm_metadata = Some(super::AgpmMetadata {
                    managed: true,
                    source: entry.source.clone(),
                    version: entry.version.clone(),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                    dependency_name: Some(entry.name.clone()),
                });

                // Use lookup_name for the MCP server key (manifest alias if present, otherwise canonical name)
                mcp_servers.insert(entry.lookup_name().to_string(), config);
            }

            // Configure MCP servers by merging into .mcp.json
            let mcp_config_path = project_root.join(".mcp.json");
            let changed_count = super::merge_mcp_servers(&mcp_config_path, mcp_servers).await?;

            Ok((all_applied_patches, changed_count))
        })
    }

    fn clean_mcp_servers(&self, project_root: &Path, _artifact_base: &Path) -> Result<()> {
        // Use existing clean_mcp_servers function
        super::clean_mcp_servers(project_root)
    }
}

/// MCP handler for OpenCode.
///
/// OpenCode configures MCP servers directly in `.opencode/opencode.json`
/// by reading from source locations (no intermediate file copying).
pub struct OpenCodeMcpHandler;

impl McpHandler for OpenCodeMcpHandler {
    fn name(&self) -> &str {
        "opencode"
    }

    fn configure_mcp_servers(
        &self,
        project_root: &Path,
        artifact_base: &Path,
        lockfile_entries: &[crate::lockfile::LockedResource],
        cache: &crate::cache::Cache,
        manifest: &crate::manifest::Manifest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<(
                        Vec<(String, crate::manifest::patches::AppliedPatches)>,
                        usize,
                    )>,
                > + Send
                + '_,
        >,
    > {
        let project_root = project_root.to_path_buf();
        let artifact_base = artifact_base.to_path_buf();
        let entries = lockfile_entries.to_vec();
        let cache = cache.clone();
        let manifest = manifest.clone();

        Box::pin(async move {
            if entries.is_empty() {
                return Ok((Vec::new(), 0));
            }

            let mut all_applied_patches = Vec::new();

            // Read MCP server configurations directly from source files
            let mut mcp_servers: std::collections::HashMap<String, super::McpServerConfig> =
                std::collections::HashMap::new();

            for entry in &entries {
                // Get the source file path
                let source_path = if let Some(source_name) = &entry.source {
                    let url = entry
                        .url
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("MCP server {} has no URL", entry.name))?;

                    // Check if this is a local directory source
                    let is_local_source =
                        entry.resolved_commit.as_deref().is_none_or(str::is_empty);

                    if is_local_source {
                        // Local directory source - use URL as path directly
                        std::path::PathBuf::from(url).join(&entry.path)
                    } else {
                        // Git-based source - get worktree
                        let sha = entry.resolved_commit.as_deref().ok_or_else(|| {
                            anyhow::anyhow!("MCP server {} missing resolved commit SHA", entry.name)
                        })?;

                        let worktree = cache
                            .get_or_create_worktree_for_sha(
                                source_name,
                                url,
                                sha,
                                Some(&entry.name),
                            )
                            .await?;
                        worktree.join(&entry.path)
                    }
                } else {
                    // Local file - resolve relative to project root
                    let candidate = Path::new(&entry.path);
                    if candidate.is_absolute() {
                        candidate.to_path_buf()
                    } else {
                        project_root.join(candidate)
                    }
                };

                // Read the MCP server configuration as string first (for patch application)
                let json_content =
                    tokio::fs::read_to_string(&source_path).await.with_context(|| {
                        format!("Failed to read MCP server file: {}", source_path.display())
                    })?;

                // Apply patches if present
                let (patched_content, applied_patches) = {
                    // Look up patches for this MCP server
                    let lookup_name = entry.lookup_name();
                    let project_patches = manifest.project_patches.get("mcp-servers", lookup_name);
                    let private_patches = manifest.private_patches.get("mcp-servers", lookup_name);

                    if project_patches.is_some() || private_patches.is_some() {
                        use crate::manifest::patches::apply_patches_to_content_with_origin;
                        apply_patches_to_content_with_origin(
                            &json_content,
                            &source_path.display().to_string(),
                            project_patches.unwrap_or(&std::collections::BTreeMap::new()),
                            private_patches.unwrap_or(&std::collections::BTreeMap::new()),
                        )?
                    } else {
                        (json_content, crate::manifest::patches::AppliedPatches::default())
                    }
                };

                // Collect applied patches for this server
                all_applied_patches.push((entry.name.clone(), applied_patches));

                // Parse the patched JSON
                let mut config: super::McpServerConfig = serde_json::from_str(&patched_content)
                    .with_context(|| {
                        format!("Failed to parse MCP server JSON from {}", source_path.display())
                    })?;

                // Add AGPM metadata
                config.agpm_metadata = Some(super::AgpmMetadata {
                    managed: true,
                    source: entry.source.clone(),
                    version: entry.version.clone(),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                    dependency_name: Some(entry.name.clone()),
                });

                // Use lookup_name for the MCP server key (manifest alias if present, otherwise canonical name)
                mcp_servers.insert(entry.lookup_name().to_string(), config);
            }

            // Load or create opencode.json
            let opencode_config_path = artifact_base.join("opencode.json");
            let mut opencode_config: serde_json::Value = if opencode_config_path.exists() {
                crate::utils::read_json_file(&opencode_config_path).with_context(|| {
                    format!("Failed to read OpenCode config: {}", opencode_config_path.display())
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
                .expect("opencode_config must be an object after is_object() check");
            let mcp_section = config_obj.entry("mcp").or_insert_with(|| serde_json::json!({}));

            // Count how many servers actually changed (ignoring timestamps)
            let mut changed_count = 0;
            if let Some(mcp_obj) = mcp_section.as_object() {
                for (name, new_config) in &mcp_servers {
                    match mcp_obj.get(name) {
                        Some(existing_value) => {
                            // Server exists - check if it's actually different
                            if let Ok(existing_config) =
                                serde_json::from_value::<super::McpServerConfig>(
                                    existing_value.clone(),
                                )
                            {
                                // Create copies without the timestamp for comparison
                                let mut existing_without_time = existing_config;
                                let mut new_without_time = new_config.clone();

                                // Remove timestamp from metadata for comparison
                                if let Some(ref mut meta) = existing_without_time.agpm_metadata {
                                    meta.installed_at.clear();
                                }
                                if let Some(ref mut meta) = new_without_time.agpm_metadata {
                                    meta.installed_at.clear();
                                }

                                if existing_without_time != new_without_time {
                                    changed_count += 1;
                                }
                            } else {
                                // Different format - count as changed
                                changed_count += 1;
                            }
                        }
                        None => {
                            // New server - will be added
                            changed_count += 1;
                        }
                    }
                }
            } else {
                // No existing MCP section - all servers are new
                changed_count = mcp_servers.len();
            }

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
                    format!("Failed to write OpenCode config: {}", opencode_config_path.display())
                })?;

            Ok((all_applied_patches, changed_count))
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
                    format!("Failed to read OpenCode config: {}", opencode_config_path.display())
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
                        config.agpm_metadata.as_ref().is_none_or(|meta| !meta.managed)
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

/// Concrete MCP handler enum for different tools.
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

    fn configure_mcp_servers(
        &self,
        project_root: &Path,
        artifact_base: &Path,
        lockfile_entries: &[crate::lockfile::LockedResource],
        cache: &crate::cache::Cache,
        manifest: &crate::manifest::Manifest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<(
                        Vec<(String, crate::manifest::patches::AppliedPatches)>,
                        usize,
                    )>,
                > + Send
                + '_,
        >,
    > {
        match self {
            Self::ClaudeCode(h) => h.configure_mcp_servers(
                project_root,
                artifact_base,
                lockfile_entries,
                cache,
                manifest,
            ),
            Self::OpenCode(h) => h.configure_mcp_servers(
                project_root,
                artifact_base,
                lockfile_entries,
                cache,
                manifest,
            ),
        }
    }

    fn clean_mcp_servers(&self, project_root: &Path, artifact_base: &Path) -> Result<()> {
        match self {
            Self::ClaudeCode(h) => h.clean_mcp_servers(project_root, artifact_base),
            Self::OpenCode(h) => h.clean_mcp_servers(project_root, artifact_base),
        }
    }
}

/// Get the appropriate MCP handler for a tool.
///
/// # Arguments
///
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
///
/// # Returns
///
/// An MCP handler for the given tool, or None if no handler exists.
pub fn get_mcp_handler(artifact_type: &str) -> Option<ConcreteMcpHandler> {
    match artifact_type {
        "claude-code" => Some(ConcreteMcpHandler::ClaudeCode(ClaudeCodeMcpHandler)),
        "opencode" => Some(ConcreteMcpHandler::OpenCode(OpenCodeMcpHandler)),
        _ => None, // Other tools don't have MCP support
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
    }

    #[test]
    fn test_get_mcp_handler_opencode() {
        let handler = get_mcp_handler("opencode");
        assert!(handler.is_some());
        let handler = handler.unwrap();
        assert_eq!(handler.name(), "opencode");
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
}
