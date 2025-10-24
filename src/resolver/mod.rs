//! Dependency resolution and conflict detection for AGPM.
//!
//! This module implements the core dependency resolution algorithm that transforms
//! manifest dependencies into locked versions. It handles version constraint solving,
//! conflict detection, transitive dependency resolution,
//! parallel source synchronization, and relative path preservation during installation.
//!
//! # Service-Based Architecture
//!
//! This resolver has been refactored to use a service-based architecture:
//! - **ResolutionCore**: Shared immutable state
//! - **VersionResolutionService**: Git operations and version resolution
//! - **PatternExpansionService**: Glob pattern expansion
//! - **TransitiveDependencyService**: Transitive dependency resolution
//! - **ConflictService**: Conflict detection
//! - **ResourceFetchingService**: Resource content fetching

// Declare service modules
pub mod conflict_service;
pub mod dependency_graph;
pub mod lockfile_builder;
pub mod path_resolver;
pub mod pattern_expander;
pub mod resource_service;
pub mod transitive_resolver;
pub mod types;
pub mod version_resolver;

// Re-export utility functions for compatibility
pub use path_resolver::{extract_meaningful_path, is_file_relative_path, normalize_bare_filename};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::cache::Cache;
use crate::core::{OperationContext, ResourceType};
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency};
use crate::metadata::MetadataExtractor;
use crate::source::SourceManager;

// Re-export services for external use
pub use conflict_service::ConflictService;
pub use pattern_expander::PatternExpansionService;
pub use resource_service::ResourceFetchingService;
pub use transitive_resolver::resolve_transitive_dependencies;
pub use types::ResolutionCore;
pub use version_resolver::{
    VersionResolutionService, VersionResolver as VersionResolverExport, find_best_matching_tag,
    is_version_constraint, parse_tags_to_versions,
};

// Legacy re-exports for compatibility
pub use dependency_graph::{DependencyGraph, DependencyNode};
pub use lockfile_builder::LockfileBuilder;
pub use pattern_expander::{expand_pattern_to_concrete_deps, generate_dependency_name};
pub use types::{DependencyKey, ResolutionContext, TransitiveContext};
pub use version_resolver::{PreparedSourceVersion, VersionResolver, WorktreeManager};

/// Main dependency resolver with service-based architecture.
///
/// This orchestrates multiple specialized services to handle different aspects
/// of the dependency resolution process while maintaining compatibility
/// with existing interfaces.
#[allow(dead_code)] // Some fields not yet used in service-based refactoring
pub struct DependencyResolver {
    /// Core shared context with immutable state
    core: ResolutionCore,

    /// Version resolution and Git operations service
    version_service: VersionResolutionService,

    /// Pattern expansion service for glob dependencies
    pattern_service: PatternExpansionService,

    /// Conflict detection service
    conflict_service: ConflictService,

    /// Resource fetching and metadata service
    resource_service: ResourceFetchingService,

    /// Conflict detector for version conflicts
    conflict_detector: crate::version::conflict::ConflictDetector,

    /// Dependency tracking state
    dependency_map: HashMap<DependencyKey, Vec<String>>,

    /// Pattern alias tracking for expanded patterns
    pattern_alias_map: HashMap<(ResourceType, String), String>,

    /// Transitive dependency custom names
    transitive_custom_names: HashMap<DependencyKey, String>,

    /// Track if sources have been pre-synced to avoid duplicate work
    sources_pre_synced: bool,
}

impl DependencyResolver {
    /// Create a new dependency resolver.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    ///
    /// # Errors
    ///
    /// Returns an error if source manager cannot be created
    pub async fn new(manifest: Manifest, cache: Cache) -> Result<Self> {
        Self::new_with_context(manifest, cache, None).await
    }

    /// Create a new dependency resolver with operation context.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    /// * `operation_context` - Optional context for warning deduplication
    ///
    /// # Errors
    ///
    /// Returns an error if source manager cannot be created
    pub async fn new_with_context(
        manifest: Manifest,
        cache: Cache,
        operation_context: Option<Arc<OperationContext>>,
    ) -> Result<Self> {
        // Create source manager from manifest
        let source_manager = SourceManager::from_manifest(&manifest)?;

        // Create resolution core with shared state
        let core = ResolutionCore::new(manifest, cache, source_manager, operation_context);

        // Initialize all services
        let version_service = VersionResolutionService::new(core.cache().clone());
        let pattern_service = PatternExpansionService::new();
        let conflict_service = ConflictService::new();
        let resource_service = ResourceFetchingService::new();

        Ok(Self {
            core,
            version_service,
            pattern_service,
            conflict_service,
            resource_service,
            conflict_detector: crate::version::conflict::ConflictDetector::new(),
            dependency_map: HashMap::new(),
            pattern_alias_map: HashMap::new(),
            transitive_custom_names: HashMap::new(),
            sources_pre_synced: false,
        })
    }

    /// Create a new resolver with global configuration support.
    ///
    /// This loads both manifest sources and global sources from `~/.agpm/config.toml`.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    ///
    /// # Errors
    ///
    /// Returns an error if global configuration cannot be loaded
    pub async fn new_with_global(manifest: Manifest, cache: Cache) -> Result<Self> {
        Self::new_with_global_context(manifest, cache, None).await
    }

    /// Creates a new dependency resolver with custom cache directory.
    ///
    /// # Arguments
    ///
    /// * `cache` - Cache for Git operations and worktrees
    ///
    /// # Errors
    ///
    /// Returns an error if source manager cannot be created
    pub async fn with_cache(manifest: Manifest, cache: Cache) -> Result<Self> {
        Self::new_with_context(manifest, cache, None).await
    }

