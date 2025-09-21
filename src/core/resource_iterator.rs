//! Resource iteration and collection utilities for parallel installation
//!
//! This module provides unified abstractions for working with multiple resource types,
//! enabling efficient parallel processing and reducing code duplication. It supports
//! the new installer architecture by providing iteration utilities that work seamlessly
//! with the worktree-based parallel installation system.
//!
//! # Features
//!
//! - **Type-safe iteration**: Work with all resource types through unified interfaces
//! - **Parallel processing support**: Optimized for concurrent resource handling
//! - **Target directory resolution**: Maps resource types to their installation directories
//! - **Resource lookup**: Fast lookup of resources by name across all types
//!
//! # Use Cases
//!
//! - **Installation**: Collecting all resources for parallel installation
//! - **Updates**: Finding specific resources for selective updates
//! - **Validation**: Iterating over all resources for consistency checks
//! - **Reporting**: Gathering statistics across all resource types

use crate::core::ResourceType;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency, TargetConfig};
use std::collections::HashMap;

/// Extension trait for ResourceType that adds lockfile and manifest operations
///
/// This trait extends the base [`ResourceType`] enum with methods for working with
/// lockfiles and manifests in a type-safe way. It provides the foundation for
/// unified resource processing across all resource types.
pub trait ResourceTypeExt {
    /// Get all resource types in their processing order
    ///
    /// Returns the complete list of resource types in the order they should be
    /// processed during installation. This order is optimized for dependency
    /// resolution and parallel processing efficiency.
    ///
    /// # Returns
    ///
    /// A vector containing all resource types in processing order
    fn all() -> Vec<ResourceType>;

    /// Get lockfile entries for this resource type
    ///
    /// Retrieves the slice of locked resources for this specific resource type
    /// from the provided lockfile. This enables type-safe access to resources
    /// without having to match on the resource type manually.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile to extract entries from
    ///
    /// # Returns
    ///
    /// A slice of [`LockedResource`] entries for this resource type
    fn get_lockfile_entries<'a>(&self, lockfile: &'a LockFile) -> &'a [LockedResource];

    /// Get mutable lockfile entries for this resource type
    ///
    /// Retrieves a mutable reference to the vector of locked resources for this
    /// specific resource type. This is used when modifying lockfile contents
    /// during resolution or updates.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile to extract entries from
    ///
    /// # Returns
    ///
    /// A mutable reference to the vector of [`LockedResource`] entries
    fn get_lockfile_entries_mut<'a>(
        &mut self,
        lockfile: &'a mut LockFile,
    ) -> &'a mut Vec<LockedResource>;

    /// Get target directory for this resource type
    ///
    /// Retrieves the target installation directory for this resource type from
    /// the manifest's target configuration. This determines where resources of
    /// this type will be installed in the project.
    ///
    /// # Arguments
    ///
    /// * `targets` - The target configuration from the manifest
    ///
    /// # Returns
    ///
    /// The target directory path as a string reference
    fn get_target_dir<'a>(&self, targets: &'a TargetConfig) -> &'a str;

    /// Get manifest entries for this resource type
    fn get_manifest_entries<'a>(
        &self,
        manifest: &'a Manifest,
    ) -> &'a HashMap<String, ResourceDependency>;
}

impl ResourceTypeExt for ResourceType {
    fn all() -> Vec<ResourceType> {
        vec![
            ResourceType::Agent,
            ResourceType::Snippet,
            ResourceType::Command,
            ResourceType::McpServer,
            ResourceType::Script,
            ResourceType::Hook,
        ]
    }

