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
pub mod backtracking;
pub mod conflict_service;
pub mod dependency_graph;
mod dependency_processing;
mod entry_builder;
mod incremental_update;
pub mod lockfile_builder;
pub mod path_resolver;
pub mod pattern_expander;
pub mod resource_service;
pub mod sha_conflict_detector;
pub mod skills;
pub mod source_context;
pub mod transitive_extractor;
pub mod transitive_resolver;
pub mod types;
pub mod version_resolver;

#[cfg(test)]
mod tests;

// Re-export utility functions for compatibility
pub use path_resolver::{extract_meaningful_path, is_file_relative_path, normalize_bare_filename};

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use dashmap::DashMap;

use crate::cache::Cache;
use crate::core::{OperationContext, ResourceType};
use crate::lockfile::{LockFile, LockedResource};
use crate::manifest::{Manifest, ResourceDependency};
use crate::source::SourceManager;

// Re-export services for external use
pub use conflict_service::ConflictService;
pub use pattern_expander::PatternExpansionService;
pub use resource_service::ResourceFetchingService;
pub use types::ResolutionCore;
pub use version_resolver::{
    VersionResolutionService, VersionResolver as VersionResolverExport, find_best_matching_tag,
    is_version_constraint, parse_tags_to_versions,
};

// Legacy re-exports for compatibility
pub use dependency_graph::{DependencyGraph, DependencyNode};
pub use lockfile_builder::LockfileBuilder;
pub use pattern_expander::{expand_pattern_to_concrete_deps, generate_dependency_name};
pub use types::{
    ConflictDetectionKey, DependencyKey, ManifestOverride, ManifestOverrideIndex, OverrideKey,
    ResolutionContext, ResolvedDependenciesMap, ResolvedDependencyInfo, TransitiveContext,
};

pub use version_resolver::{PreparedSourceVersion, VersionResolver, WorktreeManager};

/// Main dependency resolver with service-based architecture.
///
/// This orchestrates multiple specialized services to handle different aspects
/// of the dependency resolution process while maintaining compatibility
/// with existing interfaces.
///
/// # Architecture
///
/// The resolver follows a modular service pattern where each complex aspect
/// of resolution is delegated to a specialized service:
/// - [`VersionResolutionService`] handles Git operations and batch SHA resolution
/// - [`PatternExpansionService`] expands glob patterns into concrete dependencies
/// - Version conflict detection identifies and reports version conflicts
///
/// # Resolution Process
///
/// The resolution occurs in distinct phases:
///
/// 1. **Collection Phase**: Extract dependencies from the project manifest
/// 2. **Version Resolution Phase**: Batch resolve all version constraints to commit SHAs
/// 3. **Pattern Expansion Phase**: Expand glob patterns (e.g., `agents/*.md`) into individual resources
/// 4. **Transitive Resolution Phase** (optional): Resolve dependencies declared within resources
/// 5. **Conflict Detection Phase**: Detect and report version conflicts across the dependency graph
///
/// # Examples
///
/// ```ignore
/// use agpm_cli::resolver::DependencyResolver;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::cache::Cache;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Load manifest and create cache
/// let manifest = Manifest::load_from_file("agpm.toml")?;
/// let cache = Cache::new()?;
///
/// // Create resolver and resolve dependencies
/// let mut resolver = DependencyResolver::new(manifest, cache).await?;
/// let lockfile = resolver.resolve().await?;
///
/// println!("Resolved {} dependencies", lockfile.total_resources());
/// # Ok(())
/// # }
/// ```
///
/// # Key Features
///
/// - **Parallel Processing**: Configurable concurrency for performance
/// - **SHA-based Deduplication**: Shared worktrees for identical commits
/// - **Transitive Dependencies**: Optional resolution of dependencies of dependencies
/// - **Version Constraints**: Support for semver-style constraints (`^1.0`, `~2.1`)
/// - **Pattern Support**: Glob patterns for bulk dependency inclusion
/// - **Conflict Detection**: Comprehensive detection of version conflicts
///
/// # Related Services
///
/// - [`VersionResolutionService`]: Git operations and SHA resolution
/// - [`PatternExpansionService`]: Glob pattern handling
/// - Service container for transitive resolution (transitive_resolver::ResolutionServices)
/// - Version conflict detection and reporting (conflict_detector::ConflictDetector)
pub struct DependencyResolver {
    /// Core shared context with immutable state
    core: ResolutionCore,