    /// Create a new resolver with global configuration and operation context.
    ///
    /// This loads both manifest sources and global sources from `~/.agpm/config.toml`.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    /// * `operation_context` - Optional context for warning deduplication
    ///
    /// # Errors
    ///
    /// Returns an error if global configuration cannot be loaded
    pub async fn new_with_global_context(
        manifest: Manifest,
        cache: Cache,
        _operation_context: Option<Arc<OperationContext>>,
    ) -> Result<Self> {
        let source_manager = SourceManager::from_manifest_with_global(&manifest).await?;

        let core = ResolutionCore::new(manifest, cache, source_manager, _operation_context);

        let version_service = VersionResolutionService::new(core.cache().clone());
        let pattern_service = PatternExpansionService::new();
        let conflict_service = ConflictService::new();
        let resource_service = ResourceFetchingService::new();

        Ok(Self {
            core,
            version_service,
            pattern_service,
            conflict_service,
            resource_service,
            conflict_detector: crate::version::conflict::ConflictDetector::new(),
            dependency_map: HashMap::new(),
            pattern_alias_map: HashMap::new(),
            transitive_custom_names: HashMap::new(),
            sources_pre_synced: false,
        })
    }

    /// Get a reference to the resolution core.
    pub fn core(&self) -> &ResolutionCore {
        &self.core
    }

    /// Resolve all dependencies and generate a complete lockfile.
    ///
    /// This is the main resolution method.
    ///
    /// # Errors
    ///
    /// Returns an error if any step of resolution fails
    pub async fn resolve(&mut self) -> Result<LockFile> {
        self.resolve_with_options(true).await
    }

    /// Resolve dependencies with transitive resolution option.
    ///
    /// # Arguments
    ///
    /// * `enable_transitive` - Whether to resolve transitive dependencies
    ///
    /// # Errors
    ///
    /// Returns an error if resolution fails
    pub async fn resolve_with_options(&mut self, enable_transitive: bool) -> Result<LockFile> {
        let mut lockfile = LockFile::new();

        // Add sources to lockfile
        for (name, url) in &self.core.manifest().sources {
            lockfile.add_source(name.clone(), url.clone(), String::new());
        }

        // Phase 1: Extract dependencies from manifest with types
        let base_deps: Vec<(String, ResourceDependency, ResourceType)> = self
            .core
            .manifest()
            .all_dependencies_with_types()
            .into_iter()
            .map(|(name, dep, resource_type)| (name.to_string(), dep.into_owned(), resource_type))
            .collect();

        // Add direct dependencies to conflict detector
        for (name, dep, _) in &base_deps {
            self.add_to_conflict_detector(name, dep, "manifest");
        }

        // Phase 2: Pre-sync all sources if not already done
        if !self.sources_pre_synced {
            let deps_for_sync: Vec<(String, ResourceDependency)> =
                base_deps.iter().map(|(name, dep, _)| (name.clone(), dep.clone())).collect();
            self.version_service.pre_sync_sources(&self.core, &deps_for_sync).await?;
            self.sources_pre_synced = true;
        }

        // Phase 3: Resolve transitive dependencies
        let all_deps = if enable_transitive {
            self.resolve_transitive_dependencies(&base_deps).await?
        } else {
            base_deps.clone()
        };

        // Phase 4: Resolve each dependency to a locked resource
        for (name, dep, resource_type) in &all_deps {
            if dep.is_pattern() {
                // Pattern dependencies resolve to multiple resources
                let entries = self.resolve_pattern_dependency(name, dep, *resource_type).await?;

                // Add each resolved entry with deduplication
                for entry in entries {
                    let entry_name = entry.name.clone();
                    self.add_or_update_lockfile_entry(&mut lockfile, &entry_name, entry);
                }
            } else {
                // Regular single dependency
                let entry = self.resolve_dependency(name, dep, *resource_type).await?;
                self.add_or_update_lockfile_entry(&mut lockfile, name, entry);
            }
        }

        // Phase 5: Detect conflicts
        let conflicts = self.conflict_detector.detect_conflicts();
        if !conflicts.is_empty() {
            let mut error_msg = String::from("Version conflicts detected:\n\n");
            for conflict in &conflicts {
                error_msg.push_str(&format!("{conflict}\n"));
            }
            return Err(anyhow::anyhow!("{}", error_msg));
        }

        // Phase 6: Post-process dependencies and detect target conflicts
        self.add_version_to_dependencies(&mut lockfile)?;
        self.detect_target_conflicts(&lockfile)?;

        Ok(lockfile)
    }

    /// Pre-sync sources for the given dependencies.
    ///
    /// This performs Git operations to ensure all required sources are available
    /// before the main resolution process begins.
    ///
    /// # Arguments
    ///
    /// * `deps` - List of (name, dependency) pairs to sync sources for
    ///
    /// # Errors
    ///
    /// Returns an error if source synchronization fails
    pub async fn pre_sync_sources(&mut self, deps: &[(String, ResourceDependency)]) -> Result<()> {
        // Pre-sync all sources using version service
        self.version_service.pre_sync_sources(&self.core, deps).await?;
        self.sources_pre_synced = true;
        Ok(())
    }

