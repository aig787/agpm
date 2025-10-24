//! Transitive dependency resolution for AGPM.
//!
//! This module handles the discovery and resolution of transitive dependencies,
//! building dependency graphs, detecting cycles, and providing high-level
//! orchestration for the entire transitive resolution process. It processes
//! dependencies declared within resource files and resolves them in topological order.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::core::ResourceType;
use crate::manifest::{DetailedDependency, ResourceDependency, json_value_to_toml};
use crate::metadata::MetadataExtractor;
use crate::utils;

use super::dependency_graph::{DependencyGraph, DependencyNode};
use super::pattern_expander::generate_dependency_name;
use super::types::{DependencyKey, TransitiveContext};
use super::version_resolver::PreparedSourceVersion;
use super::{is_file_relative_path, normalize_bare_filename};

/// Configuration for transitive dependency resolution functions.
pub struct TransitiveResolver<F1, F2, F3, F4> {
    /// Function to fetch resource content for metadata extraction
    pub fetch_content: F1,
    /// Function to expand pattern dependencies
    pub expand_pattern: F2,
    /// Function to get canonical path for a dependency
    pub get_canonical_path: F3,
    /// Function to resolve version conflicts
    pub resolve_version_conflict: F4,
}

/// Input data for transitive dependency resolution.
pub struct TransitiveInput<'a> {
    /// Initial dependencies from the manifest with their resource types
    pub base_deps: &'a [(String, ResourceDependency, ResourceType)],
    /// Whether to enable transitive dependency resolution
    pub enable_transitive: bool,
    /// Map of prepared worktrees keyed by source::version
    pub prepared_versions: &'a HashMap<String, PreparedSourceVersion>,
    /// Mutable map tracking pattern alias relationships
    pub pattern_alias_map: &'a mut HashMap<(ResourceType, String), String>,
}

