//! Installation path resolution for resources.
//!
//! This module provides a unified interface for computing installation paths for all
//! resource types, handling both merge-target resources (Hooks, MCP servers) and
//! regular file-based resources (Agents, Commands, Snippets, Scripts).

use crate::core::ResourceType;
use crate::manifest::{Manifest, ResourceDependency};
use crate::utils::{compute_relative_install_path, normalize_path, normalize_path_for_storage};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Resolves the installation path for any resource type.
///
/// This is the main entry point for computing where a resource will be installed.
/// It handles both merge-target resources (Hooks, MCP servers) and regular resources
/// (Agents, Commands, Snippets, Scripts).
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type
/// * `source_filename` - The filename/path from the source repository
///
/// # Returns
///
/// The normalized installation path, or an error if the resource type is not supported
/// by the specified tool.
///
/// # Errors
///
/// Returns an error if:
/// - The resource type is not supported by the specified tool
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::install_path_resolver::resolve_install_path;
///
/// # fn example() -> anyhow::Result<()> {
/// let manifest = Manifest::new();
/// # let dep: agpm_cli::manifest::ResourceDependency = todo!();
/// let path = resolve_install_path(
///     &manifest,
///     &dep,
///     "claude-code",
///     ResourceType::Agent,
///     "agents/helper.md"
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn resolve_install_path(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
    source_filename: &str,
) -> Result<String> {
    match resource_type {
        ResourceType::Hook | ResourceType::McpServer => {
            Ok(resolve_merge_target_path(manifest, artifact_type, resource_type))
        }
        _ => resolve_regular_resource_path(
            manifest,
            dep,
            artifact_type,
            resource_type,
            source_filename,
        ),
    }
}

/// Resolves the installation path for merge-target resources (Hook, McpServer).
///
/// These resources are not installed as files but are merged into configuration files.
/// Uses configured merge targets or falls back to hardcoded defaults.
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - Must be Hook or McpServer
///
/// # Returns
///
/// The normalized path to the merge target configuration file.
pub fn resolve_merge_target_path(
    manifest: &Manifest,
    artifact_type: &str,
    resource_type: ResourceType,
) -> String {
    if let Some(merge_target) = manifest.get_merge_target(artifact_type, resource_type) {
        normalize_path_for_storage(merge_target.display().to_string())
    } else {
        // Fallback to hardcoded defaults if not configured
        match resource_type {
            ResourceType::Hook => ".claude/settings.local.json".to_string(),
            ResourceType::McpServer => {
                if artifact_type == "opencode" {
                    ".opencode/opencode.json".to_string()
                } else {
                    ".mcp.json".to_string()
                }
            }
            _ => unreachable!(
                "resolve_merge_target_path should only be called for Hook or McpServer"
            ),
        }
    }
}

/// Resolves the installation path for regular file-based resources.
///
/// Handles agents, commands, snippets, and scripts by:
/// 1. Getting the base artifact path from tool configuration
/// 2. Applying custom target overrides if specified
/// 3. Computing the relative path based on flatten behavior
/// 4. Avoiding redundant directory prefixes
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Agent, Command, Snippet, Script)
/// * `source_filename` - The filename/path from the source repository
///
/// # Returns
///
/// The normalized installation path, or an error if the resource type is not supported.
///
/// # Errors
///
/// Returns an error if the resource type is not supported by the specified tool.
pub fn resolve_regular_resource_path(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
    source_filename: &str,
) -> Result<String> {
    // Get the artifact path for this resource type
    let artifact_path =
        manifest.get_artifact_resource_path(artifact_type, resource_type).ok_or_else(|| {
            create_unsupported_resource_error(artifact_type, resource_type, dep.get_path())
        })?;

    // Determine flatten behavior
    let flatten = get_flatten_behavior(manifest, dep, artifact_type, resource_type);

    // Compute the final path
    let path = if let Some(custom_target) = dep.get_target() {
        compute_custom_target_path(&artifact_path, custom_target, source_filename, flatten)
    } else {
        compute_default_path(&artifact_path, source_filename, flatten)
    };

    Ok(normalize_path_for_storage(normalize_path(&path)))
}