    /// Update dependencies with existing lockfile and specific dependencies to update.
    ///
    /// # Arguments
    ///
    /// * `existing` - Existing lockfile to update
    /// * `deps_to_update` - Optional specific dependency names to update (None = all)
    ///
    /// # Errors
    ///
    /// Returns an error if update process fails
    pub async fn update(
        &mut self,
        existing: &LockFile,
        deps_to_update: Option<Vec<String>>,
    ) -> Result<LockFile> {
        // For now, just resolve all dependencies
        // TODO: Implement proper incremental update logic using deps_to_update names
        let _existing = existing; // Suppress unused warning for now
        let _deps_to_update = deps_to_update; // Suppress unused warning for now
        self.resolve_with_options(true).await
    }

    /// Get available versions for a repository.
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the Git repository
    ///
    /// # Returns
    ///
    /// List of available version strings (tags and branches)
    pub async fn get_available_versions(&self, repo_path: &Path) -> Result<Vec<String>> {
        VersionResolutionService::get_available_versions(&self.core, repo_path).await
    }

    /// Verify that existing lockfile is still valid.
    ///
    /// # Arguments
    ///
    /// * `_lockfile` - Existing lockfile to verify
    ///
    /// # Errors
    ///
    /// Returns an error if verification fails
    pub async fn verify(&self, _lockfile: &LockFile) -> Result<()> {
        // TODO: Implement verification logic using services
        Ok(())
    }

    /// Get current operation context if available.
    pub fn operation_context(&self) -> Option<&Arc<OperationContext>> {
        self.core.operation_context()
    }

    /// Set the operation context for warning deduplication.
    ///
    /// # Arguments
    ///
    /// * `context` - The operation context to use
    pub fn set_operation_context(&mut self, context: Arc<OperationContext>) {
        self.core.operation_context = Some(context);
    }
}

// Private helper methods
impl DependencyResolver {
    /// Resolve transitive dependencies starting from base dependencies.
    ///
    /// Discovers dependencies declared in resource files, expands patterns,
    /// builds dependency graph with cycle detection, and returns all dependencies
    /// in topological order.
    async fn resolve_transitive_dependencies(
        &mut self,
        base_deps: &[(String, ResourceDependency, ResourceType)],
    ) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
        use crate::manifest::{ProjectConfig, json_value_to_toml};
        use crate::resolver::dependency_graph::{DependencyGraph, DependencyNode};
        use crate::templating::deep_merge_json;
        use std::collections::{HashMap, HashSet};

        // Clear state from any previous resolution
        self.dependency_map.clear();

        let mut graph = DependencyGraph::new();
        let mut all_deps: HashMap<DependencyKey, ResourceDependency> = HashMap::new();
        let mut processed: HashSet<DependencyKey> = HashSet::new();
        let mut queue: Vec<(String, ResourceDependency, Option<ResourceType>)> = Vec::new();

        // Add initial dependencies to queue with their resource types
        for (name, dep, resource_type) in base_deps {
            let source = dep.get_source().map(ToString::to_string);
            let tool = dep.get_tool().map(ToString::to_string);

            tracing::debug!(
                "[TRANSITIVE] Adding base dep '{}' (type: {:?}, tool: {:?})",
                name,
                resource_type,
                tool
            );

            queue.push((name.clone(), dep.clone(), Some(*resource_type)));
            all_deps.insert((*resource_type, name.clone(), source, tool), dep.clone());
        }