    fn get_lockfile_entries<'a>(&self, lockfile: &'a LockFile) -> &'a [LockedResource] {
        match self {
            ResourceType::Agent => &lockfile.agents,
            ResourceType::Snippet => &lockfile.snippets,
            ResourceType::Command => &lockfile.commands,
            ResourceType::Script => &lockfile.scripts,
            ResourceType::Hook => &lockfile.hooks,
            ResourceType::McpServer => &lockfile.mcp_servers,
        }
    }

    fn get_lockfile_entries_mut<'a>(
        &mut self,
        lockfile: &'a mut LockFile,
    ) -> &'a mut Vec<LockedResource> {
        match self {
            ResourceType::Agent => &mut lockfile.agents,
            ResourceType::Snippet => &mut lockfile.snippets,
            ResourceType::Command => &mut lockfile.commands,
            ResourceType::Script => &mut lockfile.scripts,
            ResourceType::Hook => &mut lockfile.hooks,
            ResourceType::McpServer => &mut lockfile.mcp_servers,
        }
    }

    fn get_target_dir<'a>(&self, targets: &'a TargetConfig) -> &'a str {
        match self {
            ResourceType::Agent => targets.agents.as_str(),
            ResourceType::Snippet => targets.snippets.as_str(),
            ResourceType::Command => targets.commands.as_str(),
            ResourceType::Script => targets.scripts.as_str(),
            ResourceType::Hook => targets.hooks.as_str(),
            ResourceType::McpServer => targets.mcp_servers.as_str(),
        }
    }

    fn get_manifest_entries<'a>(
        &self,
        manifest: &'a Manifest,
    ) -> &'a HashMap<String, ResourceDependency> {
        match self {
            ResourceType::Agent => &manifest.agents,
            ResourceType::Snippet => &manifest.snippets,
            ResourceType::Command => &manifest.commands,
            ResourceType::Script => &manifest.scripts,
            ResourceType::Hook => &manifest.hooks,
            ResourceType::McpServer => &manifest.mcp_servers,
        }
    }
}

/// Iterator utilities for working with resources across all types
///
/// The [`ResourceIterator`] provides static methods for collecting and processing
/// resources from lockfiles in a unified way. It's designed to support the parallel
/// installation architecture by providing efficient collection methods that work
/// with the worktree-based installer.
///
/// # Use Cases
///
/// - **Parallel Installation**: Collecting all resources for concurrent processing
/// - **Resource Discovery**: Finding specific resources across all types
/// - **Statistics**: Gathering counts and information across resource types
/// - **Validation**: Iterating over all resources for consistency checks
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::core::resource_iterator::ResourceIterator;
/// use ccpm::lockfile::LockFile;
/// use ccpm::manifest::Manifest;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let lockfile = LockFile::load(Path::new("ccpm.lock"))?;
/// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
///
/// // Collect all resources for parallel installation
/// let all_entries = ResourceIterator::collect_all_entries(&lockfile, &manifest);
/// println!("Found {} total resources", all_entries.len());
///
/// // Find a specific resource by name
/// if let Some((resource_type, entry)) =
///     ResourceIterator::find_resource_by_name(&lockfile, "my-agent") {
///     println!("Found {} in {}", entry.name, resource_type);
/// }
/// # Ok(())
/// # }
/// ```
pub struct ResourceIterator;

