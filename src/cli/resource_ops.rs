//! Shared resource handling operations for CLI commands
//!
//! This module provides common functionality for fetching, installing, and managing
//! resources across different CLI commands, reducing code duplication.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

use crate::cache::Cache;
use crate::lockfile::LockedResource;
use crate::manifest::{Manifest, ResourceDependency};
use crate::markdown::MarkdownFile;
use crate::mcp::{ClaudeSettings, McpServerConfig};
use crate::utils::fs::{atomic_write, ensure_dir};

/// Determines the target installation path for a resource
pub fn get_resource_target_path(
    name: &str,
    resource_type: &str,
    manifest: &Manifest,
    project_root: &Path,
) -> Result<PathBuf> {
    let target_dir = match resource_type {
        "agent" => &manifest.target.agents,
        "snippet" => &manifest.target.snippets,
        "command" => &manifest.target.commands,
        "script" => &manifest.target.scripts,
        "hook" => &manifest.target.hooks,
        "mcp-server" => &manifest.target.mcp_servers,
        _ => return Err(anyhow!("Unknown resource type: {}", resource_type)),
    };

    let file_extension = get_resource_extension(resource_type);
    Ok(project_root
        .join(target_dir)
        .join(format!("{}.{}", name, file_extension)))
}

/// Determines the file extension for a resource type
pub fn get_resource_extension(resource_type: &str) -> &'static str {
    match resource_type {
        "hook" | "mcp-server" => "json",
        "script" => "sh", // Default to .sh, actual extension preserved during install
        _ => "md",
    }
}

/// Fetches resource content from a dependency specification
pub async fn fetch_resource_content(
    dependency: &ResourceDependency,
    manifest: &Manifest,
    cache: &Cache,
) -> Result<(PathBuf, String)> {
    // Determine source path
    let source_path = resolve_dependency_path(dependency, manifest, cache).await?;

    // Check if source file exists
    if !source_path.exists() {
        return Err(anyhow!("Source file not found: {}", source_path.display()));
    }

    // Read the source file
    let content = std::fs::read_to_string(&source_path)
        .with_context(|| format!("Failed to read source file: {}", source_path.display()))?;

    Ok((source_path, content))
}

/// Resolves a dependency to its filesystem path
pub async fn resolve_dependency_path(
    dependency: &ResourceDependency,
    manifest: &Manifest,
    cache: &Cache,
) -> Result<PathBuf> {
    match dependency {
        ResourceDependency::Detailed(detailed) => {
            if let Some(ref source_name) = detailed.source {
                // Remote dependency - get from cache
                let source_url = manifest
                    .sources
                    .get(source_name)
                    .ok_or_else(|| anyhow!("Source '{}' not found in manifest", source_name))?;

                let version_ref = detailed
                    .rev
                    .as_deref()
                    .or(detailed.branch.as_deref())
                    .or(detailed.version.as_deref());

                let cache_dir = cache
                    .get_or_clone_source(source_name, source_url, version_ref)
                    .await?;

                Ok(cache_dir.join(&detailed.path))
            } else {
                // Local dependency with detailed path
                Ok(Path::new(&detailed.path).to_path_buf())
            }
        }
        ResourceDependency::Simple(path) => {
            // Simple local dependency
            Ok(Path::new(path).to_path_buf())
        }
    }
}

/// Validates resource content based on type
pub fn validate_resource_content(content: &str, resource_type: &str, name: &str) -> Result<()> {
    match resource_type {
        "hook" | "mcp-server" => {
            // Parse as JSON to validate
            serde_json::from_str::<serde_json::Value>(content)
                .with_context(|| format!("{} '{}' must be valid JSON", resource_type, name))?;
        }
        "agent" | "snippet" | "command" => {
            // Parse as markdown to validate
            MarkdownFile::parse(content).with_context(|| {
                format!("Invalid markdown file for {} '{}'", resource_type, name)
            })?;
        }
        "script" => {
            // Scripts don't need validation beyond existence
        }
        _ => {}
    }
    Ok(())
}

/// Installs a resource file to the target location
pub fn install_resource_file(target_path: &Path, content: &str) -> Result<()> {
    // Ensure destination directory exists
    if let Some(parent) = target_path.parent() {
        ensure_dir(parent)?;
    }

    // Write file atomically
    atomic_write(target_path, content.as_bytes())?;
    Ok(())
}

