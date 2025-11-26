//! Transitive dependency resolution for AGPM.
//!
//! This module handles the discovery and resolution of transitive dependencies,
//! building dependency graphs, detecting cycles, and providing high-level
//! orchestration for the entire transitive resolution process. It processes
//! dependencies declared within resource files and resolves them in topological order.
//!
//! ## Parallel Processing Algorithm
//!
//! Transitive dependencies are resolved in parallel batches:
//! 1. Calculate batch size: min(max(10, CPU cores × 2), remaining queue length)
//! 2. Extract batch from queue (LIFO order to match serial behavior)
//! 3. Process batch concurrently using join_all
//! 4. Repeat until queue empty
//!
//! Concurrent safety is ensured via `Arc<DashMap>` for shared state.
//! Each batch processes dependencies independently, with coordination
//! happening through the shared DashMap-backed registries.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use dashmap::DashMap;
use futures::future::join_all;

use crate::core::ResourceType;
use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
use crate::manifest::{DetailedDependency, ResourceDependency};
use crate::metadata::MetadataExtractor;
use crate::utils;
use crate::version::conflict::ConflictDetector;

use super::dependency_graph::{DependencyGraph, DependencyNode};
use super::pattern_expander::generate_dependency_name;
use super::types::{
    DependencyKey, TransitiveContext, apply_manifest_override, compute_dependency_variant_hash,
};
use super::version_resolver::{PreparedSourceVersion, VersionResolutionService};
use super::{PatternExpansionService, ResourceFetchingService, is_file_relative_path};

/// Container for resolution services to reduce parameter count.
pub struct ResolutionServices<'a> {
    /// Service for version resolution and commit SHA lookup
    pub version_service: &'a VersionResolutionService,
    /// Service for pattern expansion (glob patterns)
    pub pattern_service: &'a PatternExpansionService,
}

/// Parameters for transitive resolution to reduce function argument count.
pub struct TransitiveResolutionParams<'a> {
    /// Core resolution context
    pub ctx: &'a mut TransitiveContext<'a>,
    /// Core resolution services
    pub core: &'a super::ResolutionCore,
    /// Base dependencies to resolve
    pub base_deps: &'a [(String, ResourceDependency, ResourceType)],
    /// Whether transitive resolution is enabled
    pub enable_transitive: bool,
    /// Pre-prepared source versions for resolution (concurrent)
    pub prepared_versions: &'a Arc<DashMap<String, PreparedSourceVersion>>,
    /// Map for pattern aliases (concurrent)
    pub pattern_alias_map: &'a Arc<DashMap<(ResourceType, String), String>>,
    /// Resolution services
    pub services: &'a ResolutionServices<'a>,
    /// Optional progress tracking
    pub progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
}

/// Parameters for processing a transitive dependency specification.
/// This struct reduces cognitive load by grouping related parameters
/// and makes the function signature more maintainable.
struct TransitiveDepProcessingParams<'a> {
    /// The transitive resolution context
    ctx: &'a TransitiveContext<'a>,
    /// The core resolution services
    core: &'a super::ResolutionCore,
    /// The parent dependency
    parent_dep: &'a ResourceDependency,
    /// The resource type of the dependency
    dep_resource_type: ResourceType,
    /// The resource type of the parent
    parent_resource_type: ResourceType,
    /// The name of the parent resource
    parent_name: &'a str,
    /// The dependency specification
    dep_spec: &'a crate::manifest::DependencySpec,
    /// The version resolution service
    version_service: &'a VersionResolutionService,
    /// Pre-prepared source versions for resolution (concurrent)
    prepared_versions: &'a Arc<DashMap<String, PreparedSourceVersion>>,
}

/// Context for processing a single transitive dependency.
///
/// Bundles shared state and context to reduce parameter count from 17 to 1,
/// making the function more maintainable and easier to understand.
/// This context groups related parameters into logical sections:
/// - Input: The specific dependency being processed
/// - Shared: Concurrent state shared across all parallel workers
/// - Resolution: Core resolution services and context
/// - Progress: Optional UI progress tracking
struct TransitiveProcessingContext<'a> {
    /// Input data for this specific dependency
    input: TransitiveInput,

    /// Shared concurrent state for processing
    shared: TransitiveSharedState<'a>,

    /// Resolution context and services
    resolution: TransitiveResolutionContext<'a>,

    /// Optional progress tracking
    progress: Option<Arc<utils::MultiPhaseProgress>>,
}

/// Input data for processing a single dependency.
///
/// Contains the specific dependency information that varies for each
/// function call: name, dependency spec, resource type, and variant hash.
#[derive(Debug, Clone)]
struct TransitiveInput {
    name: String,
    dep: ResourceDependency,
    resource_type: ResourceType,
    variant_hash: String,
}

