//! Incremental update and lockfile merging for selective dependency updates.
//!
//! This module handles incremental updates where only specific dependencies
//! are re-resolved while others remain at their locked versions.

use std::collections::HashSet;

use crate::core::ResourceType;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::Manifest;

use super::DependencyResolver;

impl DependencyResolver {
    /// Create filtered manifest containing only specified dependencies.
    ///
    /// Creates manifest with same sources/config but only specified dependencies.
    pub(super) fn create_filtered_manifest(
        &self,
        deps_to_update: &[(String, ResourceType)],
    ) -> Manifest {
        let mut filtered = Manifest {
            sources: self.core.manifest.sources.clone(),
            tools: self.core.manifest.tools.clone(),
            patches: self.core.manifest.patches.clone(),
            project_patches: self.core.manifest.project_patches.clone(),
            private_patches: self.core.manifest.private_patches.clone(),
            manifest_dir: self.core.manifest.manifest_dir.clone(),
            ..Default::default()
        };

        // Filter each resource type
        for (dep_name, resource_type) in deps_to_update {
            let source_map = match resource_type {
                ResourceType::Agent => &self.core.manifest.agents,
                ResourceType::Snippet => &self.core.manifest.snippets,
                ResourceType::Command => &self.core.manifest.commands,
                ResourceType::Script => &self.core.manifest.scripts,
                ResourceType::Hook => &self.core.manifest.hooks,
                ResourceType::McpServer => &self.core.manifest.mcp_servers,
                ResourceType::Skill => &self.core.manifest.skills,
            };

            if let Some(dep_spec) = source_map.get(dep_name) {
                // Add to filtered manifest
                let target_map = match resource_type {
                    ResourceType::Agent => &mut filtered.agents,
                    ResourceType::Snippet => &mut filtered.snippets,
                    ResourceType::Command => &mut filtered.commands,
                    ResourceType::Script => &mut filtered.scripts,
                    ResourceType::Hook => &mut filtered.hooks,
                    ResourceType::McpServer => &mut filtered.mcp_servers,
                    ResourceType::Skill => &mut filtered.skills,
                };
                target_map.insert(dep_name.clone(), dep_spec.clone());
            }
        }

        filtered
    }

    /// Filter lockfile entries into unchanged and to-update groups.
    ///
    /// Separates entries based on whether they match dependency names.
    /// Matches against `manifest_alias` (direct) and `name` (transitive).
    ///
    /// Returns (unchanged_lockfile, deps_requiring_resolution).
    pub(super) fn filter_lockfile_entries(
        existing: &LockFile,
        deps_to_update: &[String],
    ) -> (LockFile, Vec<(String, ResourceType)>) {
        // Convert deps_to_update to a HashSet for faster lookups
        let update_set: HashSet<&String> = deps_to_update.iter().collect();

        let mut unchanged = LockFile::new();
        let mut deps_requiring_resolution = Vec::new();

        // Helper to check if a resource should be updated
        let should_update = |resource: &LockedResource| {
            // Check manifest_alias first (for direct dependencies)
            if let Some(alias) = &resource.manifest_alias {
                if update_set.contains(alias) {
                    return true;
                }
            }
            // Then check canonical name (for transitive dependencies)
            if update_set.contains(&resource.name) {
                return true;
            }
            false
        };

        // Process each resource type
        for resource_type in [
            ResourceType::Agent,
            ResourceType::Snippet,
            ResourceType::Command,
            ResourceType::Script,
            ResourceType::Hook,
            ResourceType::McpServer,
        ] {
            let resources = existing.get_resources(&resource_type);
            let unchanged_resources = unchanged.get_resources_mut(&resource_type);

            for resource in resources {
                if should_update(resource) {
                    // Add to resolution list
                    let name = resource.manifest_alias.as_ref().unwrap_or(&resource.name);
                    deps_requiring_resolution.push((name.clone(), resource_type));
                } else {
                    // Keep in unchanged lockfile
                    unchanged_resources.push(resource.clone());
                }
            }
        }

        // Copy sources as-is (they'll be reused during resolution)
        unchanged.sources = existing.sources.clone();

        (unchanged, deps_requiring_resolution)
    }

    /// Merge unchanged and updated lockfiles.
    ///
    /// Combines entries with updated entries winning conflicts (same name/source/tool/variant).
    ///
    /// Returns merged LockFile.
    pub(super) fn merge_lockfiles(mut unchanged: LockFile, updated: LockFile) -> LockFile {
        // Helper to build identity key for deduplication
        let identity_key = |resource: &LockedResource| -> String {
            format!(
                "{}::{}::{}::{}",
                resource.name,
                resource.source.as_deref().unwrap_or("local"),
                resource.tool.as_deref().unwrap_or(""),
                resource.variant_inputs.hash()
            )
        };

        for resource_type in [
            ResourceType::Agent,
            ResourceType::Snippet,
            ResourceType::Command,
            ResourceType::Script,
            ResourceType::Hook,
            ResourceType::McpServer,
        ] {
            let updated_resources = updated.get_resources(&resource_type);
            let unchanged_resources = unchanged.get_resources_mut(&resource_type);

            // Build set of identities from updated resources
            let updated_identities: HashSet<String> =
                updated_resources.iter().map(&identity_key).collect();

            // Remove unchanged entries that conflict with updated entries
            unchanged_resources.retain(|resource| {
                let key = identity_key(resource);
                !updated_identities.contains(&key)
            });

            // Add all updated resources
            unchanged_resources.extend(updated_resources.iter().cloned());
        }

        // Merge sources (prefer updated sources)
        // Build set of source names from updated
        let updated_source_names: HashSet<&str> =
            updated.sources.iter().map(|s| s.name.as_str()).collect();

        // Remove unchanged sources that are also in updated
        unchanged.sources.retain(|source| !updated_source_names.contains(source.name.as_str()));

        // Add all updated sources
        unchanged.sources.extend(updated.sources);

        unchanged
    }
}
