//! Dependency handling for template context building.
//!
//! This module provides functionality for extracting dependency information,
//! custom names, and building the dependency data structure for template rendering.

use anyhow::{Context as _, Result};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use crate::core::ResourceType;
use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
use crate::lockfile::{LockFile, LockedResource, ResourceId};

use super::cache::{RenderCache, RenderCacheKey};
use super::content::{
    ContentExtractor, NON_TEMPLATED_LITERAL_GUARD_START, content_contains_template_syntax,
};
use super::context::DependencyData;
use super::renderer::TemplateRenderer;
use super::utils::to_native_path_display;

/// Helper function to create a LockfileDependencyRef string from a resource.
///
/// This centralizes the logic for creating dependency references based on whether
/// the resource has a source (Git) or is local.
fn create_dependency_ref_string(
    source: Option<String>,
    resource_type: ResourceType,
    name: String,
    version: Option<String>,
) -> String {
    if let Some(source) = source {
        LockfileDependencyRef::git(source, resource_type, name, version).to_string()
    } else {
        LockfileDependencyRef::local(resource_type, name, version).to_string()
    }
}

/// Trait for dependency extraction methods on TemplateContextBuilder.
pub(crate) trait DependencyExtractor: ContentExtractor {
    /// Get the lockfile
    fn lockfile(&self) -> &Arc<LockFile>;

    /// Get the render cache
    fn render_cache(&self) -> &Arc<std::sync::Mutex<RenderCache>>;

    /// Get the custom names cache
    fn custom_names_cache(
        &self,
    ) -> &Arc<std::sync::Mutex<HashMap<String, BTreeMap<String, String>>>>;

    /// Get the dependency specs cache
    fn dependency_specs_cache(
        &self,
    ) -> &Arc<std::sync::Mutex<HashMap<String, BTreeMap<String, crate::manifest::DependencySpec>>>>;

