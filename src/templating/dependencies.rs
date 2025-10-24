//! Dependency handling for template context building.
//!
//! This module provides functionality for extracting dependency information,
//! custom names, and building the dependency data structure for template rendering.

use anyhow::{Context as _, Result};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::core::ResourceType;
use crate::lockfile::{LockFile, LockedResource, ResourceId};

use super::cache::{RenderCache, RenderCacheKey};
use super::content::{
    ContentExtractor, NON_TEMPLATED_LITERAL_GUARD_START, content_contains_template_syntax,
};
use super::context::DependencyData;
use super::renderer::TemplateRenderer;
use super::utils::to_native_path_display;

/// Trait for dependency extraction methods on TemplateContextBuilder.
pub(crate) trait DependencyExtractor: ContentExtractor {
    /// Get the lockfile
    fn lockfile(&self) -> &Arc<LockFile>;

    /// Get the render cache
    fn render_cache(&self) -> &Arc<std::sync::Mutex<RenderCache>>;

    /// Extract custom dependency names from a resource's frontmatter.
    ///
    /// Parses the resource file to extract the `dependencies` declaration with `name:` fields
    /// and maps dependency references to their custom names.
    ///
    /// # Returns
    ///
    /// A HashMap mapping dependency references (e.g., "snippet/rust-best-practices") to custom
    /// names (e.g., "best_practices") as declared in the resource's YAML frontmatter.
    async fn extract_dependency_custom_names(
        &self,
        resource: &LockedResource,
    ) -> HashMap<String, String> {
        let mut custom_names = HashMap::new();

        // Get the resolved dependencies from the lockfile
        // These are in the format "type/name" where name is the resolved path
        let lockfile_deps = &resource.dependencies;

        // Debug: log ALL resources to understand what's being processed
        if !lockfile_deps.is_empty() {
            tracing::info!(
                "Processing resource '{}' (type: {:?}) with {} lockfile dependencies",
                resource.name,
                resource.resource_type,
                lockfile_deps.len()
            );
            for dep in lockfile_deps {
                tracing::info!("  Lockfile dep: '{}'", dep);
            }
        }

        // Build a lookup structure upfront to avoid O(n³) nested loops
        // Map: type -> Vec<(basename, full_dep_ref)>
        let mut lockfile_lookup: HashMap<&str, Vec<(String, String)>> = HashMap::new();

        for lockfile_dep_ref in lockfile_deps {
            // Parse lockfile dependency ref: "type/name" or "type/name@version"
            let parts: Vec<&str> = lockfile_dep_ref.splitn(2, '/').collect();
            if parts.len() != 2 {
                continue;
            }

            let lockfile_type = parts[0];
            // Strip version suffix if present (format: name@version)
            let lockfile_name = parts[1].split('@').next().unwrap_or(parts[1]);

            // Extract basename from lockfile name
            let lockfile_basename = std::path::Path::new(lockfile_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(lockfile_name)
                .to_string();

            lockfile_lookup
                .entry(lockfile_type)
                .or_default()
                .push((lockfile_basename, lockfile_dep_ref.to_string()));
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
            // Parse markdown frontmatter
            if let Ok(content) = tokio::fs::read_to_string(&source_path).await {
                if let Ok(doc) = crate::markdown::MarkdownDocument::parse(&content) {
                    // Extract raw frontmatter string and parse as DependencyMetadata
                    // This handles both nested (agpm.dependencies) and root-level (dependencies) locations
                    if let Some(frontmatter) = doc.frontmatter_str() {
                        if let Ok(metadata) =
                            serde_yaml::from_str::<crate::manifest::DependencyMetadata>(frontmatter)
                        {
                            if let Some(deps_map) = metadata.get_dependencies() {
                                // Process each resource type (agents, snippets, commands, etc.)
                                for (resource_type_str, deps_array) in deps_map {
                                    // Convert frontmatter type to lockfile type (singular)
                                    let lockfile_type = match resource_type_str.as_str() {
                                        "agents" | "agent" => "agent",
                                        "snippets" | "snippet" => "snippet",
                                        "commands" | "command" => "command",
                                        "scripts" | "script" => "script",
                                        "hooks" | "hook" => "hook",
                                        "mcp-servers" | "mcp-server" => "mcp-server",
                                        _ => continue, // Skip unknown types
                                    };

                                    // Get lockfile entries for this type only (O(1) lookup instead of O(n) iteration)
                                    let type_entries = match lockfile_lookup.get(lockfile_type) {
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
                                                if let Some(static_suffix_start) =
                                                    basename.find("}}")
                                                {
                                                    let static_suffix =
                                                        &basename[static_suffix_start + 2..];

                                                    // Search for any lockfile basename ending with this suffix
                                                    for (lockfile_basename, lockfile_dep_ref) in
                                                        type_entries
                                                    {
                                                        if lockfile_basename
                                                            .ends_with(static_suffix)
                                                        {
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
            }
        }
        // TODO: Add JSON support if needed

        custom_names
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

        // Helper function to determine the key name for a resource
        let get_key_names = |resource: &LockedResource,
                             dep_type: ResourceType|
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

        // Collect ALL transitive dependencies (not just direct dependencies!)
        // Use a set to track which dependencies we've already added to avoid duplicates
        let mut resources_to_process: Vec<(&LockedResource, ResourceType, bool)> = Vec::new();
        let mut visited_dep_ids = HashSet::new();
        let mut queue: VecDeque<String> = current_resource.dependencies.iter().cloned().collect();

        while let Some(dep_id) = queue.pop_front() {
            // Skip if we've already processed this dependency
            if !visited_dep_ids.insert(dep_id.clone()) {
                continue;
            }

            // Parse dependency ID format: "type/name" or "type/name@version"
            // (e.g., "snippet/helper", "agent/foo@v1.0.0")
            if let Some((type_str, name_with_version)) = dep_id.split_once('/') {
                // Strip the version suffix if present (format: "name@version")
                let name = if let Some((base_name, _version)) = name_with_version.split_once('@') {
                    base_name
                } else {
                    name_with_version
                };
                // Convert type string to ResourceType
                let resource_type = match type_str {
                    "agent" => ResourceType::Agent,
                    "snippet" => ResourceType::Snippet,
                    "command" => ResourceType::Command,
                    "script" => ResourceType::Script,
                    "hook" => ResourceType::Hook,
                    "mcp-server" => ResourceType::McpServer,
                    _ => {
                        tracing::warn!(
                            "Unknown resource type '{}' in dependency '{}' for resource '{}'",
                            type_str,
                            dep_id,
                            current_resource.name
                        );
                        continue;
                    }
                };

                // Look up the dependency in the lockfile
                if let Some(dep_resource) = self.lockfile().find_resource(name, resource_type) {
                    // Add this dependency to resources to process (true = declared dependency)
                    resources_to_process.push((dep_resource, resource_type, true));

                    tracing::debug!(
                        "  [TRANSITIVE] Found dependency '{}' with {} dependencies: {:?}",
                        name,
                        dep_resource.dependencies.len(),
                        dep_resource.dependencies
                    );

                    // Add its dependencies to the queue for recursive processing
                    for transitive_dep in &dep_resource.dependencies {
                        queue.push_back(transitive_dep.clone());
                    }
                } else {
                    tracing::warn!(
                        "Dependency '{}' (type: {:?}) not found in lockfile for resource '{}'",
                        name,
                        resource_type,
                        current_resource.name
                    );
                }
            } else {
                tracing::warn!(
                    "Invalid dependency ID format '{}' for resource '{}' (expected 'type/name')",
                    dep_id,
                    current_resource.name
                );
            }
        }

        // Add ALL lockfile resources (not just transitive dependencies)
        // This ensures templates can reference any resource in the lockfile
        // These are added with is_dependency=false so they don't get rendered recursively

        // Track which resources we've already added to avoid duplicates
        let mut already_added: HashSet<(String, ResourceType)> =
            resources_to_process.iter().map(|(r, rt, _)| (r.name.clone(), *rt)).collect();

        for resource in &self.lockfile().agents {
            if already_added.insert((resource.name.clone(), ResourceType::Agent)) {
                resources_to_process.push((resource, ResourceType::Agent, false));
            }
        }
        for resource in &self.lockfile().commands {
            if already_added.insert((resource.name.clone(), ResourceType::Command)) {
                resources_to_process.push((resource, ResourceType::Command, false));
            }
        }
        for resource in &self.lockfile().snippets {
            if already_added.insert((resource.name.clone(), ResourceType::Snippet)) {
                resources_to_process.push((resource, ResourceType::Snippet, false));
            }
        }
        for resource in &self.lockfile().scripts {
            if already_added.insert((resource.name.clone(), ResourceType::Script)) {
                resources_to_process.push((resource, ResourceType::Script, false));
            }
        }
        for resource in &self.lockfile().hooks {
            if already_added.insert((resource.name.clone(), ResourceType::Hook)) {
                resources_to_process.push((resource, ResourceType::Hook, false));
            }
        }
        for resource in &self.lockfile().mcp_servers {
            if already_added.insert((resource.name.clone(), ResourceType::McpServer)) {
                resources_to_process.push((resource, ResourceType::McpServer, false));
            }
        }

        tracing::debug!(
            "Building dependencies data with {} total resources from lockfile",
            resources_to_process.len()
        );

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
        let current_resource_id =
            format!("{}::{:?}", current_resource.name, current_resource.resource_type);

        // Process each resource (excluding the current resource to prevent self-reference)
        for (resource, dep_type, is_dependency) in &resources_to_process {
            let resource_id = format!("{}::{:?}", resource.name, dep_type);

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
                get_key_names(resource, *dep_type);

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
                let cache_key =
                    RenderCacheKey::new(resource.path.clone(), *dep_type, &resource.template_vars);

                // Check cache first (ensure guard is dropped before any awaits)
                let cache_result = {
                    if let Ok(mut cache) = self.render_cache().lock() {
                        cache.get(&cache_key).cloned()
                    } else {
                        None
                    }
                }; // MutexGuard dropped here

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
                    let dep_id = format!("{}::{:?}", resource.name, dep_type);
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
                    let render_result = Box::pin(
                        self.build_context_with_visited(&dep_resource_id, rendering_stack),
                    )
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
            let type_deps = deps.entry(type_str_plural.clone()).or_insert_with(BTreeMap::new);
            type_deps.insert(sanitized_key.clone(), dependency_data);

            tracing::debug!(
                "  Added resource: {}[{}] -> {}",
                type_str_plural,
                sanitized_key,
                resource.path
            );
        }

        // Add custom alias mappings for the entire dependency tree
        // Each resource in the tree defines custom names for its own dependencies,
        // and we need all of them available when rendering (because embedded content
        // from transitive dependencies may reference their own named dependencies).
        tracing::debug!(
            "Extracting custom dependency names from entire dependency tree for: '{}'",
            current_resource.name
        );

        // Walk the dependency tree and collect custom names from each resource
        let mut to_process: Vec<String> = current_resource.dependencies.clone();
        let mut processed = HashSet::new();

        // Also process the current resource itself
        let current_custom_names = self.extract_dependency_custom_names(current_resource).await;
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

        // Process all transitive dependencies
        while let Some(dep_ref) = to_process.pop() {
            if !processed.insert(dep_ref.clone()) {
                continue; // Already processed
            }

            // Parse dependency reference format: "type/name"
            let parts: Vec<&str> = dep_ref.splitn(2, '/').collect();
            if parts.len() != 2 {
                continue;
            }

            let dep_type_str = parts[0];
            let dep_name = parts[1];

            // Convert to ResourceType enum
            let dep_type = match dep_type_str {
                "agent" => ResourceType::Agent,
                "snippet" => ResourceType::Snippet,
                "command" => ResourceType::Command,
                "script" => ResourceType::Script,
                "hook" => ResourceType::Hook,
                "mcp-server" => ResourceType::McpServer,
                _ => continue,
            };

            // Find the dependency resource in the lockfile
            // Note: We search by name only since dep_ref doesn't include template_vars.
            // The first match should be correct for extracting transitive custom names,
            // as custom names apply to all variants of a resource.
            let dep_resource = match self.lockfile().find_resource(dep_name, dep_type) {
                Some(res) => res,
                None => {
                    tracing::warn!(
                        "Dependency '{}' not found in lockfile for '{}'",
                        dep_ref,
                        current_resource.name
                    );
                    continue;
                }
            };

            // Extract custom names from this dependency (for ITS dependencies)
            let dep_custom_names = self.extract_dependency_custom_names(dep_resource).await;
            for (child_dep_ref, custom_name) in dep_custom_names {
                add_custom_alias(&mut deps, &child_dep_ref, &custom_name);
            }

            // Add this dependency's own dependencies to the queue
            to_process.extend(dep_resource.dependencies.clone());
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
    // Parse dependency reference format: "type/name" or "type/name@version"
    let parts: Vec<&str> = dep_ref.splitn(2, '/').collect();
    if parts.len() != 2 {
        tracing::debug!(
            "Skipping invalid dep_ref format '{}' for custom name '{}'",
            dep_ref,
            custom_name
        );
        return;
    }

    let dep_type_str = parts[0];
    // Strip version suffix if present (format: name@version)
    let dep_name = parts[1].split('@').next().unwrap_or(parts[1]);

    // Convert to ResourceType enum to get plural form
    let dep_type = match dep_type_str {
        "agent" => ResourceType::Agent,
        "snippet" => ResourceType::Snippet,
        "command" => ResourceType::Command,
        "script" => ResourceType::Script,
        "hook" => ResourceType::Hook,
        "mcp-server" => ResourceType::McpServer,
        _ => {
            tracing::debug!(
                "Skipping unknown resource type '{}' in dep_ref '{}' for custom name '{}'",
                dep_type_str,
                dep_ref,
                custom_name
            );
            return;
        }
    };

    let type_str_plural = dep_type.to_plural().to_string();

    // Search for the resource in the deps map (already populated from lockfile)
    if let Some(type_deps) = deps.get_mut(&type_str_plural) {
        // The resource should already exist in the map with its path-based key
        // Find it by matching the DependencyData.name field (which is the lockfile name)
        let existing_data = type_deps
            .values()
            .find(|data| {
                // Match by the actual lockfile resource name
                data.name == dep_name
            })
            .cloned();

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