/// Shared concurrent state used during processing.
///
/// Type alias for the queue entry tuple to reduce type complexity
type QueueEntry = (String, ResourceDependency, Option<ResourceType>, String);

/// Contains all the Arc-wrapped and shared state structures that need
/// to be accessed concurrently by multiple workers processing dependencies in parallel.
/// These are the data structures that were previously passed as individual parameters.
struct TransitiveSharedState<'a> {
    graph: Arc<Mutex<DependencyGraph>>,
    all_deps: Arc<DashMap<DependencyKey, ResourceDependency>>,
    processed: Arc<DashMap<DependencyKey, ()>>,
    queue: Arc<Mutex<Vec<QueueEntry>>>,
    pattern_alias_map: Arc<DashMap<(ResourceType, String), String>>,
    completed_counter: Arc<std::sync::atomic::AtomicUsize>,
    dependency_map: &'a Arc<DashMap<DependencyKey, Vec<String>>>,
    custom_names: &'a Arc<DashMap<DependencyKey, String>>,
    prepared_versions: &'a Arc<DashMap<String, PreparedSourceVersion>>,
}

/// Resolution context and services.
///
/// Bundles the core resolution context, manifest overrides, core services,
/// and resolution services that are needed for processing transitive dependencies.
/// These are the context references that were previously passed as individual parameters.
struct TransitiveResolutionContext<'a> {
    ctx_base: &'a super::types::ResolutionContext<'a>,
    manifest_overrides: &'a super::types::ManifestOverrideIndex,
    core: &'a super::ResolutionCore,
    services: &'a ResolutionServices<'a>,
}

/// Process a single transitive dependency specification.
async fn process_transitive_dependency_spec(
    params: TransitiveDepProcessingParams<'_>,
) -> Result<(ResourceDependency, String)> {
    // Get the canonical path to the parent resource file
    let parent_file_path = ResourceFetchingService::get_canonical_path(
        params.core,
        params.parent_dep,
        params.version_service,
    )
    .await
    .with_context(|| {
        format!("Failed to get parent path for transitive dependencies of '{}'", params.parent_name)
    })?;

    // Resolve the transitive dependency path
    let trans_canonical =
        resolve_transitive_path(&parent_file_path, &params.dep_spec.path, params.parent_name)?;

    // Create the transitive dependency
    let trans_dep = create_transitive_dependency(
        params.ctx,
        params.parent_dep,
        params.dep_resource_type,
        params.parent_resource_type,
        params.parent_name,
        params.dep_spec,
        &parent_file_path,
        &trans_canonical,
        params.prepared_versions,
    )
    .await?;

    // Generate a name for the transitive dependency using source context
    let trans_name = if trans_dep.get_source().is_none() {
        // Local dependency - use manifest directory as source context
        // Use trans_dep.get_path() which is already relative to manifest directory
        // (computed in create_path_only_transitive_dep)
        let manifest_dir = params
            .ctx
            .base
            .manifest
            .manifest_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Manifest directory not available"))?;

        let source_context = crate::resolver::source_context::SourceContext::local(manifest_dir);
        generate_dependency_name(trans_dep.get_path(), &source_context)
    } else {
        // Git dependency - use remote source context
        let source_name = trans_dep
            .get_source()
            .ok_or_else(|| anyhow::anyhow!("Git dependency missing source name"))?;
        let source_context = crate::resolver::source_context::SourceContext::remote(source_name);
        generate_dependency_name(trans_dep.get_path(), &source_context)
    };

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
    } else if is_file_relative_path(dep_path) || !dep_path.contains('/') {
        // File-relative path (starts with ./ or ../) or bare filename
        // For bare filenames, treat as file-relative by resolving from parent directory
        let parent_dir = parent_file_path.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to resolve transitive dependency '{}' for '{}': parent file has no directory",
                dep_path,
                parent_name
            )
        })?;

        let resolved = parent_dir.join(dep_path);
        resolved.canonicalize().map_err(|e| {
            // Create a FileOperationError for canonicalization failures
            let file_error = crate::core::file_error::FileOperationError::new(
                crate::core::file_error::FileOperationContext::new(
                    crate::core::file_error::FileOperation::Canonicalize,
                    &resolved,
                    format!("resolving transitive dependency '{}' for '{}'", dep_path, parent_name),
                    "transitive_resolver::resolve_transitive_path",
                ),
                e,
            );
            anyhow::Error::from(file_error)
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
    _parent_name: &str,
    dep_spec: &crate::manifest::DependencySpec,
    parent_file_path: &Path,
    trans_canonical: &Path,
    prepared_versions: &Arc<DashMap<String, PreparedSourceVersion>>,
) -> Result<ResourceDependency> {
    use super::types::{OverrideKey, compute_dependency_variant_hash, normalize_lookup_path};

    // Create the dependency as before
    let mut dep = if parent_dep.get_source().is_none() {
        create_path_only_transitive_dep(
            ctx,
            parent_dep,
            dep_resource_type,
            parent_resource_type,
            dep_spec,
            trans_canonical,
        )?
    } else {
        create_git_backed_transitive_dep(
            ctx,
            parent_dep,
            dep_resource_type,
            parent_resource_type,
            dep_spec,
            parent_file_path,
            trans_canonical,
            prepared_versions,
        )
        .await?
    };

    // Check for manifest override
    let normalized_path = normalize_lookup_path(dep.get_path());
    let source = dep.get_source().map(std::string::ToString::to_string);

    // Determine tool for the dependency
    let tool = dep
        .get_tool()
        .map(str::to_string)
        .unwrap_or_else(|| ctx.base.manifest.get_default_tool(dep_resource_type));

    let variant_hash = compute_dependency_variant_hash(&dep);

    let override_key = OverrideKey {
        resource_type: dep_resource_type,
        normalized_path: normalized_path.clone(),
        source,
        tool,
        variant_hash,
    };

    // Apply manifest override if found
    if let Some(override_info) = ctx.manifest_overrides.get(&override_key) {
        apply_manifest_override(&mut dep, override_info, &normalized_path);
    }

    Ok(dep)
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
        template_vars: Some(super::lockfile_builder::build_merged_variant_inputs(
            ctx.base.manifest,
            parent_dep,
        )),
    })))
}

