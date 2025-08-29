//! Resource iteration and collection utilities
//!
//! This module provides abstractions for working with multiple resource types
//! in a unified way, reducing code duplication when iterating over all resource types.

use crate::core::ResourceType;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, TargetConfig};

/// Extension trait for ResourceType that adds lockfile and manifest operations
pub trait ResourceTypeExt {
    /// Get all resource types in iteration order
    fn all() -> Vec<ResourceType>;

    /// Get lockfile entries for this resource type
    fn get_lockfile_entries<'a>(&self, lockfile: &'a LockFile) -> &'a [LockedResource];

    /// Get mutable lockfile entries for this resource type
    fn get_lockfile_entries_mut<'a>(
        &mut self,
        lockfile: &'a mut LockFile,
    ) -> &'a mut Vec<LockedResource>;

    /// Get target directory for this resource type
    fn get_target_dir<'a>(&self, targets: &'a TargetConfig) -> &'a str;
}

impl ResourceTypeExt for ResourceType {
    fn all() -> Vec<ResourceType> {
        vec![
            ResourceType::Agent,
            ResourceType::Snippet,
            ResourceType::Command,
            ResourceType::Script,
            ResourceType::Hook,
            ResourceType::McpServer,
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
}

/// Iterator utilities for working with resources
pub struct ResourceIterator;

impl ResourceIterator {
    /// Collect all lockfile entries with their target directories
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
    fn test_count_and_has_resources() {
        let lockfile = create_test_lockfile();
        assert_eq!(ResourceIterator::count_total_resources(&lockfile), 2);
        assert!(ResourceIterator::has_resources(&lockfile));

        let empty_lockfile = LockFile::new();
        assert_eq!(ResourceIterator::count_total_resources(&empty_lockfile), 0);
        assert!(!ResourceIterator::has_resources(&empty_lockfile));
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
}
