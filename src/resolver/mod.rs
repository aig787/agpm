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
pub mod lockfile_builder;
pub mod path_resolver;
pub mod pattern_expander;
pub mod resource_service;
pub mod sha_conflict_detector;
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
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;

use crate::cache::Cache;
use crate::core::{OperationContext, ResourceType};
use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;
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
    /// This method processes dependencies in batches with automatic retry logic
    /// for lock ordering violations. When the cache's `LockManager` detects
    /// potential deadlocks (out-of-order lock acquisition), dependencies are
    /// retried individually with exponential backoff.
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

            // Execute all futures in this batch concurrently
            let batch_results = join_all(batch_futures.into_iter().map(
                |(fut, name, dep, resource_type, progress_key, display_name)| {
                    let progress_clone = progress_clone.clone();
                    let counter_clone = completed_counter.clone();
                    async move {
                        let result = fut.await;

                        // Mark item as complete
                        if let Some(pm) = &progress_clone {
                            let completed =
                                counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
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
            ))
            .await;

            // Handle results and retry failed dependencies due to lock ordering violations
            let mut failed_deps = Vec::new();
            for (name, dep, resource_type, result) in batch_results {
                match result {
                    Ok(entries) => {
                        all_results.push(Ok((name, dep, resource_type, entries)));
                    }
                    Err(e) => {
                        // Check if this is a lock ordering violation that can be retried
                        if let Some(lock_order_error) = e.downcast_ref::<crate::core::AgpmError>() {
                            if let crate::core::AgpmError::LockOrderViolation {
                                held_locks: _,
                                requested_lock: _,
                            } = lock_order_error
                            {
                                tracing::debug!(
                                    "Dependency '{}' failed due to lock ordering, will retry individually",
                                    name
                                );
                                failed_deps.push((name, dep, resource_type, progress.clone()));
                            } else {
                                // Other type of AgpmError - don't retry
                                all_results.push(Err(e));
                            }
                        } else {
                            // Other type of error - don't retry
                            all_results.push(Err(e));
                        }
                    }
                }
            }

            // Retry failed dependencies individually using the retry function
            if !failed_deps.is_empty() {
                tracing::info!(
                    "Retrying {} dependencies due to lock ordering conflicts",
                    failed_deps.len()
                );

                for (name, dep, resource_type, progress_retry) in failed_deps {
                    let retry_result = self
                        .resolve_dependency_with_retry_loop(
                            name.clone(),
                            dep.clone(),
                            resource_type,
                            progress_retry,
                        )
                        .await;

                    match retry_result {
                        Ok(entries) => {
                            all_results.push(Ok((name, dep, resource_type, entries)));
                        }
                        Err(e) => {
                            all_results.push(Err(e));
                        }
                    }
                }
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
                + lockfile.mcp_servers.len();
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

    /// Resolve a dependency with retry logic to handle lock ordering violations.
    ///
    /// This function implements retry logic when the cache's LockManager detects
    /// potential deadlocks. The cache enforces alphabetical lock ordering, and
    /// when violations are detected, it returns LockOrderError which we handle
    /// here with a retry mechanism.
    ///
    /// # Arguments
    ///
    /// * `name` - The dependency name
    /// * `dep` - The dependency specification
    /// * `resource_type` - The type of resource
    /// * `progress` - Optional progress tracker
    ///
    /// # Returns
    ///
    /// A vector of locked resources if successful
    ///
    /// # Errors
    ///
    /// Returns an error if resolution fails after retry attempts or other errors occur
    async fn resolve_dependency_with_retry_loop(
        &mut self,
        name: String,
        dep: ResourceDependency,
        resource_type: ResourceType,
        _progress: Option<std::sync::Arc<crate::utils::MultiPhaseProgress>>,
    ) -> Result<Vec<LockedResource>> {
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;

        loop {
            // Attempt resolution - the cache's LockManager will enforce ordering
            let resolution_result = if dep.is_pattern() {
                self.resolve_pattern_dependency(&name, &dep, resource_type).await
            } else {
                self.resolve_dependency(&name, &dep, resource_type)
                    .await
                    .map(|resource| vec![resource])
            };

            match resolution_result {
                Ok(resources) => {
                    // Success! Return the resolved resources
                    return Ok(resources);
                }
                Err(e) => {
                    // Check if this is a lock ordering violation from the cache's LockManager
                    if let Some(crate::core::AgpmError::LockOrderViolation {
                        held_locks: _,
                        requested_lock: _,
                    }) = e.downcast_ref::<crate::core::AgpmError>()
                    {
                        tracing::debug!(
                            "Lock order violation detected for dependency '{}', retrying... ({}/{})",
                            name,
                            retry_count + 1,
                            MAX_RETRIES
                        );

                        // Prevent infinite retry loops
                        retry_count += 1;
                        if retry_count > MAX_RETRIES {
                            return Err(anyhow::anyhow!(
                                "Exceeded maximum retry attempts ({}) while resolving dependency '{}' due to lock ordering conflicts",
                                MAX_RETRIES,
                                name
                            ));
                        }

                        // Exponential backoff before retry
                        let delay_ms = 100 * (2_u64.pow(retry_count - 1));
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

                        continue;
                    }

                    // Other type of error - propagate it
                    return Err(e);
                }
            }
        }
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
        let prepared_versions = self.version_service.prepared_versions_arc();

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

    /// Resolve a single dependency to a lockfile entry.
    ///
    /// Delegates to specialized resolvers based on dependency type.
    async fn resolve_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        tracing::debug!(
            "resolve_dependency: name={}, path={}, source={:?}, is_local={}",
            name,
            dep.get_path(),
            dep.get_source(),
            dep.is_local()
        );

        if dep.is_local() {
            self.resolve_local_dependency(name, dep, resource_type)
        } else {
            self.resolve_git_dependency(name, dep, resource_type).await
        }
    }

    /// Determine the filename for a dependency.
    ///
    /// Returns the custom filename if specified, otherwise extracts
    /// a meaningful name from the dependency path.
    fn resolve_filename(dep: &ResourceDependency) -> String {
        dep.get_filename()
            .map_or_else(|| extract_meaningful_path(Path::new(dep.get_path())), |f| f.to_string())
    }

    /// Get the tool/artifact type for a dependency.
    ///
    /// Returns the explicitly specified tool or the default tool for the resource type.
    fn resolve_tool(&self, dep: &ResourceDependency, resource_type: ResourceType) -> String {
        dep.get_tool()
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.core.manifest().get_default_tool(resource_type))
    }

    /// Determine manifest_alias for a dependency.
    ///
    /// Returns Some for direct manifest dependencies or pattern-expanded dependencies,
    /// None for transitive dependencies.
    fn resolve_manifest_alias(&self, name: &str, resource_type: ResourceType) -> Option<String> {
        let has_pattern_alias = self.get_pattern_alias_for_dependency(name, resource_type);
        let is_in_manifest = self
            .core
            .manifest()
            .get_dependencies(resource_type)
            .is_some_and(|deps| deps.contains_key(name));

        if let Some(pattern_alias) = has_pattern_alias {
            // Pattern-expanded dependency - use pattern name as manifest_alias
            Some(pattern_alias)
        } else if is_in_manifest {
            // Direct manifest dependency - use name as manifest_alias
            Some(name.to_string())
        } else {
            // Transitive dependency - no manifest_alias
            None
        }
    }

    /// Resolve local file system dependency to locked resource.
    fn resolve_local_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        use crate::resolver::lockfile_builder;
        use crate::resolver::path_resolver as install_path_resolver;
        use crate::utils::normalize_path_for_storage;

        let filename = Self::resolve_filename(dep);
        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        let installed_at = install_path_resolver::resolve_install_path(
            self.core.manifest(),
            dep,
            artifact_type,
            resource_type,
            &filename,
        )?;

        let manifest_alias = self.resolve_manifest_alias(name, resource_type);

        tracing::debug!(
            "Local dependency: name={}, path={}, manifest_alias={:?}",
            name,
            dep.get_path(),
            manifest_alias
        );

        let applied_patches = lockfile_builder::get_patches_for_resource(
            self.core.manifest(),
            resource_type,
            name,
            manifest_alias.as_deref(),
        );

        // Generate canonical name for local dependencies
        // For transitive dependencies (manifest_alias=None), use the name as-is since it's
        // already the correct relative path computed by the transitive resolver
        // For direct dependencies (manifest_alias=Some), normalize the path
        let canonical_name = self.compute_local_canonical_name(name, dep, &manifest_alias)?;

        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        Ok(LockedResource {
            name: canonical_name,
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
                variant_inputs.hash(),
            ),
            resource_type,
            tool: Some(artifact_type_string),
            manifest_alias,
            applied_patches,
            install: dep.get_install(),
            variant_inputs,
            context_checksum: None,
        })
    }

    /// Compute canonical name for local dependencies.
    ///
    /// For transitive dependencies (manifest_alias=None), returns name as-is.
    /// For direct dependencies (manifest_alias=Some), normalizes path relative to manifest.
    fn compute_local_canonical_name(
        &self,
        name: &str,
        dep: &ResourceDependency,
        manifest_alias: &Option<String>,
    ) -> Result<String> {
        if manifest_alias.is_none() {
            // Transitive dependency - name is already correct (e.g., "../snippets/agents/backend-engineer")
            Ok(name.to_string())
        } else if let Some(manifest_dir) = self.core.manifest().manifest_dir.as_ref() {
            // Direct dependency - normalize path relative to manifest
            let full_path = if Path::new(dep.get_path()).is_absolute() {
                PathBuf::from(dep.get_path())
            } else {
                manifest_dir.join(dep.get_path())
            };

            // Normalize the path to handle ../ and ./ components deterministically
            let canonical_path = crate::utils::fs::normalize_path(&full_path);

            let source_context =
                crate::resolver::source_context::SourceContext::local(manifest_dir);
            Ok(generate_dependency_name(&canonical_path.to_string_lossy(), &source_context))
        } else {
            // Fallback to name if manifest_dir is not available
            Ok(name.to_string())
        }
    }

    /// Resolve Git-based dependency to locked resource.
    async fn resolve_git_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<LockedResource> {
        use crate::resolver::lockfile_builder;
        use crate::resolver::path_resolver as install_path_resolver;
        use crate::utils::normalize_path_for_storage;

        let source_name = dep
            .get_source()
            .ok_or_else(|| anyhow::anyhow!("Dependency '{}' has no source specified", name))?;

        // Generate canonical name using remote source context
        let source_context = crate::resolver::source_context::SourceContext::remote(source_name);
        let canonical_name = generate_dependency_name(dep.get_path(), &source_context);

        let source_url = self
            .core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let version_key = dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
        let group_key = format!("{}::{}", source_name, version_key);

        let prepared = self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
            anyhow::anyhow!(
                "Prepared state missing for source '{}' @ '{}'",
                source_name,
                version_key
            )
        })?;

        let filename = Self::resolve_filename(dep);
        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        let installed_at = install_path_resolver::resolve_install_path(
            self.core.manifest(),
            dep,
            artifact_type,
            resource_type,
            &filename,
        )?;

        let manifest_alias = self.resolve_manifest_alias(name, resource_type);

        let applied_patches = lockfile_builder::get_patches_for_resource(
            self.core.manifest(),
            resource_type,
            name,
            manifest_alias.as_deref(),
        );

        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        // Extract data from prepared before storing variant_inputs
        let resolved_version = prepared.resolved_version.clone();
        let resolved_commit = prepared.resolved_commit.clone();

        // Store variant_inputs in PreparedSourceVersion for backtracking
        // DashMap allows concurrent inserts, so we don't need mutable access
        let resource_id = format!("{}:{}", source_name, dep.get_path());
        prepared.resource_variants.insert(resource_id, Some(variant_inputs.json().clone()));

        Ok(LockedResource {
            name: canonical_name,
            source: Some(source_name.to_string()),
            url: Some(source_url.clone()),
            path: normalize_path_for_storage(dep.get_path()),
            version: resolved_version,
            resolved_commit: Some(resolved_commit),
            checksum: String::new(),
            installed_at,
            dependencies: self.get_dependencies_for(
                name,
                Some(source_name),
                resource_type,
                Some(&artifact_type_string),
                variant_inputs.hash(),
            ),
            resource_type,
            tool: Some(artifact_type_string),
            manifest_alias,
            applied_patches,
            install: dep.get_install(),
            variant_inputs,
            context_checksum: None,
        })
    }

    /// Resolve a pattern dependency to multiple locked resources.
    ///
    /// Delegates to local or Git pattern resolvers based on dependency type.
    async fn resolve_pattern_dependency(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        if !dep.is_pattern() {
            return Err(anyhow::anyhow!(
                "Expected pattern dependency but no glob characters found in path"
            ));
        }

        if dep.is_local() {
            self.resolve_local_pattern(name, dep, resource_type)
        } else {
            self.resolve_git_pattern(name, dep, resource_type).await
        }
    }

    /// Resolve local pattern dependency to multiple locked resources.
    fn resolve_local_pattern(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        use crate::pattern::PatternResolver;
        use crate::resolver::{lockfile_builder, path_resolver};

        let pattern = dep.get_path();
        let (base_path, pattern_str) = path_resolver::parse_pattern_base_path(pattern);
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(&pattern_str, &base_path)?;

        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        // Compute variant inputs once for all matched files in the pattern
        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        let mut resources = Vec::new();
        for matched_path in matches {
            let resource_name = crate::pattern::extract_resource_name(&matched_path);
            let full_relative_path =
                path_resolver::construct_full_relative_path(&base_path, &matched_path);
            let filename = path_resolver::extract_pattern_filename(&base_path, &matched_path);

            let installed_at = path_resolver::resolve_install_path(
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
                    &resource_name, // Use canonical resource name
                    Some(name),     // Use manifest_alias for patch lookups
                ),
                install: dep.get_install(),
                variant_inputs: variant_inputs.clone(),
                context_checksum: None,
            });
        }

        Ok(resources)
    }

    /// Resolve Git-based pattern dependency to multiple locked resources.
    async fn resolve_git_pattern(
        &self,
        name: &str,
        dep: &ResourceDependency,
        resource_type: ResourceType,
    ) -> Result<Vec<LockedResource>> {
        use crate::pattern::PatternResolver;
        use crate::resolver::{lockfile_builder, path_resolver};
        use crate::utils::{
            compute_relative_install_path, normalize_path, normalize_path_for_storage,
        };

        let pattern = dep.get_path();
        let pattern_name = name;

        let source_name = dep.get_source().ok_or_else(|| {
            anyhow::anyhow!("Pattern dependency '{}' has no source specified", name)
        })?;

        let source_url = self
            .core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let version_key = dep.get_version().map_or_else(|| "HEAD".to_string(), |v| v.to_string());
        let group_key = format!("{}::{}", source_name, version_key);

        let prepared = self.version_service.get_prepared_version(&group_key).ok_or_else(|| {
            anyhow::anyhow!(
                "Prepared state missing for source '{}' @ '{}'",
                source_name,
                version_key
            )
        })?;

        // Extract data from prepared before mutable borrow (needed for loop)
        let worktree_path = prepared.worktree_path.clone();
        let resolved_version = prepared.resolved_version.clone();
        let resolved_commit = prepared.resolved_commit.clone();

        let repo_path = Path::new(&worktree_path);
        let pattern_resolver = PatternResolver::new();
        let matches = pattern_resolver.resolve(pattern, repo_path)?;

        let artifact_type_string = self.resolve_tool(dep, resource_type);
        let artifact_type = artifact_type_string.as_str();

        // Compute variant inputs once for all matched files in the pattern
        let variant_inputs = lockfile_builder::VariantInputs::new(
            lockfile_builder::build_merged_variant_inputs(self.core.manifest(), dep),
        );

        let mut resources = Vec::new();
        for matched_path in matches {
            let resource_name = crate::pattern::extract_resource_name(&matched_path);

            // Compute installation path
            let installed_at = match resource_type {
                ResourceType::Hook | ResourceType::McpServer => {
                    path_resolver::resolve_merge_target_path(
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
                    let relative_path =
                        compute_relative_install_path(&base_target, Path::new(&filename), flatten);
                    normalize_path_for_storage(normalize_path(&base_target.join(relative_path)))
                }
            };

            // Store variant_inputs in PreparedSourceVersion for backtracking
            // DashMap allows concurrent inserts, so we access through regular get()
            let resource_id = format!("{}:{}", source_name, matched_path.to_string_lossy());
            if let Some(prepared_ref) = self.version_service.get_prepared_version(&group_key) {
                prepared_ref
                    .resource_variants
                    .insert(resource_id, Some(variant_inputs.json().clone()));
            }

            resources.push(LockedResource {
                name: resource_name.clone(),
                source: Some(source_name.to_string()),
                url: Some(source_url.clone()),
                path: normalize_path_for_storage(matched_path.to_string_lossy().to_string()),
                version: resolved_version.clone(),
                resolved_commit: Some(resolved_commit.clone()),
                checksum: String::new(),
                installed_at,
                dependencies: vec![],
                resource_type,
                tool: Some(artifact_type_string.clone()),
                manifest_alias: Some(pattern_name.to_string()),
                applied_patches: lockfile_builder::get_patches_for_resource(
                    self.core.manifest(),
                    resource_type,
                    &resource_name,     // Use canonical resource name
                    Some(pattern_name), // Use manifest_alias for patch lookups
                ),
                install: dep.get_install(),
                variant_inputs: variant_inputs.clone(),
                context_checksum: None,
            });
        }

        Ok(resources)
    }

    /// Add or update a lockfile entry with deduplication.
    fn add_or_update_lockfile_entry(&self, lockfile: &mut LockFile, entry: LockedResource) {
        let resources = lockfile.get_resources_mut(&entry.resource_type);

        if let Some(existing) =
            resources.iter_mut().find(|e| lockfile_builder::is_duplicate_entry(e, &entry))
        {
            // Use the lockfile_builder's deterministic merge strategy
            // This ensures consistent behavior regardless of processing order
            if lockfile_builder::should_replace_duplicate(existing, &entry) {
                tracing::debug!(
                    "Replacing {} (manifest_alias={:?}) with {} (manifest_alias={:?})",
                    existing.name,
                    existing.manifest_alias,
                    entry.name,
                    entry.manifest_alias
                );
                *existing = entry;
            } else {
                tracing::debug!(
                    "Keeping {} (manifest_alias={:?}) over {} (manifest_alias={:?})",
                    existing.name,
                    existing.manifest_alias,
                    entry.name,
                    entry.manifest_alias
                );
            }
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

    /// Track a resolved dependency for later conflict detection.
    fn track_resolved_dependency_for_conflicts(
        &mut self,
        name: &str,
        dep: &ResourceDependency,
        locked_entry: &LockedResource,
        resource_type: ResourceType,
    ) {
        // Skip if install=false (content-only dependencies don't conflict)
        if dep.get_install() == Some(false) {
            tracing::debug!(
                "Skipping conflict tracking for content-only dependency '{}' (install=false)",
                name
            );
            return;
        }

        // Skip local dependencies (no version conflicts possible)
        if dep.is_local() {
            return;
        }

        // Build a unique resource identifier that includes variant/context information
        let resource_id = Self::build_resource_identity(dep, locked_entry, resource_type);

        // Get version constraint (None means HEAD/unspecified)
        let version_constraint = dep.get_version().unwrap_or("HEAD");

        // Get resolved SHA from locked entry
        let resolved_sha = locked_entry.resolved_commit.as_deref().unwrap_or("");

        // Skip if no resolved commit (shouldn't happen for Git deps, but be safe)
        if resolved_sha.is_empty() {
            tracing::warn!("Skipping conflict tracking for '{}': no resolved commit", name);
            return;
        }

        // Determine parent resources using the reverse dependency map populated by previously
        // processed parents (topological order ensures parents run first).
        let current_dep_ref =
            LockfileDependencyRef::local(resource_type, locked_entry.name.clone(), None)
                .to_string();

        if let Some(required_by_list) = self.reverse_dependency_map.get(&current_dep_ref) {
            // Transitive dependency - track all parents with their metadata
            for required_by in required_by_list.value() {
                // Look up parent metadata from resolved_deps_for_conflict_check
                // Format of required_by: "type/name" (e.g., "agents/agent-a")
                let (parent_version, parent_sha) = self.lookup_parent_metadata(required_by);

                tracing::debug!(
                    "TRACK: TRANSITIVE resource_id='{}' required_by='{}' version='{}' SHA={} parent_version={:?} parent_sha={:?}",
                    resource_id,
                    required_by,
                    version_constraint,
                    &resolved_sha[..8.min(resolved_sha.len())],
                    parent_version,
                    parent_sha.as_ref().map(|s| &s[..8.min(s.len())])
                );

                let key = (resource_id.clone(), required_by.to_string(), name.to_string());
                let dependency_info = ResolvedDependencyInfo {
                    version_constraint: version_constraint.to_string(),
                    resolved_sha: resolved_sha.to_string(),
                    parent_version,
                    parent_sha,
                    resolution_mode: dep.resolution_mode(),
                };
                self.resolved_deps_for_conflict_check.insert(key, dependency_info);
            }
        } else {
            // Direct dependency from manifest - no parent
            tracing::debug!(
                "TRACK: DIRECT resource_id='{}' required_by='manifest' version='{}' SHA={}",
                resource_id,
                version_constraint,
                &resolved_sha[..8.min(resolved_sha.len())]
            );

            let key = (resource_id.clone(), "manifest".to_string(), name.to_string());
            let dependency_info = ResolvedDependencyInfo {
                version_constraint: version_constraint.to_string(),
                resolved_sha: resolved_sha.to_string(),
                parent_version: None,
                parent_sha: None,
                resolution_mode: dep.resolution_mode(),
            };
            self.resolved_deps_for_conflict_check.insert(key, dependency_info);
        }

        tracing::debug!(
            "Tracked for conflict detection: '{}' (resource_id: {}, constraint: {}, sha: {})",
            name,
            resource_id,
            version_constraint,
            &resolved_sha[..8.min(resolved_sha.len())],
        );

        // Record reverse dependency relationships for future child lookups.
        for child_ref in &locked_entry.dependencies {
            self.reverse_dependency_map
                .entry(child_ref.clone())
                .or_default()
                .value_mut()
                .push(current_dep_ref.clone());
        }
    }

    /// Look up parent resource metadata from already-tracked dependencies.
    ///
    /// For a parent resource ID like "agents/agent-a", searches for an entry
    /// where the parent was tracked as a direct dependency or transitive dependency.
    ///
    /// Returns (parent_version_constraint, parent_resolved_sha) if found.
    fn lookup_parent_metadata(&self, parent_id: &str) -> (Option<String>, Option<String>) {
        // Normalize the parent ID to just the dependency path without extensions.
        // Handles formats like "snippet:snippets/foo", "source/snippet:snippets/foo@v1", etc.
        let normalized_parent_path = LockfileDependencyRef::from_str(parent_id)
            .map(|dep| dep.path)
            .unwrap_or_else(|_| {
                parent_id
                    .split('@')
                    .next()
                    .and_then(|s| s.split(':').next_back())
                    .unwrap_or(parent_id)
                    .to_string()
            })
            .trim_end_matches(".md")
            .trim_end_matches(".json")
            .to_string();

        // Search for an entry where resource_id name matches the parent path
        // E.g., for parent_path = "agents/agent-a", look for resource_id with name = "agents/agent-a"
        for entry in self.resolved_deps_for_conflict_check.iter() {
            let ((resource_id, _required_by, _name), dependency_info) =
                (entry.key(), entry.value());
            let ResolvedDependencyInfo {
                version_constraint,
                resolved_sha,
                parent_version: _,
                parent_sha: _,
                resolution_mode: _,
            } = dependency_info;

            // The ResourceId name is the canonical resource name (e.g., "agents/agent-a")
            // Compare directly with normalized parent path
            if resource_id.name() == normalized_parent_path {
                return (Some(version_constraint.clone()), Some(resolved_sha.clone()));
            }
        }

        // Not found - parent might not have been tracked yet (ordering issue)
        (None, None)
    }

    /// Build a unique resource identity string plus user-facing display identifier.
    ///
    /// The display identifier is `source:path`, while the unique identifier includes
    /// additional disambiguators (name/tool/variant) so distinct variants of the
    /// same file do not collide in conflict tracking.
    ///
    /// **Note**: `manifest_alias` is intentionally NOT included in the unique ID.
    /// Different manifest aliases pointing to the same resource with different versions
    /// (e.g., `agent-v1` and `agent-v2` both pointing to `agents/agent-a.md`) should be
    /// detected as version conflicts. Including `manifest_alias` would make them appear
    /// as separate resources and prevent conflict detection.
    fn build_resource_identity(
        dep: &ResourceDependency,
        locked_entry: &LockedResource,
        resource_type: ResourceType,
    ) -> crate::lockfile::ResourceId {
        let source = locked_entry.source.as_deref().or_else(|| dep.get_source());
        let tool = locked_entry.tool.clone().or_else(|| dep.get_tool().map(str::to_string));
        let variant_hash = locked_entry.variant_inputs.hash().to_string();

        crate::lockfile::ResourceId::new(
            &locked_entry.name,
            source,
            tool.as_deref(),
            resource_type,
            variant_hash,
        )
    }

    /// Apply backtracking updates to the resolver state.
    ///
    /// # Algorithm Overview
    ///
    /// 1. **Parse Resource IDs**: Extract source name from "source:required_by" format
    /// 2. **Retrieve Source Configuration**: Get source URL from source manager
    /// 3. **Create Worktrees**: Generate worktree for each updated SHA
    /// 4. **Update Version Service**: Modify PreparedSourceVersion entries
    /// 5. **Handle Key Format**: Process keys as "source::version_constraint"
    ///
    /// This method ensures that all resolver state reflects the new versions
    /// found during backtracking, updating both worktree paths and resolved
    /// commit SHAs to maintain consistency across the resolution process.
    ///
    /// # Arguments
    ///
    /// * `updates` - List of version updates from backtracking
    ///
    /// # Errors
    ///
    /// Returns an error if source lookup fails or worktree creation encounters issues
    async fn apply_backtracking_updates(
        &mut self,
        updates: &[backtracking::VersionUpdate],
    ) -> Result<()> {
        tracing::debug!("Applying {} backtracking update(s)", updates.len());

        for update in updates {
            // Parse resource_id: "source:required_by"
            let parts: Vec<&str> = update.resource_id.splitn(2, ':').collect();
            if parts.len() != 2 {
                tracing::warn!("Invalid resource_id format: {}", update.resource_id);
                continue;
            }
            let source_name = parts[0];
            let _required_by = parts[1];

            // Get source URL
            let source_url = self
                .core
                .source_manager()
                .get_source_url(source_name)
                .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

            // Create worktree for the new SHA
            tracing::debug!(
                "Creating worktree for {}@{} (SHA: {})",
                source_name,
                update.new_version,
                &update.new_sha[..8.min(update.new_sha.len())]
            );

            let worktree_path = self
                .core
                .cache()
                .get_or_create_worktree_for_sha(
                    source_name,
                    &source_url,
                    &update.new_sha,
                    Some(source_name),
                )
                .await?;

            // Update PreparedSourceVersion in version service
            // The key format is "source::version_constraint"
            // We need to update entries that match this source and old version
            let prepared_versions = self.version_service.prepared_versions();

            // Find and update the entry
            // Note: The key uses the constraint, not the resolved version
            // We need to find which constraint resolved to the old version
            for mut entry in prepared_versions.iter_mut() {
                let (key, prepared) = entry.pair_mut();
                if key.starts_with(&format!("{}::", source_name))
                    && prepared.resolved_commit == update.old_sha
                {
                    tracing::debug!("Updating prepared version key: {}", key);
                    prepared.worktree_path = worktree_path.clone();
                    prepared.resolved_version = Some(update.new_version.clone());
                    prepared.resolved_commit = update.new_sha.clone();
                }
            }
        }

        Ok(())
    }

    /// Update lockfile entries after backtracking.
    ///
    /// Finds entries with old SHAs and updates them with new SHAs and worktree paths.
    ///
    /// # Arguments
    ///
    /// * `lockfile` - The lockfile to update
    /// * `updates` - List of version updates from backtracking
    ///
    /// # Errors
    ///
    /// Returns an error if updates cannot be applied
    fn update_lockfile_entries(
        &self,
        lockfile: &mut LockFile,
        updates: &[backtracking::VersionUpdate],
    ) -> Result<()> {
        tracing::debug!("Updating lockfile entries for {} backtracking update(s)", updates.len());

        for update in updates {
            // Parse resource_id: "source:required_by"
            let parts: Vec<&str> = update.resource_id.splitn(2, ':').collect();
            if parts.len() != 2 {
                tracing::warn!("Invalid resource_id format: {}", update.resource_id);
                continue;
            }
            let source_name = parts[0];

            // Find all lockfile entries with the old SHA
            // Update them to use the new SHA and worktree path
            for resource_type in [
                ResourceType::Agent,
                ResourceType::Snippet,
                ResourceType::Command,
                ResourceType::Script,
                ResourceType::Hook,
                ResourceType::McpServer,
            ] {
                let resources = lockfile.get_resources_mut(&resource_type);

                for resource in resources.iter_mut() {
                    // Check if this resource matches: same source and old SHA
                    let matches = resource.source.as_deref() == Some(source_name)
                        && resource.resolved_commit.as_deref() == Some(&update.old_sha);

                    if matches {
                        tracing::debug!(
                            "Updating lockfile entry: {} (SHA: {} → {})",
                            resource.name,
                            &update.old_sha[..8.min(update.old_sha.len())],
                            &update.new_sha[..8.min(update.new_sha.len())]
                        );

                        // Update the resolved commit
                        resource.resolved_commit = Some(update.new_sha.clone());

                        // Update the version if present
                        if resource.version.is_some() {
                            resource.version = Some(update.new_version.clone());
                        }

                        // Note: installed_at path doesn't change - it's the target path
                        // The source path is implicitly from the updated worktree
                    }
                }
            }
        }

        Ok(())
    }

    /// Create a filtered manifest containing only specified dependencies.
    ///
    /// This method creates a new manifest with the same sources and configuration
    /// as the original, but with only the specified dependencies included.
    ///
    /// # Arguments
    ///
    /// * `deps_to_update` - List of (name, resource_type) pairs to include
    ///
    /// # Returns
    ///
    /// A filtered Manifest containing only the specified dependencies
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let filtered = self.create_filtered_manifest(&[
    ///     ("agent1".to_string(), ResourceType::Agent),
    ///     ("snippet2".to_string(), ResourceType::Snippet),
    /// ]);
    /// // filtered contains only agent1 and snippet2
    /// ```
    fn create_filtered_manifest(&self, deps_to_update: &[(String, ResourceType)]) -> Manifest {
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
                };
                target_map.insert(dep_name.clone(), dep_spec.clone());
            }
        }

        filtered
    }

    /// Filter lockfile entries into unchanged and to-update groups.
    ///
    /// This method separates lockfile entries based on whether they match any of
    /// the dependency names in `deps_to_update`. Matching is done against both
    /// `manifest_alias` (for direct dependencies) and `name` (for transitive).
    ///
    /// # Arguments
    ///
    /// * `existing` - The current lockfile to filter
    /// * `deps_to_update` - List of dependency names to update
    ///
    /// # Returns
    ///
    /// A tuple of (unchanged_lockfile, deps_requiring_resolution):
    /// - `unchanged_lockfile`: Entries that should remain at their current versions
    /// - `deps_requiring_resolution`: List of (name, resource_type) pairs to resolve
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let (unchanged, to_resolve) = self.filter_lockfile_entries(&lockfile, &["agent1", "snippet2"]);
    /// // unchanged contains all entries except agent1 and snippet2
    /// // to_resolve contains [("agent1", ResourceType::Agent), ("snippet2", ResourceType::Snippet)]
    /// ```
    fn filter_lockfile_entries(
        existing: &LockFile,
        deps_to_update: &[String],
    ) -> (LockFile, Vec<(String, ResourceType)>) {
        use std::collections::HashSet;

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
    /// This method combines entries from two lockfiles, with updated entries
    /// taking precedence over unchanged entries when conflicts occur (same
    /// name, source, tool, and variant_inputs_hash).
    ///
    /// # Arguments
    ///
    /// * `unchanged` - Lockfile entries that were not re-resolved
    /// * `updated` - Lockfile entries from selective resolution
    ///
    /// # Returns
    ///
    /// A new LockFile containing all entries, with updated entries winning conflicts
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let merged = self.merge_lockfiles(unchanged, updated);
    /// // merged contains all entries from both lockfiles
    /// // Updated entries replace unchanged entries with matching identity
    /// ```
    fn merge_lockfiles(mut unchanged: LockFile, updated: LockFile) -> LockFile {
        use std::collections::HashSet;

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