/// Updates settings.local.json with hook configuration
pub fn update_settings_for_hook(name: &str, content: &str, project_root: &Path) -> Result<()> {
    // Parse hook content as JSON
    let hook_json: serde_json::Value =
        serde_json::from_str(content).context("Failed to parse hook content as JSON")?;

    // Update .claude/settings.local.json with the hook
    let claude_dir = project_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");
    ensure_dir(&claude_dir)?;

    let mut settings = ClaudeSettings::load_or_default(&settings_path)?;

    // Initialize hooks if None
    if settings.hooks.is_none() {
        settings.hooks = Some(serde_json::json!({}));
    }

    // Add the hook to settings
    if let Some(hooks) = &mut settings.hooks {
        if let Some(hooks_obj) = hooks.as_object_mut() {
            hooks_obj.insert(name.to_string(), hook_json);
        }
    }

    settings.save(&settings_path)?;
    Ok(())
}

/// Updates settings.local.json with MCP server configuration
pub fn update_settings_for_mcp_server(
    name: &str,
    content: &str,
    project_root: &Path,
) -> Result<()> {
    // Parse MCP server content as JSON
    let mcp_json: McpServerConfig =
        serde_json::from_str(content).context("Failed to parse MCP server content as JSON")?;

    // Update .claude/settings.local.json with the MCP server
    let claude_dir = project_root.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");
    ensure_dir(&claude_dir)?;

    let mut settings = ClaudeSettings::load_or_default(&settings_path)?;

    // Initialize mcpServers if None
    if settings.mcp_servers.is_none() {
        settings.mcp_servers = Some(std::collections::HashMap::new());
    }

    // Add the MCP server to settings
    if let Some(servers) = &mut settings.mcp_servers {
        servers.insert(name.to_string(), mcp_json);
    }

    settings.save(&settings_path)?;
    Ok(())
}

/// Creates a lockfile entry for an installed resource
pub fn create_lock_entry(
    name: &str,
    dependency: &ResourceDependency,
    manifest: &Manifest,
    target_path: &Path,
    content: &str,
    resolved_commit: Option<String>,
) -> Result<LockedResource> {
    // Calculate checksum
    let checksum = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("sha256:{:x}", hasher.finalize())
    };

    // Determine source info
    let (source_name, source_url, path_str) = match dependency {
        ResourceDependency::Detailed(d) => {
            let url = d
                .source
                .as_ref()
                .and_then(|s| manifest.sources.get(s))
                .cloned();
            (d.source.clone(), url, d.path.clone())
        }
        ResourceDependency::Simple(p) => (None, None, p.clone()),
    };

    Ok(LockedResource {
        name: name.to_string(),
        source: source_name,
        url: source_url,
        path: path_str,
        version: match dependency {
            ResourceDependency::Detailed(d) => {
                d.version.clone().or(d.branch.clone()).or(d.rev.clone())
            }
            ResourceDependency::Simple(_) => None,
        },
        resolved_commit,
        checksum,
        installed_at: target_path
            .strip_prefix(std::env::current_dir()?)
            .unwrap_or(target_path)
            .to_string_lossy()
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_resource_extension() {
        assert_eq!(get_resource_extension("agent"), "md");
        assert_eq!(get_resource_extension("snippet"), "md");
        assert_eq!(get_resource_extension("command"), "md");
        assert_eq!(get_resource_extension("script"), "sh");
        assert_eq!(get_resource_extension("hook"), "json");
        assert_eq!(get_resource_extension("mcp-server"), "json");
    }

    #[test]
    fn test_validate_resource_content() {
        // Valid markdown
        assert!(validate_resource_content("# Test", "agent", "test").is_ok());

        // Valid JSON for hooks
        assert!(validate_resource_content("{\"key\": \"value\"}", "hook", "test").is_ok());

        // Invalid JSON for hooks
        assert!(validate_resource_content("not json", "hook", "test").is_err());

        // Scripts don't need validation
        assert!(validate_resource_content("#!/bin/bash", "script", "test").is_ok());
    }

    #[test]
    fn test_install_resource_file() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("test.md");

        install_resource_file(&target, "test content").unwrap();

        assert!(target.exists());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "test content");
    }
}
