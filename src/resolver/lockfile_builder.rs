//! Lockfile building and management functionality.
//!
//! This module handles the creation, updating, and maintenance of lockfile entries,
//! including conflict detection, stale entry removal, and transitive dependency management.

use crate::core::ResourceType;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::Manifest;
use std::collections::HashSet;

/// Manages lockfile operations including entry creation, updates, and cleanup.
pub struct LockfileBuilder<'a> {
    manifest: &'a Manifest,
}

impl<'a> LockfileBuilder<'a> {
    /// Create a new lockfile builder with the given manifest.
    pub fn new(manifest: &'a Manifest) -> Self {
        Self {
            manifest,
        }
    }

    /// Add or update a lockfile entry, replacing existing entries with the same name, source, and tool.
    ///
    /// This method handles deduplication by using (name, source, tool) tuples as the unique key.
    /// This allows multiple entries with the same name from different sources or tools,
    /// which will be caught by conflict detection if they map to the same path.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The mutable lockfile to update
    /// * `name` - The name of the dependency (for documentation purposes)
    /// * `entry` - The locked resource entry to add or update
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut lockfile = LockFile::new();
    /// let entry = LockedResource {
    ///     name: "my-agent".to_string(),
    ///     source: Some("community".to_string()),
    ///     tool: "claude-code".to_string(),
    ///     // ... other fields
    /// };
    ///
    /// resolver.add_or_update_lockfile_entry(&mut lockfile, "my-agent", entry);
    ///
    /// // Later updates replace the existing entry
    /// let updated_entry = LockedResource {
    ///     name: "my-agent".to_string(),
    ///     source: Some("community".to_string()),
    ///     tool: "claude-code".to_string(),
    ///     // ... updated fields
    /// };
    /// resolver.add_or_update_lockfile_entry(&mut lockfile, "my-agent", updated_entry);
    /// ```
    pub fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        _name: &str,
        entry: LockedResource,
    ) {
        // Get the appropriate resource collection based on the entry's type
        let resources = lockfile.get_resources_mut(entry.resource_type);

        // Use (name, source, tool) matching for deduplication
        // This allows multiple entries with the same name from different sources or tools,
        // which will be caught by conflict detection if they map to the same path
        if let Some(existing) = resources
            .iter_mut()
            .find(|e| e.name == entry.name && e.source == entry.source && e.tool == entry.tool)
        {
            *existing = entry;
        } else {
            resources.push(entry);
        }
    }

    /// Removes stale lockfile entries that are no longer in the manifest.
    ///
    /// This method removes lockfile entries for direct manifest dependencies that have been
    /// commented out or removed from the manifest. This must be called BEFORE
    /// `remove_manifest_entries_for_update()` to ensure stale entries don't cause conflicts
    /// during resolution.
    ///
    /// A manifest-level entry is identified by:
    /// - `manifest_alias.is_none()` - Direct dependency with no pattern expansion
    /// - `manifest_alias.is_some()` - Pattern-expanded dependency (alias must be in manifest)
    ///
    /// For each stale entry found, this also removes its transitive children to maintain
    /// lockfile consistency.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The mutable lockfile to clean
    ///
    /// # Examples
    ///
    /// If a user comments out an agent in agpm.toml:
    /// ```toml
    /// # [agents]
    /// # example = { source = "community", path = "agents/example.md", version = "v1.0.0" }
    /// ```
    ///
    /// This function will remove the "example" agent from the lockfile and all its transitive
    /// dependencies before the update process begins.
    pub fn remove_stale_manifest_entries(&self, lockfile: &mut LockFile) {
        // Collect all current manifest keys for each resource type
        let manifest_agents: HashSet<String> =
            self.manifest.agents.keys().map(|k| k.to_string()).collect();
        let manifest_snippets: HashSet<String> =
            self.manifest.snippets.keys().map(|k| k.to_string()).collect();
        let manifest_commands: HashSet<String> =
            self.manifest.commands.keys().map(|k| k.to_string()).collect();
        let manifest_scripts: HashSet<String> =
            self.manifest.scripts.keys().map(|k| k.to_string()).collect();
        let manifest_hooks: HashSet<String> =
            self.manifest.hooks.keys().map(|k| k.to_string()).collect();
        let manifest_mcp_servers: HashSet<String> =
            self.manifest.mcp_servers.keys().map(|k| k.to_string()).collect();

        // Helper to get the right manifest keys for a resource type
        let get_manifest_keys = |resource_type: ResourceType| match resource_type {
            ResourceType::Agent => &manifest_agents,
            ResourceType::Snippet => &manifest_snippets,
            ResourceType::Command => &manifest_commands,
            ResourceType::Script => &manifest_scripts,
            ResourceType::Hook => &manifest_hooks,
            ResourceType::McpServer => &manifest_mcp_servers,
        };

        // Collect (name, source) pairs to remove
        let mut entries_to_remove: HashSet<(String, Option<String>)> = HashSet::new();
        let mut direct_entries: Vec<(String, Option<String>)> = Vec::new();

        // Find all manifest-level entries that are no longer in the manifest
        for resource_type in ResourceType::all() {
            let manifest_keys = get_manifest_keys(*resource_type);
            let resources = lockfile.get_resources(*resource_type);

            for entry in resources {
                // Determine if this is a stale manifest-level entry (no longer in manifest)
                let is_stale = if let Some(ref alias) = entry.manifest_alias {
                    // Pattern-expanded entry: stale if alias is NOT in manifest
                    !manifest_keys.contains(alias)
                } else {
                    // Direct entry: stale if name is NOT in manifest
                    !manifest_keys.contains(&entry.name)
                };

                if is_stale {
                    let key = (entry.name.clone(), entry.source.clone());
                    entries_to_remove.insert(key.clone());
                    direct_entries.push(key);
                }
            }
        }

        // For each stale entry, recursively collect its transitive children
        for (parent_name, parent_source) in direct_entries {
            for resource_type in ResourceType::all() {
                if let Some(parent_entry) = lockfile
                    .get_resources(*resource_type)
                    .iter()
                    .find(|e| e.name == parent_name && e.source == parent_source)
                {
                    Self::collect_transitive_children(
                        lockfile,
                        parent_entry,
                        &mut entries_to_remove,
                    );
                }
            }
        }

        // Remove all marked entries
        let should_remove = |entry: &LockedResource| {
            entries_to_remove.contains(&(entry.name.clone(), entry.source.clone()))
        };

        lockfile.agents.retain(|entry| !should_remove(entry));
        lockfile.snippets.retain(|entry| !should_remove(entry));
        lockfile.commands.retain(|entry| !should_remove(entry));
        lockfile.scripts.retain(|entry| !should_remove(entry));
        lockfile.hooks.retain(|entry| !should_remove(entry));
        lockfile.mcp_servers.retain(|entry| !should_remove(entry));
    }

    /// Removes lockfile entries for manifest dependencies that will be re-resolved.
    ///
    /// This method removes old entries for direct manifest dependencies before updating,
    /// which handles the case where a dependency's source or resource type changes.
    /// This prevents duplicate entries with the same name but different sources.
    ///
    /// Pattern-expanded and transitive dependencies are preserved because:
    /// - Pattern expansions will be re-added during resolution with (name, source) matching
    /// - Transitive dependencies aren't manifest keys and won't be removed
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The mutable lockfile to clean
    /// * `manifest_keys` - Set of manifest dependency keys being updated
    pub fn remove_manifest_entries_for_update(
        &self,
        lockfile: &mut LockFile,
        manifest_keys: &HashSet<String>,
    ) {
        // Collect (name, source) pairs to remove
        // We use (name, source) tuples to distinguish same-named resources from different sources
        let mut entries_to_remove: HashSet<(String, Option<String>)> = HashSet::new();

        // Step 1: Find direct manifest entries and collect them for transitive traversal
        let mut direct_entries: Vec<(String, Option<String>)> = Vec::new();

        for resource_type in ResourceType::all() {
            let resources = lockfile.get_resources(*resource_type);
            for entry in resources {
                // Check if this entry originates from a manifest key being updated
                if manifest_keys.contains(&entry.name)
                    || entry
                        .manifest_alias
                        .as_ref()
                        .is_some_and(|alias| manifest_keys.contains(alias))
                {
                    let key = (entry.name.clone(), entry.source.clone());
                    entries_to_remove.insert(key.clone());
                    direct_entries.push(key);
                }
            }
        }

        // Step 2: For each direct entry, recursively collect its transitive children
        // This ensures that when "agent-A" changes from repo1 to repo2, we also remove
        // all transitive dependencies that came from repo1 via agent-A
        for (parent_name, parent_source) in direct_entries {
            // Find the parent entry in the lockfile
            for resource_type in ResourceType::all() {
                if let Some(parent_entry) = lockfile
                    .get_resources(*resource_type)
                    .iter()
                    .find(|e| e.name == parent_name && e.source == parent_source)
                {
                    // Walk its dependency tree
                    Self::collect_transitive_children(
                        lockfile,
                        parent_entry,
                        &mut entries_to_remove,
                    );
                }
            }
        }

        // Step 3: Remove all marked entries
        let should_remove = |entry: &LockedResource| {
            entries_to_remove.contains(&(entry.name.clone(), entry.source.clone()))
        };

        lockfile.agents.retain(|entry| !should_remove(entry));
        lockfile.snippets.retain(|entry| !should_remove(entry));
        lockfile.commands.retain(|entry| !should_remove(entry));
        lockfile.scripts.retain(|entry| !should_remove(entry));
        lockfile.hooks.retain(|entry| !should_remove(entry));
        lockfile.mcp_servers.retain(|entry| !should_remove(entry));
    }

    /// Recursively collect all transitive children of a lockfile entry.
    ///
    /// This walks the dependency graph starting from `parent`, following the `dependencies`
    /// field to find all resources that transitively depend on the parent. Only dependencies
    /// with the same source as the parent are collected (to avoid removing unrelated resources).
    ///
    /// The `dependencies` field contains strings in the format:
    /// - `"resource_type/name"` for dependencies from the same source
    /// - `"source:resource_type/name:version"` for explicit source references
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile to search for dependencies
    /// * `parent` - The parent entry whose children we want to collect
    /// * `entries_to_remove` - Set of (name, source) pairs to populate with found children
    fn collect_transitive_children(
        lockfile: &LockFile,
        parent: &LockedResource,
        entries_to_remove: &mut HashSet<(String, Option<String>)>,
    ) {
        // For each dependency declared by this parent
        for dep_ref in &parent.dependencies {
            // Parse dependency reference: "source:resource_type/name:version" or "resource_type/name"
            // Examples: "repo1:snippet/utils:v1.0.0" or "agent/helper"
            let (dep_source, dep_name) = if let Some(colon_pos) = dep_ref.find(':') {
                // Format: "source:resource_type/name:version"
                let source_part = &dep_ref[..colon_pos];
                let rest = &dep_ref[colon_pos + 1..];
                // Find the resource_type/name part (before optional :version)
                let type_name_part = if let Some(ver_colon) = rest.rfind(':') {
                    &rest[..ver_colon]
                } else {
                    rest
                };
                // Extract name from "resource_type/name"
                if let Some(slash_pos) = type_name_part.find('/') {
                    let name = &type_name_part[slash_pos + 1..];
                    (Some(source_part.to_string()), name.to_string())
                } else {
                    continue; // Invalid format, skip
                }
            } else {
                // Format: "resource_type/name"
                if let Some(slash_pos) = dep_ref.find('/') {
                    let name = &dep_ref[slash_pos + 1..];
                    // Inherit parent's source
                    (parent.source.clone(), name.to_string())
                } else {
                    continue; // Invalid format, skip
                }
            };

            // Find the dependency entry with matching name and source
            for resource_type in ResourceType::all() {
                if let Some(dep_entry) = lockfile
                    .get_resources(*resource_type)
                    .iter()
                    .find(|e| e.name == dep_name && e.source == dep_source)
                {
                    let key = (dep_entry.name.clone(), dep_entry.source.clone());

                    // Add to removal set and recurse (if not already processed)
                    if !entries_to_remove.contains(&key) {
                        entries_to_remove.insert(key);
                        // Recursively collect this dependency's children
                        Self::collect_transitive_children(lockfile, dep_entry, entries_to_remove);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ResourceType;
    use crate::lockfile::LockedResource;
    use crate::manifest::ResourceDependency;

    fn create_test_manifest() -> Manifest {
        let mut manifest = Manifest::default();
        manifest.agents.insert(
            "test-agent".to_string(),
            ResourceDependency::Simple("agents/test-agent.md".to_string()),
        );
        manifest.snippets.insert(
            "test-snippet".to_string(),
            ResourceDependency::Simple("snippets/test-snippet.md".to_string()),
        );
        manifest
    }

    fn create_test_lockfile() -> LockFile {
        let mut lockfile = LockFile::default();

        // Add some test entries
        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123".to_string()),
            checksum: "sha256:test".to_string(),
            installed_at: ".claude/agents/test-agent.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: "{}".to_string(),
        });

        lockfile.snippets.push(LockedResource {
            name: "test-snippet".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "snippets/test-snippet.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("def456".to_string()),
            checksum: "sha256:test2".to_string(),
            installed_at: ".claude/snippets/test-snippet.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Snippet,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: "{}".to_string(),
        });

        lockfile
    }

    #[test]
    fn test_add_or_update_lockfile_entry_new() {
        let manifest = create_test_manifest();
        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = LockFile::default();

        let entry = LockedResource {
            resource_type: ResourceType::Agent,
            name: "new-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/new-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            installed_at: ".claude/agents/new-agent.md".to_string(),
            resolved_commit: Some("xyz789".to_string()),
            checksum: "sha256:new".to_string(),
            dependencies: vec![],
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: "{}".to_string(),
        };

        builder.add_or_update_lockfile_entry(&mut lockfile, "new-agent", entry);

        assert_eq!(lockfile.agents.len(), 1);
        assert_eq!(lockfile.agents[0].name, "new-agent");
    }

    #[test]
    fn test_add_or_update_lockfile_entry_replace() {
        let manifest = create_test_manifest();
        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = create_test_lockfile();

        let updated_entry = LockedResource {
            resource_type: ResourceType::Agent,
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            installed_at: ".claude/agents/test-agent.md".to_string(),
            resolved_commit: Some("updated123".to_string()), // Updated commit
            checksum: "sha256:updated".to_string(),          // Updated checksum
            dependencies: vec![],
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: "{}".to_string(),
        };

        builder.add_or_update_lockfile_entry(&mut lockfile, "test-agent", updated_entry);

        assert_eq!(lockfile.agents.len(), 1);
        assert_eq!(lockfile.agents[0].resolved_commit, Some("updated123".to_string()));
        assert_eq!(lockfile.agents[0].checksum, "sha256:updated");
    }

    #[test]
    fn test_remove_stale_manifest_entries() {
        let mut manifest = create_test_manifest();
        // Remove one agent from manifest to make it stale
        manifest.agents.remove("test-agent");

        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = create_test_lockfile();

        builder.remove_stale_manifest_entries(&mut lockfile);

        // test-agent should be removed, test-snippet should remain
        assert_eq!(lockfile.agents.len(), 0);
        assert_eq!(lockfile.snippets.len(), 1);
        assert_eq!(lockfile.snippets[0].name, "test-snippet");
    }

    #[test]
    fn test_remove_manifest_entries_for_update() {
        let manifest = create_test_manifest();
        let builder = LockfileBuilder::new(&manifest);
        let mut lockfile = create_test_lockfile();

        let mut manifest_keys = HashSet::new();
        manifest_keys.insert("test-agent".to_string());

        builder.remove_manifest_entries_for_update(&mut lockfile, &manifest_keys);

        // test-agent should be removed, test-snippet should remain
        assert_eq!(lockfile.agents.len(), 0);
        assert_eq!(lockfile.snippets.len(), 1);
        assert_eq!(lockfile.snippets[0].name, "test-snippet");
    }

    #[test]
    fn test_collect_transitive_children() {
        let lockfile = create_test_lockfile();
        let mut entries_to_remove = HashSet::new();

        // Create a parent with dependencies
        let parent = LockedResource {
            resource_type: ResourceType::Agent,
            name: "parent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/test/repo.git".to_string()),
            path: "agents/parent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            installed_at: ".claude/agents/parent.md".to_string(),
            resolved_commit: Some("parent123".to_string()),
            checksum: "sha256:parent".to_string(),
            dependencies: vec!["agent/test-agent".to_string()], // Reference to test-agent
            applied_patches: std::collections::HashMap::new(),
            install: None,
            template_vars: "{}".to_string(),
        };

        LockfileBuilder::collect_transitive_children(&lockfile, &parent, &mut entries_to_remove);

        // Should find the test-agent dependency
        assert!(
            entries_to_remove.contains(&("test-agent".to_string(), Some("community".to_string())))
        );
    }
}
