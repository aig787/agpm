//! Lockfile building and management functionality.
//!
//! This module handles the creation, updating, and maintenance of lockfile entries,
//! including conflict detection, stale entry removal, and transitive dependency management.

use crate::core::ResourceType;
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency};
use crate::resolver::types as dependency_helpers;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

// Type aliases for internal lookups
type ResourceKey = (ResourceType, String, Option<String>);
type ResourceInfo = (Option<String>, Option<String>);

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

/// Adds pattern-expanded entries to the lockfile with deduplication.
///
/// This function adds multiple resolved entries from a pattern dependency to the
/// appropriate resource type collection in the lockfile, using (name, source) as
/// the deduplication key.
///
/// # Arguments
///
/// * `lockfile` - The mutable lockfile to update
/// * `entries` - Vector of resolved resources from pattern expansion
/// * `resource_type` - The type of resource being added
///
/// # Deduplication
///
/// Entries are matched by (name, source) tuples. If an entry with the same name
/// and source exists, it is replaced; otherwise the new entry is appended.
pub fn add_pattern_entries(
    lockfile: &mut LockFile,
    entries: Vec<LockedResource>,
    resource_type: ResourceType,
) {
    for entry in entries {
        match resource_type {
            ResourceType::Agent => {
                if let Some(existing) = lockfile
                    .agents
                    .iter_mut()
                    .find(|e| e.name == entry.name && e.source == entry.source)
                {
                    *existing = entry;
                } else {
                    lockfile.agents.push(entry);
                }
            }
            ResourceType::Snippet => {
                if let Some(existing) = lockfile
                    .snippets
                    .iter_mut()
                    .find(|e| e.name == entry.name && e.source == entry.source)
                {
                    *existing = entry;
                } else {
                    lockfile.snippets.push(entry);
                }
            }
            ResourceType::Command => {
                if let Some(existing) = lockfile
                    .commands
                    .iter_mut()
                    .find(|e| e.name == entry.name && e.source == entry.source)
                {
                    *existing = entry;
                } else {
                    lockfile.commands.push(entry);
                }
            }
            ResourceType::Script => {
                if let Some(existing) = lockfile
                    .scripts
                    .iter_mut()
                    .find(|e| e.name == entry.name && e.source == entry.source)
                {
                    *existing = entry;
                } else {
                    lockfile.scripts.push(entry);
                }
            }
            ResourceType::Hook => {
                if let Some(existing) = lockfile
                    .hooks
                    .iter_mut()
                    .find(|e| e.name == entry.name && e.source == entry.source)
                {
                    *existing = entry;
                } else {
                    lockfile.hooks.push(entry);
                }
            }
            ResourceType::McpServer => {
                if let Some(existing) = lockfile
                    .mcp_servers
                    .iter_mut()
                    .find(|e| e.name == entry.name && e.source == entry.source)
                {
                    *existing = entry;
                } else {
                    lockfile.mcp_servers.push(entry);
                }
            }
        }
    }
}

