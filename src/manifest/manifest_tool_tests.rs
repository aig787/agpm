//! Tests for tool configuration in manifests.
//!
//! These tests verify the multi-tool support functionality including:
//! - Default tool configurations
//! - Tool-specific resource paths
//! - Merge target configurations

use crate::manifest::Manifest;
use anyhow::Result;
use std::path::PathBuf;

#[test]
fn test_tools_config_default() {
    let manifest = Manifest::new();
    let tools = manifest.get_tools_config();

    // Should have default tools in the types map
    assert!(tools.types.contains_key("claude-code"));
    assert!(tools.types.contains_key("opencode"));
    assert!(tools.types.contains_key("agpm"));
}

#[test]
fn test_get_artifact_resource_path_agents() -> Result<()> {
    let manifest = Manifest::new();

    let path = manifest
        .get_artifact_resource_path("claude-code", crate::core::ResourceType::Agent)
        .ok_or_else(|| anyhow::anyhow!("Path should exist for claude-code agents"))?;
    // Use platform-specific path comparison
    #[cfg(windows)]
    assert_eq!(path.to_str().unwrap(), r".claude\agents");
    #[cfg(not(windows))]
    assert_eq!(path.to_str().unwrap(), ".claude/agents");

    let path = manifest
        .get_artifact_resource_path("opencode", crate::core::ResourceType::Agent)
        .ok_or_else(|| anyhow::anyhow!("Path should exist for opencode agents"))?;
    #[cfg(windows)]
    assert_eq!(path.to_str().unwrap(), r".opencode\agent");
    #[cfg(not(windows))]
    assert_eq!(path.to_str().unwrap(), ".opencode/agent");
    Ok(())
}

#[test]
fn test_get_artifact_resource_path_snippets() -> Result<()> {
    let manifest = Manifest::new();

    let path = manifest
        .get_artifact_resource_path("agpm", crate::core::ResourceType::Snippet)
        .ok_or_else(|| anyhow::anyhow!("Path should exist for agpm snippets"))?;
    #[cfg(windows)]
    assert_eq!(path.to_str().unwrap(), r".agpm\snippets");
    #[cfg(not(windows))]
    assert_eq!(path.to_str().unwrap(), ".agpm/snippets");

    // Claude Code also supports snippets (for override cases)
    let path = manifest
        .get_artifact_resource_path("claude-code", crate::core::ResourceType::Snippet)
        .ok_or_else(|| anyhow::anyhow!("Path should exist for claude-code snippets"))?;
    #[cfg(windows)]
    assert_eq!(path.to_str().unwrap(), r".claude\snippets");
    #[cfg(not(windows))]
    assert_eq!(path.to_str().unwrap(), ".claude/snippets");
    Ok(())
}

#[test]
fn test_get_artifact_resource_path_commands() -> Result<()> {
    let manifest = Manifest::new();

    let path = manifest
        .get_artifact_resource_path("claude-code", crate::core::ResourceType::Command)
        .ok_or_else(|| anyhow::anyhow!("Path should exist for claude-code commands"))?;
    #[cfg(windows)]
    assert_eq!(path.to_str().unwrap(), r".claude\commands");
    #[cfg(not(windows))]
    assert_eq!(path.to_str().unwrap(), ".claude/commands");

    let path = manifest
        .get_artifact_resource_path("opencode", crate::core::ResourceType::Command)
        .ok_or_else(|| anyhow::anyhow!("Path should exist for opencode commands"))?;
    #[cfg(windows)]
    assert_eq!(path.to_str().unwrap(), r".opencode\command");
    #[cfg(not(windows))]
    assert_eq!(path.to_str().unwrap(), ".opencode/command");
    Ok(())
}

#[test]
fn test_get_artifact_resource_path_unsupported() {
    let manifest = Manifest::new();

    // AGPM doesn't support agents
    let result = manifest.get_artifact_resource_path("agpm", crate::core::ResourceType::Agent);
    assert!(result.is_none());

    // OpenCode doesn't support scripts
    let result = manifest.get_artifact_resource_path("opencode", crate::core::ResourceType::Script);
    assert!(result.is_none());
}

#[test]
fn test_get_merge_target_hooks() -> Result<()> {
    let manifest = Manifest::new();

    let merge_target = manifest
        .get_merge_target("claude-code", crate::core::ResourceType::Hook)
        .ok_or_else(|| anyhow::anyhow!("Merge target should exist for claude-code hooks"))?;
    assert_eq!(merge_target, PathBuf::from(".claude/settings.local.json"));
    Ok(())
}

#[test]
fn test_get_merge_target_mcp_servers() -> Result<()> {
    let manifest = Manifest::new();

    let merge_target = manifest
        .get_merge_target("claude-code", crate::core::ResourceType::McpServer)
        .ok_or_else(|| anyhow::anyhow!("Merge target should exist for claude-code mcp servers"))?;
    assert_eq!(merge_target, PathBuf::from(".mcp.json"));

    let merge_target = manifest
        .get_merge_target("opencode", crate::core::ResourceType::McpServer)
        .ok_or_else(|| anyhow::anyhow!("Merge target should exist for opencode mcp servers"))?;
    assert_eq!(merge_target, PathBuf::from(".opencode/opencode.json"));
    Ok(())
}

#[test]
fn test_get_merge_target_non_mergeable() {
    let manifest = Manifest::new();

    // Agents don't have merge targets
    let result = manifest.get_merge_target("claude-code", crate::core::ResourceType::Agent);
    assert!(result.is_none());

    // Commands don't have merge targets
    let result = manifest.get_merge_target("opencode", crate::core::ResourceType::Command);
    assert!(result.is_none());
}
