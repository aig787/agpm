//! Dependency extraction functionality for templates.
//!
//! This module provides methods for extracting custom dependency names and
//! dependency specifications from resource files.

use crate::core::file_error::{FileOperation, FileResultExt};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::core::ResourceType;
use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
use crate::lockfile::{LockFile, LockedResource, ResourceId};

use crate::templating::cache::RenderCache;
use crate::templating::content::ContentExtractor;

/// Helper function to create a LockfileDependencyRef string from a resource.
///
/// This centralizes logic for creating dependency references based on whether
/// resource has a source (Git) or is local.
pub(crate) fn create_dependency_ref_string(
    source: Option<&str>,
    resource_type: ResourceType,
    name: &str,
    version: Option<&str>,
) -> String {
    if let Some(source) = source {
        LockfileDependencyRef::git(
            source.to_string(),
            resource_type,
            name.to_string(),
            version.map(|v| v.to_string()),
        )
        .to_string()
    } else {
        LockfileDependencyRef::local(
            resource_type,
            name.to_string(),
            version.map(|v| v.to_string()),
        )
        .to_string()
    }
}

/// Canonicalize a dependency path relative to a resource path.
///
/// If the dependency path is relative (starts with `../` or `./`), resolves it
/// relative to the resource's parent directory and normalizes for storage.
/// Otherwise, returns the path as-is.
///
/// # Arguments
///
/// * `dep_path` - The dependency path from frontmatter
/// * `resource_path` - The path of the resource declaring the dependency
///
/// # Returns
///
/// Canonical path suitable for lockfile lookups
///
/// # Examples
///
/// ```
/// // This example demonstrates the canonicalize_dep_path function behavior
/// // The function is internal to the crate, but its behavior is tested below
///
/// // Relative path resolution: "../utils/helper.md" from "agents/primary.md"
/// // would result in "utils/helper.md"
///
/// // Absolute path passes through: "agents/helper.md" from "agents/primary.md"
/// // would result in "agents/helper.md"
/// ```
pub(crate) fn canonicalize_dep_path(dep_path: &str, resource_path: &str) -> String {
    if dep_path.starts_with("../") || dep_path.starts_with("./") {
        // Relative path - resolve using source-relative paths, not filesystem paths
        // Get the parent directory of the resource within the source
        let resource_parent = std::path::Path::new(resource_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));

        // Join with the relative dependency path (still may have ..)
        let joined = resource_parent.join(dep_path);

        // Normalize to remove .. and . components, then format for storage
        let normalized = crate::utils::normalize_path(&joined);
        crate::utils::normalize_path_for_storage(&normalized)
    } else {
        // Absolute or already canonical
        dep_path.to_string()
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
    ///
    /// # Errors
    ///
    /// Returns an error if the dependency file cannot be read or parsed.
    async fn extract_dependency_custom_names(
        &self,
        resource: &LockedResource,
    ) -> Result<BTreeMap<String, String>> {
        tracing::info!(
            "[EXTRACT_CUSTOM_NAMES] Called for resource '{}' (type: {:?}), variant_inputs: {:?}",
            resource.name,
            resource.resource_type,
            resource.variant_inputs.json()
        );

        // Build cache key from resource name and type
        let cache_key = format!("{}@{:?}", resource.name, resource.resource_type);

        // Check cache first
        if let Ok(cache) = self.custom_names_cache().lock() {
            if let Some(cached_names) = cache.get(&cache_key) {
                tracing::info!(
                    "Custom names cache HIT for '{}' ({} names)",
                    resource.name,
                    cached_names.len()
                );
                return Ok(cached_names.clone());
            }
        }

        tracing::info!("Custom names cache MISS for '{}', extracting from file", resource.name);

        let mut custom_names = BTreeMap::new();

        // Build a lookup structure upfront to avoid O(n³) nested loops
        // Map: type -> Vec<(basename, full_dep_ref)>
        // Use BTreeMap for deterministic iteration order
        let mut lockfile_lookup: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

        // Use parsed_dependencies() helper to parse all dependencies from lockfile
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
                None => bail!("Resource '{}' has source but no URL", resource.name),
            };

            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            if is_local_source {
                // Local source
                std::path::PathBuf::from(url).join(&resource.path)
            } else {
                // Git source
                let sha = match resource.resolved_commit.as_deref() {
                    Some(s) => s,
                    None => bail!("Resource '{}' has no resolved commit", resource.name),
                };
                match self.cache().get_worktree_path(url, sha) {
                    Ok(worktree_dir) => worktree_dir.join(&resource.path),
                    Err(e) => {
                        bail!("Failed to get worktree path for resource '{}': {}", resource.name, e)
                    }
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
            let content = tokio::fs::read_to_string(&source_path).await.with_file_context(
                FileOperation::Read,
                &source_path,
                "reading markdown dependency file",
                "templating_dependencies",
            )?;

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
                            let Some(resource_type) =
                                crate::core::ResourceType::from_frontmatter_str(
                                    resource_type_str.as_str(),
                                )
                            else {
                                continue; // Skip unknown types
                            };
                            let lockfile_type = resource_type.to_string();

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
                                        "Found custom name '{}' for path '{}' (basename: '{}') in resource '{}'",
                                        custom_name,
                                        path,
                                        basename,
                                        resource.name
                                    );

                                    // Check if basename has template variables
                                    if basename.contains("{{") {
                                        // Template variable in basename - try suffix matching
                                        // e.g., "{{ agpm.project.language }}-best-practices" -> "-best-practices"
                                        if let Some(static_suffix_start) = basename.find("}}") {
                                            let static_suffix =
                                                &basename[static_suffix_start + 2..];

                                            tracing::info!(
                                                "  Extracted suffix '{}' from templated basename '{}' in resource '{}'",
                                                static_suffix,
                                                basename,
                                                resource.name
                                            );

                                            // Search for any lockfile basename ending with this suffix
                                            let mut found_count = 0;
                                            for (lockfile_basename, lockfile_dep_ref) in
                                                type_entries
                                            {
                                                tracing::info!(
                                                    "    Checking lockfile basename '{}' against suffix '{}': match={}",
                                                    lockfile_basename,
                                                    static_suffix,
                                                    lockfile_basename.ends_with(static_suffix)
                                                );

                                                if lockfile_basename.ends_with(static_suffix) {
                                                    tracing::info!(
                                                        "  [MATCH] Adding custom name '{}' for lockfile entry '{}' (basename: '{}')",
                                                        custom_name,
                                                        lockfile_dep_ref,
                                                        lockfile_basename
                                                    );
                                                    custom_names.insert(
                                                        lockfile_dep_ref.clone(),
                                                        custom_name.to_string(),
                                                    );
                                                    found_count += 1;
                                                }
                                            }

                                            if found_count == 0 {
                                                tracing::warn!(
                                                    "  [NO MATCH] No lockfile entries found ending with suffix '{}' for custom name '{}' in resource '{}'",
                                                    static_suffix,
                                                    custom_name,
                                                    resource.name
                                                );
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
        } else if resource.path.ends_with(".json") {
            // Parse JSON dependencies field with template rendering
            let content = tokio::fs::read_to_string(&source_path).await.with_file_context(
                FileOperation::Read,
                &source_path,
                "reading JSON dependency file",
                "templating_dependencies",
            )?;

            // Apply templating to JSON content to handle conditional blocks
            let mut parser = crate::markdown::frontmatter::FrontmatterParser::new();
            let templated_content = parser
                .apply_templating(&content, Some(resource.variant_inputs.json()), &source_path)
                .unwrap_or_else(|_| content.clone());

            // Parse JSON and extract dependencies field
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&templated_content) {
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
                        let Some(resource_type) = crate::core::ResourceType::from_frontmatter_str(
                            resource_type_str.as_str(),
                        ) else {
                            continue; // Skip unknown types
                        };
                        let lockfile_type = resource_type.to_string();

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
                                        let static_suffix = &basename[static_suffix_start + 2..];

                                        // Search for any lockfile basename ending with this suffix
                                        for (lockfile_basename, lockfile_dep_ref) in type_entries {
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

        // Store in cache before returning
        if let Ok(mut cache) = self.custom_names_cache().lock() {
            cache.insert(cache_key, custom_names.clone());
            tracing::info!(
                "[EXTRACT_RESULT] Extracted and stored {} custom names in cache for resource '{}' (type: {:?})",
                custom_names.len(),
                resource.name,
                resource.resource_type
            );
        }

        if custom_names.is_empty() {
            tracing::warn!(
                "[EXTRACT_EMPTY] No custom names found for resource '{}' (type: {:?}). lockfile_lookup had {} types, resource has {} dependencies",
                resource.name,
                resource.resource_type,
                lockfile_lookup.len(),
                resource.dependencies.len()
            );
        }

        Ok(custom_names)
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
    ///
    /// # Errors
    ///
    /// Returns an error if the dependency file cannot be read or parsed.
    async fn extract_dependency_specs(
        &self,
        resource: &LockedResource,
    ) -> Result<BTreeMap<String, crate::manifest::DependencySpec>> {
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
                return Ok(cached_specs.clone());
            }
        }

        tracing::debug!("Dependency specs cache MISS for '{}'", resource.name);

        let mut dependency_specs = BTreeMap::new();

        // Determine source path (same logic as extract_content)
        let source_path = if let Some(_source_name) = &resource.source {
            // Has source - check if local or Git
            let url = match resource.url.as_ref() {
                Some(u) => u,
                None => bail!("Resource '{}' has source but no URL", resource.name),
            };

            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            if is_local_source {
                // Local source
                std::path::PathBuf::from(url).join(&resource.path)
            } else {
                // Git source
                let sha = match resource.resolved_commit.as_deref() {
                    Some(s) => s,
                    None => bail!("Resource '{}' has no resolved commit", resource.name),
                };
                match self.cache().get_worktree_path(url, sha) {
                    Ok(worktree_dir) => worktree_dir.join(&resource.path),
                    Err(e) => {
                        bail!("Failed to get worktree path for resource '{}': {}", resource.name, e)
                    }
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
            let content = tokio::fs::read_to_string(&source_path).await.with_file_context(
                FileOperation::Read,
                &source_path,
                "reading markdown dependency file",
                "templating_dependencies",
            )?;

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
                            let Some(resource_type) =
                                crate::core::ResourceType::from_frontmatter_str(
                                    resource_type_str.as_str(),
                                )
                            else {
                                continue;
                            };

                            // Store each DependencySpec with its lockfile reference as key
                            for dep_spec in deps_array {
                                // Canonicalize the frontmatter path to match lockfile format
                                // Frontmatter paths are relative to the resource file itself
                                // We need to resolve them relative to source root (not filesystem paths!)
                                let canonical_path =
                                    canonicalize_dep_path(&dep_spec.path, &resource.path);

                                // Remove extension to match lockfile format
                                let normalized_path = std::path::Path::new(&canonical_path)
                                    .with_extension("")
                                    .to_string_lossy()
                                    .to_string();

                                // Build the dependency reference string WITHOUT version
                                // Cache key should only use path to match any version of this dependency
                                // Version is for resolution purposes, not for identifying the spec
                                let dep_ref = if let Some(ref src) = resource.source {
                                    LockfileDependencyRef::git(
                                        src.clone(),
                                        resource_type,
                                        normalized_path,
                                        None, // No version in cache key
                                    )
                                    .to_string()
                                } else {
                                    LockfileDependencyRef::local(
                                        resource_type,
                                        normalized_path,
                                        None, // No version in cache key
                                    )
                                    .to_string()
                                };

                                dependency_specs.insert(dep_ref, dep_spec.clone());
                            }
                        }
                    }
                }
            }
        } else if resource.path.ends_with(".json") {
            // Parse JSON dependencies field with template rendering
            let content = tokio::fs::read_to_string(&source_path).await.with_file_context(
                FileOperation::Read,
                &source_path,
                "reading JSON dependency file",
                "templating_dependencies",
            )?;

            // Apply templating to JSON content to handle conditional blocks
            let mut parser = crate::markdown::frontmatter::FrontmatterParser::new();
            let templated_content = parser
                .apply_templating(&content, Some(resource.variant_inputs.json()), &source_path)
                .unwrap_or_else(|_| content.clone());

            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&templated_content) {
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
                            "mcp-servers" | "mcp-server" => crate::core::ResourceType::McpServer,
                            _ => continue,
                        };

                        // Store each DependencySpec with its lockfile reference as key
                        for dep_spec in deps_array {
                            // Canonicalize the frontmatter path to match lockfile format
                            // Frontmatter paths are relative to the resource file itself
                            // We need to resolve them relative to source root (not filesystem paths!)
                            let canonical_path =
                                canonicalize_dep_path(&dep_spec.path, &resource.path);

                            // Remove extension to match lockfile format
                            let normalized_path = std::path::Path::new(&canonical_path)
                                .with_extension("")
                                .to_string_lossy()
                                .to_string();

                            // Build the dependency reference string WITHOUT version
                            // Cache key should only use path to match any version of this dependency
                            // Version is for resolution purposes, not for identifying the spec
                            let dep_ref = if let Some(ref src) = resource.source {
                                LockfileDependencyRef::git(
                                    src.clone(),
                                    resource_type,
                                    normalized_path,
                                    None, // No version in cache key
                                )
                                .to_string()
                            } else {
                                LockfileDependencyRef::local(
                                    resource_type,
                                    normalized_path,
                                    None, // No version in cache key
                                )
                                .to_string()
                            };

                            dependency_specs.insert(dep_ref, dep_spec.clone());
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

        Ok(dependency_specs)
    }

    /// Generate dependency name from a path (matching resolver logic).
    ///
    /// For local transitive dependencies, the resolver uses the full relative path
    /// (without extension) as the resource name to maintain uniqueness.
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
        current_resource: &crate::lockfile::LockedResource,
        rendering_stack: &mut HashSet<String>,
    ) -> Result<BTreeMap<String, BTreeMap<String, crate::templating::context::DependencyData>>>;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonicalize_dep_path_relative_up() {
        // Test relative path with ../
        let result = canonicalize_dep_path("../utils/helper.md", "agents/primary.md");
        assert_eq!(result, "utils/helper.md");
    }

    #[test]
    fn test_canonicalize_dep_path_relative_current() {
        // Test relative path with ./
        let result = canonicalize_dep_path("./helper.md", "agents/primary.md");
        assert_eq!(result, "agents/helper.md");
    }

    #[test]
    fn test_canonicalize_dep_path_relative_nested() {
        // Test nested relative path
        let result = canonicalize_dep_path("../utils/helper.md", "agents/ai/assistant.md");
        assert_eq!(result, "agents/utils/helper.md");
    }

    #[test]
    fn test_canonicalize_dep_path_absolute() {
        // Test absolute path (passes through)
        let result = canonicalize_dep_path("agents/helper.md", "agents/primary.md");
        assert_eq!(result, "agents/helper.md");
    }

    #[test]
    fn test_canonicalize_dep_path_absolute_nested() {
        // Test absolute nested path
        let result = canonicalize_dep_path("snippets/utils/helper.md", "agents/primary.md");
        assert_eq!(result, "snippets/utils/helper.md");
    }

    #[test]
    fn test_canonicalize_dep_path_root_resource() {
        // Test with resource at root (no parent directory)
        let result = canonicalize_dep_path("./agents/helper.md", "root.md");
        assert_eq!(result, "agents/helper.md");
    }

    #[test]
    fn test_canonicalize_dep_path_multiple_levels_up() {
        // Test multiple levels up
        let result = canonicalize_dep_path("../../shared/base.md", "agents/ai/models/gpt.md");
        assert_eq!(result, "agents/shared/base.md");
    }

    #[test]
    fn test_canonicalize_dep_path_same_directory() {
        // Test same directory reference
        let result = canonicalize_dep_path("./helper.md", "agents/primary.md");
        assert_eq!(result, "agents/helper.md");
    }

    #[test]
    fn test_canonicalize_dep_path_no_extension() {
        // Test path without extension
        let result = canonicalize_dep_path("../utils/helper", "agents/primary.md");
        assert_eq!(result, "utils/helper");
    }

    #[test]
    fn test_canonicalize_dep_path_with_complex_extension() {
        // Test path with complex extension
        let result = canonicalize_dep_path("../scripts/setup.sh", "agents/primary.md");
        assert_eq!(result, "scripts/setup.sh");
    }
}