impl ResourceIterator {
    /// Collect all lockfile entries with their target directories for parallel processing
    ///
    /// This method is optimized for the parallel installation architecture, collecting
    /// all resources from the lockfile along with their target installation directories.
    /// The returned collection can be directly used by the parallel installer.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile containing all resolved resources
    /// * `manifest` - The manifest containing target directory configuration
    ///
    /// # Returns
    ///
    /// A vector of tuples containing each resource and its target directory.
    /// The order matches the processing order defined by [`ResourceTypeExt::all()`].
    ///
    /// # Performance
    ///
    /// This method is optimized for parallel processing:
    /// - Single allocation for the result vector
    /// - Minimal copying of data (references are returned)
    /// - Predictable iteration order for consistent parallel processing
    pub fn collect_all_entries<'a>(
        lockfile: &'a LockFile,
        manifest: &'a Manifest,
    ) -> Vec<(&'a LockedResource, &'a str)> {
        let mut all_entries = Vec::new();

        for resource_type in ResourceType::all() {
            let entries = resource_type.get_lockfile_entries(lockfile);
            let target_dir = resource_type.get_target_dir(&manifest.target);

            for entry in entries {
                all_entries.push((entry, target_dir));
            }
        }

        all_entries
    }

    /// Find a resource by name across all resource types
    pub fn find_resource_by_name<'a>(
        lockfile: &'a LockFile,
        name: &str,
    ) -> Option<(ResourceType, &'a LockedResource)> {
        for resource_type in ResourceType::all().iter() {
            if let Some(entry) = resource_type
                .get_lockfile_entries(lockfile)
                .iter()
                .find(|e| e.name == name)
            {
                return Some((*resource_type, entry));
            }
        }
        None
    }

    /// Count total resources in a lockfile
    pub fn count_total_resources(lockfile: &LockFile) -> usize {
        ResourceType::all()
            .iter()
            .map(|rt| rt.get_lockfile_entries(lockfile).len())
            .sum()
    }

    /// Count total dependencies defined in a manifest
    pub fn count_manifest_dependencies(manifest: &Manifest) -> usize {
        ResourceType::all()
            .iter()
            .map(|rt| rt.get_manifest_entries(manifest).len())
            .sum()
    }

    /// Check if a lockfile has any resources
    pub fn has_resources(lockfile: &LockFile) -> bool {
        ResourceType::all()
            .iter()
            .any(|rt| !rt.get_lockfile_entries(lockfile).is_empty())
    }

    /// Get all resource names from a lockfile
    pub fn get_all_resource_names(lockfile: &LockFile) -> Vec<String> {
        let mut names = Vec::new();
        for resource_type in ResourceType::all() {
            for entry in resource_type.get_lockfile_entries(lockfile) {
                names.push(entry.name.clone());
            }
        }
        names
    }

    /// Get resources of a specific type by source
    pub fn get_resources_by_source<'a>(
        lockfile: &'a LockFile,
        resource_type: ResourceType,
        source: &str,
    ) -> Vec<&'a LockedResource> {
        resource_type
            .get_lockfile_entries(lockfile)
            .iter()
            .filter(|e| e.source.as_deref() == Some(source))
            .collect()
    }

    /// Apply a function to all resources of all types
    pub fn for_each_resource<F>(lockfile: &LockFile, mut f: F)
    where
        F: FnMut(ResourceType, &LockedResource),
    {
        for resource_type in ResourceType::all().iter() {
            for entry in resource_type.get_lockfile_entries(lockfile) {
                f(*resource_type, entry);
            }
        }
    }

    /// Map over all resources and collect results
    pub fn map_resources<T, F>(lockfile: &LockFile, mut f: F) -> Vec<T>
    where
        F: FnMut(ResourceType, &LockedResource) -> T,
    {
        let mut results = Vec::new();
        Self::for_each_resource(lockfile, |rt, entry| {
            results.push(f(rt, entry));
        });
        results
    }

    /// Filter resources based on a predicate
    pub fn filter_resources<F>(
        lockfile: &LockFile,
        mut predicate: F,
    ) -> Vec<(ResourceType, LockedResource)>
    where
        F: FnMut(ResourceType, &LockedResource) -> bool,
    {
        let mut results = Vec::new();
        Self::for_each_resource(lockfile, |rt, entry| {
            if predicate(rt, entry) {
                results.push((rt, entry.clone()));
            }
        });
        results
    }

    /// Group resources by source
    pub fn group_by_source(
        lockfile: &LockFile,
    ) -> std::collections::HashMap<String, Vec<(ResourceType, LockedResource)>> {
        let mut groups = std::collections::HashMap::new();

        Self::for_each_resource(lockfile, |rt, entry| {
            if let Some(ref source) = entry.source {
                groups
                    .entry(source.clone())
                    .or_insert_with(Vec::new)
                    .push((rt, entry.clone()));
            }
        });

        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{LockFile, LockedResource};
    use crate::manifest::{Manifest, TargetConfig};

    fn create_test_lockfile() -> LockFile {
        let mut lockfile = LockFile::new();

        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:abc".to_string(),
            installed_at: ".claude/agents/test-agent.md".to_string(),
        });

        lockfile.snippets.push(LockedResource {
            name: "test-snippet".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "snippets/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("def456".to_string()),
            checksum: "sha256:def".to_string(),
            installed_at: ".claude/snippets/test-snippet.md".to_string(),
        });

        lockfile
    }

    fn create_test_manifest() -> Manifest {
        Manifest {
            target: TargetConfig::default(),
            ..Default::default()
        }
    }

    fn create_multi_resource_lockfile() -> LockFile {
        let mut lockfile = LockFile::new();

        // Add agents from different sources
        lockfile.agents.push(LockedResource {
            name: "agent1".to_string(),
            source: Some("source1".to_string()),
            url: Some("https://github.com/source1/repo.git".to_string()),
            path: "agents/agent1.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:abc1".to_string(),
            installed_at: ".claude/agents/agent1.md".to_string(),
        });

        lockfile.agents.push(LockedResource {
            name: "agent2".to_string(),
            source: Some("source2".to_string()),
            url: Some("https://github.com/source2/repo.git".to_string()),
            path: "agents/agent2.md".to_string(),
            version: Some("v2.0.0".to_string()),
            resolved_commit: Some("def456".to_string()),
            checksum: "sha256:def2".to_string(),
            installed_at: ".claude/agents/agent2.md".to_string(),
        });

        // Add commands from source1
        lockfile.commands.push(LockedResource {
            name: "command1".to_string(),
            source: Some("source1".to_string()),
            url: Some("https://github.com/source1/repo.git".to_string()),
            path: "commands/command1.md".to_string(),
            version: Some("v1.1.0".to_string()),
            resolved_commit: Some("ghi789".to_string()),
            checksum: "sha256:ghi3".to_string(),
            installed_at: ".claude/commands/command1.md".to_string(),
        });

        // Add scripts
        lockfile.scripts.push(LockedResource {
            name: "script1".to_string(),
            source: Some("source1".to_string()),
            url: Some("https://github.com/source1/repo.git".to_string()),
            path: "scripts/build.sh".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("jkl012".to_string()),
            checksum: "sha256:jkl4".to_string(),
            installed_at: ".claude/ccpm/scripts/script1.sh".to_string(),
        });

        // Add hooks
        lockfile.hooks.push(LockedResource {
            name: "hook1".to_string(),
            source: Some("source2".to_string()),
            url: Some("https://github.com/source2/repo.git".to_string()),
            path: "hooks/pre-commit.json".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("mno345".to_string()),
            checksum: "sha256:mno5".to_string(),
            installed_at: ".claude/ccpm/hooks/hook1.json".to_string(),
        });

        // Add MCP servers
        lockfile.mcp_servers.push(LockedResource {
            name: "mcp1".to_string(),
            source: Some("source1".to_string()),
            url: Some("https://github.com/source1/repo.git".to_string()),
            path: "mcp-servers/filesystem.json".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("pqr678".to_string()),
            checksum: "sha256:pqr6".to_string(),
            installed_at: ".claude/ccpm/mcp-servers/mcp1.json".to_string(),
        });

        // Add resource without source
        lockfile.snippets.push(LockedResource {
            name: "local-snippet".to_string(),
            source: None,
            url: None,
            path: "local/snippet.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:local".to_string(),
            installed_at: ".claude/ccpm/snippets/local-snippet.md".to_string(),
        });

        lockfile
    }

    #[test]
    fn test_resource_type_all() {
        let all_types = ResourceType::all();
        assert_eq!(all_types.len(), 6);
        // Order from ResourceTypeExt::all() implementation (consistent with resource.rs)
        assert_eq!(all_types[0], ResourceType::Agent);
        assert_eq!(all_types[1], ResourceType::Snippet);
        assert_eq!(all_types[2], ResourceType::Command);
        assert_eq!(all_types[3], ResourceType::McpServer);
        assert_eq!(all_types[4], ResourceType::Script);
        assert_eq!(all_types[5], ResourceType::Hook);
    }

    #[test]
    fn test_get_lockfile_entries_mut() {
        let mut lockfile = create_test_lockfile();

        // Test with agent type
        let mut agent_type = ResourceType::Agent;
        let entries = agent_type.get_lockfile_entries_mut(&mut lockfile);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test-agent");

        // Add a new agent
        entries.push(LockedResource {
            name: "new-agent".to_string(),
            source: Some("test".to_string()),
            url: Some("https://example.com/repo.git".to_string()),
            path: "agents/new.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("xyz789".to_string()),
            checksum: "sha256:xyz".to_string(),
            installed_at: ".claude/agents/new-agent.md".to_string(),
        });

        // Verify the agent was added
        assert_eq!(lockfile.agents.len(), 2);
        assert_eq!(lockfile.agents[1].name, "new-agent");

        // Test with all resource types
        let mut snippet_type = ResourceType::Snippet;
        let snippet_entries = snippet_type.get_lockfile_entries_mut(&mut lockfile);
        assert_eq!(snippet_entries.len(), 1);

        let mut command_type = ResourceType::Command;
        let command_entries = command_type.get_lockfile_entries_mut(&mut lockfile);
        assert_eq!(command_entries.len(), 0);

        let mut script_type = ResourceType::Script;
        let script_entries = script_type.get_lockfile_entries_mut(&mut lockfile);
        assert_eq!(script_entries.len(), 0);

        let mut hook_type = ResourceType::Hook;
        let hook_entries = hook_type.get_lockfile_entries_mut(&mut lockfile);
        assert_eq!(hook_entries.len(), 0);

        let mut mcp_type = ResourceType::McpServer;
        let mcp_entries = mcp_type.get_lockfile_entries_mut(&mut lockfile);
        assert_eq!(mcp_entries.len(), 0);
    }

    #[test]
    fn test_collect_all_entries() {
        let lockfile = create_test_lockfile();
        let manifest = create_test_manifest();

        let entries = ResourceIterator::collect_all_entries(&lockfile, &manifest);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].0.name, "test-agent");
        assert_eq!(entries[0].1, ".claude/agents");

        assert_eq!(entries[1].0.name, "test-snippet");
        assert_eq!(entries[1].1, ".claude/ccpm/snippets");
    }

    #[test]
    fn test_collect_all_entries_empty_lockfile() {
        let empty_lockfile = LockFile::new();
        let manifest = create_test_manifest();

        let entries = ResourceIterator::collect_all_entries(&empty_lockfile, &manifest);
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_collect_all_entries_multiple_resources() {
        let lockfile = create_multi_resource_lockfile();
        let manifest = create_test_manifest();

        let entries = ResourceIterator::collect_all_entries(&lockfile, &manifest);

        // Should have 7 resources total (2 agents, 1 command, 1 script, 1 hook, 1 mcp_server, 1 snippet)
        assert_eq!(entries.len(), 7);

        // Check that we have entries from all resource types
        let mut found_types = std::collections::HashSet::new();
        for (resource, _) in &entries {
            match resource.name.as_str() {
                "agent1" | "agent2" => {
                    found_types.insert("agent");
                }
                "local-snippet" => {
                    found_types.insert("snippet");
                }
                "command1" => {
                    found_types.insert("command");
                }
                "script1" => {
                    found_types.insert("script");
                }
                "hook1" => {
                    found_types.insert("hook");
                }
                "mcp1" => {
                    found_types.insert("mcp");
                }
                _ => {}
            }
        }

        assert_eq!(found_types.len(), 6);
    }

    #[test]
    fn test_find_resource_by_name() {
        let lockfile = create_test_lockfile();

        let result = ResourceIterator::find_resource_by_name(&lockfile, "test-agent");
        assert!(result.is_some());
        let (rt, resource) = result.unwrap();
        assert_eq!(rt, ResourceType::Agent);
        assert_eq!(resource.name, "test-agent");

        let result = ResourceIterator::find_resource_by_name(&lockfile, "nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_resource_by_name_multiple_types() {
        let lockfile = create_multi_resource_lockfile();

        // Find agent
        let result = ResourceIterator::find_resource_by_name(&lockfile, "agent1");
        assert!(result.is_some());
        let (rt, resource) = result.unwrap();
        assert_eq!(rt, ResourceType::Agent);
        assert_eq!(resource.name, "agent1");

        // Find command
        let result = ResourceIterator::find_resource_by_name(&lockfile, "command1");
        assert!(result.is_some());
        let (rt, resource) = result.unwrap();
        assert_eq!(rt, ResourceType::Command);
        assert_eq!(resource.name, "command1");

        // Find script
        let result = ResourceIterator::find_resource_by_name(&lockfile, "script1");
        assert!(result.is_some());
        let (rt, resource) = result.unwrap();
        assert_eq!(rt, ResourceType::Script);
        assert_eq!(resource.name, "script1");

        // Find hook
        let result = ResourceIterator::find_resource_by_name(&lockfile, "hook1");
        assert!(result.is_some());
        let (rt, resource) = result.unwrap();
        assert_eq!(rt, ResourceType::Hook);
        assert_eq!(resource.name, "hook1");

        // Find MCP server
        let result = ResourceIterator::find_resource_by_name(&lockfile, "mcp1");
        assert!(result.is_some());
        let (rt, resource) = result.unwrap();
        assert_eq!(rt, ResourceType::McpServer);
        assert_eq!(resource.name, "mcp1");

        // Find local snippet (no source)
        let result = ResourceIterator::find_resource_by_name(&lockfile, "local-snippet");
        assert!(result.is_some());
        let (rt, resource) = result.unwrap();
        assert_eq!(rt, ResourceType::Snippet);
        assert_eq!(resource.name, "local-snippet");
        assert!(resource.source.is_none());
    }

    #[test]
    fn test_count_and_has_resources() {
        let lockfile = create_test_lockfile();
        assert_eq!(ResourceIterator::count_total_resources(&lockfile), 2);
        assert!(ResourceIterator::has_resources(&lockfile));

        let empty_lockfile = LockFile::new();
        assert_eq!(ResourceIterator::count_total_resources(&empty_lockfile), 0);
        assert!(!ResourceIterator::has_resources(&empty_lockfile));

        let multi_lockfile = create_multi_resource_lockfile();
        assert_eq!(ResourceIterator::count_total_resources(&multi_lockfile), 7);
        assert!(ResourceIterator::has_resources(&multi_lockfile));
    }

    #[test]
    fn test_get_all_resource_names() {
        let lockfile = create_test_lockfile();
        let names = ResourceIterator::get_all_resource_names(&lockfile);

        assert_eq!(names.len(), 2);
        assert!(names.contains(&"test-agent".to_string()));
        assert!(names.contains(&"test-snippet".to_string()));
    }

    #[test]
    fn test_get_all_resource_names_empty() {
        let empty_lockfile = LockFile::new();
        let names = ResourceIterator::get_all_resource_names(&empty_lockfile);
        assert_eq!(names.len(), 0);
    }

    #[test]
    fn test_get_all_resource_names_multiple() {
        let lockfile = create_multi_resource_lockfile();
        let names = ResourceIterator::get_all_resource_names(&lockfile);

        assert_eq!(names.len(), 7);
        assert!(names.contains(&"agent1".to_string()));
        assert!(names.contains(&"agent2".to_string()));
        assert!(names.contains(&"local-snippet".to_string()));
        assert!(names.contains(&"command1".to_string()));
        assert!(names.contains(&"script1".to_string()));
        assert!(names.contains(&"hook1".to_string()));
        assert!(names.contains(&"mcp1".to_string()));
    }

    #[test]
    fn test_get_resources_by_source() {
        let lockfile = create_multi_resource_lockfile();

        // Test source1 - should have agent1, command1, script1, and mcp1
        let source1_resources =
            ResourceIterator::get_resources_by_source(&lockfile, ResourceType::Agent, "source1");
        assert_eq!(source1_resources.len(), 1);
        assert_eq!(source1_resources[0].name, "agent1");

        let source1_commands =
            ResourceIterator::get_resources_by_source(&lockfile, ResourceType::Command, "source1");
        assert_eq!(source1_commands.len(), 1);
        assert_eq!(source1_commands[0].name, "command1");

        let source1_scripts =
            ResourceIterator::get_resources_by_source(&lockfile, ResourceType::Script, "source1");
        assert_eq!(source1_scripts.len(), 1);
        assert_eq!(source1_scripts[0].name, "script1");

        let source1_mcps = ResourceIterator::get_resources_by_source(
            &lockfile,
            ResourceType::McpServer,
            "source1",
        );
        assert_eq!(source1_mcps.len(), 1);
        assert_eq!(source1_mcps[0].name, "mcp1");

        // Test source2 - should have agent2 and hook1
        let source2_agents =
            ResourceIterator::get_resources_by_source(&lockfile, ResourceType::Agent, "source2");
        assert_eq!(source2_agents.len(), 1);
        assert_eq!(source2_agents[0].name, "agent2");

        let source2_hooks =
            ResourceIterator::get_resources_by_source(&lockfile, ResourceType::Hook, "source2");
        assert_eq!(source2_hooks.len(), 1);
        assert_eq!(source2_hooks[0].name, "hook1");

        // Test nonexistent source
        let nonexistent = ResourceIterator::get_resources_by_source(
            &lockfile,
            ResourceType::Agent,
            "nonexistent",
        );
        assert_eq!(nonexistent.len(), 0);

        // Test empty resource type
        let source1_snippets =
            ResourceIterator::get_resources_by_source(&lockfile, ResourceType::Snippet, "source1");
        assert_eq!(source1_snippets.len(), 0);
    }

    #[test]
    fn test_for_each_resource() {
        let lockfile = create_multi_resource_lockfile();
        let mut visited_resources = Vec::new();

        ResourceIterator::for_each_resource(&lockfile, |resource_type, resource| {
            visited_resources.push((resource_type, resource.name.clone()));
        });

        assert_eq!(visited_resources.len(), 7);

        // Check that we visited all expected resources
        let expected_resources = vec![
            (ResourceType::Agent, "agent1".to_string()),
            (ResourceType::Agent, "agent2".to_string()),
            (ResourceType::Snippet, "local-snippet".to_string()),
            (ResourceType::Command, "command1".to_string()),
            (ResourceType::Script, "script1".to_string()),
            (ResourceType::Hook, "hook1".to_string()),
            (ResourceType::McpServer, "mcp1".to_string()),
        ];

        for expected in expected_resources {
            assert!(visited_resources.contains(&expected));
        }
    }

    #[test]
    fn test_for_each_resource_empty() {
        let empty_lockfile = LockFile::new();
        let mut count = 0;

        ResourceIterator::for_each_resource(&empty_lockfile, |_, _| {
            count += 1;
        });

        assert_eq!(count, 0);
    }

    #[test]
    fn test_map_resources() {
        let lockfile = create_multi_resource_lockfile();

        // Map to resource names
        let names = ResourceIterator::map_resources(&lockfile, |_, resource| resource.name.clone());

        assert_eq!(names.len(), 7);
        assert!(names.contains(&"agent1".to_string()));
        assert!(names.contains(&"agent2".to_string()));
        assert!(names.contains(&"local-snippet".to_string()));
        assert!(names.contains(&"command1".to_string()));
        assert!(names.contains(&"script1".to_string()));
        assert!(names.contains(&"hook1".to_string()));
        assert!(names.contains(&"mcp1".to_string()));

        // Map to resource type and name pairs
        let type_name_pairs =
            ResourceIterator::map_resources(&lockfile, |resource_type, resource| {
                format!("{}:{}", resource_type, resource.name)
            });

        assert_eq!(type_name_pairs.len(), 7);
        assert!(type_name_pairs.contains(&"agent:agent1".to_string()));
        assert!(type_name_pairs.contains(&"agent:agent2".to_string()));
        assert!(type_name_pairs.contains(&"snippet:local-snippet".to_string()));
        assert!(type_name_pairs.contains(&"command:command1".to_string()));
        assert!(type_name_pairs.contains(&"script:script1".to_string()));
        assert!(type_name_pairs.contains(&"hook:hook1".to_string()));
        assert!(type_name_pairs.contains(&"mcp-server:mcp1".to_string()));
    }

    #[test]
    fn test_map_resources_empty() {
        let empty_lockfile = LockFile::new();

        let results =
            ResourceIterator::map_resources(&empty_lockfile, |_, resource| resource.name.clone());

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_filter_resources() {
        let lockfile = create_multi_resource_lockfile();

        // Filter by source1
        let source1_resources = ResourceIterator::filter_resources(&lockfile, |_, resource| {
            resource.source.as_deref() == Some("source1")
        });

        assert_eq!(source1_resources.len(), 4); // agent1, command1, script1, mcp1
        let source1_names: Vec<String> = source1_resources
            .iter()
            .map(|(_, r)| r.name.clone())
            .collect();
        assert!(source1_names.contains(&"agent1".to_string()));
        assert!(source1_names.contains(&"command1".to_string()));
        assert!(source1_names.contains(&"script1".to_string()));
        assert!(source1_names.contains(&"mcp1".to_string()));

        // Filter by resource type
        let agents = ResourceIterator::filter_resources(&lockfile, |resource_type, _| {
            resource_type == ResourceType::Agent
        });

        assert_eq!(agents.len(), 2); // agent1, agent2
        let agent_names: Vec<String> = agents.iter().map(|(_, r)| r.name.clone()).collect();
        assert!(agent_names.contains(&"agent1".to_string()));
        assert!(agent_names.contains(&"agent2".to_string()));

        // Filter resources without source
        let no_source_resources =
            ResourceIterator::filter_resources(&lockfile, |_, resource| resource.source.is_none());

        assert_eq!(no_source_resources.len(), 1); // local-snippet
        assert_eq!(no_source_resources[0].1.name, "local-snippet");

        // Filter by version pattern
        let v1_resources = ResourceIterator::filter_resources(&lockfile, |_, resource| {
            resource.version.as_deref().unwrap_or("").starts_with("v1.")
        });

        assert_eq!(v1_resources.len(), 5); // agent1, command1, script1, hook1, mcp1 all have v1.x.x

        // Filter that matches nothing
        let no_matches = ResourceIterator::filter_resources(&lockfile, |_, resource| {
            resource.name == "nonexistent"
        });

        assert_eq!(no_matches.len(), 0);
    }

    #[test]
    fn test_filter_resources_empty() {
        let empty_lockfile = LockFile::new();

        let results = ResourceIterator::filter_resources(&empty_lockfile, |_, _| true);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_group_by_source() {
        let lockfile = create_multi_resource_lockfile();

        let groups = ResourceIterator::group_by_source(&lockfile);

        assert_eq!(groups.len(), 2); // source1 and source2

        // Check source1 group
        let source1_group = groups.get("source1").unwrap();
        assert_eq!(source1_group.len(), 4); // agent1, command1, script1, mcp1

        let source1_names: Vec<String> =
            source1_group.iter().map(|(_, r)| r.name.clone()).collect();
        assert!(source1_names.contains(&"agent1".to_string()));
        assert!(source1_names.contains(&"command1".to_string()));
        assert!(source1_names.contains(&"script1".to_string()));
        assert!(source1_names.contains(&"mcp1".to_string()));

        // Check source2 group
        let source2_group = groups.get("source2").unwrap();
        assert_eq!(source2_group.len(), 2); // agent2, hook1

        let source2_names: Vec<String> =
            source2_group.iter().map(|(_, r)| r.name.clone()).collect();
        assert!(source2_names.contains(&"agent2".to_string()));
        assert!(source2_names.contains(&"hook1".to_string()));

        // Resources without source should not be included
        assert!(!groups.contains_key(""));
    }

    #[test]
    fn test_group_by_source_empty() {
        let empty_lockfile = LockFile::new();

        let groups = ResourceIterator::group_by_source(&empty_lockfile);
        assert_eq!(groups.len(), 0);
    }

    #[test]
    fn test_group_by_source_no_sources() {
        let mut lockfile = LockFile::new();

        // Add resource without source
        lockfile.agents.push(LockedResource {
            name: "local-agent".to_string(),
            source: None,
            url: None,
            path: "local/agent.md".to_string(),
            version: None,
            resolved_commit: None,
            checksum: "sha256:local".to_string(),
            installed_at: ".claude/agents/local-agent.md".to_string(),
        });

        let groups = ResourceIterator::group_by_source(&lockfile);
        assert_eq!(groups.len(), 0); // No groups because resource has no source
    }

    #[test]
    fn test_resource_type_ext() {
        let lockfile = create_test_lockfile();

        assert_eq!(ResourceType::Agent.get_lockfile_entries(&lockfile).len(), 1);
        assert_eq!(
            ResourceType::Snippet.get_lockfile_entries(&lockfile).len(),
            1
        );
        assert_eq!(
            ResourceType::Command.get_lockfile_entries(&lockfile).len(),
            0
        );
    }

    #[test]
    fn test_resource_type_ext_all_types() {
        let lockfile = create_multi_resource_lockfile();

        assert_eq!(ResourceType::Agent.get_lockfile_entries(&lockfile).len(), 2);
        assert_eq!(
            ResourceType::Snippet.get_lockfile_entries(&lockfile).len(),
            1
        );
        assert_eq!(
            ResourceType::Command.get_lockfile_entries(&lockfile).len(),
            1
        );
        assert_eq!(
            ResourceType::Script.get_lockfile_entries(&lockfile).len(),
            1
        );
        assert_eq!(ResourceType::Hook.get_lockfile_entries(&lockfile).len(), 1);
        assert_eq!(
            ResourceType::McpServer
                .get_lockfile_entries(&lockfile)
                .len(),
            1
        );
    }

    #[test]
    fn test_resource_type_get_target_dir() {
        let manifest = create_test_manifest();
        let targets = &manifest.target;

        assert_eq!(
            ResourceType::Agent.get_target_dir(targets),
            ".claude/agents"
        );
        assert_eq!(
            ResourceType::Snippet.get_target_dir(targets),
            ".claude/ccpm/snippets"
        );
        assert_eq!(
            ResourceType::Command.get_target_dir(targets),
            ".claude/commands"
        );
        assert_eq!(
            ResourceType::Script.get_target_dir(targets),
            ".claude/ccpm/scripts"
        );
        assert_eq!(
            ResourceType::Hook.get_target_dir(targets),
            ".claude/ccpm/hooks"
        );
        assert_eq!(
            ResourceType::McpServer.get_target_dir(targets),
            ".claude/ccpm/mcp-servers"
        );
    }
}