/// Resolve transitive dependencies starting from a set of base dependencies.
///
/// This is the main entry point for transitive dependency resolution. It:
/// 1. Discovers dependencies declared in resource files
/// 2. Expands pattern dependencies to concrete files
/// 3. Builds a dependency graph with cycle detection
/// 4. Resolves version conflicts
/// 5. Returns dependencies in topological order
///
/// # Arguments
///
/// * `ctx` - Transitive resolution context with manifest, cache, and mutable state
/// * `input` - Input data containing base dependencies and resolution configuration
/// * `resolver` - Resolver functions for content fetching, pattern expansion, and conflict resolution
///
/// # Returns
///
/// A vector of all dependencies (direct + transitive) in topological order
pub async fn resolve_transitive_dependencies<F1, F2, F3, F4, Fut1, Fut2, Fut3, Fut4>(
    ctx: &mut TransitiveContext<'_>,
    input: TransitiveInput<'_>,
    mut resolver: TransitiveResolver<F1, F2, F3, F4>,
) -> Result<Vec<(String, ResourceDependency, ResourceType)>>
where
    F1: FnMut(&str, &ResourceDependency) -> Fut1,
    Fut1: std::future::Future<Output = Result<String>>,
    F2: FnMut(&ResourceDependency, ResourceType) -> Fut2,
    Fut2: std::future::Future<Output = Result<Vec<(String, ResourceDependency)>>>,
    F3: FnMut(&ResourceDependency) -> Fut3,
    Fut3: std::future::Future<Output = Result<PathBuf>>,
    F4: FnMut(&str, &ResourceDependency, &ResourceDependency, &str) -> Fut4,
    Fut4: std::future::Future<Output = Result<ResourceDependency>>,
{
    // Clear state from any previous resolution
    ctx.dependency_map.clear();

    if !input.enable_transitive {
        // If transitive resolution is disabled, return base dependencies as-is
        return Ok(input.base_deps.to_vec());
    }

    let mut graph = DependencyGraph::new();
    let mut all_deps: HashMap<DependencyKey, ResourceDependency> = HashMap::new();
    let mut processed: HashSet<DependencyKey> = HashSet::new();
    let mut queue: Vec<(String, ResourceDependency, Option<ResourceType>)> = Vec::new();

    // Add initial dependencies to queue with their threaded types
    for (name, dep, resource_type) in input.base_deps {
        let source = dep.get_source().map(std::string::ToString::to_string);
        let tool = dep.get_tool().map(std::string::ToString::to_string);
        queue.push((name.clone(), dep.clone(), Some(*resource_type)));
        all_deps.insert((*resource_type, name.clone(), source, tool), dep.clone());
    }

    // Process queue to discover transitive dependencies
    while let Some((name, dep, resource_type)) = queue.pop() {
        let source = dep.get_source().map(std::string::ToString::to_string);
        let tool = dep.get_tool().map(std::string::ToString::to_string);
        let resource_type =
            resource_type.expect("resource_type should always be threaded through queue");
        let key = (resource_type, name.clone(), source.clone(), tool.clone());

        tracing::debug!(
            "[QUEUE_POP] Popped from queue: '{}' (type: {:?}, source: {:?}, tool: {:?})",
            name,
            resource_type,
            source,
            tool
        );

        // Check if this queue entry is stale (superseded by conflict resolution)
        if let Some(current_dep) = all_deps.get(&key) {
            if current_dep.get_version() != dep.get_version() {
                tracing::debug!("[QUEUE_POP] SKIPPED (stale): '{}' - version mismatch", name);
                continue;
            }
        }

        if processed.contains(&key) {
            tracing::debug!("[QUEUE_POP] SKIPPED (already processed): '{}'", name);
            continue;
        }

        tracing::debug!("[QUEUE_POP] PROCESSING: '{}'", name);
        processed.insert(key.clone());

        // Handle pattern dependencies by expanding them to concrete files
        if dep.is_pattern() {
            tracing::debug!("[QUEUE_POP] '{}' is a PATTERN, expanding to concrete deps", name);
            match (resolver.expand_pattern)(&dep, resource_type).await {
                Ok(concrete_deps) => {
                    for (concrete_name, concrete_dep) in concrete_deps {
                        // Record the mapping from concrete resource name to pattern alias
                        input
                            .pattern_alias_map
                            .insert((resource_type, concrete_name.clone()), name.clone());

                        let concrete_source =
                            concrete_dep.get_source().map(std::string::ToString::to_string);
                        let concrete_tool =
                            concrete_dep.get_tool().map(std::string::ToString::to_string);
                        let concrete_key =
                            (resource_type, concrete_name.clone(), concrete_source, concrete_tool);

                        // Only add if not already processed or queued
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            all_deps.entry(concrete_key)
                        {
                            e.insert(concrete_dep.clone());
                            queue.push((concrete_name, concrete_dep, Some(resource_type)));
                        }
                    }
                }
                Err(e) => {
                    anyhow::bail!(
                        "Failed to expand pattern '{}' for transitive dependency extraction: {}",
                        dep.get_path(),
                        e
                    );
                }
            }
            continue;
        }

        tracing::debug!(
            "[QUEUE_POP] '{}' is NOT a pattern, fetching content for metadata extraction",
            name
        );

        // Get the resource content to extract metadata
        let content = (resolver.fetch_content)(&name, &dep).await.with_context(|| {
            format!("Failed to fetch resource '{name}' for transitive dependency extraction")
        })?;

        tracing::debug!(
            "[QUEUE_POP] '{}' content fetched ({} bytes), extracting metadata",
            name,
            content.len()
        );

        // Merge resource-specific template_vars with global project config
        let project_config = build_project_config(ctx, &dep)?;

        // Extract metadata from the resource with merged config
        let path = PathBuf::from(dep.get_path());
        let metadata = MetadataExtractor::extract(
            &path,
            &content,
            project_config.as_ref(),
            ctx.base.operation_context.map(|arc| arc.as_ref()),
        )?;

        // Process transitive dependencies if present
        if let Some(deps_map) = metadata.get_dependencies() {
            tracing::debug!(
                "Processing transitive deps for: {} (has source: {:?})",
                name,
                dep.get_source()
            );

            for (dep_resource_type_str, dep_specs) in deps_map {
                // Convert plural form from YAML (e.g., "agents") to ResourceType enum
                let dep_resource_type: ResourceType =
                    dep_resource_type_str.parse().unwrap_or(ResourceType::Snippet);

                for dep_spec in dep_specs {
                    // Process each transitive dependency spec
                    let (trans_dep, trans_name) = process_transitive_dependency_spec(
                        ctx,
                        &dep,
                        dep_resource_type,
                        resource_type,
                        &name,
                        dep_spec,
                        &mut resolver.get_canonical_path,
                        input.prepared_versions,
                    )
                    .await?;

                    let trans_source = trans_dep.get_source().map(std::string::ToString::to_string);
                    let trans_tool = trans_dep.get_tool().map(std::string::ToString::to_string);

                    // Store custom name if provided, for use as manifest_alias
                    if let Some(custom_name) = &dep_spec.name {
                        let trans_key = (
                            dep_resource_type,
                            trans_name.clone(),
                            trans_source.clone(),
                            trans_tool.clone(),
                        );
                        ctx.transitive_custom_names.insert(trans_key, custom_name.clone());
                        tracing::debug!(
                            "Storing custom name '{}' for transitive dependency '{}'",
                            custom_name,
                            trans_name
                        );
                    }

                    // Add to graph
                    let from_node =
                        DependencyNode::with_source(resource_type, &name, source.clone());
                    let to_node = DependencyNode::with_source(
                        dep_resource_type,
                        &trans_name,
                        trans_source.clone(),
                    );
                    graph.add_dependency(from_node.clone(), to_node.clone());

                    // Track in dependency map
                    let from_key = (resource_type, name.clone(), source.clone(), tool.clone());
                    let dep_ref = format!("{dep_resource_type}/{trans_name}");
                    ctx.dependency_map.entry(from_key).or_default().push(dep_ref);

                    // Add to conflict detector
                    add_to_conflict_detector(ctx, &trans_name, &trans_dep, &name);

                    // Check for version conflicts and resolve them
                    let trans_key = (
                        dep_resource_type,
                        trans_name.clone(),
                        trans_source.clone(),
                        trans_tool.clone(),
                    );

                    if let Some(existing_dep) = all_deps.get(&trans_key) {
                        // Version conflict detected
                        let resolved_dep = (resolver.resolve_version_conflict)(
                            &trans_name,
                            existing_dep,
                            &trans_dep,
                            &name,
                        )
                        .await?;

                        let needs_reprocess =
                            resolved_dep.get_version() != existing_dep.get_version();

                        all_deps.insert(trans_key.clone(), resolved_dep.clone());

                        if needs_reprocess {
                            processed.remove(&trans_key);
                            queue.push((trans_name.clone(), resolved_dep, Some(dep_resource_type)));
                        }
                    } else {
                        // No conflict, add the dependency
                        tracing::debug!(
                            "Adding transitive dep '{}' to all_deps and queue (parent: {})",
                            trans_name,
                            name
                        );
                        all_deps.insert(trans_key.clone(), trans_dep.clone());
                        queue.push((trans_name, trans_dep, Some(dep_resource_type)));
                    }
                }
            }
        }
    }

    // Check for circular dependencies
    graph.detect_cycles()?;

    // Get topological order for dependencies that have relationships
    let ordered_nodes = graph.topological_order()?;

    // Build result: start with topologically ordered dependencies
    build_ordered_result(all_deps, ordered_nodes)
}