    /// Extract custom dependency names from a resource's frontmatter.
    ///
    /// Parses the resource file to extract the `dependencies` declaration with `name:` fields
    /// and maps dependency references to their custom names.
    ///
    /// # Returns
    ///
    /// A BTreeMap mapping dependency references (e.g., "snippet/rust-best-practices") to custom
    /// names (e.g., "best_practices") as declared in the resource's YAML frontmatter.
    /// BTreeMap ensures deterministic iteration order for consistent context checksums.
    async fn extract_dependency_custom_names(
        &self,
        resource: &LockedResource,
    ) -> BTreeMap<String, String> {
        // Build cache key from resource name and type
        let cache_key = format!("{}@{:?}", resource.name, resource.resource_type);

        // Check cache first
        if let Ok(cache) = self.custom_names_cache().lock() {
            if let Some(cached_names) = cache.get(&cache_key) {
                tracing::debug!(
                    "Custom names cache HIT for '{}' ({} names)",
                    resource.name,
                    cached_names.len()
                );
                return cached_names.clone();
            }
        }

        tracing::debug!("Custom names cache MISS for '{}'", resource.name);

        let mut custom_names = BTreeMap::new();

        // Build a lookup structure upfront to avoid O(n³) nested loops
        // Map: type -> Vec<(basename, full_dep_ref)>
        // Use BTreeMap for deterministic iteration order
        let mut lockfile_lookup: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

        // Use parsed_dependencies() helper to parse all dependencies
        for dep_ref in resource.parsed_dependencies() {
            let lockfile_type = dep_ref.resource_type.to_string();
            let lockfile_name = &dep_ref.path;
            let lockfile_dep_ref = dep_ref.to_string();

            // Extract basename from lockfile name
            let lockfile_basename = std::path::Path::new(lockfile_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(lockfile_name)
                .to_string();

            lockfile_lookup
                .entry(lockfile_type)
                .or_default()
                .push((lockfile_basename, lockfile_dep_ref));
        }

        // Determine source path (same logic as extract_content)
        let source_path = if let Some(_source_name) = &resource.source {
            // Has source - check if local or Git
            let url = match resource.url.as_ref() {
                Some(u) => u,
                None => return custom_names,
            };

            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            if is_local_source {
                // Local source
                std::path::PathBuf::from(url).join(&resource.path)
            } else {
                // Git source
                let sha = match resource.resolved_commit.as_deref() {
                    Some(s) => s,
                    None => return custom_names,
                };
                match self.cache().get_worktree_path(url, sha) {
                    Ok(worktree_dir) => worktree_dir.join(&resource.path),
                    Err(_) => return custom_names,
                }
            }
        } else {
            // Local file
            let local_path = std::path::Path::new(&resource.path);
            if local_path.is_absolute() {
                local_path.to_path_buf()
            } else {
                self.project_dir().join(local_path)
            }
        };

        // Read and parse the file based on type
        if resource.path.ends_with(".md") {
            // Parse markdown frontmatter with template rendering
            if let Ok(content) = tokio::fs::read_to_string(&source_path).await {
                // Use templated parsing to handle conditional blocks ({% if %}) in frontmatter
                if let Ok(doc) = crate::markdown::MarkdownDocument::parse_with_templating(
                    &content,
                    Some(resource.variant_inputs.json()),
                    Some(&source_path),
                ) {
                    // Extract dependencies from parsed metadata
                    if let Some(markdown_metadata) = &doc.metadata {
                        // Convert MarkdownMetadata to DependencyMetadata
                        // Merge both root-level dependencies and agpm.dependencies
                        let dependency_metadata = crate::manifest::DependencyMetadata::new(
                            markdown_metadata.dependencies.clone(),
                            markdown_metadata.get_agpm_metadata(),
                        );

                        if let Some(deps_map) = dependency_metadata.get_dependencies() {
                            // Process each resource type (agents, snippets, commands, etc.)
                            for (resource_type_str, deps_array) in deps_map {
                                // Convert frontmatter type to lockfile type (singular)
                                let lockfile_type: String = match resource_type_str.as_str() {
                                    "agents" | "agent" => "agent".to_string(),
                                    "snippets" | "snippet" => "snippet".to_string(),
                                    "commands" | "command" => "command".to_string(),
                                    "scripts" | "script" => "script".to_string(),
                                    "hooks" | "hook" => "hook".to_string(),
                                    "mcp-servers" | "mcp-server" => "mcp-server".to_string(),
                                    _ => continue, // Skip unknown types
                                };

                                // Get lockfile entries for this type only (O(1) lookup instead of O(n) iteration)
                                let type_entries = match lockfile_lookup.get(&lockfile_type) {
                                    Some(entries) => entries,
                                    None => continue, // No lockfile deps of this type
                                };

                                // deps_array is Vec<DependencySpec>
                                for dep_spec in deps_array {
                                    let path = &dep_spec.path;
                                    if let Some(custom_name) = &dep_spec.name {
                                        // Extract basename from the path (without extension)
                                        let basename = std::path::Path::new(path)
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or(path);

                                        tracing::info!(
                                            "Found custom name '{}' for path '{}' (basename: '{}')",
                                            custom_name,
                                            path,
                                            basename
                                        );

                                        // Check if basename has template variables
                                        if basename.contains("{{") {
                                            // Template variable in basename - try suffix matching
                                            // e.g., "{{ agpm.project.language }}-best-practices" -> "-best-practices"
                                            if let Some(static_suffix_start) = basename.find("}}") {
                                                let static_suffix =
                                                    &basename[static_suffix_start + 2..];

                                                // Search for any lockfile basename ending with this suffix
                                                for (lockfile_basename, lockfile_dep_ref) in
                                                    type_entries
                                                {
                                                    if lockfile_basename.ends_with(static_suffix) {
                                                        custom_names.insert(
                                                            lockfile_dep_ref.clone(),
                                                            custom_name.to_string(),
                                                        );
                                                    }
                                                }
                                            }
                                        } else {
                                            // No template variables - exact basename match (O(n) but only within type)
                                            for (lockfile_basename, lockfile_dep_ref) in
                                                type_entries
                                            {
                                                if lockfile_basename == basename {
                                                    custom_names.insert(
                                                        lockfile_dep_ref.clone(),
                                                        custom_name.to_string(),
                                                    );
                                                    break; // Found exact match, no need to continue
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if resource.path.ends_with(".json") {
            // Parse JSON dependencies field with template rendering
            if let Ok(content) = tokio::fs::read_to_string(&source_path).await {
                // Apply templating to JSON content to handle conditional blocks
                let mut parser = crate::markdown::frontmatter::FrontmatterParser::new();
                let templated_content = parser
                    .apply_templating(&content, Some(resource.variant_inputs.json()), &source_path)
                    .unwrap_or_else(|_| content.clone());

                // Parse JSON and extract dependencies field
                if let Ok(json_value) =
                    serde_json::from_str::<serde_json::Value>(&templated_content)
                {
                    // Extract both root-level dependencies and agpm.dependencies
                    let root_deps = json_value.get("dependencies").and_then(|v| {
                        serde_json::from_value::<
                            BTreeMap<String, Vec<crate::manifest::DependencySpec>>,
                        >(v.clone())
                        .ok()
                    });

                    let agpm_metadata = json_value.get("agpm").and_then(|v| {
                        serde_json::from_value::<crate::manifest::dependency_spec::AgpmMetadata>(
                            v.clone(),
                        )
                        .ok()
                    });

                    // Merge both dependency sources
                    let dependency_metadata =
                        crate::manifest::DependencyMetadata::new(root_deps, agpm_metadata);

                    if let Some(deps_map) = dependency_metadata.get_dependencies() {
                        // Process each resource type (agents, snippets, commands, etc.)
                        for (resource_type_str, deps_array) in deps_map {
                            // Convert frontmatter type to lockfile type (singular)
                            let lockfile_type: String = match resource_type_str.as_str() {
                                "agents" | "agent" => "agent".to_string(),
                                "snippets" | "snippet" => "snippet".to_string(),
                                "commands" | "command" => "command".to_string(),
                                "scripts" | "script" => "script".to_string(),
                                "hooks" | "hook" => "hook".to_string(),
                                "mcp-servers" | "mcp-server" => "mcp-server".to_string(),
                                _ => continue, // Skip unknown types
                            };

                            // Get lockfile entries for this type only (O(1) lookup instead of O(n) iteration)
                            let type_entries = match lockfile_lookup.get(&lockfile_type) {
                                Some(entries) => entries,
                                None => continue, // No lockfile deps of this type
                            };

                            // deps_array is Vec<DependencySpec>
                            for dep_spec in deps_array {
                                let path = &dep_spec.path;
                                if let Some(custom_name) = &dep_spec.name {
                                    // Extract basename from the path (without extension)
                                    let basename = std::path::Path::new(path)
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or(path);

                                    tracing::info!(
                                        "Found custom name '{}' for path '{}' (basename: '{}') from JSON",
                                        custom_name,
                                        path,
                                        basename
                                    );

                                    // Check if basename has template variables
                                    if basename.contains("{{") {
                                        // Template variable in basename - try suffix matching
                                        // e.g., "{{ agpm.project.language }}-best-practices" -> "-best-practices"
                                        if let Some(static_suffix_start) = basename.find("}}") {
                                            let static_suffix =
                                                &basename[static_suffix_start + 2..];

                                            // Search for any lockfile basename ending with this suffix
                                            for (lockfile_basename, lockfile_dep_ref) in
                                                type_entries
                                            {
                                                if lockfile_basename.ends_with(static_suffix) {
                                                    custom_names.insert(
                                                        lockfile_dep_ref.clone(),
                                                        custom_name.to_string(),
                                                    );
                                                }
                                            }
                                        }
                                    } else {
                                        // No template variables - exact basename match (O(n) but only within type)
                                        for (lockfile_basename, lockfile_dep_ref) in type_entries {
                                            if lockfile_basename == basename {
                                                custom_names.insert(
                                                    lockfile_dep_ref.clone(),
                                                    custom_name.to_string(),
                                                );
                                                break; // Found exact match, no need to continue
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Store in cache before returning
        if let Ok(mut cache) = self.custom_names_cache().lock() {
            cache.insert(cache_key, custom_names.clone());
            tracing::debug!(
                "Stored {} custom names in cache for '{}'",
                custom_names.len(),
                resource.name
            );
        }

        custom_names
    }

    /// Extract full dependency specifications from a resource's frontmatter.
    ///
    /// Parses the resource file to extract complete DependencySpec objects including
    /// tool, name, flatten, and install fields. This information is used to build
    /// complete ResourceIds for dependency lookups.
    ///
    /// # Returns
    ///
    /// A BTreeMap mapping dependency references (e.g., "snippet:snippets/commands/commit")
    /// to their full DependencySpec objects. BTreeMap ensures deterministic iteration.
    async fn extract_dependency_specs(
        &self,
        resource: &LockedResource,
    ) -> BTreeMap<String, crate::manifest::DependencySpec> {
        // Build cache key from resource name and type
        let cache_key = format!("{}@{:?}", resource.name, resource.resource_type);

        // Check cache first
        if let Ok(cache) = self.dependency_specs_cache().lock() {
            if let Some(cached_specs) = cache.get(&cache_key) {
                tracing::debug!(
                    "Dependency specs cache HIT for '{}' ({} specs)",
                    resource.name,
                    cached_specs.len()
                );
                return cached_specs.clone();
            }
        }

        tracing::debug!("Dependency specs cache MISS for '{}'", resource.name);

        let mut dependency_specs = BTreeMap::new();

        // Determine source path (same logic as extract_content)
        let source_path = if let Some(_source_name) = &resource.source {
            // Has source - check if local or Git
            let url = match resource.url.as_ref() {
                Some(u) => u,
                None => return dependency_specs,
            };

            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            if is_local_source {
                // Local source
                std::path::PathBuf::from(url).join(&resource.path)
            } else {
                // Git source
                let sha = match resource.resolved_commit.as_deref() {
                    Some(s) => s,
                    None => return dependency_specs,
                };
                match self.cache().get_worktree_path(url, sha) {
                    Ok(worktree_dir) => worktree_dir.join(&resource.path),
                    Err(_) => return dependency_specs,
                }
            }
        } else {
            // Local file
            let local_path = std::path::Path::new(&resource.path);
            if local_path.is_absolute() {
                local_path.to_path_buf()
            } else {
                self.project_dir().join(local_path)
            }
        };

        // Read and parse the file based on type
        if resource.path.ends_with(".md") {
            // Parse markdown frontmatter with template rendering
            if let Ok(content) = tokio::fs::read_to_string(&source_path).await {
                // Use templated parsing to handle conditional blocks ({% if %}) in frontmatter
                if let Ok(doc) = crate::markdown::MarkdownDocument::parse_with_templating(
                    &content,
                    Some(resource.variant_inputs.json()),
                    Some(&source_path),
                ) {
                    // Extract dependencies from parsed metadata
                    if let Some(markdown_metadata) = &doc.metadata {
                        // Convert MarkdownMetadata to DependencyMetadata
                        let dependency_metadata = crate::manifest::DependencyMetadata::new(
                            markdown_metadata.dependencies.clone(),
                            markdown_metadata.get_agpm_metadata(),
                        );

                        if let Some(deps_map) = dependency_metadata.get_dependencies() {
                            // Process each resource type
                            for (resource_type_str, deps_array) in deps_map {
                                // Convert frontmatter type to ResourceType
                                let resource_type = match resource_type_str.as_str() {
                                    "agents" | "agent" => crate::core::ResourceType::Agent,
                                    "snippets" | "snippet" => crate::core::ResourceType::Snippet,
                                    "commands" | "command" => crate::core::ResourceType::Command,
                                    "scripts" | "script" => crate::core::ResourceType::Script,
                                    "hooks" | "hook" => crate::core::ResourceType::Hook,
                                    "mcp-servers" | "mcp-server" => {
                                        crate::core::ResourceType::McpServer
                                    }
                                    _ => continue,
                                };

                                // Store each DependencySpec with its lockfile reference as key
                                for dep_spec in deps_array {
                                    // Canonicalize the frontmatter path to match lockfile format
                                    // Frontmatter paths are relative to the resource file itself
                                    // We need to resolve them relative to source root (not filesystem paths!)
                                    let canonical_path = if dep_spec.path.starts_with("../")
                                        || dep_spec.path.starts_with("./")
                                    {
                                        // Relative path - resolve using source-relative paths, not filesystem paths
                                        // Get the parent directory of the resource within the source
                                        let resource_parent = std::path::Path::new(&resource.path)
                                            .parent()
                                            .unwrap_or_else(|| std::path::Path::new(""));

                                        // Join with the relative dependency path (still may have ..)
                                        let joined = resource_parent.join(&dep_spec.path);

                                        // Normalize to remove .. and . components, then format for storage
                                        let normalized = crate::utils::normalize_path(&joined);
                                        crate::utils::normalize_path_for_storage(&normalized)
                                    } else {
                                        // Absolute or already canonical
                                        dep_spec.path.clone()
                                    };

                                    // Remove extension to match lockfile format
                                    let normalized_path = std::path::Path::new(&canonical_path)
                                        .with_extension("")
                                        .to_string_lossy()
                                        .to_string();

                                    // Build the dependency reference string
                                    let dep_ref = if let Some(ref src) = resource.source {
                                        LockfileDependencyRef::git(
                                            src.clone(),
                                            resource_type,
                                            normalized_path,
                                            resource.version.clone(),
                                        )
                                        .to_string()
                                    } else {
                                        LockfileDependencyRef::local(
                                            resource_type,
                                            normalized_path,
                                            resource.version.clone(),
                                        )
                                        .to_string()
                                    };

                                    dependency_specs.insert(dep_ref, dep_spec.clone());
                                }
                            }
                        }
                    }
                }
            }
        } else if resource.path.ends_with(".json") {
            // Parse JSON dependencies field with template rendering
            if let Ok(content) = tokio::fs::read_to_string(&source_path).await {
                // Apply templating to JSON content to handle conditional blocks
                let mut parser = crate::markdown::frontmatter::FrontmatterParser::new();
                let templated_content = parser
                    .apply_templating(&content, Some(resource.variant_inputs.json()), &source_path)
                    .unwrap_or_else(|_| content.clone());

                if let Ok(json_value) =
                    serde_json::from_str::<serde_json::Value>(&templated_content)
                {
                    // Extract both root-level dependencies and agpm.dependencies
                    let root_deps = json_value.get("dependencies").and_then(|v| {
                        serde_json::from_value::<
                            BTreeMap<String, Vec<crate::manifest::DependencySpec>>,
                        >(v.clone())
                        .ok()
                    });

                    let agpm_metadata = json_value.get("agpm").and_then(|v| {
                        serde_json::from_value::<crate::manifest::dependency_spec::AgpmMetadata>(
                            v.clone(),
                        )
                        .ok()
                    });

                    // Merge both dependency sources
                    let dependency_metadata =
                        crate::manifest::DependencyMetadata::new(root_deps, agpm_metadata);

                    if let Some(deps_map) = dependency_metadata.get_dependencies() {
                        // Process each resource type
                        for (resource_type_str, deps_array) in deps_map {
                            // Convert frontmatter type to ResourceType
                            let resource_type = match resource_type_str.as_str() {
                                "agents" | "agent" => crate::core::ResourceType::Agent,
                                "snippets" | "snippet" => crate::core::ResourceType::Snippet,
                                "commands" | "command" => crate::core::ResourceType::Command,
                                "scripts" | "script" => crate::core::ResourceType::Script,
                                "hooks" | "hook" => crate::core::ResourceType::Hook,
                                "mcp-servers" | "mcp-server" => {
                                    crate::core::ResourceType::McpServer
                                }
                                _ => continue,
                            };

                            // Store each DependencySpec with its lockfile reference as key
                            for dep_spec in deps_array {
                                // Canonicalize the frontmatter path to match lockfile format
                                // Frontmatter paths are relative to the resource file itself
                                // We need to resolve them relative to source root (not filesystem paths!)
                                let canonical_path = if dep_spec.path.starts_with("../")
                                    || dep_spec.path.starts_with("./")
                                {
                                    // Relative path - resolve using source-relative paths, not filesystem paths
                                    // Get the parent directory of the resource within the source
                                    let resource_parent = std::path::Path::new(&resource.path)
                                        .parent()
                                        .unwrap_or_else(|| std::path::Path::new(""));

                                    // Join with the relative dependency path (still may have ..)
                                    let joined = resource_parent.join(&dep_spec.path);

                                    // Normalize to remove .. and . components, then format for storage
                                    let normalized = crate::utils::normalize_path(&joined);
                                    crate::utils::normalize_path_for_storage(&normalized)
                                } else {
                                    // Absolute or already canonical
                                    dep_spec.path.clone()
                                };

                                // Remove extension to match lockfile format
                                let normalized_path = std::path::Path::new(&canonical_path)
                                    .with_extension("")
                                    .to_string_lossy()
                                    .to_string();

                                // Build the dependency reference string
                                let dep_ref = if let Some(ref src) = resource.source {
                                    LockfileDependencyRef::git(
                                        src.clone(),
                                        resource_type,
                                        normalized_path,
                                        resource.version.clone(),
                                    )
                                    .to_string()
                                } else {
                                    LockfileDependencyRef::local(
                                        resource_type,
                                        normalized_path,
                                        resource.version.clone(),
                                    )
                                    .to_string()
                                };

                                dependency_specs.insert(dep_ref, dep_spec.clone());
                            }
                        }
                    }
                }
            }
        }

        // Store in cache before returning
        if let Ok(mut cache) = self.dependency_specs_cache().lock() {
            cache.insert(cache_key, dependency_specs.clone());
            tracing::debug!(
                "Stored {} dependency specs in cache for '{}'",
                dependency_specs.len(),
                resource.name
            );
        }

        dependency_specs
    }

    /// Generate dependency name from a path (matching resolver logic).
    ///
    /// For local transitive dependencies, the resolver uses the full relative path
    /// (without extension) as the resource name to maintain uniqueness.
    #[allow(dead_code)]
    fn generate_dependency_name_from_path(&self, path: &str) -> String {
        // Strip file extension - this matches what the resolver stores as the name
        path.strip_suffix(".md").or_else(|| path.strip_suffix(".json")).unwrap_or(path).to_string()
    }

    /// Build dependency data for the template context.
    ///
    /// This creates a nested structure containing:
    /// 1. ALL resources from the lockfile (path-based names) - for universal access
    /// 2. Current resource's declared dependencies (custom alias names) - for scoped access
    ///
    /// This dual approach ensures:
    /// - Any resource can access any other resource via path-based names
    /// - Resources can use custom aliases for their dependencies without collisions
    ///
    /// # Arguments
    ///
    /// * `current_resource` - The resource being rendered (for scoped alias mapping)
    async fn build_dependencies_data(
        &self,
        current_resource: &LockedResource,
        rendering_stack: &mut HashSet<String>,
    ) -> Result<BTreeMap<String, BTreeMap<String, DependencyData>>> {
        let mut deps = BTreeMap::new();

        // Extract dependency specifications from current resource's frontmatter
        // This provides tool, name, flatten, and install fields for each dependency
        let dependency_specs = self.extract_dependency_specs(current_resource).await;

        // Helper function to determine the key name for a resource
        let get_key_names = |resource: &LockedResource,
                             dep_type: &ResourceType|
         -> (String, String, String, String) {
            let type_str_plural = dep_type.to_plural().to_string();
            let type_str_singular = dep_type.to_string();

            // Determine the key to use for universal access in the template context
            // DO NOT use manifest_alias - it's only for pattern aliases from manifest,
            // not transitive custom names which are extracted during template rendering
            let key_name = if resource.name.contains('/') || resource.name.contains('\\') {
                // Name looks like a path - extract basename without extension
                std::path::Path::new(&resource.name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&resource.name)
                    .to_string()
            } else {
                // Use name as-is
                resource.name.clone()
            };

            // Sanitize the key name by replacing hyphens with underscores
            // to avoid Tera interpreting them as minus operators
            let sanitized_key = key_name.replace('-', "_");

            (type_str_plural, type_str_singular, key_name, sanitized_key)
        };

        // Collect ONLY direct dependencies (not transitive!)
        // Each dependency will be rendered with its own context containing its own direct deps.
        let mut resources_to_process: Vec<(&LockedResource, ResourceType, bool)> = Vec::new();
        let mut visited_dep_ids = HashSet::new();

        for dep_ref in current_resource.parsed_dependencies() {
            // Build dep_id for deduplication tracking
            let dep_id = dep_ref.to_string();

            // Skip if we've already processed this dependency
            if !visited_dep_ids.insert(dep_id.clone()) {
                continue;
            }

            let resource_type = dep_ref.resource_type;
            let name = &dep_ref.path;

            // Get the dependency spec for this reference (if declared in frontmatter)
            // NOTE: dependency_specs keys are normalized (no ../ segments) because
            // extract_dependency_specs normalizes paths using Path component iteration.
            // We must normalize the lookup key to match.
            let dep_spec = {
                // Normalize the path to match what extract_dependency_specs stored
                let normalized_path = {
                    let path = std::path::Path::new(&dep_ref.path);
                    let normalized = crate::utils::normalize_path(path);
                    normalized.to_string_lossy().to_string()
                };

                // Create a normalized dep_ref for cache lookup only
                let normalized_dep_ref = LockfileDependencyRef::new(
                    dep_ref.source.clone(),
                    dep_ref.resource_type,
                    normalized_path,
                    dep_ref.version.clone(),
                );
                let normalized_dep_id = normalized_dep_ref.to_string();

                dependency_specs.get(&normalized_dep_id)
            };

            tracing::debug!(
                "Looking up dep_spec for dep_id='{}', found={}, available_keys={:?}",
                dep_id,
                dep_spec.is_some(),
                dependency_specs.keys().collect::<Vec<_>>()
            );

            // Determine the tool for this dependency
            // Priority: explicit tool in DependencySpec > inherited from parent
            let dep_tool =
                dep_spec.and_then(|spec| spec.tool.as_ref()).or(current_resource.tool.as_ref());

            // Determine the source for this dependency
            // Use dep_ref.source if present, otherwise inherit from parent
            let dep_source = dep_ref.source.as_ref().or(current_resource.source.as_ref());

            // Build complete ResourceId for precise lookup
            // Try parent's variant_inputs_hash first (for transitive deps that inherit context)
            let dep_resource_id_with_parent_hash = ResourceId::new(
                name.clone(),
                dep_source.cloned(),
                dep_tool.cloned(),
                resource_type,
                current_resource.variant_inputs.hash().to_string(),
            );

            tracing::debug!(
                "[DEBUG] Template context looking up: name='{}', type={:?}, source={:?}, tool={:?}, hash={}",
                name,
                resource_type,
                dep_source,
                dep_tool,
                &current_resource.variant_inputs.hash().to_string()[..8]
            );

            // Look up the dependency in the lockfile by full ResourceId
            // Try with parent's hash first, then fall back to empty hash for direct manifest deps
            let mut dep_resource =
                self.lockfile().find_resource_by_id(&dep_resource_id_with_parent_hash);

            // If not found with parent's hash, try with empty hash (direct manifest dependencies)
            if dep_resource.is_none() {
                let dep_resource_id_empty_hash = ResourceId::new(
                    name.clone(),
                    dep_source.cloned(),
                    dep_tool.cloned(),
                    resource_type,
                    crate::resolver::lockfile_builder::VariantInputs::default().hash().to_string(),
                );
                dep_resource = self.lockfile().find_resource_by_id(&dep_resource_id_empty_hash);

                if dep_resource.is_some() {
                    tracing::debug!(
                        "  [DIRECT MANIFEST DEP] Found dependency '{}' with empty variant_hash (direct manifest dependency)",
                        name
                    );
                }
            }

            if let Some(dep_resource) = dep_resource {
                // Add this dependency to resources to process (true = declared dependency)
                resources_to_process.push((dep_resource, resource_type, true));

                tracing::debug!(
                    "  [DIRECT DEP] Found dependency '{}' (tool: {:?}) for '{}'",
                    name,
                    dep_tool,
                    current_resource.name
                );
            } else {
                tracing::warn!(
                    "Dependency '{}' (type: {:?}, tool: {:?}) not found in lockfile for resource '{}'",
                    name,
                    resource_type,
                    dep_tool,
                    current_resource.name
                );
            }
        }

        tracing::debug!(
            "Building dependencies data with {} direct dependencies for '{}'",
            resources_to_process.len(),
            current_resource.name
        );

        // CRITICAL: Sort resources_to_process for deterministic ordering!
        // This ensures that even if resources were added in different orders,
        // we process them in a consistent order, leading to deterministic context building.
        // Sort by: (resource_type, name, is_dependency) for full determinism
        resources_to_process.sort_by(|a, b| {
            use std::cmp::Ordering;
            // First by resource type
            match a.1.cmp(&b.1) {
                Ordering::Equal => {
                    // Then by resource name
                    match a.0.name.cmp(&b.0.name) {
                        Ordering::Equal => {
                            // Finally by is_dependency (dependencies first)
                            b.2.cmp(&a.2) // Reverse to put true before false
                        }
                        other => other,
                    }
                }
                other => other,
            }
        });

        // Debug: log all resources being processed
        for (resource, dep_type, is_dep) in &resources_to_process {
            tracing::debug!(
                "  [LOCKFILE] Resource: {} (type: {:?}, install: {:?}, is_dependency: {})",
                resource.name,
                dep_type,
                resource.install,
                is_dep
            );
        }

        // Get current resource ID for filtering
        let current_resource_id = create_dependency_ref_string(
            current_resource.source.clone(),
            current_resource.resource_type,
            current_resource.name.clone(),
            current_resource.version.clone(),
        );

        // Process each resource (excluding the current resource to prevent self-reference)
        for (resource, dep_type, is_dependency) in &resources_to_process {
            let resource_id = create_dependency_ref_string(
                resource.source.clone(),
                *dep_type,
                resource.name.clone(),
                resource.version.clone(),
            );

            // Skip if this is the current resource (prevent self-dependency)
            if resource_id == current_resource_id {
                tracing::debug!(
                    "  Skipping current resource: {} (preventing self-reference)",
                    resource.name
                );
                continue;
            }

            tracing::debug!("  Processing resource: {} ({})", resource.name, dep_type);

            let (type_str_plural, type_str_singular, _key_name, sanitized_key) =
                get_key_names(resource, dep_type);

            // Extract content from source file FIRST (before creating the struct)
            // Declared dependencies should be rendered with their own context before being made available
            // Non-dependencies just get raw content extraction (to avoid circular dependency issues)
            let raw_content = self.extract_content(resource).await;

            // Check if the dependency should be rendered
            // Only render if this is a declared dependency AND content has template syntax
            let should_render = if *is_dependency {
                if let Some(content) = &raw_content {
                    // Don't render if content has literal guards (from templating: false)
                    if content.contains(NON_TEMPLATED_LITERAL_GUARD_START) {
                        false
                    } else {
                        // Only render if the content has template syntax
                        content_contains_template_syntax(content)
                    }
                } else {
                    false
                }
            } else {
                // Not a declared dependency - don't render to avoid circular deps
                false
            };

            // Compute the final content (either rendered, cached, or raw)
            let final_content: String = if should_render {
                // Build cache key to check if we've already rendered this exact resource
                // CRITICAL: Include tool and resolved_commit in cache key to prevent cache pollution!
                // Same path renders differently for different tools (claude-code vs opencode)
                // and different commits must have different cache entries.
                let cache_key = RenderCacheKey::new(
                    resource.path.clone(),
                    *dep_type,
                    resource.tool.clone(),
                    resource.variant_inputs.hash().to_string(),
                    resource.resolved_commit.clone(),
                );

                // Check cache first (ensure guard is dropped before any awaits)
                let cache_result = self
                    .render_cache()
                    .lock()
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Render cache lock poisoned for resource '{}': {}. \
                         This indicates a panic occurred while holding the lock.",
                            resource.name,
                            e
                        )
                    })?
                    .get(&cache_key)
                    .cloned(); // MutexGuard dropped here

                if let Some(cached_content) = cache_result {
                    tracing::debug!("Render cache hit for '{}' ({})", resource.name, dep_type);
                    cached_content
                } else {
                    // Cache miss - need to render
                    tracing::debug!(
                        "Render cache miss for '{}' ({}), rendering...",
                        resource.name,
                        dep_type
                    );

                    // Check if we're already rendering this dependency (cycle detection)
                    let dep_id = create_dependency_ref_string(
                        resource.source.clone(),
                        *dep_type,
                        resource.name.clone(),
                        resource.version.clone(),
                    );
                    if rendering_stack.contains(&dep_id) {
                        let chain: Vec<String> = rendering_stack.iter().cloned().collect();
                        anyhow::bail!(
                            "Circular dependency detected while rendering '{}'. \
                                Dependency chain: {} -> {}",
                            resource.name,
                            chain.join(" -> "),
                            dep_id
                        );
                    }

                    // Add to rendering stack
                    rendering_stack.insert(dep_id.clone());

                    // Build a template context for this dependency so it can be rendered with its own dependencies
                    let dep_resource_id = ResourceId::from_resource(resource);
                    let render_result = Box::pin(self.build_context_with_visited(
                        &dep_resource_id,
                        resource.variant_inputs.json(),
                        rendering_stack,
                    ))
                    .await;

                    // Remove from stack after rendering (whether success or failure)
                    rendering_stack.remove(&dep_id);

                    match render_result {
                        Ok(dep_context) => {
                            // Render the dependency's content
                            if let Some(content) = raw_content {
                                let mut renderer = TemplateRenderer::new(
                                        true,
                                        self.project_dir().clone(),
                                        None,
                                    )
                                    .with_context(|| {
                                        format!(
                                            "Failed to create template renderer for dependency '{}' (type: {:?})",
                                            resource.name,
                                            dep_type
                                        )
                                    })?;

                                let rendered = renderer
                                        .render_template(&content, &dep_context)
                                        .with_context(|| {
                                            format!(
                                                "Failed to render dependency '{}' (type: {:?}). \
                                                This is a HARD FAILURE - dependency content MUST render successfully.\n\
                                                Resource: {} (source: {}, path: {})",
                                                resource.name,
                                                dep_type,
                                                resource.name,
                                                resource.source.as_deref().unwrap_or("local"),
                                                resource.path
                                            )
                                        })?;

                                tracing::debug!(
                                    "Successfully rendered dependency content for '{}'",
                                    resource.name
                                );

                                // Store in cache for future use
                                if let Ok(mut cache) = self.render_cache().lock() {
                                    cache.insert(cache_key.clone(), rendered.clone());
                                    tracing::debug!(
                                        "Stored rendered content in cache for '{}'",
                                        resource.name
                                    );
                                }

                                rendered
                            } else {
                                // No content extracted - use empty string
                                String::new()
                            }
                        }
                        Err(e) => {
                            // Hard failure - context building must succeed for dependency rendering
                            return Err(e.context(format!(
                                    "Failed to build template context for dependency '{}' (type: {:?}). \
                                    This is a HARD FAILURE - all dependencies must have valid contexts.\n\
                                    Resource: {} (source: {}, path: {})",
                                    resource.name,
                                    dep_type,
                                    resource.name,
                                    resource.source.as_deref().unwrap_or("local"),
                                    resource.path
                                )));
                        }
                    }
                }
            } else {
                // No rendering needed, use raw content (guards will be collapsed after parent renders)
                raw_content.unwrap_or_default()
            };

            // Create DependencyData with all fields including content
            let dependency_data = DependencyData {
                resource_type: type_str_singular,
                name: resource.name.clone(),
                install_path: to_native_path_display(&resource.installed_at),
                source: resource.source.clone(),
                version: resource.version.clone(),
                resolved_commit: resource.resolved_commit.clone(),
                checksum: resource.checksum.clone(),
                path: resource.path.clone(),
                content: final_content,
            };

            // Insert into the nested structure
            let type_deps: &mut BTreeMap<String, DependencyData> =
                deps.entry(type_str_plural.clone()).or_insert_with(BTreeMap::new);
            type_deps.insert(sanitized_key.clone(), dependency_data);

            tracing::debug!(
                "  Added resource: {}[{}] -> {}",
                type_str_plural,
                sanitized_key,
                resource.path
            );
        }

        // Add custom alias mappings for the current resource's direct dependencies only.
        // Each dependency will be rendered with its own context containing its own custom names.
        tracing::debug!(
            "Extracting custom dependency names for direct deps of: '{}'",
            current_resource.name
        );

        // Process only the current resource's custom names (for its direct dependencies)
        let current_custom_names = self.extract_dependency_custom_names(current_resource).await;
        tracing::debug!(
            "Extracted {} custom names from current resource '{}' (type: {:?})",
            current_custom_names.len(),
            current_resource.name,
            current_resource.resource_type
        );
        if !current_custom_names.is_empty() || current_resource.name.contains("golang") {
            tracing::info!(
                "Extracted {} custom names from current resource '{}' (type: {:?})",
                current_custom_names.len(),
                current_resource.name,
                current_resource.resource_type
            );
            for (dep_ref, custom_name) in &current_custom_names {
                tracing::info!("  Will add alias: '{}' -> '{}'", dep_ref, custom_name);
            }
        }
        for (dep_ref, custom_name) in current_custom_names {
            add_custom_alias(&mut deps, &dep_ref, &custom_name);
        }

        // Debug: Print what we built
        tracing::debug!(
            "Built dependencies data with {} resource types for '{}'",
            deps.len(),
            current_resource.name
        );
        for (resource_type, resources) in &deps {
            tracing::debug!("  Type {}: {} resources", resource_type, resources.len());
            if resource_type == "snippets" {
                for (key, data) in resources {
                    tracing::debug!(
                        "    - key='{}', name='{}', path='{}'",
                        key,
                        data.name,
                        data.path
                    );
                }
            } else {
                for name in resources.keys() {
                    tracing::debug!("    - {}", name);
                }
            }
        }

        Ok(deps)
    }

    /// Build context with visited tracking (for recursive rendering).
    ///
    /// This method should be implemented by the context builder to support
    /// recursive template rendering with cycle detection.
    async fn build_context_with_visited(
        &self,
        resource_id: &ResourceId,
        variant_inputs: &serde_json::Value,
        rendering_stack: &mut HashSet<String>,
    ) -> Result<tera::Context>;
}

/// Helper function to add a custom name alias to the dependencies map.
///
/// This function searches for an already-processed resource in the `deps` map and creates
/// an alias entry with the custom name. The resource should have already been added to
/// `deps` with its path-based key during the main processing loop.
///
/// Note: This function doesn't need to do lockfile lookups with ResourceId because it
/// searches within the already-built `deps` map. The deps map was built from the lockfile
/// with all the correct template_vars and content.
pub(crate) fn add_custom_alias(
    deps: &mut BTreeMap<String, BTreeMap<String, DependencyData>>,
    dep_ref: &str,
    custom_name: &str,
) {
    // Parse dependency reference using centralized LockfileDependencyRef logic
    let dep_ref_parsed = match LockfileDependencyRef::from_str(dep_ref) {
        Ok(dep_ref) => dep_ref,
        Err(e) => {
            tracing::debug!(
                "Skipping invalid dep_ref format '{}' for custom name '{}': {}",
                dep_ref,
                custom_name,
                e
            );
            return;
        }
    };

    let dep_type = dep_ref_parsed.resource_type;
    let dep_name = &dep_ref_parsed.path;

    let type_str_plural = dep_type.to_plural().to_string();

    // Search for the resource in the deps map (already populated from lockfile)
    if let Some(type_deps) = deps.get_mut(&type_str_plural) {
        // Build name → key index for O(1) lookup instead of O(N²) linear search
        let name_to_key: HashMap<String, String> = type_deps
            .iter()
            .flat_map(|(key, data)| {
                // Map both the full name and various fallback names to the key
                let mut mappings = vec![(data.name.clone(), key.clone())];

                // Add basename fallbacks for direct manifest deps
                if let Some(basename) = Path::new(&data.name).file_name().and_then(|n| n.to_str()) {
                    mappings.push((basename.to_string(), key.clone()));
                }
                if let Some(stem) = Path::new(&data.path).file_stem().and_then(|n| n.to_str()) {
                    mappings.push((stem.to_string(), key.clone()));
                }
                if let Some(path_basename) =
                    Path::new(&data.path).file_name().and_then(|n| n.to_str())
                {
                    mappings.push((path_basename.to_string(), key.clone()));
                }

                mappings
            })
            .collect();

        // Find the resource by name using O(1) lookup
        let existing_data =
            name_to_key.get(dep_name).and_then(|key| type_deps.get(key).cloned()).or_else(|| {
                // Some direct manifest dependencies use the bare manifest key (no type prefix)
                // even though transitive refs include the source-relative path (snippets/foo/bar).
                // Fall back to matching by the last path segment to align the two representations.
                Path::new(dep_name)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|basename| name_to_key.get(basename))
                    .and_then(|key| type_deps.get(key).cloned())
            });

        if let Some(data) = existing_data {
            // Sanitize the alias (replace hyphens with underscores for Tera)
            let sanitized_alias = custom_name.replace('-', "_");

            tracing::info!(
                "✓ Added {} alias '{}' -> resource '{}' (path: {})",
                type_str_plural,
                sanitized_alias,
                dep_name,
                data.path
            );

            // Add an alias entry pointing to the same data
            type_deps.insert(sanitized_alias.clone(), data);
        } else {
            tracing::error!(
                "❌ NOT FOUND: {} resource '{}' for alias '{}'.\n  \
                Dep ref: '{}'\n  \
                Available {} (first 5): {}",
                type_str_plural,
                dep_name,
                custom_name,
                dep_ref,
                type_deps.len(),
                type_deps
                    .iter()
                    .take(5)
                    .map(|(k, v)| format!("'{}' (name='{}')", k, v.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    } else {
        tracing::debug!(
            "Resource type '{}' not found in deps map when adding custom alias '{}' for '{}'",
            type_str_plural,
            custom_name,
            dep_ref
        );
    }
}