    /// Version resolution and Git operations service
    version_service: VersionResolutionService,

    /// Pattern expansion service for glob dependencies
    pattern_service: PatternExpansionService,

    /// Conflict detector for version conflicts
    conflict_detector: crate::version::conflict::ConflictDetector,

    /// SHA-based conflict detector
    sha_conflict_detector: crate::resolver::sha_conflict_detector::ShaConflictDetector,

    /// Dependency tracking state (concurrent)
    dependency_map: Arc<DashMap<DependencyKey, Vec<String>>>,

    /// Pattern alias tracking for expanded patterns (concurrent)
    pattern_alias_map: Arc<DashMap<(ResourceType, String), String>>,

    /// Transitive dependency custom names (concurrent)
    transitive_custom_names: Arc<DashMap<DependencyKey, String>>,

    /// Track if sources have been pre-synced to avoid duplicate work
    /// Uses AtomicBool with Acquire/Release ordering for thread-safe synchronization during parallel dependency resolution
    sources_pre_synced: std::sync::atomic::AtomicBool,

    /// Tracks resolved SHAs for conflict detection
    /// Key: (resource_id, required_by, name)
    /// Value: (version_constraint, resolved_sha, parent_version, parent_sha, resolution_mode)
    /// Uses DashMap for concurrent access during parallel dependency resolution
    resolved_deps_for_conflict_check: ResolvedDependenciesMap,

    /// Reverse lookup from dependency reference → parents that require it.
    ///
    /// Key: Dependency reference (e.g., "agents/helper", "snippet:snippets/foo")
    /// Value: List of parent resource IDs that depend on this resource
    ///
    /// Populated during resolution to enable efficient parent metadata lookups
    /// without searching through all resolved dependencies.
    /// Uses DashMap for concurrent access during parallel dependency resolution
    reverse_dependency_map: std::sync::Arc<dashmap::DashMap<String, Vec<String>>>,
}

impl DependencyResolver {
    /// Initialize a DependencyResolver with the given core and services.
    ///
    /// This private helper function centralizes the struct initialization logic
    /// to reduce duplication across constructors.
    ///
    /// # Arguments
    ///
    /// * `core` - Resolution core with manifest, cache, and source manager
    /// * `version_service` - Version resolution service
    /// * `pattern_service` - Pattern expansion service
    fn init_dependencies(
        core: ResolutionCore,
        version_service: VersionResolutionService,
        pattern_service: PatternExpansionService,
    ) -> Result<Self> {
        Ok(Self {
            core,
            version_service,
            pattern_service,
            conflict_detector: crate::version::conflict::ConflictDetector::new(),
            sha_conflict_detector: crate::resolver::sha_conflict_detector::ShaConflictDetector::new(
            ),
            dependency_map: Arc::new(DashMap::new()),
            pattern_alias_map: Arc::new(DashMap::new()),
            transitive_custom_names: Arc::new(DashMap::new()),
            sources_pre_synced: std::sync::atomic::AtomicBool::new(false),
            resolved_deps_for_conflict_check: Arc::new(DashMap::new()),
            reverse_dependency_map: std::sync::Arc::new(dashmap::DashMap::new()),
        })
    }

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