/// Determines the flatten behavior for a resource installation.
///
/// Checks in order:
/// 1. Explicit `flatten` setting on the dependency
/// 2. Tool configuration default for this resource type
/// 3. Global default (false)
fn get_flatten_behavior(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
) -> bool {
    dep.get_flatten()
        .or_else(|| {
            manifest
                .get_tool_config(artifact_type)
                .and_then(|config| config.resources.get(resource_type.to_plural()))
                .and_then(|resource_config| resource_config.flatten)
        })
        .unwrap_or(false)
}

/// Computes the installation path when a custom target directory is specified.
///
/// Custom targets are relative to the artifact's resource directory. The function
/// uses the original artifact path (not the custom target) for prefix stripping
/// to avoid duplicate directories.
fn compute_custom_target_path(
    artifact_path: &Path,
    custom_target: &str,
    source_filename: &str,
    flatten: bool,
) -> PathBuf {
    let base_target = PathBuf::from(artifact_path.display().to_string())
        .join(custom_target.trim_start_matches('/'));
    // For custom targets, still strip prefix based on the original artifact path
    let relative_path =
        compute_relative_install_path(artifact_path, Path::new(source_filename), flatten);
    base_target.join(relative_path)
}

/// Computes the installation path using the default artifact path.
fn compute_default_path(artifact_path: &Path, source_filename: &str, flatten: bool) -> PathBuf {
    let relative_path =
        compute_relative_install_path(artifact_path, Path::new(source_filename), flatten);
    artifact_path.join(relative_path)
}

/// Creates a detailed error message when a resource type is not supported by a tool.
///
/// Provides helpful hints if it looks like a tool name was used as a resource type.
fn create_unsupported_resource_error(
    artifact_type: &str,
    resource_type: ResourceType,
    source_path: &str,
) -> anyhow::Error {
    let base_msg =
        format!("Resource type '{}' is not supported by tool '{}'", resource_type, artifact_type);

    let resource_type_str = resource_type.to_string();
    let hint = if ["claude-code", "opencode", "agpm"].contains(&resource_type_str.as_str()) {
        format!(
            "\n\nIt looks like '{}' is a tool name, not a resource type.\n\
            In transitive dependencies, use resource types (agents, snippets, commands)\n\
            as section headers, then specify 'tool: {}' within each dependency.",
            resource_type_str, resource_type_str
        )
    } else {
        format!(
            "\n\nValid resource types: agent, command, snippet, hook, mcp-server, script\n\
            Source file: {}",
            source_path
        )
    };

    anyhow::anyhow!("{}{}", base_msg, hint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{DetailedDependency, ResourceDependency};

    #[test]
    fn test_resolve_merge_target_path_hook_default() {
        let manifest = Manifest::new();
        let path = resolve_merge_target_path(&manifest, "claude-code", ResourceType::Hook);
        assert_eq!(path, ".claude/settings.local.json");
    }

    #[test]
    fn test_resolve_merge_target_path_mcp_claude() {
        let manifest = Manifest::new();
        let path = resolve_merge_target_path(&manifest, "claude-code", ResourceType::McpServer);
        assert_eq!(path, ".mcp.json");
    }

    #[test]
    fn test_resolve_merge_target_path_mcp_opencode() {
        let manifest = Manifest::new();
        let path = resolve_merge_target_path(&manifest, "opencode", ResourceType::McpServer);
        assert_eq!(path, ".opencode/opencode.json");
    }

    #[test]
    fn test_get_flatten_behavior_default() {
        let manifest = Manifest::new();
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test-source".to_string()),
            path: "agents/helper.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        let flatten = get_flatten_behavior(&manifest, &dep, "claude-code", ResourceType::Agent);
        assert!(flatten); // Agents flatten by default (tool_config.rs line 260)
    }
}