/// Build the merged project config for a dependency.
fn build_project_config(
    ctx: &TransitiveContext<'_>,
    dep: &ResourceDependency,
) -> Result<Option<crate::manifest::ProjectConfig>> {
    use crate::manifest::ProjectConfig;
    use crate::templating::deep_merge_json;

    if let Some(template_vars) = dep.get_template_vars() {
        // Extract the "project" key from template_vars
        let project_overrides = template_vars
            .get("project")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let global_json = ctx
            .base
            .manifest
            .project
            .as_ref()
            .map(|p| p.to_json_value())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        // Deep merge: global config + resource-specific project overrides
        let merged_json = deep_merge_json(global_json, &project_overrides);

        // Convert merged JSON back to TOML for ProjectConfig
        let mut config_map = toml::map::Map::new();
        if let Some(merged_obj) = merged_json.as_object() {
            for (key, value) in merged_obj {
                config_map.insert(key.clone(), json_value_to_toml(value));
            }
        }

        Ok(Some(ProjectConfig::from(config_map)))
    } else {
        // No template_vars - use global config
        Ok(ctx.base.manifest.project.clone())
    }
}

/// Process a single transitive dependency specification.
#[allow(clippy::too_many_arguments)]
async fn process_transitive_dependency_spec<F, Fut>(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    parent_name: &str,
    dep_spec: &crate::manifest::DependencySpec,
    get_canonical_path: &mut F,
    prepared_versions: &HashMap<String, PreparedSourceVersion>,
) -> Result<(ResourceDependency, String)>
where
    F: FnMut(&ResourceDependency) -> Fut,
    Fut: std::future::Future<Output = Result<PathBuf>>,
{
    // Get the canonical path to the parent resource file
    let parent_file_path = get_canonical_path(parent_dep).await.with_context(|| {
        format!("Failed to get parent path for transitive dependencies of '{}'", parent_name)
    })?;

    // Resolve the transitive dependency path
    let trans_canonical = resolve_transitive_path(&parent_file_path, &dep_spec.path, parent_name)?;

    // Create the transitive dependency
    let trans_dep = create_transitive_dependency(
        ctx,
        parent_dep,
        dep_resource_type,
        parent_resource_type,
        parent_name,
        dep_spec,
        &parent_file_path,
        &trans_canonical,
        prepared_versions,
    )
    .await?;

    // Generate a name for the transitive dependency
    let trans_name = generate_dependency_name(trans_dep.get_path());

    Ok((trans_dep, trans_name))
}