        Self::init_dependencies(core, version_service, pattern_service)
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
        Self::new_with_global_concurrency(manifest, cache, None, None).await
    }

    /// Creates a new dependency resolver with global config and custom concurrency limit.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest with dependencies
    /// * `cache` - Cache for Git operations and worktrees
    /// * `max_concurrency` - Optional concurrency limit for parallel operations
    /// * `operation_context` - Optional context for warning deduplication
    ///
    /// # Errors
    ///
    /// Returns an error if global configuration cannot be loaded
    pub async fn new_with_global_concurrency(
        manifest: Manifest,
        cache: Cache,
        max_concurrency: Option<usize>,
        operation_context: Option<Arc<OperationContext>>,
    ) -> Result<Self> {
        let source_manager = SourceManager::from_manifest_with_global(&manifest).await?;

        let core = ResolutionCore::new(manifest, cache, source_manager, operation_context);

        let version_service = if let Some(concurrency) = max_concurrency {
            VersionResolutionService::with_concurrency(core.cache().clone(), concurrency)
        } else {
            VersionResolutionService::new(core.cache().clone())
        };
        let pattern_service = PatternExpansionService::new();

        Self::init_dependencies(core, version_service, pattern_service)
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
        operation_context: Option<Arc<OperationContext>>,
    ) -> Result<Self> {
        Self::new_with_global_concurrency(manifest, cache, None, operation_context).await
    }

    /// Get a reference to the resolution core.
    pub fn core(&self) -> &ResolutionCore {
        &self.core
    }

    /// Resolve all dependencies and generate a complete lockfile.
    ///
    /// Performs dependency resolution with automatic conflict detection and
    /// backtracking to find compatible versions when conflicts occur.
    ///
    /// # Errors
    ///
    /// Returns an error if any step of resolution fails
    pub async fn resolve(&mut self) -> Result<LockFile> {
        self.resolve_with_options(true, None).await
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
    pub async fn resolve_with_options(
        &mut self,
        enable_transitive: bool,
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<LockFile> {
        // Phase 1: Preparation and manifest loading
        let (base_deps, mut lockfile) = self.prepare_resolution(&progress).await?;

        // Phase 2: Pre-sync sources
        self.pre_sync_sources_if_needed(&base_deps, progress.clone()).await?;

        // Phase 3: Resolve transitive dependencies
        let all_deps = self
            .resolve_transitive_dependencies_phase(&base_deps, enable_transitive, progress.clone())
            .await?;

        // Phase 4: Resolve individual dependencies
        self.resolve_individual_dependencies(&all_deps, &mut lockfile, progress.clone()).await?;

        // Phase 5: Handle conflicts and backtracking
        self.handle_conflicts_and_backtracking(&mut lockfile).await?;

        // Phase 6: Final post-processing
        self.finalize_resolution(&mut lockfile, &progress)?;

        Ok(lockfile)
    }

    /// Phase 1: Prepare resolution context and extract base dependencies
    ///
    /// Returns the base dependencies from the manifest and an initialized lockfile.
    async fn prepare_resolution(
        &mut self,
        progress: &Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<(Vec<(String, ResourceDependency, ResourceType)>, LockFile)> {
        // Clear state from previous resolution
        self.resolved_deps_for_conflict_check.clear();
        self.reverse_dependency_map.clear();
        self.conflict_detector = crate::version::conflict::ConflictDetector::new();

        let mut lockfile = LockFile::new();

        // Add sources to lockfile
        for (name, url) in &self.core.manifest().sources {
            lockfile.add_source(name.clone(), url.clone(), String::new());
        }

        // Extract dependencies from manifest with types
        let base_deps: Vec<(String, ResourceDependency, ResourceType)> = self
            .core
            .manifest()
            .all_dependencies_with_types()
            .into_iter()
            .map(|(name, dep, resource_type)| (name.to_string(), dep.into_owned(), resource_type))
            .collect();

        // Start the ResolvingDependencies phase with windowed tracking
        // This phase includes: transitive resolution (Phase 3), individual resolution (Phase 4),
        // and conflict detection (Phase 6). We start with base deps count as initial estimate.
        let window_size = 7;
        if let Some(pm) = progress {
            tracing::debug!(
                "Starting ResolvingDependencies phase with windowed tracking: {} base deps, {} slots",
                base_deps.len(),
                window_size
            );
            pm.start_phase_with_active_tracking(
                crate::utils::InstallationPhase::ResolvingDependencies,
                base_deps.len(),
                window_size,
            );
        }

        Ok((base_deps, lockfile))
    }

    /// Phase 2: Pre-sync all sources if not already done
    async fn pre_sync_sources_if_needed(
        &mut self,
        base_deps: &[(String, ResourceDependency, ResourceType)],
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<()> {
        if !self.sources_pre_synced.load(std::sync::atomic::Ordering::Acquire) {
            let deps_for_sync: Vec<(String, ResourceDependency)> =
                base_deps.iter().map(|(name, dep, _)| (name.clone(), dep.clone())).collect();
            self.version_service.pre_sync_sources(&self.core, &deps_for_sync, progress).await?;
            self.sources_pre_synced.store(true, std::sync::atomic::Ordering::Release);
        }
        Ok(())
    }

    /// Phase 3: Resolve transitive dependencies
    async fn resolve_transitive_dependencies_phase(
        &mut self,
        base_deps: &[(String, ResourceDependency, ResourceType)],
        enable_transitive: bool,
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
        tracing::info!(
            "Phase 3: Starting transitive dependency resolution (enable_transitive={})",
            enable_transitive
        );

        if enable_transitive {
            tracing::info!(
                "Phase 3: Calling resolve_transitive_dependencies with {} base deps",
                base_deps.len()
            );
            let result = self.resolve_transitive_dependencies(base_deps, progress).await?;
            tracing::info!("Phase 3: Resolved {} total deps (including transitive)", result.len());
            Ok(result)
        } else {
            tracing::info!(
                "Phase 3: Transitive resolution disabled, using {} base deps",
                base_deps.len()
            );
            Ok(base_deps.to_vec())
        }
    }

    /// Phase 4: Resolve each dependency to a locked resource in parallel.
    ///
    /// This method processes dependencies in batches for parallelism.
    async fn resolve_individual_dependencies(
        &mut self,
        all_deps: &[(String, ResourceDependency, ResourceType)],
        lockfile: &mut LockFile,
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<()> {
        let completed_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let total_deps = all_deps.len();

        // Use same default concurrency as version resolution: max(10, 2 × CPU cores)
        let cores = std::thread::available_parallelism().map(std::num::NonZero::get).unwrap_or(4);
        let max_concurrent = std::cmp::max(10, cores * 2);

        // Process dependencies in batches to achieve parallelism
        let mut all_results = Vec::new();
        for chunk in all_deps.chunks(max_concurrent) {
            use futures::future::join_all;

            // Clone progress for each batch to avoid closure capture issues
            let progress_clone = progress.clone();

            // Create futures for this batch by calling async methods directly
            let batch_futures: Vec<_> = chunk
                .iter()
                .map(|(name, dep, resource_type)| {
                    // Build display name for progress tracking
                    let display_name = if dep.get_source().is_some() {
                        if let Some(version) = dep.get_version() {
                            format!("{}@{}", name, version)
                        } else {
                            format!("{}@HEAD", name)
                        }
                    } else {
                        name.clone()
                    };
                    let progress_key = format!("{}:{}", resource_type, &display_name);

                    // Mark as active in progress window
                    if let Some(pm) = &progress_clone {
                        pm.mark_item_active(&display_name, &progress_key);
                    }

                    // Call the async resolution method directly (returns a Future)
                    let resolution_fut = if dep.is_pattern() {
                        Box::pin(self.resolve_pattern_dependency(name, dep, *resource_type))
                            as std::pin::Pin<
                                Box<
                                    dyn std::future::Future<Output = Result<Vec<LockedResource>>>
                                        + Send
                                        + '_,
                                >,
                            >
                    } else {
                        Box::pin(async {
                            self.resolve_dependency(name, dep, *resource_type)
                                .await
                                .map(|e| vec![e])
                        })
                            as std::pin::Pin<
                                Box<
                                    dyn std::future::Future<Output = Result<Vec<LockedResource>>>
                                        + Send
                                        + '_,
                                >,
                            >
                    };

                    (
                        resolution_fut,
                        name.clone(),
                        dep.clone(),
                        *resource_type,
                        progress_key,
                        display_name,
                    )
                })
                .collect();

            // Execute all futures in this batch concurrently with timeout
            let timeout_duration = crate::constants::batch_operation_timeout();
            let batch_results = tokio::time::timeout(
                timeout_duration,
                join_all(batch_futures.into_iter().map(
                    |(fut, name, dep, resource_type, progress_key, display_name)| {
                        let progress_clone = progress_clone.clone();
                        let counter_clone = completed_counter.clone();
                        async move {
                            let result = fut.await;

                            // Mark item as complete
                            if let Some(pm) = &progress_clone {
                                let completed = counter_clone
                                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                                    + 1;
                                pm.mark_item_complete(
                                    &progress_key,
                                    Some(&display_name),
                                    completed,
                                    total_deps,
                                    "Resolving dependencies",
                                );
                            }

                            (name, dep, resource_type, result)
                        }
                    },
                )),
            )
            .await
            .with_context(|| {
                format!(
                    "Batch dependency resolution timed out after {:?} - possible deadlock",
                    timeout_duration
                )
            })?;

            // Collect batch results
            for (name, dep, resource_type, result) in batch_results {
                all_results.push(result.map(|entries| (name, dep, resource_type, entries)));
            }
        }

        // Process results: track for conflicts and add to lockfile
        for result in all_results {
            let (name, dep, resource_type, entries) = result?;

            for entry in entries {
                // Track for conflict detection using manifest alias
                // (critical for detecting conflicts between different manifest entries
                // that resolve to the same canonical resource)
                self.track_resolved_dependency_for_conflicts(&name, &dep, &entry, resource_type);

                self.add_or_update_lockfile_entry(lockfile, entry);
            }
        }

        Ok(())
    }

    /// Phase 5: Handle conflict detection and backtracking
    async fn handle_conflicts_and_backtracking(&mut self, lockfile: &mut LockFile) -> Result<()> {
        // Add resolved dependencies to conflict detector with SHAs and parent metadata
        tracing::debug!(
            "Phase 5: Processing {} tracked dependencies for conflict detection",
            self.resolved_deps_for_conflict_check.len()
        );
        for entry in self.resolved_deps_for_conflict_check.iter() {
            let ((resource_id, required_by, _name), dependency_info) = (entry.key(), entry.value());
            let ResolvedDependencyInfo {
                version_constraint,
                resolved_sha,
                parent_version,
                parent_sha,
                resolution_mode,
            } = dependency_info;

            // Only add Version path conflicts to the backtracking conflict detector
            // Git path conflicts are unresolvable and will be handled separately
            if matches!(resolution_mode, crate::resolver::types::ResolutionMode::Version) {
                tracing::debug!(
                    "Adding VERSION path to conflict detector: resource_id={}, required_by={}, version={}, sha={}",
                    resource_id,
                    required_by,
                    version_constraint,
                    &resolved_sha[..8.min(resolved_sha.len())]
                );
                self.conflict_detector.add_requirement_with_parent(
                    resource_id.clone(),
                    required_by,
                    version_constraint,
                    resolved_sha,
                    parent_version.clone(),
                    parent_sha.clone(),
                );
            } else {
                tracing::debug!(
                    "Skipping GIT path for backtracking: resource_id={}, required_by={}, git_ref={}",
                    resource_id,
                    required_by,
                    version_constraint
                );
            }
        }

        // Phase 5: SHA-based conflict detection
        let conflict_start = std::time::Instant::now();

        // Populate SHA conflict detector with Git path dependencies only
        for entry in self.resolved_deps_for_conflict_check.iter() {
            let ((resource_id, required_by, _name), dependency_info) = (entry.key(), entry.value());
            let ResolvedDependencyInfo {
                version_constraint,
                resolved_sha,
                parent_version: _parent_version,
                parent_sha: _parent_sha,
                resolution_mode,
            } = dependency_info;

            // Only process Git path conflicts in SHA conflict detector
            if matches!(resolution_mode, crate::resolver::types::ResolutionMode::GitRef) {
                // Parse source and path from resource_id
                let source_str = resource_id.source();
                let source = source_str.unwrap_or("local");
                let path = resource_id.name();

                tracing::debug!(
                    "Adding GIT path to SHA conflict detector: source={}, path={}, git_ref={}, sha={}",
                    source,
                    path,
                    version_constraint,
                    &resolved_sha[..8.min(resolved_sha.len())]
                );

                // Add to SHA conflict detector
                self.sha_conflict_detector.add_requirement(
                    crate::resolver::sha_conflict_detector::ResolvedRequirement {
                        source: source.to_string(),
                        path: path.to_string(),
                        resolved_sha: resolved_sha.clone(),
                        requested_version: version_constraint.clone(),
                        required_by: required_by.clone(),
                        resolution_mode: *resolution_mode,
                    },
                );
            }
        }

        // Detect SHA conflicts
        let sha_conflicts = self.sha_conflict_detector.detect_conflicts()?;
        let conflict_detect_duration = conflict_start.elapsed();
        tracing::debug!(
            "Phase 5: SHA conflict detection took {:?} for {} tracked dependencies",
            conflict_detect_duration,
            self.resolved_deps_for_conflict_check.len()
        );

        if !sha_conflicts.is_empty() {
            // SHA conflicts are true conflicts that cannot be resolved by backtracking
            // Report them as errors
            let error_messages: Vec<String> =
                sha_conflicts.iter().map(|conflict| conflict.format_error()).collect();

            return Err(anyhow::anyhow!(
                "Unresolvable SHA conflicts detected:\n{}",
                error_messages.join("\n")
            ));
        }

        // Phase 6: Version-based conflict detection (only for Version path dependencies)
        // Use the original conflict detector for version constraint conflicts
        let conflicts = self.conflict_detector.detect_conflicts();

        if !conflicts.is_empty() {
            tracing::info!(
                "Detected {} version constraint conflict(s), attempting automatic resolution...",
                conflicts.len()
            );

            // Attempt backtracking to find compatible versions
            let mut backtracker =
                backtracking::BacktrackingResolver::new(&self.core, &mut self.version_service);

            // Populate the backtracker's resource registry from the conflict detector
            // This provides the complete dependency graph for conflict detection during backtracking
            backtracker.populate_from_conflict_detector(&self.conflict_detector);

            match backtracker.resolve_conflicts(&conflicts).await {
                Ok(result) if result.resolved => {
                    // Log success with all metrics
                    if result.total_transitive_reresolutions > 0 {
                        tracing::info!(
                            "✓ Resolved conflicts after {} iteration(s): {} version(s) adjusted, {} transitive re-resolution(s)",
                            result.iterations,
                            result.updates.len(),
                            result.total_transitive_reresolutions
                        );
                    } else {
                        tracing::info!(
                            "✓ Resolved conflicts after {} iteration(s): {} version(s) adjusted",
                            result.iterations,
                            result.updates.len()
                        );
                    }

                    // Log what changed
                    for update in &result.updates {
                        tracing::info!(
                            "  {} : {} → {}",
                            update.resource_id,
                            update.old_version,
                            update.new_version
                        );
                    }

                    // Apply the backtracking updates to prepared versions
                    self.apply_backtracking_updates(&result.updates).await?;

                    // Update lockfile entries with new SHAs and paths
                    self.update_lockfile_entries(lockfile, &result.updates)?;

                    tracing::info!("Applied backtracking updates, backtracking complete");
                }
                Ok(result) => {
                    // Backtracking failed - log the reason
                    let reason_msg = match result.termination_reason {
                        backtracking::TerminationReason::MaxIterations => {
                            format!("reached max iterations ({})", result.iterations)
                        }
                        backtracking::TerminationReason::Timeout => "timeout exceeded".to_string(),
                        backtracking::TerminationReason::NoProgress => {
                            "no progress made (same conflicts persist)".to_string()
                        }
                        backtracking::TerminationReason::Oscillation => {
                            "oscillation detected (cycling between conflict states)".to_string()
                        }
                        backtracking::TerminationReason::NoCompatibleVersion => {
                            "no compatible version found".to_string()
                        }
                        _ => "unknown reason".to_string(),
                    };

                    tracing::warn!("Backtracking failed: {}", reason_msg);

                    // Use original error with detailed conflict information
                    let mut error_msg = format!(
                        "Version conflicts detected (automatic resolution failed: {}):\n\n",
                        reason_msg
                    );
                    for conflict in &conflicts {
                        error_msg.push_str(&format!("{conflict}\n"));
                    }
                    error_msg.push_str(
                        "\nSuggestion: Manually specify compatible versions in agpm.toml",
                    );
                    return Err(anyhow::anyhow!("{}", error_msg));
                }
                Err(e) => {
                    // Backtracking encountered an error
                    tracing::error!("Backtracking error: {}", e);
                    let mut error_msg = format!(
                        "Version conflicts detected (automatic resolution error: {}):\n\n",
                        e
                    );
                    for conflict in &conflicts {
                        error_msg.push_str(&format!("{conflict}\n"));
                    }
                    return Err(anyhow::anyhow!("{}", error_msg));
                }
            }
        }

        Ok(())
    }

    /// Phase 6: Final post-processing and cleanup
    fn finalize_resolution(
        &mut self,
        lockfile: &mut LockFile,
        progress: &Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<()> {
        // Post-process dependencies and detect target conflicts
        self.add_version_to_dependencies(lockfile)?;
        self.detect_target_conflicts(lockfile)?;

        // Complete the resolution phase (includes all phases: version resolution,
        // transitive deps, conflict detection)
        if let Some(pm) = progress {
            let total_resources = lockfile.agents.len()
                + lockfile.commands.len()
                + lockfile.scripts.len()
                + lockfile.hooks.len()
                + lockfile.snippets.len()
                + lockfile.mcp_servers.len()
                + lockfile.skills.len();
            pm.complete_phase_with_window(Some(&format!(
                "Resolved {} dependencies",
                total_resources
            )));
        }

        Ok(())
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
    pub async fn pre_sync_sources(
        &mut self,
        deps: &[(String, ResourceDependency)],
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<()> {
        // Pre-sync all sources using version service
        self.version_service.pre_sync_sources(&self.core, deps, progress).await?;
        self.sources_pre_synced.store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Update dependencies with existing lockfile and specific dependencies to update.
    ///
    /// # Arguments
    ///
    /// * `existing` - Existing lockfile to update
    /// * `deps_to_update` - Optional specific dependency names to update (None = all)
    /// * `progress` - Optional multi-phase progress tracker for UI updates
    ///
    /// # Current Implementation (MVP)
    ///
    /// Currently performs full resolution regardless of `deps_to_update` value.
    /// This is correct but not optimized - all dependencies are re-resolved even
    /// when only specific ones are requested.
    ///
    /// # Future Enhancement
    ///
    /// Full incremental update would:
    /// 1. Match `deps_to_update` names to lockfile entries
    /// 2. Keep unchanged dependencies at their locked versions
    /// 3. Re-resolve only specified dependencies to latest matching versions
    /// 4. Re-extract transitive dependencies for updated resources
    /// 5. Merge updated entries with unchanged entries from existing lockfile
    /// 6. Detect and resolve any new conflicts
    ///
    /// This requires significant changes to the resolution pipeline to support
    /// "pinned" versions alongside "latest" resolution.
    ///
    /// # Errors
    ///
    /// Returns an error if update process fails
    pub async fn update(
        &mut self,
        existing: &LockFile,
        deps_to_update: Option<Vec<String>>,
        progress: Option<Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<LockFile> {
        match deps_to_update {
            None => {
                // Update all dependencies (full resolution)
                tracing::debug!("Performing full resolution for all dependencies");
                self.resolve_with_options(true, progress).await
            }
            Some(names) => {
                // Incremental update requested
                tracing::debug!("Incremental update requested for: {:?}", names);

                // Phase 1: Filter lockfile entries into unchanged and to-update
                let (unchanged, to_resolve) = Self::filter_lockfile_entries(existing, &names);

                if to_resolve.is_empty() {
                    tracing::warn!("No matching dependencies found in lockfile: {:?}", names);
                    return Ok(existing.clone());
                }

                tracing::debug!(
                    "Resolving {} dependencies, keeping {} unchanged",
                    to_resolve.len(),
                    unchanged.agents.len()
                        + unchanged.snippets.len()
                        + unchanged.commands.len()
                        + unchanged.scripts.len()
                        + unchanged.hooks.len()
                        + unchanged.mcp_servers.len()
                        + unchanged.skills.len()
                );

                // Phase 2: Create filtered manifest with only deps to update
                let filtered_manifest = self.create_filtered_manifest(&to_resolve);

                // Phase 3: Create temporary resolver for filtered manifest
                // Re-use the same cache and operation context
                let mut temp_resolver = DependencyResolver::new_with_context(
                    filtered_manifest,
                    self.core.cache().clone(),
                    self.core.operation_context().cloned(),
                )
                .await?;

                // Phase 4: Resolve filtered dependencies with updates allowed
                let updated = temp_resolver.resolve_with_options(true, progress).await?;

                // Phase 5: Merge unchanged and updated lockfiles
                let merged = Self::merge_lockfiles(unchanged, updated);

                tracing::debug!(
                    "Incremental update complete: merged lockfile has {} total entries",
                    merged.agents.len()
                        + merged.snippets.len()
                        + merged.commands.len()
                        + merged.scripts.len()
                        + merged.hooks.len()
                        + merged.mcp_servers.len()
                        + merged.skills.len()
                );

                Ok(merged)
            }
        }
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
    /// Build an index of manifest overrides for deduplication with transitive deps.
    ///
    /// This method creates a mapping from resource identity (source, path, tool, variant_hash)
    /// to the customizations (filename, target, install, template_vars) specified in the manifest.
    /// When a transitive dependency is discovered that matches a manifest dependency, the manifest
    /// version's customizations will take precedence.
    fn build_manifest_override_index(
        &self,
        base_deps: &[(String, ResourceDependency, ResourceType)],
    ) -> types::ManifestOverrideIndex {
        use crate::resolver::types::{ManifestOverride, OverrideKey, normalize_lookup_path};

        let mut index = HashMap::new();

        for (name, dep, resource_type) in base_deps {
            // Skip pattern dependencies (they expand later)
            if dep.is_pattern() {
                continue;
            }

            // Build the override key
            let normalized_path = normalize_lookup_path(dep.get_path());
            let source = dep.get_source().map(std::string::ToString::to_string);

            // Determine tool for this dependency
            let tool = dep
                .get_tool()
                .map(str::to_string)
                .unwrap_or_else(|| self.core.manifest().get_default_tool(*resource_type));

            // Compute variant_hash from MERGED variant_inputs (dep + global config)
            // This ensures manifest overrides use the same hash as LockedResources
            let merged_variant_inputs =
                lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep);
            let variant_hash = crate::utils::compute_variant_inputs_hash(&merged_variant_inputs)
                .unwrap_or_else(|_| crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string());

            let key = OverrideKey {
                resource_type: *resource_type,
                normalized_path,
                source,
                tool,
                variant_hash,
            };

            // Build the override info
            let override_info = ManifestOverride {
                filename: dep.get_filename().map(std::string::ToString::to_string),
                target: dep.get_target().map(std::string::ToString::to_string),
                install: dep.get_install(),
                manifest_alias: Some(name.clone()),
                template_vars: dep.get_template_vars().cloned(),
            };

            tracing::debug!(
                "Adding manifest override for {:?}:{} (tool={}, variant_hash={})",
                resource_type,
                dep.get_path(),
                key.tool,
                key.variant_hash
            );

            index.insert(key, override_info);
        }

        tracing::info!("Built manifest override index with {} entries", index.len());
        index
    }

    /// Resolve transitive dependencies starting from base dependencies.
    ///
    /// Discovers dependencies declared in resource files, expands patterns,
    /// builds dependency graph with cycle detection, and returns all dependencies
    /// in topological order.
    async fn resolve_transitive_dependencies(
        &mut self,
        base_deps: &[(String, ResourceDependency, ResourceType)],
        progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<Vec<(String, ResourceDependency, ResourceType)>> {
        use crate::resolver::transitive_resolver;

        // Build override index FIRST from manifest dependencies
        let manifest_overrides = self.build_manifest_override_index(base_deps);

        // Build ResolutionContext for the transitive resolver
        let resolution_ctx = ResolutionContext {
            manifest: self.core.manifest(),
            cache: self.core.cache(),
            source_manager: self.core.source_manager(),
            operation_context: self.core.operation_context(),
        };

        // Build TransitiveContext with concurrent state and the override index
        let mut ctx = TransitiveContext {
            base: resolution_ctx,
            dependency_map: &self.dependency_map,
            transitive_custom_names: &self.transitive_custom_names,
            conflict_detector: &mut self.conflict_detector,
            manifest_overrides: &manifest_overrides,
        };

        // Get prepared versions from version service (clone Arc for shared access)
        // Use prepared_versions_ready_arc to get only Ready versions, filtering out Preparing states
        let prepared_versions = self.version_service.prepared_versions_ready_arc();

        // Create services container
        let services = transitive_resolver::ResolutionServices {
            version_service: &self.version_service,
            pattern_service: &self.pattern_service,
        };

        // Call the service-based transitive resolver
        transitive_resolver::resolve_with_services(
            transitive_resolver::TransitiveResolutionParams {
                ctx: &mut ctx,
                core: &self.core,
                base_deps,
                enable_transitive: true,
                prepared_versions: &prepared_versions,
                pattern_alias_map: &self.pattern_alias_map,
                services: &services,
                progress,
            },
        )
        .await
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
        variant_hash: &str,
    ) -> Vec<String> {
        let key = (
            resource_type,
            name.to_string(),
            source.map(std::string::ToString::to_string),
            tool.map(std::string::ToString::to_string),
            variant_hash.to_string(),
        );
        let result = self.dependency_map.get(&key).map(|v| v.clone()).unwrap_or_default();
        tracing::debug!(
            "[DEBUG] get_dependencies_for: name='{}', type={:?}, source={:?}, tool={:?}, hash={}, found={} deps",
            name,
            resource_type,
            source,
            tool,
            &variant_hash[..8],
            result.len()
        );
        result
    }

    /// Get pattern alias for a concrete dependency.
    ///
    /// Returns the pattern name if this dependency was created from a pattern expansion.
    fn get_pattern_alias_for_dependency(
        &self,
        name: &str,
        resource_type: ResourceType,
    ) -> Option<String> {
        // Check if this dependency was created from a pattern expansion
        self.pattern_alias_map.get(&(resource_type, name.to_string())).map(|v| v.clone())
    }
}

#[cfg(test)]
mod resolver_tests {
    use super::*;

    #[tokio::test]
    async fn test_resolver_creation() -> Result<()> {
        let manifest = Manifest::default();
        let cache = Cache::new()?;
        DependencyResolver::new(manifest, cache).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_resolver_with_global() -> Result<()> {
        let manifest = Manifest::default();
        let cache = Cache::new()?;
        DependencyResolver::new_with_global(manifest, cache).await?;
        Ok(())
    }
}
