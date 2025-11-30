//! Helper functions for remove command operations.

use crate::core::ResourceType;
use crate::lockfile::{LockFile, PrivateLockFile};
use crate::manifest::{Manifest, ResourceDependency};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Get dependencies for a specific resource type
pub fn get_dependencies_for_type(
    manifest: &Manifest,
    resource_type: ResourceType,
) -> &HashMap<String, ResourceDependency> {
    match resource_type {
        ResourceType::Agent => &manifest.agents,
        ResourceType::Snippet => &manifest.snippets,
        ResourceType::Command => &manifest.commands,
        ResourceType::McpServer => &manifest.mcp_servers,
        ResourceType::Script => &manifest.scripts,
        ResourceType::Hook => &manifest.hooks,
        ResourceType::Skill => &manifest.skills,
    }
}

/// Get mutable dependencies for a specific resource type
pub fn get_dependencies_for_type_mut(
    manifest: &mut Manifest,
    resource_type: ResourceType,
) -> &mut HashMap<String, ResourceDependency> {
    match resource_type {
        ResourceType::Agent => &mut manifest.agents,
        ResourceType::Snippet => &mut manifest.snippets,
        ResourceType::Command => &mut manifest.commands,
        ResourceType::McpServer => &mut manifest.mcp_servers,
        ResourceType::Script => &mut manifest.scripts,
        ResourceType::Hook => &mut manifest.hooks,
        ResourceType::Skill => &mut manifest.skills,
    }
}

/// Get the installed path for a resource from lockfile
pub fn get_installed_path_from_lockfile(
    lockfile: &LockFile,
    name: &str,
    resource_type: ResourceType,
    project_root: &Path,
) -> Option<std::path::PathBuf> {
    match resource_type {
        ResourceType::Agent => lockfile
            .agents
            .iter()
            .find(|a| a.lookup_name() == name)
            .map(|a| project_root.join(&a.installed_at)),
        ResourceType::Snippet => lockfile
            .snippets
            .iter()
            .find(|s| s.lookup_name() == name)
            .map(|s| project_root.join(&s.installed_at)),
        ResourceType::Command => lockfile
            .commands
            .iter()
            .find(|c| c.lookup_name() == name)
            .map(|c| project_root.join(&c.installed_at)),
        ResourceType::McpServer => lockfile
            .mcp_servers
            .iter()
            .find(|m| m.lookup_name() == name)
            .map(|m| project_root.join(&m.installed_at)),
        ResourceType::Script => lockfile
            .scripts
            .iter()
            .find(|s| s.lookup_name() == name)
            .map(|s| project_root.join(&s.installed_at)),
        ResourceType::Hook => lockfile
            .hooks
            .iter()
            .find(|h| h.lookup_name() == name)
            .map(|h| project_root.join(&h.installed_at)),
        ResourceType::Skill => lockfile
            .skills
            .iter()
            .find(|s| s.lookup_name() == name)
            .map(|s| project_root.join(&s.installed_at)),
    }
}

/// Remove a resource from lockfile
pub fn remove_from_lockfile(lockfile: &mut LockFile, name: &str, resource_type: ResourceType) {
    match resource_type {
        ResourceType::Agent => lockfile.agents.retain(|a| a.lookup_name() != name),
        ResourceType::Snippet => lockfile.snippets.retain(|s| s.lookup_name() != name),
        ResourceType::Command => lockfile.commands.retain(|c| c.lookup_name() != name),
        ResourceType::McpServer => lockfile.mcp_servers.retain(|m| m.lookup_name() != name),
        ResourceType::Script => lockfile.scripts.retain(|s| s.lookup_name() != name),
        ResourceType::Hook => lockfile.hooks.retain(|h| h.lookup_name() != name),
        ResourceType::Skill => lockfile.skills.retain(|s| s.lookup_name() != name),
    }
}

/// Remove entries from lockfile for a specific source
pub fn remove_source_from_lockfile(lockfile: &mut LockFile, source_name: &str) {
    lockfile.sources.retain(|s| s.name != source_name);
    lockfile.agents.retain(|a| a.source.as_deref() != Some(source_name));
    lockfile.snippets.retain(|s| s.source.as_deref() != Some(source_name));
    lockfile.commands.retain(|c| c.source.as_deref() != Some(source_name));
    lockfile.mcp_servers.retain(|m| m.source.as_deref() != Some(source_name));
    lockfile.scripts.retain(|s| s.source.as_deref() != Some(source_name));
    lockfile.hooks.retain(|h| h.source.as_deref() != Some(source_name));
    lockfile.skills.retain(|s| s.source.as_deref() != Some(source_name));
}

/// Collect installed file paths for a source from lockfile
pub fn collect_installed_paths_for_source(lockfile: &LockFile, source_name: &str) -> Vec<String> {
    let agents: Vec<String> = lockfile
        .agents
        .iter()
        .filter(|a| a.source.as_deref() == Some(source_name))
        .map(|a| a.installed_at.clone())
        .collect();

    let snippets: Vec<String> = lockfile
        .snippets
        .iter()
        .filter(|s| s.source.as_deref() == Some(source_name))
        .map(|s| s.installed_at.clone())
        .collect();

    let commands: Vec<String> = lockfile
        .commands
        .iter()
        .filter(|c| c.source.as_deref() == Some(source_name))
        .map(|c| c.installed_at.clone())
        .collect();

    agents.into_iter().chain(snippets).chain(commands).collect()
}

/// Delete installed files from filesystem
pub async fn delete_installed_files(project_root: &Path, file_paths: &[String]) -> Result<()> {
    for path_str in file_paths {
        let path = project_root.join(path_str);
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .with_context(|| format!("Failed to remove installed file: {}", path.display()))?;
        }
    }
    Ok(())
}

/// Update private lockfile by removing entries for deleted resources
pub fn update_private_lockfile(
    project_root: &Path,
    resource_names: &[String],
    resource_type: ResourceType,
) -> Result<()> {
    if let Ok(Some(mut private_lock)) = PrivateLockFile::load(project_root) {
        for name in resource_names {
            match resource_type {
                ResourceType::Agent => private_lock.agents.retain(|r| &r.name != name),
                ResourceType::Snippet => private_lock.snippets.retain(|r| &r.name != name),
                ResourceType::Command => private_lock.commands.retain(|r| &r.name != name),
                ResourceType::Script => private_lock.scripts.retain(|r| &r.name != name),
                ResourceType::McpServer => private_lock.mcp_servers.retain(|r| &r.name != name),
                ResourceType::Hook => private_lock.hooks.retain(|r| &r.name != name),
                ResourceType::Skill => private_lock.skills.retain(|r| &r.name != name),
            }
        }
        private_lock.save(project_root)?;
    }
    Ok(())
}

/// Update Claude settings file to remove MCP server or hook
pub fn update_settings_file(
    settings_path: &Path,
    name: &str,
    resource_type: ResourceType,
) -> Result<()> {
    if !settings_path.exists() {
        return Ok(());
    }

    match resource_type {
        ResourceType::McpServer => {
            let mut settings = crate::mcp::ClaudeSettings::load_or_default(settings_path)?;
            if let Some(servers) = &mut settings.mcp_servers {
                servers.remove(name);
            }
            settings.save(settings_path)?;
        }
        ResourceType::Hook => {
            let mut settings = crate::mcp::ClaudeSettings::load_or_default(settings_path)?;
            if let Some(hooks) = &mut settings.hooks
                && let Some(hooks_obj) = hooks.as_object_mut()
            {
                hooks_obj.remove(name);
            }
            settings.save(settings_path)?;
        }
        _ => {}
    }
    Ok(())
}