/// Create a Git-backed transitive dependency (parent is Git-backed).
#[allow(clippy::too_many_arguments)]
async fn create_git_backed_transitive_dep(
    ctx: &TransitiveContext<'_>,
    parent_dep: &ResourceDependency,
    dep_resource_type: ResourceType,
    parent_resource_type: ResourceType,
    dep_spec: &crate::manifest::DependencySpec,
    parent_file_path: &Path,
    trans_canonical: &Path,
    _prepared_versions: &Arc<DashMap<String, PreparedSourceVersion>>,
) -> Result<ResourceDependency> {
    let source_name = parent_dep
        .get_source()
        .ok_or_else(|| anyhow::anyhow!("Expected source for Git-backed dependency"))?;
    let source_url = ctx
        .base
        .source_manager
        .get_source_url(source_name)
        .ok_or_else(|| anyhow::anyhow!("Source '{source_name}' not found"))?;

    // Get repo-relative path by stripping the appropriate prefix
    let repo_relative = if utils::is_local_path(&source_url) {
        strip_local_source_prefix(&source_url, trans_canonical)?
    } else {
        // For remote Git sources, derive the worktree root from the parent file path
        strip_git_worktree_prefix_from_parent(parent_file_path, trans_canonical)?
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
        template_vars: Some(super::lockfile_builder::build_merged_variant_inputs(
            ctx.base.manifest,
            parent_dep,
        )),
    })))
}

/// Strip the local source prefix from a transitive dependency path.
fn strip_local_source_prefix(source_url: &str, trans_canonical: &Path) -> Result<PathBuf> {
    let source_url_path = PathBuf::from(source_url);
    let source_path = source_url_path.canonicalize().map_err(|e| {
        let file_error = crate::core::file_error::FileOperationError::new(
            crate::core::file_error::FileOperationContext::new(
                crate::core::file_error::FileOperation::Canonicalize,
                &source_url_path,
                "canonicalizing local source path for transitive dependency".to_string(),
                "transitive_resolver::strip_local_source_prefix",
            ),
            e,
        );
        anyhow::Error::from(file_error)
    })?;

    // Check if this is a pattern path (contains glob characters)
    let trans_str = trans_canonical.to_string_lossy();
    let is_pattern = trans_str.contains('*') || trans_str.contains('?') || trans_str.contains('[');

    if is_pattern {
        // For patterns, canonicalize the directory part while keeping the pattern filename intact
        let parent_dir = trans_canonical.parent().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no parent directory: {}", trans_canonical.display())
        })?;
        let filename = trans_canonical.file_name().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no filename: {}", trans_canonical.display())
        })?;

        // Canonicalize the directory part
        let canonical_dir = parent_dir.canonicalize().map_err(|e| {
            let file_error = crate::core::file_error::FileOperationError::new(
                crate::core::file_error::FileOperationContext::new(
                    crate::core::file_error::FileOperation::Canonicalize,
                    parent_dir,
                    "canonicalizing pattern directory for local source".to_string(),
                    "transitive_resolver::strip_local_source_prefix",
                ),
                e,
            );
            anyhow::Error::from(file_error)
        })?;

        // Reconstruct the full path with canonical directory and pattern filename
        let canonical_pattern = canonical_dir.join(filename);

        // Now strip the source prefix
        canonical_pattern
            .strip_prefix(&source_path)
            .with_context(|| {
                format!(
                    "Transitive pattern dep outside parent's source: {} not under {}",
                    canonical_pattern.display(),
                    source_path.display()
                )
            })
            .map(|p| p.to_path_buf())
    } else {
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
}