/// Resolve a transitive dependency path relative to its parent.
fn resolve_transitive_path(
    parent_file_path: &Path,
    dep_path: &str,
    parent_name: &str,
) -> Result<PathBuf> {
    // Check if this is a glob pattern
    let is_pattern = dep_path.contains('*') || dep_path.contains('?') || dep_path.contains('[');

    if is_pattern {
        // For patterns, normalize (resolve .. and .) but don't canonicalize
        let parent_dir = parent_file_path.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to resolve transitive dependency '{}' for '{}': parent file has no directory",
                dep_path,
                parent_name
            )
        })?;
        let resolved = parent_dir.join(dep_path);

        // Preserve the root component when normalizing
        let mut result = PathBuf::new();
        for component in resolved.components() {
            match component {
                std::path::Component::RootDir => result.push(component),
                std::path::Component::ParentDir => {
                    result.pop();
                }
                std::path::Component::CurDir => {}
                _ => result.push(component),
            }
        }
        Ok(result)
    } else if is_file_relative_path(dep_path) {
        // File-relative path
        let normalized_path = normalize_bare_filename(dep_path);
        utils::resolve_file_relative_path(parent_file_path, &normalized_path).with_context(|| {
            format!("Failed to resolve transitive dependency '{}' for '{}'", dep_path, parent_name)
        })
    } else {
        // Repo-relative path
        resolve_repo_relative_path(parent_file_path, dep_path, parent_name)
    }
}

/// Resolve a repository-relative transitive dependency path.
fn resolve_repo_relative_path(
    parent_file_path: &Path,
    dep_path: &str,
    parent_name: &str,
) -> Result<PathBuf> {
    // For Git sources, find the worktree root; for local sources, find the source root
    let repo_root = parent_file_path
        .ancestors()
        .find(|p| {
            // Worktree directories have format: owner_repo_sha8
            p.file_name().and_then(|n| n.to_str()).map(|s| s.contains('_')).unwrap_or(false)
        })
        .or_else(|| parent_file_path.ancestors().nth(2)) // Fallback for local sources
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to find repository root for transitive dependency '{}'",
                dep_path
            )
        })?;

    let full_path = repo_root.join(dep_path);
    full_path.canonicalize().with_context(|| {
        format!(
            "Failed to resolve repo-relative transitive dependency '{}' for '{}': {} (repo root: {})",
            dep_path,
            parent_name,
            full_path.display(),
            repo_root.display()
        )
    })
}

