//! Dependency building functionality for templates.
//!
//! This module provides helper functions for building dependency data
//! structures used in template rendering.

use anyhow::{Context as _, Result};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use crate::core::ResourceType;
use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
use crate::lockfile::{LockedResource, ResourceId};

use super::extractors::{DependencyExtractor, create_dependency_ref_string};
use crate::templating::cache::RenderCacheKey;
use crate::templating::context::DependencyData;
use crate::templating::renderer::TemplateRenderer;
use crate::templating::utils::to_native_path_display;

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
/// * `extractor` - The dependency extractor implementation
/// * `current_resource` - The resource being rendered (for scoped alias mapping)
/// * `rendering_stack` - Stack for cycle detection
pub(crate) async fn build_dependencies_data<T: DependencyExtractor>(
    extractor: &T,
    current_resource: &LockedResource,
    rendering_stack: &mut HashSet<String>,
) -> Result<BTreeMap<String, BTreeMap<String, DependencyData>>> {
    let mut deps = BTreeMap::new();

    // Extract dependency specifications from current resource's frontmatter
    // This provides tool, name, flatten, and install fields for each dependency
    let dependency_specs =
        extractor.extract_dependency_specs(current_resource).await.with_context(|| {
            format!(
                "Failed to extract dependency specifications from resource '{}' (type: {:?})",
                current_resource.name, current_resource.resource_type
            )
        })?;

    // Helper function to determine the key name for a resource
    let get_key_names =
        |resource: &LockedResource, dep_type: &ResourceType| -> (String, String, String, String) {
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
            extractor.lockfile().find_resource_by_id(&dep_resource_id_with_parent_hash);

        if current_resource.name.contains("frontend-engineer") {
            tracing::info!(
                "  [LOOKUP] For '{}', looking up dep '{}' (type: {:?}) with parent hash {}: found={}",
                current_resource.name,
                name,
                resource_type,
                &current_resource.variant_inputs.hash()[..12],
                dep_resource.is_some()
            );
        }

        // If not found with parent's hash, try with empty hash (direct manifest dependencies)
        if dep_resource.is_none() {
            let dep_resource_id_empty_hash = ResourceId::new(
                name.clone(),
                dep_source.cloned(),
                dep_tool.cloned(),
                resource_type,
                crate::resolver::lockfile_builder::VariantInputs::default().hash().to_string(),
            );
            dep_resource = extractor.lockfile().find_resource_by_id(&dep_resource_id_empty_hash);

            if dep_resource.is_some() {
                tracing::debug!(
                    "  [DIRECT MANIFEST DEP] Found dependency '{}' with empty variant_hash (direct manifest dependency)",
                    name
                );
            } else if current_resource.name.contains("frontend-engineer") {
                tracing::warn!(
                    "  [NOT FOUND] Dependency '{}' not found for '{}' with parent hash {} or empty hash",
                    name,
                    current_resource.name,
                    &current_resource.variant_inputs.hash()[..12]
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

    // Compute dependency hash for cache invalidation
    // This ensures that if dependencies change, the cache entry is invalidated
    let dependency_hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash all dependency specs
        for (dep_id, spec) in &dependency_specs {
            dep_id.hash(&mut hasher);
            if let Some(tool) = &spec.tool {
                tool.hash(&mut hasher);
            }
            if let Some(version) = &spec.version {
                version.hash(&mut hasher);
            }
            spec.path.hash(&mut hasher);
        }
        
        format!("{:x}", hasher.finish())
    };

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
        // extract_content() returns (content, has_templating) tuple for markdown files
        let (raw_content, has_templating) = match extractor.extract_content(resource).await {
            Some((content, templating)) => (Some(content), templating),
            None => (None, false),
        };

        if current_resource.name.contains("frontend-engineer") && resource.name.contains("best-practices") {
            if let Some(content) = &raw_content {
                tracing::warn!(
                    "  [RAW] Extracted content for '{}': len={}, preview={}, has_templating={}",
                    resource.name,
                    content.len(),
                    &content.chars().take(100).collect::<String>(),
                    has_templating
                );
            }
        }

        // Check if the dependency should be rendered
        // Only render dependencies that have templating: true
        // For templating: false, extract_content() already strips frontmatter
        let should_render = *is_dependency && raw_content.is_some() && has_templating;

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
                dependency_hash.clone(),
            );

            // Check cache first (ensure guard is dropped before any awaits)
            let cache_result = extractor
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
                let render_result = Box::pin(extractor.build_context_with_visited(
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
                                    extractor.project_dir().clone(),
                                    None,
                                )
                                .with_context(|| {
                                    format!(
                                        "Failed to create template renderer for dependency '{}' (type: {:?})",
                                        resource.name,
                                        dep_type
                                    )
                                })?;

                            // Create metadata for dependency rendering with basic chain info
                            let metadata = crate::templating::renderer::RenderingMetadata {
                                resource_name: resource.name.clone(),
                                resource_type: *dep_type,
                                dependency_chain: vec![], // TODO: Build full dependency chain
                                source_path: None,
                                depth: rendering_stack.len(),
                            };

                            let rendered = renderer
                                    .render_template(&content, &dep_context, Some(&metadata))
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

                            // Strip frontmatter from rendered markdown content
                            let final_content = if resource.path.ends_with(".md") {
                                match crate::markdown::MarkdownDocument::parse(&rendered) {
                                    Ok(doc) => doc.content,
                                    Err(_) => {
                                        // If parsing fails, try to strip manually
                                        let frontmatter_parser =
                                            crate::markdown::frontmatter::FrontmatterParser::new();
                                        frontmatter_parser.strip_frontmatter(&rendered)
                                    }
                                }
                            } else {
                                rendered
                            };

                            tracing::debug!(
                                "Successfully rendered dependency content for '{}'",
                                resource.name
                            );

                            // Store in cache for future use (cache the final stripped content)
                            if let Ok(mut cache) = extractor.render_cache().lock() {
                                cache.insert(cache_key.clone(), final_content.clone());
                                tracing::debug!(
                                    "Stored rendered content in cache for '{}'",
                                    resource.name
                                );
                            }

                            final_content
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
            // No rendering needed - use raw content as-is
            // IMPORTANT: Do NOT collapse literal guards here!
            // Guards must remain intact to protect template syntax when content is embedded
            raw_content.unwrap_or_default()
        };

        // Create DependencyData with all fields including content
        let dependency_data = DependencyData {
            resource_type: type_str_singular.clone(),
            name: resource.name.clone(),
            install_path: to_native_path_display(&resource.installed_at),
            source: resource.source.clone(),
            version: resource.version.clone(),
            resolved_commit: resource.resolved_commit.clone(),
            checksum: resource.checksum.clone(),
            path: resource.path.clone(),
            content: final_content.clone(),
        };

        if current_resource.name.contains("frontend-engineer") && resource.name.contains("best-practices") {
            tracing::warn!(
                "  [CONTENT] Adding '{}' to context of '{}': content_len={}, content_preview={}",
                resource.name,
                current_resource.name,
                final_content.len(),
                &final_content.chars().take(100).collect::<String>()
            );
        }

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
    let current_custom_names =
        extractor.extract_dependency_custom_names(current_resource).await.with_context(|| {
            format!(
                "Failed to extract custom dependency names from resource '{}' (type: {:?})",
                current_resource.name, current_resource.resource_type
            )
        })?;
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
        for (key, data) in resources {
            if resource_type == "snippets" || data.name.contains("frontend-engineer") {
                tracing::info!("    [CONTEXT-{}] For '{}': key='{}', name='{}', path='{}'",
                    resource_type, current_resource.name, key, data.name, data.path);
            } else {
                tracing::debug!("    - {}", key);
            }
        }
    }

    Ok(deps)
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