/// Strip the Git worktree prefix from a transitive dependency path by deriving
/// the worktree root from the parent file path.
fn strip_git_worktree_prefix_from_parent(
    parent_file_path: &Path,
    trans_canonical: &Path,
) -> Result<PathBuf> {
    // Find the worktree root by looking for a directory with the pattern: owner_repo_sha8
    // Start from the parent file and walk up the directory tree
    let worktree_root = parent_file_path
        .ancestors()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| {
                    // Worktree directories have format: owner_repo_sha8 (contains underscores)
                    s.contains('_')
                })
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to find worktree root from parent file: {}",
                parent_file_path.display()
            )
        })?;

    // Canonicalize worktree root to handle symlinks
    let canonical_worktree = worktree_root.canonicalize().map_err(|e| {
        let file_error = crate::core::file_error::FileOperationError::new(
            crate::core::file_error::FileOperationContext::new(
                crate::core::file_error::FileOperation::Canonicalize,
                worktree_root,
                "canonicalizing worktree root for transitive dependency".to_string(),
                "transitive_resolver::strip_git_worktree_prefix_from_parent",
            ),
            e,
        );
        anyhow::Error::from(file_error)
    })?;

    // Check if this is a pattern path (contains glob characters)
    let trans_str = trans_canonical.to_string_lossy();
    let is_pattern = trans_str.contains('*') || trans_str.contains('?') || trans_str.contains('[');

    if is_pattern {
        // For patterns, canonicalize the directory part while keeping the pattern filename intact
        let parent_dir = trans_canonical.parent().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no parent directory: {}", trans_canonical.display())
        })?;
        let filename = trans_canonical.file_name().ok_or_else(|| {
            anyhow::anyhow!("Pattern path has no filename: {}", trans_canonical.display())
        })?;

        // Canonicalize the directory part
        let canonical_dir = parent_dir.canonicalize().map_err(|e| {
            let file_error = crate::core::file_error::FileOperationError::new(
                crate::core::file_error::FileOperationContext::new(
                    crate::core::file_error::FileOperation::Canonicalize,
                    parent_dir,
                    "canonicalizing pattern directory for Git worktree".to_string(),
                    "transitive_resolver::strip_git_worktree_prefix_from_parent",
                ),
                e,
            );
            anyhow::Error::from(file_error)
        })?;

        // Reconstruct the full path with canonical directory and pattern filename
        let canonical_pattern = canonical_dir.join(filename);

        // Now strip the worktree prefix
        canonical_pattern
            .strip_prefix(&canonical_worktree)
            .with_context(|| {
                format!(
                    "Transitive pattern dep outside parent's worktree: {} not under {}",
                    canonical_pattern.display(),
                    canonical_worktree.display()
                )
            })
            .map(|p| p.to_path_buf())
    } else {
        trans_canonical
            .strip_prefix(&canonical_worktree)
            .with_context(|| {
                format!(
                    "Transitive dep outside parent's worktree: {} not under {}",
                    trans_canonical.display(),
                    canonical_worktree.display()
                )
            })
            .map(|p| p.to_path_buf())
    }
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