/// Create a ResourceDependency for a transitive dependency.
#[allow(clippy::too_many_arguments)]
async fn create_transitive_dependency(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    parent_name: &str,
    dep_spec: &crate::manifest::DependencySpec,
    parent_file_path: &Path,
    trans_canonical: &Path,
    prepared_versions: &HashMap<String, PreparedSourceVersion>,
) -> Result<ResourceDependency> {
    if parent_dep.get_source().is_none() {
        create_path_only_transitive_dep(
            ctx,
            parent_dep,
            dep_resource_type,
            parent_resource_type,
            dep_spec,
            trans_canonical,
        )
    } else {
        create_git_backed_transitive_dep(
            ctx,
            parent_dep,
            dep_resource_type,
            parent_resource_type,
            parent_name,
            dep_spec,
            parent_file_path,
            trans_canonical,
            prepared_versions,
        )
        .await
    }
}

/// Create a path-only transitive dependency (parent is path-only).
fn create_path_only_transitive_dep(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    dep_spec: &crate::manifest::DependencySpec,
    trans_canonical: &Path,
) -> Result<ResourceDependency> {
    let manifest_dir = ctx.base.manifest.manifest_dir.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Manifest directory not available for path-only transitive dep")
    })?;

    // Always compute relative path from manifest to target
    let dep_path_str = match manifest_dir.canonicalize() {
        Ok(canonical_manifest) => {
            utils::compute_relative_path(&canonical_manifest, trans_canonical)
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not canonicalize manifest directory {}: {}. Using non-canonical path.",
                manifest_dir.display(),
                e
            );
            utils::compute_relative_path(manifest_dir, trans_canonical)
        }
    };

    // Determine tool for transitive dependency
    let trans_tool = determine_transitive_tool(
        ctx,
        parent_dep,
        dep_spec,
        parent_resource_type,
        dep_resource_type,
    );

    Ok(ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: None,
        path: utils::normalize_path_for_storage(dep_path_str),
        version: None,
        branch: None,
        rev: None,
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: trans_tool,
        flatten: None,
        install: dep_spec.install.or(Some(true)),
        template_vars: Some(build_merged_template_vars(ctx, parent_dep)),
    })))
}

/// Create a Git-backed transitive dependency (parent is Git-backed).
#[allow(clippy::too_many_arguments)]
async fn create_git_backed_transitive_dep(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    _parent_name: &str,
    dep_spec: &crate::manifest::DependencySpec,
    _parent_file_path: &Path,
    trans_canonical: &Path,
    prepared_versions: &HashMap<String, PreparedSourceVersion>,
) -> Result<ResourceDependency> {
    let source_name = parent_dep
        .get_source()
        .ok_or_else(|| anyhow::anyhow!("Expected source for Git-backed dependency"))?;
    let version = parent_dep.get_version().unwrap_or("main").to_string();
    let source_url = ctx
        .base
        .source_manager
        .get_source_url(source_name)
        .ok_or_else(|| anyhow::anyhow!("Source '{source_name}' not found"))?;

    // Get repo-relative path by stripping the appropriate prefix
    let repo_relative = if utils::is_local_path(&source_url) {
        strip_local_source_prefix(&source_url, trans_canonical)?
    } else {
        strip_git_worktree_prefix(
            ctx,
            source_name,
            &version,
            &source_url,
            trans_canonical,
            prepared_versions,
        )
        .await?
    };

    // Determine tool for transitive dependency
    let trans_tool = determine_transitive_tool(
        ctx,
        parent_dep,
        dep_spec,
        parent_resource_type,
        dep_resource_type,
    );

    Ok(ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some(source_name.to_string()),
        path: utils::normalize_path_for_storage(repo_relative.to_string_lossy().to_string()),
        version: dep_spec
            .version
            .clone()
            .or_else(|| parent_dep.get_version().map(|v| v.to_string())),
        branch: None,
        rev: None,
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: trans_tool,
        flatten: None,
        install: dep_spec.install.or(Some(true)),
        template_vars: Some(build_merged_template_vars(ctx, parent_dep)),
    })))
}

/// Strip the local source prefix from a transitive dependency path.
fn strip_local_source_prefix(source_url: &str, trans_canonical: &Path) -> Result<PathBuf> {
    let source_path = PathBuf::from(source_url).canonicalize()?;
    trans_canonical
        .strip_prefix(&source_path)
        .with_context(|| {
            format!(
                "Transitive dep resolved outside parent's source directory: {} not under {}",
                trans_canonical.display(),
                source_path.display()
            )
        })
        .map(|p| p.to_path_buf())
}