        // Process queue to discover transitive dependencies
        while let Some((name, dep, resource_type)) = queue.pop() {
            let source = dep.get_source().map(ToString::to_string);
            let tool = dep.get_tool().map(ToString::to_string);
            let resource_type = resource_type.expect("resource_type should always be threaded");
            let key = (resource_type, name.clone(), source.clone(), tool.clone());

            tracing::debug!(
                "[TRANSITIVE] Processing: '{}' (type: {:?}, source: {:?})",
                name,
                resource_type,
                source
            );

            // Check if this entry is stale (superseded by conflict resolution)
            if let Some(current_dep) = all_deps.get(&key) {
                if current_dep.get_version() != dep.get_version() {
                    tracing::debug!("[TRANSITIVE] Skipped stale: '{}'", name);
                    continue;
                }
            }

            if processed.contains(&key) {
                tracing::debug!("[TRANSITIVE] Already processed: '{}'", name);
                continue;
            }

            processed.insert(key.clone());

            // Handle pattern dependencies by expanding them
            if dep.is_pattern() {
                tracing::debug!("[TRANSITIVE] Expanding pattern: '{}'", name);
                match self.expand_pattern_to_concrete_deps(&dep, resource_type).await {
                    Ok(concrete_deps) => {
                        for (concrete_name, concrete_dep) in concrete_deps {
                            // Record pattern alias mapping
                            self.pattern_alias_map
                                .insert((resource_type, concrete_name.clone()), name.clone());

                            let concrete_source =
                                concrete_dep.get_source().map(ToString::to_string);
                            let concrete_tool = concrete_dep.get_tool().map(ToString::to_string);
                            let concrete_key = (
                                resource_type,
                                concrete_name.clone(),
                                concrete_source,
                                concrete_tool,
                            );

                            // Only add if not already processed
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                all_deps.entry(concrete_key)
                            {
                                e.insert(concrete_dep.clone());
                                queue.push((concrete_name, concrete_dep, Some(resource_type)));
                            }
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("Failed to expand pattern '{}': {}", dep.get_path(), e);
                    }
                }
                continue;
            }

            // Fetch resource content for metadata extraction
            // Note: fetch_resource_content will prepare versions on-demand if needed
            let content = self.fetch_resource_content(&name, &dep).await.with_context(|| {
                format!("Failed to fetch resource '{}' for transitive deps", name)
            })?;

            tracing::debug!(
                "[TRANSITIVE] Fetched content for '{}' ({} bytes)",
                name,
                content.len()
            );

            // Build project config with template_vars merge
            let project_config = if let Some(template_vars) = dep.get_template_vars() {
                let project_overrides = template_vars
                    .get("project")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                let global_json = self
                    .core
                    .manifest()
                    .project
                    .as_ref()
                    .map(|p| p.to_json_value())
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                let merged_json = deep_merge_json(global_json, &project_overrides);

                let mut config_map = toml::map::Map::new();
                if let Some(merged_obj) = merged_json.as_object() {
                    for (key, value) in merged_obj {
                        config_map.insert(key.clone(), json_value_to_toml(value));
                    }
                }

                Some(ProjectConfig::from(config_map))
            } else {
                self.core.manifest().project.clone()
            };

            // Extract metadata from resource content
            let path = PathBuf::from(dep.get_path());
            let metadata = MetadataExtractor::extract(
                &path,
                &content,
                project_config.as_ref(),
                self.core.operation_context().as_ref().map(|arc| arc.as_ref()),
            )?;

            // Process transitive dependencies if present
            if let Some(deps_map) = metadata.get_dependencies() {
                tracing::debug!("Found transitive deps for: {}", name);

                for (dep_resource_type_str, dep_specs) in deps_map {
                    let dep_resource_type: ResourceType =
                        dep_resource_type_str.parse().unwrap_or(ResourceType::Snippet);

                    for dep_spec in dep_specs {
                        // Process each transitive dependency
                        let (trans_dep, trans_name) = self
                            .process_transitive_dependency_spec(
                                &dep,
                                dep_resource_type,
                                resource_type,
                                &name,
                                dep_spec,
                            )
                            .await?;

                        let trans_source = trans_dep.get_source().map(ToString::to_string);
                        let trans_tool = trans_dep.get_tool().map(ToString::to_string);

                        // Store custom name if provided
                        if let Some(custom_name) = &dep_spec.name {
                            let trans_key = (
                                dep_resource_type,
                                trans_name.clone(),
                                trans_source.clone(),
                                trans_tool.clone(),
                            );
                            self.transitive_custom_names.insert(trans_key, custom_name.clone());
                            tracing::debug!(
                                "Storing custom name '{}' for transitive dep '{}'",
                                custom_name,
                                trans_name
                            );
                        }

                        // Add to dependency graph
                        let from_node =
                            DependencyNode::with_source(resource_type, &name, source.clone());
                        let to_node = DependencyNode::with_source(
                            dep_resource_type,
                            &trans_name,
                            trans_source.clone(),
                        );
                        graph.add_dependency(from_node, to_node);

                        // Track in dependency map
                        let from_key = (resource_type, name.clone(), source.clone(), tool.clone());
                        let dep_ref = format!("{}/{}", dep_resource_type, trans_name);
                        self.dependency_map.entry(from_key).or_default().push(dep_ref);

                        // Add to conflict detector
                        self.add_to_conflict_detector(&trans_name, &trans_dep, &name);

                        // Check for version conflicts
                        let trans_key = (
                            dep_resource_type,
                            trans_name.clone(),
                            trans_source.clone(),
                            trans_tool.clone(),
                        );

                        tracing::debug!(
                            "[TRANSITIVE] Found transitive dep '{}' (type: {:?}, tool: {:?}, parent: {})",
                            trans_name,
                            dep_resource_type,
                            trans_tool,
                            name
                        );

                        if let Some(existing_dep) = all_deps.get(&trans_key) {
                            // Resolve version conflict
                            let resolved_dep = self
                                .resolve_version_conflict(
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
                                queue.push((trans_name, resolved_dep, Some(dep_resource_type)));
                            }
                        } else {
                            // No conflict, add the dependency
                            tracing::debug!(
                                "Adding transitive dep '{}' (parent: {})",
                                trans_name,
                                name
                            );
                            all_deps.insert(trans_key, trans_dep.clone());
                            queue.push((trans_name, trans_dep, Some(dep_resource_type)));
                        }
                    }
                }
            }
        }

        // Check for circular dependencies
        graph.detect_cycles()?;

        // Get topological order
        let ordered_nodes = graph.topological_order()?;

        // Build result with topologically ordered dependencies
        let mut result = Vec::new();
        let mut added_keys = HashSet::new();

        tracing::debug!(
            "Transitive resolution: {} nodes in order, {} total deps",
            ordered_nodes.len(),
            all_deps.len()
        );

        // Add dependencies in topological order
        // Note: We need to add ALL dependencies matching (resource_type, name, source),
        // even if they have different tools, since multiple tools can use the same resource
        for node in ordered_nodes {
            for (key, dep) in &all_deps {
                if key.0 == node.resource_type && key.1 == node.name && key.2 == node.source {
                    result.push((node.name.clone(), dep.clone(), node.resource_type));
                    added_keys.insert(key.clone());
                    // Don't break - there might be multiple entries with different tools
                }
            }
        }

        // Add remaining dependencies that weren't in the graph
        for (key, dep) in all_deps {
            if !added_keys.contains(&key) && !dep.is_pattern() {
                result.push((key.1.clone(), dep.clone(), key.0));
            }
        }

        tracing::debug!("Transitive resolution returning {} dependencies", result.len());

        Ok(result)
    }

