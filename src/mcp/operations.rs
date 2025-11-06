use crate::mcp::models::{AgpmMetadata, McpConfig, McpServerConfig};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;

/// Merge MCP server configurations into the config file.
///
/// This is a helper function used by MCP handlers to merge server configurations
/// that have already been read from source files.
///
/// Returns the number of servers that actually changed (ignoring timestamps).
pub async fn merge_mcp_servers(
    mcp_config_path: &Path,
    agpm_servers: HashMap<String, McpServerConfig>,
) -> Result<usize> {
    if agpm_servers.is_empty() {
        return Ok(0);
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

    // Count how many servers actually changed (ignoring timestamps)
    let mut changed_count = 0;
    for (name, new_config) in &agpm_servers {
        match mcp_config.mcp_servers.get(name) {
            Some(existing_config) => {
                // Server exists - check if it's actually different
                // Create copies without the timestamp for comparison
                let mut existing_without_time = existing_config.clone();
                let mut new_without_time = new_config.clone();

                // Remove timestamp from metadata for comparison
                if let Some(ref mut meta) = existing_without_time.agpm_metadata {
                    meta.installed_at = String::new();
                }
                if let Some(ref mut meta) = new_without_time.agpm_metadata {
                    meta.installed_at = String::new();
                }

                if existing_without_time != new_without_time {
                    changed_count += 1;
                }
            }
            None => {
                // New server - will be added
                changed_count += 1;
            }
        }
    }

    // Update MCP configuration with AGPM-managed servers
    mcp_config.update_managed_servers(agpm_servers)?;

    // Save the updated MCP configuration
    mcp_config.save(mcp_config_path)?;

    Ok(changed_count)
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
    merge_mcp_servers(&mcp_config_path, agpm_servers).await?;
    Ok(())
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