/// Strip the Git worktree prefix from a transitive dependency path.
async fn strip_git_worktree_prefix(
    ctx: &TransitiveContext<'_>,
    source_name: &str,
    version: &str,
    source_url: &str,
    trans_canonical: &Path,
    prepared_versions: &HashMap<String, PreparedSourceVersion>,
) -> Result<PathBuf> {
    let sha = prepared_versions
        .get(&group_key(source_name, version))
        .ok_or_else(|| anyhow::anyhow!("Parent version not resolved for {}", source_name))?
        .resolved_commit
        .clone();

    let worktree_path =
        ctx.base.cache.get_or_create_worktree_for_sha(source_name, source_url, &sha, None).await?;

    // Canonicalize worktree path to handle symlinks
    let canonical_worktree = worktree_path.canonicalize().with_context(|| {
        format!("Failed to canonicalize worktree path: {}", worktree_path.display())
    })?;

    trans_canonical
        .strip_prefix(&canonical_worktree)
        .with_context(|| {
            format!(
                "Transitive dep resolved outside parent's worktree: {} not under {}",
                trans_canonical.display(),
                canonical_worktree.display()
            )
        })
        .map(|p| p.to_path_buf())
}

/// Determine the tool for a transitive dependency.
fn determine_transitive_tool(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_spec: &crate::manifest::DependencySpec,
    parent_resource_type: ResourceType,
    dep_resource_type: ResourceType,
) -> Option<String> {
    if let Some(explicit_tool) = &dep_spec.tool {
        Some(explicit_tool.clone())
    } else {
        let parent_tool = parent_dep
            .get_tool()
            .map(str::to_string)
            .unwrap_or_else(|| ctx.base.manifest.get_default_tool(parent_resource_type));
        if ctx.base.manifest.is_resource_supported(&parent_tool, dep_resource_type) {
            Some(parent_tool)
        } else {
            Some(ctx.base.manifest.get_default_tool(dep_resource_type))
        }
    }
}

/// Build merged template variables for a dependency.
fn build_merged_template_vars(
    ctx: &TransitiveContext<'_>,
    dep: &ResourceDependency,
) -> serde_json::Value {
    super::lockfile_builder::build_merged_template_vars(ctx.base.manifest, dep)
}

/// Add a dependency to the conflict detector.
fn add_to_conflict_detector(
    ctx: &mut TransitiveContext<'_>,
    name: &str,
    dep: &ResourceDependency,
    requester: &str,
) {
    if let Some(version) = dep.get_version() {
        ctx.conflict_detector.add_requirement(name, version, requester);
    }
}

/// Build the final ordered result from the dependency graph.
fn build_ordered_result(
    all_deps: HashMap<DependencyKey, ResourceDependency>,
    ordered_nodes: Vec<DependencyNode>,
) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
    let mut result = Vec::new();
    let mut added_keys = HashSet::new();

    tracing::debug!(
        "Transitive resolution - topological order has {} nodes, all_deps has {} entries",
        ordered_nodes.len(),
        all_deps.len()
    );

    for node in ordered_nodes {
        tracing::debug!(
            "Processing ordered node: {}/{} (source: {:?})",
            node.resource_type,
            node.name,
            node.source
        );

        // Find matching dependency
        for (key, dep) in &all_deps {
            if key.0 == node.resource_type && key.1 == node.name && key.2 == node.source {
                tracing::debug!(
                    "  -> Found match in all_deps, adding to result with type {:?}",
                    node.resource_type
                );
                result.push((node.name.clone(), dep.clone(), node.resource_type));
                added_keys.insert(key.clone());
                break;
            }
        }
    }

    // Add remaining dependencies that weren't in the graph (no transitive deps)
    for (key, dep) in all_deps {
        if !added_keys.contains(&key) && !dep.is_pattern() {
            tracing::debug!(
                "Adding non-graph dependency: {}/{} (source: {:?}) with type {:?}",
                key.0,
                key.1,
                key.2,
                key.0
            );
            result.push((key.1.clone(), dep.clone(), key.0));
        }
    }

    tracing::debug!("Transitive resolution returning {} dependencies", result.len());

    Ok(result)
}

/// Generate unique key for grouping dependencies by source and version.
fn group_key(source: &str, version: &str) -> String {
    format!("{source}::{version}")
}