    /// Process a single transitive dependency specification.
    async fn process_transitive_dependency_spec(
        &mut self,
        parent_dep: &ResourceDependency,
        dep_resource_type: ResourceType,
        parent_resource_type: ResourceType,
        parent_name: &str,
        dep_spec: &crate::manifest::DependencySpec,
    ) -> Result<(ResourceDependency, String)> {
        use crate::manifest::DetailedDependency;
        use crate::resolver::pattern_expander::generate_dependency_name;

        // Get canonical path to parent resource
        let parent_file_path = self.get_canonical_path_for_dependency(parent_dep).await?;

        // Resolve transitive dependency path
        let trans_canonical =
            self.resolve_transitive_path(&parent_file_path, &dep_spec.path, parent_name)?;

        // Check if this is a pattern dependency
        let is_pattern = dep_spec.path.contains('*')
            || dep_spec.path.contains('?')
            || dep_spec.path.contains('[');

        // Create the transitive dependency
        let trans_dep = if parent_dep.get_source().is_none() {
            // Path-only transitive dependency
            let manifest_dir = self
                .core
                .manifest()
                .manifest_dir
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Manifest directory not available"))?;

            let dep_path_str = match manifest_dir.canonicalize() {
                Ok(canonical_manifest) => {
                    crate::utils::compute_relative_path(&canonical_manifest, &trans_canonical)
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Could not canonicalize manifest directory {}: {}",
                        manifest_dir.display(),
                        e
                    );
                    crate::utils::compute_relative_path(manifest_dir, &trans_canonical)
                }
            };

            let trans_tool = self.determine_transitive_tool(
                parent_dep,
                dep_spec,
                parent_resource_type,
                dep_resource_type,
            );

            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: None,
                path: crate::utils::normalize_path_for_storage(dep_path_str),
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
                template_vars: Some(self.build_merged_template_vars(parent_dep)),
            }))
        } else {
            // Git-backed transitive dependency
            let source_name = parent_dep
                .get_source()
                .ok_or_else(|| anyhow::anyhow!("Expected source for Git-backed dependency"))?;
            let version = parent_dep.get_version().unwrap_or("main");
            let source_url = self
                .core
                .source_manager()
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

            // Get repo-relative path
            let repo_relative = if crate::utils::is_local_path(&source_url) {
                let source_path = PathBuf::from(&source_url).canonicalize()?;
                if is_pattern {
                    // For patterns, compute relative path without strict validation
                    // since patterns aren't canonicalized
                    PathBuf::from(crate::utils::compute_relative_path(
                        &source_path,
                        &trans_canonical,
                    ))
                } else {
                    trans_canonical
                        .strip_prefix(&source_path)
                        .with_context(|| {
                            format!(
                                "Transitive dep outside parent's source directory: {} not under {}",
                                trans_canonical.display(),
                                source_path.display()
                            )
                        })?
                        .to_path_buf()
                }
            } else {
                let group_key = format!("{}::{}", source_name, version);
                let prepared =
                    self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
                        anyhow::anyhow!("Parent version not resolved for {}", source_name)
                    })?;

                let worktree_path = PathBuf::from(&prepared.worktree_path);

                if is_pattern {
                    // For patterns, compute relative path directly since pattern paths aren't canonicalized
                    PathBuf::from(crate::utils::compute_relative_path(
                        &worktree_path,
                        &trans_canonical,
                    ))
                } else {
                    let canonical_worktree = worktree_path.canonicalize().with_context(|| {
                        format!("Failed to canonicalize worktree path: {}", worktree_path.display())
                    })?;

                    trans_canonical
                        .strip_prefix(&canonical_worktree)
                        .with_context(|| {
                            format!(
                                "Transitive dep outside parent's worktree: {} not under {}",
                                trans_canonical.display(),
                                canonical_worktree.display()
                            )
                        })?
                        .to_path_buf()
                }
            };

            let trans_tool = self.determine_transitive_tool(
                parent_dep,
                dep_spec,
                parent_resource_type,
                dep_resource_type,
            );

            ResourceDependency::Detailed(Box::new(DetailedDependency {
                source: Some(source_name.to_string()),
                path: crate::utils::normalize_path_for_storage(
                    repo_relative.to_string_lossy().to_string(),
                ),
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
                template_vars: Some(self.build_merged_template_vars(parent_dep)),
            }))
        };

        let trans_name = generate_dependency_name(trans_dep.get_path());

        Ok((trans_dep, trans_name))
    }

    /// Resolve a transitive dependency path relative to its parent.
    fn resolve_transitive_path(
        &self,
        parent_file_path: &Path,
        dep_path: &str,
        parent_name: &str,
    ) -> Result<PathBuf> {
        let is_pattern = dep_path.contains('*') || dep_path.contains('?') || dep_path.contains('[');

        if is_pattern {
            // For patterns, normalize but don't canonicalize
            let parent_dir = parent_file_path.parent().ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to resolve transitive dependency '{}' for '{}': no parent directory",
                    dep_path,
                    parent_name
                )
            })?;
            let resolved = parent_dir.join(dep_path);

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
            // For bare filenames, add ./ prefix to treat as file-relative
            let normalized_path = if dep_path.contains('/') {
                dep_path.to_string()
            } else {
                format!("./{}", dep_path)
            };

            crate::utils::resolve_file_relative_path(parent_file_path, &normalized_path)
                .with_context(|| {
                    format!(
                        "Failed to resolve transitive dependency '{}' for '{}'",
                        dep_path, parent_name
                    )
                })
        } else {
            // Repo-relative path (absolute path within repo)
            let repo_root = parent_file_path
                .ancestors()
                .find(|p| {
                    p.file_name().and_then(|n| n.to_str()).map(|s| s.contains('_')).unwrap_or(false)
                })
                .or_else(|| parent_file_path.ancestors().nth(2))
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
    }

    /// Determine the tool for a transitive dependency.
    fn determine_transitive_tool(
        &self,
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
                .unwrap_or_else(|| self.core.manifest().get_default_tool(parent_resource_type));
            if self.core.manifest().is_resource_supported(&parent_tool, dep_resource_type) {
                Some(parent_tool)
            } else {
                Some(self.core.manifest().get_default_tool(dep_resource_type))
            }
        }
    }

    /// Build merged template variables for a dependency.
    fn build_merged_template_vars(&self, dep: &ResourceDependency) -> serde_json::Value {
        use crate::resolver::lockfile_builder;
        lockfile_builder::build_merged_template_vars(self.core.manifest(), dep)
    }

    /// Fetch resource content for a dependency.
    async fn fetch_resource_content(
        &mut self,
        _name: &str,
        dep: &ResourceDependency,
    ) -> Result<String> {
        ResourceFetchingService::fetch_content(&self.core, dep, &mut self.version_service).await
    }

    /// Get canonical path for a dependency.
    async fn get_canonical_path_for_dependency(
        &mut self,
        dep: &ResourceDependency,
    ) -> Result<PathBuf> {
        ResourceFetchingService::get_canonical_path(&self.core, dep, &mut self.version_service)
            .await
    }

    /// Expand a pattern dependency to concrete dependencies.
    async fn expand_pattern_to_concrete_deps(
        &self,
        dep: &ResourceDependency,
        _resource_type: ResourceType,
    ) -> Result<Vec<(String, ResourceDependency)>> {
        use crate::pattern::PatternResolver;
        use crate::resolver::{path_resolver, pattern_expander::generate_dependency_name};

        let pattern = dep.get_path();

        if dep.is_local() {
            // Local pattern
            let (base_path, pattern_str) = path_resolver::parse_pattern_base_path(pattern);
            let pattern_resolver = PatternResolver::new();
            let matches = pattern_resolver.resolve(&pattern_str, &base_path)?;

            let mut results = Vec::new();
            for matched_path in matches {
                let resource_name = generate_dependency_name(&matched_path.to_string_lossy());
                let full_relative_path =
                    path_resolver::construct_full_relative_path(&base_path, &matched_path);

                let mut concrete_dep = dep.clone();
                if let ResourceDependency::Detailed(ref mut detailed) = concrete_dep {
                    detailed.path = full_relative_path;
                }

                results.push((resource_name, concrete_dep));
            }

            Ok(results)
        } else {
            // Remote pattern
            let source_name = dep
                .get_source()
                .ok_or_else(|| anyhow::anyhow!("Pattern dependency has no source specified"))?;

            let version_key =
                dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
            let group_key = format!("{}::{}", source_name, version_key);

            let prepared =
                self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Prepared state missing for source '{}' @ '{}'",
                        source_name,
                        version_key
                    )
                })?;

            let repo_path = Path::new(&prepared.worktree_path);
            let pattern_resolver = PatternResolver::new();
            let matches = pattern_resolver.resolve(pattern, repo_path)?;

            let mut results = Vec::new();
            for matched_path in matches {
                let resource_name = generate_dependency_name(&matched_path.to_string_lossy());

                let mut concrete_dep = dep.clone();
                if let ResourceDependency::Detailed(ref mut detailed) = concrete_dep {
                    detailed.path = crate::utils::normalize_path_for_storage(
                        matched_path.to_string_lossy().to_string(),
                    );
                }

                results.push((resource_name, concrete_dep));
            }

            Ok(results)
        }
    }

    /// Get the list of transitive dependencies for a resource.
    ///
    /// Returns the dependency IDs (format: "type/name") for all transitive
    /// dependencies discovered during resolution.
    fn get_dependencies_for(
        &self,
        name: &str,
        source: Option<&str>,
        resource_type: ResourceType,
        tool: Option<&str>,
    ) -> Vec<String> {
        let key = (
            resource_type,
            name.to_string(),
            source.map(std::string::ToString::to_string),
            tool.map(std::string::ToString::to_string),
        );
        self.dependency_map.get(&key).cloned().unwrap_or_default()
    }

    /// Resolve version conflict between two dependencies.
    async fn resolve_version_conflict(
        &mut self,
        _name: &str,
        existing_dep: &ResourceDependency,
        new_dep: &ResourceDependency,
        _requester: &str,
    ) -> Result<ResourceDependency> {
        // For now, prefer the higher version or keep existing
        // In a full implementation, this would use semver resolution
        let existing_version = existing_dep.get_version().unwrap_or("0.0.0");
        let new_version = new_dep.get_version().unwrap_or("0.0.0");

        if new_version > existing_version {
            Ok(new_dep.clone())
        } else {
            Ok(existing_dep.clone())
        }
    }

    /// Resolve a single dependency to a lockfile entry.
    ///
    /// Handles both local and remote dependencies, computing proper installation
    /// paths and including all necessary metadata.
    async fn resolve_dependency(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        use crate::resolver::lockfile_builder;
        use crate::resolver::path_resolver as install_path_resolver;
        use crate::utils::normalize_path_for_storage;

        if dep.is_local() {
            // Local dependency
            let filename = if let Some(custom_filename) = dep.get_filename() {
                custom_filename.to_string()
            } else {
                extract_meaningful_path(Path::new(dep.get_path()))
            };

            let artifact_type_string = dep
                .get_tool()
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.core.manifest().get_default_tool(resource_type));
            let artifact_type = artifact_type_string.as_str();

            let installed_at = install_path_resolver::resolve_install_path(
                self.core.manifest(),
                dep,
                artifact_type,
                resource_type,
                &filename,
            )?;

            Ok(LockedResource {
                name: name.to_string(),
                source: None,
                url: None,
                path: normalize_path_for_storage(dep.get_path()),
                version: None,
                resolved_commit: None,
                checksum: String::new(),
                installed_at,
                dependencies: self.get_dependencies_for(
                    name,
                    None,
                    resource_type,
                    Some(&artifact_type_string),
                ),
                resource_type,
                tool: Some(artifact_type_string),
                manifest_alias: self
                    .pattern_alias_map
                    .get(&(resource_type, name.to_string()))
                    .cloned(),
                applied_patches: lockfile_builder::get_patches_for_resource(
                    self.core.manifest(),
                    resource_type,
                    name,
                ),
                install: dep.get_install(),
                template_vars: lockfile_builder::build_merged_template_vars(
                    self.core.manifest(),
                    dep,
                )
                .to_string(),
            })
        } else {
            // Remote dependency
            let source_name = dep
                .get_source()
                .ok_or_else(|| anyhow::anyhow!("Dependency '{}' has no source specified", name))?;

            let source_url = self
                .core
                .source_manager()
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

            let version_key =
                dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
            let group_key = format!("{}::{}", source_name, version_key);

            let prepared =
                self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Prepared state missing for source '{}' @ '{}'",
                        source_name,
                        version_key
                    )
                })?;

            let filename = if let Some(custom_filename) = dep.get_filename() {
                custom_filename.to_string()
            } else {
                Path::new(dep.get_path()).to_string_lossy().to_string()
            };

            let artifact_type_string = dep
                .get_tool()
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.core.manifest().get_default_tool(resource_type));
            let artifact_type = artifact_type_string.as_str();

            let installed_at = install_path_resolver::resolve_install_path(
                self.core.manifest(),
                dep,
                artifact_type,
                resource_type,
                &filename,
            )?;

            Ok(LockedResource {
                name: name.to_string(),
                source: Some(source_name.to_string()),
                url: Some(source_url.clone()),
                path: normalize_path_for_storage(dep.get_path()),
                version: prepared.resolved_version.clone(),
                resolved_commit: Some(prepared.resolved_commit.clone()),
                checksum: String::new(),
                installed_at,
                dependencies: self.get_dependencies_for(
                    name,
                    Some(source_name),
                    resource_type,
                    Some(&artifact_type_string),
                ),
                resource_type,
                tool: Some(artifact_type_string),
                manifest_alias: self
                    .pattern_alias_map
                    .get(&(resource_type, name.to_string()))
                    .cloned(),
                applied_patches: lockfile_builder::get_patches_for_resource(
                    self.core.manifest(),
                    resource_type,
                    name,
                ),
                install: dep.get_install(),
                template_vars: lockfile_builder::build_merged_template_vars(
                    self.core.manifest(),
                    dep,
                )
                .to_string(),
            })
        }
    }

    /// Resolve a pattern dependency to multiple locked resources.
    async fn resolve_pattern_dependency(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        use crate::pattern::PatternResolver;
        use crate::resolver::{
            lockfile_builder, path_resolver as install_path_resolver, path_resolver,
        };
        use crate::utils::{
            compute_relative_install_path, normalize_path, normalize_path_for_storage,
        };

        if !dep.is_pattern() {
            return Err(anyhow::anyhow!(
                "Expected pattern dependency but no glob characters found in path"
            ));
        }

        let pattern = dep.get_path();

        if dep.is_local() {
            // Local pattern
            let (base_path, pattern_str) = path_resolver::parse_pattern_base_path(pattern);
            let pattern_resolver = PatternResolver::new();
            let matches = pattern_resolver.resolve(&pattern_str, &base_path)?;

            let artifact_type_string = dep
                .get_tool()
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.core.manifest().get_default_tool(resource_type));
            let artifact_type = artifact_type_string.as_str();

            let mut resources = Vec::new();
            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);
                let full_relative_path =
                    path_resolver::construct_full_relative_path(&base_path, &matched_path);
                let filename = path_resolver::extract_pattern_filename(&base_path, &matched_path);

                let installed_at = install_path_resolver::resolve_install_path(
                    self.core.manifest(),
                    dep,
                    artifact_type,
                    resource_type,
                    &filename,
                )?;

                resources.push(LockedResource {
                    name: resource_name.clone(),
                    source: None,
                    url: None,
                    path: full_relative_path,
                    version: None,
                    resolved_commit: None,
                    checksum: String::new(),
                    installed_at,
                    dependencies: vec![],
                    resource_type,
                    tool: Some(artifact_type_string.clone()),
                    manifest_alias: Some(name.to_string()),
                    applied_patches: lockfile_builder::get_patches_for_resource(
                        self.core.manifest(),
                        resource_type,
                        name,
                    ),
                    install: dep.get_install(),
                    template_vars: "{}".to_string(),
                });
            }

            Ok(resources)
        } else {
            // Remote pattern
            let source_name = dep.get_source().ok_or_else(|| {
                anyhow::anyhow!("Pattern dependency '{}' has no source specified", name)
            })?;

            let source_url = self
                .core
                .source_manager()
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

            let version_key =
                dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
            let group_key = format!("{}::{}", source_name, version_key);

            let prepared =
                self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Prepared state missing for source '{}' @ '{}'",
                        source_name,
                        version_key
                    )
                })?;

            let repo_path = Path::new(&prepared.worktree_path);
            let pattern_resolver = PatternResolver::new();
            let matches = pattern_resolver.resolve(pattern, repo_path)?;

            let artifact_type_string = dep
                .get_tool()
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.core.manifest().get_default_tool(resource_type));
            let artifact_type = artifact_type_string.as_str();

            let mut resources = Vec::new();
            for matched_path in matches {
                let resource_name = crate::pattern::extract_resource_name(&matched_path);

                // Compute installation path
                let installed_at = match resource_type {
                    ResourceType::Hook | ResourceType::McpServer => {
                        install_path_resolver::resolve_merge_target_path(
                            self.core.manifest(),
                            artifact_type,
                            resource_type,
                        )
                    }
                    _ => {
                        let artifact_path = self
                            .core
                            .manifest()
                            .get_artifact_resource_path(artifact_type, resource_type)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Resource type '{}' is not supported by tool '{}'",
                                    resource_type,
                                    artifact_type
                                )
                            })?;

                        let dep_flatten = dep.get_flatten();
                        let tool_flatten = self
                            .core
                            .manifest()
                            .get_tool_config(artifact_type)
                            .and_then(|config| config.resources.get(resource_type.to_plural()))
                            .and_then(|resource_config| resource_config.flatten);

                        let flatten = dep_flatten.or(tool_flatten).unwrap_or(false);

                        let base_target = if let Some(custom_target) = dep.get_target() {
                            PathBuf::from(artifact_path.display().to_string())
                                .join(custom_target.trim_start_matches('/'))
                        } else {
                            artifact_path.to_path_buf()
                        };

                        let filename = repo_path.join(&matched_path).to_string_lossy().to_string();
                        let relative_path = compute_relative_install_path(
                            &base_target,
                            Path::new(&filename),
                            flatten,
                        );
                        normalize_path_for_storage(normalize_path(&base_target.join(relative_path)))
                    }
                };

                resources.push(LockedResource {
                    name: resource_name.clone(),
                    source: Some(source_name.to_string()),
                    url: Some(source_url.clone()),
                    path: normalize_path_for_storage(matched_path.to_string_lossy().to_string()),
                    version: prepared.resolved_version.clone(),
                    resolved_commit: Some(prepared.resolved_commit.clone()),
                    checksum: String::new(),
                    installed_at,
                    dependencies: vec![],
                    resource_type,
                    tool: Some(artifact_type_string.clone()),
                    manifest_alias: Some(name.to_string()),
                    applied_patches: lockfile_builder::get_patches_for_resource(
                        self.core.manifest(),
                        resource_type,
                        name,
                    ),
                    install: dep.get_install(),
                    template_vars: "{}".to_string(),
                });
            }

            Ok(resources)
        }
    }

    /// Add or update a lockfile entry with deduplication.
    fn add_or_update_lockfile_entry(
        &self,
        lockfile: &mut LockFile,
        _name: &str,
        entry: LockedResource,
    ) {
        let resources = lockfile.get_resources_mut(entry.resource_type);

        // Use (name, source, tool) matching for deduplication
        // This allows multiple entries with the same name from different sources or tools
        if let Some(existing) = resources
            .iter_mut()
            .find(|e| e.name == entry.name && e.source == entry.source && e.tool == entry.tool)
        {
            *existing = entry;
        } else {
            resources.push(entry);
        }
    }

    /// Add version information to dependency references in lockfile.
    fn add_version_to_dependencies(&self, lockfile: &mut LockFile) -> Result<()> {
        use crate::resolver::lockfile_builder;

        lockfile_builder::add_version_to_all_dependencies(lockfile);
        Ok(())
    }

    /// Detect target path conflicts between resources.
    fn detect_target_conflicts(&self, lockfile: &LockFile) -> Result<()> {
        use crate::resolver::lockfile_builder;

        lockfile_builder::detect_target_conflicts(lockfile)
    }

    /// Add a dependency to the conflict detector.
    fn add_to_conflict_detector(
        &mut self,
        _name: &str,
        dep: &ResourceDependency,
        required_by: &str,
    ) {
        use crate::resolver::types as dependency_helpers;

        // Skip local dependencies (no version conflicts possible)
        if dep.is_local() {
            return;
        }

        // Build resource identifier
        let resource_id = dependency_helpers::build_resource_id(dep);

        // Get version constraint (None means HEAD/unspecified)
        let version = dep.get_version().unwrap_or("HEAD");

        // Add to conflict detector
        self.conflict_detector.add_requirement(&resource_id, required_by, version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolver_creation() {
        let manifest = Manifest::default();
        let cache = Cache::new().unwrap();
        let resolver = DependencyResolver::new(manifest, cache).await;
        assert!(resolver.is_ok());
    }

    #[tokio::test]
    async fn test_resolver_with_global() {
        let manifest = Manifest::default();
        let cache = Cache::new().unwrap();
        let resolver = DependencyResolver::new_with_global(manifest, cache).await;
        assert!(resolver.is_ok());
    }
}