/// Post-processes lockfile entries to add version information to dependencies.
///
/// Updates the `dependencies` field in each lockfile entry from the format
/// `"resource_type/name"` to `"resource_type/name@version"` by looking up
/// the resolved version in the lockfile.
///
/// This is a standalone function used during lockfile finalization to enrich
/// dependency references with version information for better tracking and reproducibility.
pub fn add_version_to_dependencies(lockfile: &mut LockFile) -> Result<()> {
    // Build a lookup map: (resource_type, path, source) -> unique_name
    // This allows us to resolve dependency paths to lockfile names
    // We store both the full path and just the filename for flexible lookup
    let mut lookup_map: HashMap<(ResourceType, String, Option<String>), String> = HashMap::new();

    // Build lookup map from all lockfile entries
    for resource_type in ResourceType::all() {
        for entry in lockfile.get_resources(*resource_type) {
            let normalized_path = dependency_helpers::normalize_lookup_path(&entry.path);
            // Store by full path
            lookup_map.insert(
                (*resource_type, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );
            // Also store by filename for backward compatibility
            if let Some(filename) = dependency_helpers::extract_filename_from_path(&entry.path) {
                lookup_map
                    .insert((*resource_type, filename, entry.source.clone()), entry.name.clone());
            }
            // Also store by type-stripped path (for nested resources like agents/helpers/foo.md -> helpers/foo)
            if let Some(stripped) =
                dependency_helpers::strip_resource_type_directory(&normalized_path)
            {
                lookup_map
                    .insert((*resource_type, stripped, entry.source.clone()), entry.name.clone());
            }
        }
    }

    // Build a complete map of (resource_type, name, source) -> (source, version) for cross-source lookup
    // This needs to be done before we start mutating entries
    let mut resource_info_map: HashMap<ResourceKey, ResourceInfo> = HashMap::new();

    for resource_type in ResourceType::all() {
        for entry in lockfile.get_resources(*resource_type) {
            resource_info_map.insert(
                (*resource_type, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
    }

    // Helper function to update dependencies in a vector of entries
    let update_deps = |entries: &mut Vec<LockedResource>| {
        for entry in entries {
            let parent_source = entry.source.clone();

            let updated_deps: Vec<String> = entry
                .dependencies
                .iter()
                .map(|dep| {
                    // Parse "resource_type/path" format (e.g., "agent/rust-haiku.md" or "snippet/utils.md")
                    if let Some((_resource_type_str, dep_path)) = dep.split_once('/') {
                        // Parse resource type from string form (accepts both singular and plural)
                        if let Ok(resource_type) = _resource_type_str.parse::<ResourceType>() {
                            // Normalize the path for lookup
                            let dep_filename = dependency_helpers::normalize_lookup_path(dep_path);

                            // Look up the resource in the lookup map (same source as parent)
                            if let Some(dep_name) = lookup_map.get(&(
                                resource_type,
                                dep_filename.clone(),
                                parent_source.clone(),
                            )) {
                                // Found resource in same source - add version metadata
                                if let Some((_source, Some(ver))) = resource_info_map.get(&(
                                    resource_type,
                                    dep_name.clone(),
                                    parent_source.clone(),
                                )) {
                                    return format!("{resource_type}/{dep_name}@{ver}");
                                }
                                // Fallback without version if not found in resource_info_map
                                return format!("{resource_type}/{dep_name}");
                            }

                            // If not found with same source, try adding .md extension
                            let dep_filename_with_ext = format!("{dep_filename}.md");
                            if let Some(dep_name) = lookup_map.get(&(
                                resource_type,
                                dep_filename_with_ext.clone(),
                                parent_source.clone(),
                            )) {
                                // Found resource in same source - add version metadata
                                if let Some((_source, Some(ver))) = resource_info_map.get(&(
                                    resource_type,
                                    dep_name.clone(),
                                    parent_source.clone(),
                                )) {
                                    return format!("{resource_type}/{dep_name}@{ver}");
                                }
                                // Fallback without version if not found in resource_info_map
                                return format!("{resource_type}/{dep_name}");
                            }

                            // Try looking for resource from ANY source (cross-source dependency)
                            // Format: source:type/name@version
                            for ((rt, filename, src), name) in &lookup_map {
                                if *rt == resource_type
                                    && (filename == &dep_filename
                                        || filename == &dep_filename_with_ext)
                                {
                                    // Found in different source - need to include source and version
                                    // Use the pre-built resource info map
                                    if let Some((source, version)) = resource_info_map.get(&(
                                        resource_type,
                                        name.clone(),
                                        src.clone(),
                                    )) {
                                        // Build full reference: source:type/name@version
                                        let mut dep_ref = String::new();
                                        if let Some(src) = source {
                                            dep_ref.push_str(src);
                                            dep_ref.push(':');
                                        }
                                        dep_ref.push_str(&resource_type.to_string());
                                        dep_ref.push('/');
                                        dep_ref.push_str(name);
                                        if let Some(ver) = version {
                                            dep_ref.push('@');
                                            dep_ref.push_str(ver);
                                        }
                                        return dep_ref;
                                    }
                                }
                            }
                        }
                    }
                    // If parsing fails or resource not found, return as-is
                    dep.clone()
                })
                .collect();

            entry.dependencies = updated_deps;
        }
    };

    // Update all entry types
    update_deps(&mut lockfile.agents);
    update_deps(&mut lockfile.snippets);
    update_deps(&mut lockfile.commands);
    update_deps(&mut lockfile.scripts);
    update_deps(&mut lockfile.hooks);
    update_deps(&mut lockfile.mcp_servers);

    Ok(())
}

// ============================================================================
// Lockfile Helper Operations
// ============================================================================

/// Helper to generate a unique key for grouping dependencies.
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn group_key(source: &str, version: &str) -> String {
    format!("{source}::{version}")
}

/// Get patches for a specific resource from the manifest.
///
/// Looks up patches defined in `[patch.<resource_type>.<alias>]` sections
/// and returns them as a HashMap ready for inclusion in the lockfile.
///
/// # Arguments
///
/// * `manifest` - Reference to the project manifest containing patches
/// * `resource_type` - Type of the resource (agent, snippet, command, etc.)
/// * `name` - Resource name or manifest_alias to look up patches for
///
/// # Returns
///
/// HashMap of patch key-value pairs, or empty HashMap if no patches defined
pub(super) fn get_patches_for_resource(
    manifest: &Manifest,
    resource_type: ResourceType,
    name: &str,
) -> HashMap<String, toml::Value> {
    let patches = match resource_type {
        ResourceType::Agent => &manifest.patches.agents,
        ResourceType::Snippet => &manifest.patches.snippets,
        ResourceType::Command => &manifest.patches.commands,
        ResourceType::Script => &manifest.patches.scripts,
        ResourceType::Hook => &manifest.patches.hooks,
        ResourceType::McpServer => &manifest.patches.mcp_servers,
    };

    patches.get(name).cloned().unwrap_or_default()
}

/// Build the complete merged template variable context for a dependency.
///
/// This creates the full template_vars that should be stored in the lockfile,
/// combining both the global project configuration and any dependency-specific
/// template_vars overrides.
///
/// This ensures lockfile entries contain the exact template context that was
/// used during dependency resolution, enabling reproducible builds.
///
/// # Arguments
///
/// * `manifest` - Reference to the project manifest containing global project config
/// * `dep` - The dependency to build template_vars for
///
/// # Returns
///
/// Complete merged template_vars (always returns a Value, empty if no variables)
pub(super) fn build_merged_template_vars(
    manifest: &Manifest,
    dep: &ResourceDependency,
) -> serde_json::Value {
    use crate::templating::deep_merge_json;

    // Start with dependency-level template_vars (if any)
    let dep_vars = dep.get_template_vars();

    // Get global project config as JSON
    let global_project = manifest
        .project
        .as_ref()
        .map(|p| p.to_json_value())
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Build complete context
    let mut merged_map = serde_json::Map::new();

    // If dependency has template_vars, start with those
    if let Some(vars) = dep_vars {
        if let Some(obj) = vars.as_object() {
            merged_map.extend(obj.clone());
        }
    }

    // Extract project overrides from dependency template_vars (if present)
    let project_overrides = dep_vars
        .and_then(|v| v.get("project").cloned())
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Deep merge global project config with dependency-specific overrides
    let merged_project = deep_merge_json(global_project, &project_overrides);

    // Add merged project config to the template_vars only if it's not empty
    if let Some(project_obj) = merged_project.as_object() {
        if !project_obj.is_empty() {
            merged_map.insert("project".to_string(), merged_project);
        }
    }

    // Always return a Value (empty object if nothing else)
    serde_json::Value::Object(merged_map)
}

/// Adds or updates a resource entry in the lockfile based on resource type.
///
/// This helper method eliminates code duplication between the `resolve()` and `update()`
/// methods by centralizing lockfile entry management logic. It automatically determines
/// the resource type from the entry name and adds or updates the entry in the appropriate
/// collection within the lockfile.
///
/// The method performs upsert behavior - if an entry with matching name and source
/// already exists in the appropriate collection, it will be updated (including version);
/// otherwise, a new entry is added. This allows version updates (e.g., v1.0 → v2.0)
/// to replace the existing entry rather than creating duplicates.
///
/// # Arguments
///
/// * `lockfile` - Mutable reference to the lockfile to modify
/// * `entry` - The [`LockedResource`] entry to add or update
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn add_or_update_lockfile_entry(lockfile: &mut LockFile, entry: LockedResource) {
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
/// * `manifest` - Reference to the current project manifest
/// * `lockfile` - The mutable lockfile to clean
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn remove_stale_manifest_entries(manifest: &Manifest, lockfile: &mut LockFile) {
    // Collect all current manifest keys for each resource type
    let manifest_agents: HashSet<String> = manifest.agents.keys().map(|k| k.to_string()).collect();
    let manifest_snippets: HashSet<String> =
        manifest.snippets.keys().map(|k| k.to_string()).collect();
    let manifest_commands: HashSet<String> =
        manifest.commands.keys().map(|k| k.to_string()).collect();
    let manifest_scripts: HashSet<String> =
        manifest.scripts.keys().map(|k| k.to_string()).collect();
    let manifest_hooks: HashSet<String> = manifest.hooks.keys().map(|k| k.to_string()).collect();
    let manifest_mcp_servers: HashSet<String> =
        manifest.mcp_servers.keys().map(|k| k.to_string()).collect();

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
                collect_transitive_children(lockfile, parent_entry, &mut entries_to_remove);
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
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn remove_manifest_entries_for_update(
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
                || entry.manifest_alias.as_ref().is_some_and(|alias| manifest_keys.contains(alias))
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
                collect_transitive_children(lockfile, parent_entry, &mut entries_to_remove);
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
#[allow(dead_code)] // Not yet used in service-based refactoring
pub(super) fn collect_transitive_children(
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
                    collect_transitive_children(lockfile, dep_entry, entries_to_remove);
                }
            }
        }
    }
}

/// Detects conflicts where multiple dependencies resolve to the same installation path.
///
/// This method validates that no two dependencies will overwrite each other during
/// installation. It builds a map of all resolved `installed_at` paths and checks for
/// collisions across all resource types.
///
/// # Arguments
///
/// * `lockfile` - The lockfile containing all resolved dependencies
///
/// # Returns
///
/// Returns `Ok(())` if no conflicts are detected, or an error describing the conflicts.
///
/// # Errors
///
/// Returns an error if:
/// - Two or more dependencies resolve to the same `installed_at` path
/// - The error message lists all conflicting dependency names and the shared path
pub(super) fn detect_target_conflicts(lockfile: &LockFile) -> Result<()> {
    // Map of (installed_at path, resolved_commit) -> list of dependency names
    // Two dependencies with the same path AND same commit are NOT a conflict
    let mut path_map: HashMap<(String, Option<String>), Vec<String>> = HashMap::new();

    // Collect all resources from lockfile
    // Note: Hooks and MCP servers are excluded because they're configuration-only
    // resources that are designed to share config files (.claude/settings.local.json
    // for hooks, .mcp.json for MCP servers), not individual files that would conflict.
    let all_resources: Vec<(&str, &LockedResource)> = lockfile
        .agents
        .iter()
        .map(|r| (r.name.as_str(), r))
        .chain(lockfile.snippets.iter().map(|r| (r.name.as_str(), r)))
        .chain(lockfile.commands.iter().map(|r| (r.name.as_str(), r)))
        .chain(lockfile.scripts.iter().map(|r| (r.name.as_str(), r)))
        // Hooks and MCP servers intentionally omitted - they share config files
        .collect();

    // Build the path map with commit information
    for (name, resource) in &all_resources {
        let key = (resource.installed_at.clone(), resource.resolved_commit.clone());
        path_map.entry(key).or_default().push((*name).to_string());
    }

    // Now check for actual conflicts: same path but DIFFERENT commits
    // Group by path only to find potential conflicts
    let mut path_only_map: HashMap<String, Vec<(&str, &LockedResource)>> = HashMap::new();
    for (name, resource) in &all_resources {
        path_only_map.entry(resource.installed_at.clone()).or_default().push((name, resource));
    }

    // Find conflicts (same path with different commits OR local deps with same path)
    let mut conflicts: Vec<(String, Vec<String>)> = Vec::new();
    for (path, resources) in path_only_map {
        if resources.len() > 1 {
            // Check if they have different commits
            let commits: HashSet<_> = resources.iter().map(|(_, r)| &r.resolved_commit).collect();

            // Conflict if:
            // 1. Different commits (different content from Git)
            // 2. All are local dependencies (resolved_commit = None) - can't overwrite same path
            let all_local = commits.len() == 1 && commits.contains(&None);

            if commits.len() > 1 || all_local {
                let names: Vec<String> = resources.iter().map(|(n, _)| (*n).to_string()).collect();
                conflicts.push((path, names));
            }
        }
    }

    if !conflicts.is_empty() {
        // Build a detailed error message
        let mut error_msg = String::from(
            "Target path conflicts detected:\n\n\
             Multiple dependencies resolve to the same installation path with different content.\n\
             This would cause files to overwrite each other.\n\n",
        );

        for (path, names) in &conflicts {
            error_msg.push_str(&format!("  Path: {}\n  Conflicts: {}\n\n", path, names.join(", ")));
        }

        error_msg.push_str(
            "To resolve this conflict:\n\
             1. Use custom 'target' field to specify different installation paths:\n\
                Example: target = \"custom/subdir/file.md\"\n\n\
             2. Use custom 'filename' field to specify different filenames:\n\
                Example: filename = \"utils-v2.md\"\n\n\
             3. For transitive dependencies, add them as direct dependencies with custom target/filename\n\n\
             4. Ensure pattern dependencies don't overlap with single-file dependencies\n\n\
             Note: This often occurs when different dependencies have transitive dependencies\n\
             with the same name but from different sources.",
        );

        return Err(anyhow::anyhow!(error_msg));
    }

    Ok(())
}

/// Add version information to dependency references in all lockfile entries.
///
/// This post-processing step updates the `dependencies` field of each locked resource
/// to include version information (e.g., converting "agent/helper" to "agent/helper@v1.0.0").
///
/// # Arguments
///
/// * `lockfile` - The mutable lockfile to update
pub(super) fn add_version_to_all_dependencies(lockfile: &mut LockFile) {
    use crate::resolver::types as dependency_helpers;

    // Build lookup map: (resource_type, normalized_path, source) -> name
    let mut lookup_map: HashMap<(ResourceType, String, Option<String>), String> = HashMap::new();

    // Build lookup from all lockfile entries
    for resource_type in ResourceType::all() {
        for entry in lockfile.get_resources(*resource_type) {
            let normalized_path = dependency_helpers::normalize_lookup_path(&entry.path);
            lookup_map.insert(
                (*resource_type, normalized_path.clone(), entry.source.clone()),
                entry.name.clone(),
            );

            // Also store by filename for backward compatibility
            if let Some(filename) = dependency_helpers::extract_filename_from_path(&entry.path) {
                lookup_map
                    .insert((*resource_type, filename, entry.source.clone()), entry.name.clone());
            }

            // Also store by type-stripped path
            if let Some(stripped) =
                dependency_helpers::strip_resource_type_directory(&normalized_path)
            {
                lookup_map
                    .insert((*resource_type, stripped, entry.source.clone()), entry.name.clone());
            }
        }
    }

    // Build resource info map: (resource_type, name, source) -> (source, version)
    let mut resource_info_map: HashMap<ResourceKey, ResourceInfo> = HashMap::new();

    for resource_type in ResourceType::all() {
        for entry in lockfile.get_resources(*resource_type) {
            resource_info_map.insert(
                (*resource_type, entry.name.clone(), entry.source.clone()),
                (entry.source.clone(), entry.version.clone()),
            );
        }
    }

    // Update dependencies in all resources
    for resource_type in ResourceType::all() {
        let resources = lockfile.get_resources_mut(*resource_type);
        for entry in resources {
            let parent_source = entry.source.clone();

            let updated_deps: Vec<String> = entry
                .dependencies
                .iter()
                .map(|dep| {
                    // Parse "resource_type/path" format
                    if let Some((_resource_type_str, dep_path)) = dep.split_once('/') {
                        if let Ok(dep_resource_type) = _resource_type_str.parse::<ResourceType>() {
                            // Normalize path (strips extension for consistent lookup)
                            let dep_filename = dependency_helpers::normalize_lookup_path(dep_path);

                            // Look up resource in same source
                            if let Some(dep_name) = lookup_map.get(&(
                                dep_resource_type,
                                dep_filename,
                                parent_source.clone(),
                            )) {
                                if let Some((_source, Some(ver))) = resource_info_map.get(&(
                                    dep_resource_type,
                                    dep_name.clone(),
                                    parent_source.clone(),
                                )) {
                                    return format!("{dep_resource_type}/{dep_name}@{ver}");
                                }
                                return format!("{dep_resource_type}/{dep_name}");
                            }
                        }
                    }
                    dep.clone()
                })
                .collect();

            entry.dependencies = updated_deps;
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