/// Build the final ordered result from the dependency graph.
fn build_ordered_result(
    all_deps: Arc<DashMap<DependencyKey, ResourceDependency>>,
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
        for entry in all_deps.iter() {
            let (key, dep) = (entry.key(), entry.value());
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
    for entry in all_deps.iter() {
        let (key, dep) = (entry.key(), entry.value());
        if !added_keys.contains(key) && !dep.is_pattern() {
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
pub fn group_key(source: &str, version: &str) -> String {
    format!("{source}::{version}")
}

/// Process a single transitive dependency from the queue.
///
/// This function extracts the core loop body logic into a standalone async function
/// that can be executed in parallel batches for improved performance.
async fn process_single_transitive_dependency<'a>(
    ctx: TransitiveProcessingContext<'a>,
) -> Result<()> {
    let source = ctx.input.dep.get_source().map(std::string::ToString::to_string);
    let tool = ctx.input.dep.get_tool().map(std::string::ToString::to_string);

    let key = (
        ctx.input.resource_type,
        ctx.input.name.clone(),
        source.clone(),
        tool.clone(),
        ctx.input.variant_hash.clone(),
    );

    // Build display name for progress tracking
    let display_name = if source.is_some() {
        if let Some(version) = ctx.input.dep.get_version() {
            format!("{}@{}", ctx.input.name, version)
        } else {
            format!("{}@HEAD", ctx.input.name)
        }
    } else {
        ctx.input.name.clone()
    };
    let progress_key = format!("{}:{}", ctx.input.resource_type, &display_name);

    // Mark as active in progress window
    if let Some(ref pm) = ctx.progress {
        pm.mark_item_active(&display_name, &progress_key);
    }

    tracing::debug!(
        "[TRANSITIVE] Processing: '{}' (type: {:?}, source: {:?})",
        ctx.input.name,
        ctx.input.resource_type,
        source
    );

    // Check if this queue entry is stale (superseded by conflict resolution)
    // CRITICAL: Extract version comparison result before releasing DashMap lock.
    // We must not hold DashMap read locks while acquiring the queue Mutex,
    // as this creates a potential AB-BA deadlock with other parallel tasks.
    let is_stale = ctx
        .shared
        .all_deps
        .get(&key)
        .map(|current_dep| current_dep.get_version() != ctx.input.dep.get_version())
        .unwrap_or(false);

    if is_stale {
        tracing::debug!("[TRANSITIVE] Skipped stale: '{}'", ctx.input.name);
        // DashMap lock is released - safe to acquire queue lock now
        if let Some(ref pm) = ctx.progress {
            let completed =
                ctx.shared.completed_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let total = completed + ctx.shared.queue.lock().unwrap().len();
            pm.mark_item_complete(
                &progress_key,
                Some(&display_name),
                completed,
                total,
                "Scanning dependencies",
            );
        }
        return Ok(());
    }

    if ctx.shared.processed.contains_key(&key) {
        tracing::debug!("[TRANSITIVE] Already processed: '{}'", ctx.input.name);
        if let Some(ref pm) = ctx.progress {
            let completed =
                ctx.shared.completed_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let total = completed + ctx.shared.queue.lock().unwrap().len();
            pm.mark_item_complete(
                &progress_key,
                Some(&display_name),
                completed,
                total,
                "Scanning dependencies",
            );
        }
        return Ok(());
    }

    ctx.shared.processed.insert(key.clone(), ());

    // Handle pattern dependencies by expanding them to concrete files
    if ctx.input.dep.is_pattern() {
        tracing::debug!("[TRANSITIVE] Expanding pattern: '{}'", ctx.input.name);
        match ctx
            .resolution
            .services
            .pattern_service
            .expand_pattern(
                ctx.resolution.core,
                &ctx.input.dep,
                ctx.input.resource_type,
                ctx.shared.prepared_versions.as_ref(),
            )
            .await
        {
            Ok(concrete_deps) => {
                // CRITICAL: Collect items to add to queue BEFORE acquiring queue lock.
                // We must not hold DashMap entry locks while acquiring the queue Mutex,
                // as this creates a potential AB-BA deadlock with other parallel tasks.
                let mut items_to_queue = Vec::new();

                for (concrete_name, concrete_dep) in concrete_deps {
                    ctx.shared.pattern_alias_map.insert(
                        (ctx.input.resource_type, concrete_name.clone()),
                        ctx.input.name.clone(),
                    );

                    let concrete_source =
                        concrete_dep.get_source().map(std::string::ToString::to_string);
                    let concrete_tool =
                        concrete_dep.get_tool().map(std::string::ToString::to_string);
                    let concrete_variant_hash = compute_dependency_variant_hash(&concrete_dep);
                    let concrete_key = (
                        ctx.input.resource_type,
                        concrete_name.clone(),
                        concrete_source,
                        concrete_tool,
                        concrete_variant_hash.clone(),
                    );

                    // Check and insert atomically, but DON'T hold entry lock while queuing
                    if let dashmap::mapref::entry::Entry::Vacant(e) =
                        ctx.shared.all_deps.entry(concrete_key)
                    {
                        e.insert(concrete_dep.clone());
                        // Collect for later queue insertion (after DashMap entry is released)
                        items_to_queue.push((
                            concrete_name,
                            concrete_dep,
                            Some(ctx.input.resource_type),
                            concrete_variant_hash,
                        ));
                    }
                    // DashMap entry lock is released here at end of if-let scope
                }

                // Now safely acquire queue lock without holding any DashMap locks
                if !items_to_queue.is_empty() {
                    let mut queue = ctx.shared.queue.lock().unwrap();
                    queue.extend(items_to_queue);
                }
            }
            Err(e) => {
                anyhow::bail!("Failed to expand pattern '{}': {}", ctx.input.dep.get_path(), e);
            }
        }
        // Pattern expansion complete
        if let Some(ref pm) = ctx.progress {
            let completed =
                ctx.shared.completed_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let total = completed + ctx.shared.queue.lock().unwrap().len();
            pm.mark_item_complete(
                &progress_key,
                Some(&display_name),
                completed,
                total,
                "Scanning dependencies",
            );
        }
        return Ok(());
    }

    // Fetch resource content for metadata extraction
    let content = ResourceFetchingService::fetch_content(
        ctx.resolution.core,
        &ctx.input.dep,
        ctx.resolution.services.version_service,
    )
    .await
    .with_context(|| {
        format!(
            "Failed to fetch resource '{}' ({}) for transitive deps",
            ctx.input.name,
            ctx.input.dep.get_path()
        )
    })?;

    // Note: With single-pass rendering, we no longer need to wrap non-templated
    // content in guards. Dependencies are rendered once with their own context
    // and embedded as-is.

    tracing::debug!(
        "[TRANSITIVE] Fetched content for '{}' ({} bytes)",
        ctx.input.name,
        content.len()
    );

    // Build complete template_vars including global project config for metadata extraction
    // This ensures transitive dependencies can use template variables like {{ agpm.project.language }}
    let variant_inputs_value = super::lockfile_builder::build_merged_variant_inputs(
        ctx.resolution.ctx_base.manifest,
        &ctx.input.dep,
    );
    let variant_inputs = Some(&variant_inputs_value);

    // Extract metadata from the resource with complete variant_inputs
    let path = PathBuf::from(ctx.input.dep.get_path());
    let metadata = MetadataExtractor::extract(
        &path,
        &content,
        variant_inputs,
        ctx.resolution.ctx_base.operation_context.map(|arc| arc.as_ref()),
    )?;

    tracing::debug!(
        "[DEBUG] Extracted metadata for '{}': has_deps={}",
        ctx.input.name,
        metadata.get_dependencies().is_some()
    );

    // Process transitive dependencies if present
    if let Some(deps_map) = metadata.get_dependencies() {
        tracing::debug!(
            "[DEBUG] Found {} dependency type(s) for '{}': {:?}",
            deps_map.len(),
            ctx.input.name,
            deps_map.keys().collect::<Vec<_>>()
        );

        // CRITICAL: Collect items to queue BEFORE acquiring queue lock.
        // We must not hold DashMap entry locks while acquiring the queue Mutex,
        // as this creates a potential AB-BA deadlock with other parallel tasks.
        let mut items_to_queue = Vec::new();

        for (dep_resource_type_str, dep_specs) in deps_map {
            let dep_resource_type: ResourceType =
                dep_resource_type_str.parse().unwrap_or(ResourceType::Snippet);

            for dep_spec in dep_specs {
                // Create a temporary TransitiveContext for this call
                // Note: conflict_detector is not used in parallel code (was removed in Phase 4)
                let mut dummy_conflict_detector = ConflictDetector::new();
                let temp_ctx = super::types::TransitiveContext {
                    base: *ctx.resolution.ctx_base,
                    dependency_map: ctx.shared.dependency_map,
                    transitive_custom_names: ctx.shared.custom_names,
                    conflict_detector: &mut dummy_conflict_detector,
                    manifest_overrides: ctx.resolution.manifest_overrides,
                };

                // Process each transitive dependency spec
                let (trans_dep, trans_name) =
                    process_transitive_dependency_spec(TransitiveDepProcessingParams {
                        ctx: &temp_ctx,
                        core: ctx.resolution.core,
                        parent_dep: &ctx.input.dep,
                        dep_resource_type,
                        parent_resource_type: ctx.input.resource_type,
                        parent_name: &ctx.input.name,
                        dep_spec,
                        version_service: ctx.resolution.services.version_service,
                        prepared_versions: ctx.shared.prepared_versions,
                    })
                    .await?;

                let trans_source = trans_dep.get_source().map(std::string::ToString::to_string);
                let trans_tool = trans_dep.get_tool().map(std::string::ToString::to_string);
                let trans_variant_hash = compute_dependency_variant_hash(&trans_dep);

                // Store custom name if provided
                if let Some(custom_name) = &dep_spec.name {
                    let trans_key = (
                        dep_resource_type,
                        trans_name.clone(),
                        trans_source.clone(),
                        trans_tool.clone(),
                        trans_variant_hash.clone(),
                    );
                    ctx.shared.custom_names.insert(trans_key, custom_name.clone());
                    tracing::debug!(
                        "Storing custom name '{}' for transitive dep '{}'",
                        custom_name,
                        trans_name
                    );
                }

                // Add to dependency graph
                let from_node = DependencyNode::with_source(
                    ctx.input.resource_type,
                    &ctx.input.name,
                    source.clone(),
                );
                let to_node = DependencyNode::with_source(
                    dep_resource_type,
                    &trans_name,
                    trans_source.clone(),
                );
                ctx.shared.graph.lock().unwrap().add_dependency(from_node, to_node);

                // Track in dependency map
                let from_key = (
                    ctx.input.resource_type,
                    ctx.input.name.clone(),
                    source.clone(),
                    tool.clone(),
                    ctx.input.variant_hash.clone(),
                );
                let dep_ref =
                    LockfileDependencyRef::local(dep_resource_type, trans_name.clone(), None)
                        .to_string();
                tracing::debug!(
                    "[DEBUG] Adding to dependency_map: parent='{}' (type={:?}, source={:?}, tool={:?}, hash={}), child='{}' (type={:?})",
                    ctx.input.name,
                    ctx.input.resource_type,
                    source,
                    tool,
                    &ctx.input.variant_hash[..8],
                    dep_ref,
                    dep_resource_type
                );
                ctx.shared.dependency_map.entry(from_key).or_default().push(dep_ref);

                // DON'T add to conflict detector yet - we'll do it after SHA resolution
                // (Removed: add_to_conflict_detector call)

                // Check for version conflicts
                let trans_key = (
                    dep_resource_type,
                    trans_name.clone(),
                    trans_source.clone(),
                    trans_tool.clone(),
                    trans_variant_hash.clone(),
                );

                tracing::debug!(
                    "[TRANSITIVE] Found transitive dep '{}' (type: {:?}, tool: {:?}, parent: {})",
                    trans_name,
                    dep_resource_type,
                    trans_tool,
                    ctx.input.name
                );

                // Check if we already have this dependency - DON'T hold entry lock while queuing
                if let dashmap::mapref::entry::Entry::Vacant(e) =
                    ctx.shared.all_deps.entry(trans_key)
                {
                    // No conflict, add the dependency
                    tracing::debug!(
                        "Adding transitive dep '{}' (parent: {})",
                        trans_name,
                        ctx.input.name
                    );
                    e.insert(trans_dep.clone());
                    // Collect for later queue insertion (after DashMap entry is released)
                    items_to_queue.push((
                        trans_name,
                        trans_dep,
                        Some(dep_resource_type),
                        trans_variant_hash,
                    ));
                } else {
                    // Dependency already exists - conflict detector will handle version requirement conflicts
                    tracing::debug!(
                        "[TRANSITIVE] Skipping duplicate transitive dep '{}' (already processed)",
                        trans_name
                    );
                }
                // DashMap entry lock is released here at end of if-let scope
            }
        }

        // Now safely acquire queue lock without holding any DashMap locks
        if !items_to_queue.is_empty() {
            let mut queue = ctx.shared.queue.lock().unwrap();
            queue.extend(items_to_queue);
        }
    }

    // Mark item as complete in progress window
    if let Some(ref pm) = ctx.progress {
        let completed =
            ctx.shared.completed_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let total = completed + ctx.shared.queue.lock().unwrap().len();
        pm.mark_item_complete(
            &progress_key,
            Some(&display_name),
            completed,
            total,
            "Scanning dependencies",
        );
    }

    Ok(())
}

/// Service-based wrapper for transitive dependency resolution.
///
/// This provides a simpler API for internal use that takes service references
/// directly instead of requiring closure-based dependency injection.
pub async fn resolve_with_services(
    params: TransitiveResolutionParams<'_>,
) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
    let TransitiveResolutionParams {
        ctx,
        core,
        base_deps,
        enable_transitive,
        prepared_versions,
        pattern_alias_map,
        services,
        progress,
    } = params;
    // Clear state from any previous resolution
    ctx.dependency_map.clear();

    if !enable_transitive {
        return Ok(base_deps.to_vec());
    }

    let graph = Arc::new(Mutex::new(DependencyGraph::new()));
    let all_deps: Arc<DashMap<DependencyKey, ResourceDependency>> = Arc::new(DashMap::new());
    let processed: Arc<DashMap<DependencyKey, ()>> = Arc::new(DashMap::new()); // Simulates HashSet

    // Type alias to reduce complexity
    type QueueItem = (String, ResourceDependency, Option<ResourceType>, String);
    #[allow(clippy::type_complexity)]
    let queue: Arc<Mutex<Vec<QueueItem>>> = Arc::new(Mutex::new(Vec::new()));

    // Add initial dependencies to queue with their threaded types
    for (name, dep, resource_type) in base_deps {
        let source = dep.get_source().map(std::string::ToString::to_string);
        let tool = dep.get_tool().map(std::string::ToString::to_string);

        // Compute variant_hash from MERGED variant_inputs (dep + global config)
        // This ensures consistency with how LockedResource computes its hash
        let merged_variant_inputs =
            super::lockfile_builder::build_merged_variant_inputs(ctx.base.manifest, dep);
        let variant_hash = crate::utils::compute_variant_inputs_hash(&merged_variant_inputs)
            .unwrap_or_else(|_| crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string());

        tracing::debug!(
            "[DEBUG] Adding base dep to queue: '{}' (type: {:?}, source: {:?}, tool: {:?}, is_local: {})",
            name,
            resource_type,
            source,
            tool,
            dep.is_local()
        );
        // Store pre-computed hash in queue to avoid duplicate computation
        queue.lock().unwrap().push((
            name.clone(),
            dep.clone(),
            Some(*resource_type),
            variant_hash.clone(),
        ));
        all_deps.insert((*resource_type, name.clone(), source, tool, variant_hash), dep.clone());
    }

    // Track progress: total items to process = base_deps + discovered transitives
    let completed_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Calculate concurrency based on CPU cores
    let cores = std::thread::available_parallelism().map(std::num::NonZero::get).unwrap_or(4);
    let max_concurrent = std::cmp::max(10, cores * 2);

    // Extract ctx references for parallel access (conflict_detector needs &mut, so we keep it outside)
    let ctx_dependency_map = ctx.dependency_map;
    let ctx_custom_names = ctx.transitive_custom_names;
    let ctx_base = &ctx.base;
    let ctx_manifest_overrides = ctx.manifest_overrides;

    // Process queue in parallel batches to discover transitive dependencies
    loop {
        // Extract batch from queue (drain from end, same as serial pop order)
        let batch: Vec<QueueEntry> = {
            let mut q = queue.lock().unwrap();
            let queue_len = q.len();
            let batch_size = std::cmp::min(max_concurrent, queue_len);
            if batch_size == 0 {
                break; // Queue empty
            }
            // Drain from end and reverse to maintain LIFO ordering like serial version
            let mut batch_vec = q.drain(queue_len.saturating_sub(batch_size)..).collect::<Vec<_>>();
            batch_vec.reverse(); // Reverse to process in same order as serial (last added first)
            batch_vec
        };

        // Process batch in parallel
        let batch_futures: Vec<_> = batch
            .into_iter()
            .map(|(name, dep, resource_type, variant_hash)| {
                // Clone Arc refs for concurrent access
                let graph_clone = Arc::clone(&graph);
                let all_deps_clone = Arc::clone(&all_deps);
                let processed_clone = Arc::clone(&processed);
                let queue_clone = Arc::clone(&queue);
                let pattern_alias_map_clone = Arc::clone(pattern_alias_map);
                let progress_clone = progress.clone();
                let counter_clone = Arc::clone(&completed_counter);
                let prepared_versions_clone = Arc::clone(prepared_versions);
                let dependency_map_clone = ctx_dependency_map;
                let custom_names_clone = ctx_custom_names;
                let manifest_overrides_clone = ctx_manifest_overrides;

                async move {
                    let resource_type = resource_type
                        .expect("resource_type should always be threaded through queue");

                    // Construct the processing context
                    let ctx = TransitiveProcessingContext {
                        input: TransitiveInput {
                            name,
                            dep,
                            resource_type,
                            variant_hash,
                        },
                        shared: TransitiveSharedState {
                            graph: graph_clone,
                            all_deps: all_deps_clone,
                            processed: processed_clone,
                            queue: queue_clone,
                            pattern_alias_map: pattern_alias_map_clone,
                            completed_counter: counter_clone,
                            dependency_map: dependency_map_clone,
                            custom_names: custom_names_clone,
                            prepared_versions: &prepared_versions_clone,
                        },
                        resolution: TransitiveResolutionContext {
                            ctx_base,
                            manifest_overrides: manifest_overrides_clone,
                            core,
                            services,
                        },
                        progress: progress_clone,
                    };

                    process_single_transitive_dependency(ctx).await
                }
            })
            .collect();

        // Execute batch concurrently
        let results = join_all(batch_futures).await;

        // Check for errors
        for result in results {
            result?;
        }
    }

    // Check for circular dependencies
    graph.lock().unwrap().detect_cycles()?;

    // Get topological order
    let ordered_nodes = graph.lock().unwrap().topological_order()?;

    // Build result with topologically ordered dependencies
    build_ordered_result(all_deps, ordered_nodes)
}
