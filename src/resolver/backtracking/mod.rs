//! Automatic version backtracking for SHA conflict resolution.
//!
//! This module implements automatic resolution of version conflicts by finding
//! alternative versions that satisfy all constraints and resolve to the same commit SHA.
//!
//! # Algorithm
//!
//! When SHA conflicts are detected (multiple requirements for the same resource resolving
//! to different commits), the backtracking resolver attempts to find compatible versions:
//!
//! 1. **Query available versions**: Fetch all tags from the Git repository
//! 2. **Filter by constraints**: Find versions satisfying all requirements
//! 3. **Try alternatives**: Test versions in preference order (latest first)
//! 4. **Verify SHA match**: Check if alternative version resolves to same SHA as other requirements
//! 5. **Handle transitive deps**: Re-resolve transitive dependencies after version changes
//! 6. **Iterate if needed**: Continue until all conflicts resolved or limits reached
//!
//! # Performance Limits
//!
//! To prevent excessive computation:
//! - Maximum 100 version resolution attempts per conflict
//! - 10-second timeout for entire backtracking process
//! - Early termination if no progress made

mod algorithm;
mod registry;
mod transitive;
mod types;

use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::resolver::ResolutionCore;
use crate::resolver::version_resolver::VersionResolutionService;
use crate::version::conflict::{ConflictDetector, VersionConflict};

pub use registry::{ResourceParams, parse_resource_id_string, resource_id_to_string};
pub use types::{BacktrackingIteration, BacktrackingResult, TerminationReason, VersionUpdate};

use registry::{ResourceRegistry, TransitiveChangeTracker};

/// Maximum number of version resolution attempts before giving up
const MAX_ATTEMPTS: usize = 100;

/// Maximum duration for backtracking before timeout (increased for transitive resolution)
const MAX_DURATION: Duration = Duration::from_secs(10);

/// Maximum number of backtracking iterations before giving up
const MAX_ITERATIONS: usize = 10;

/// Automatic version backtracking resolver.
///
/// Attempts to resolve SHA conflicts by finding alternative versions
/// that satisfy all constraints and resolve to the same commit.
pub struct BacktrackingResolver<'a> {
    /// Core resolution context with manifest, cache, and source manager
    core: &'a ResolutionCore,

    /// Version resolution service for Git operations
    version_service: &'a mut VersionResolutionService,

    /// Maximum version resolution attempts
    max_attempts: usize,

    /// Maximum duration before timeout
    timeout: Duration,

    /// Start time for timeout tracking
    start_time: Instant,

    /// Number of attempts made so far
    attempts: usize,

    /// Tracks resources whose versions changed (need transitive re-resolution)
    change_tracker: TransitiveChangeTracker,

    /// Iteration history for debugging and oscillation detection
    iteration_history: Vec<BacktrackingIteration>,

    /// Maximum iterations before giving up
    max_iterations: usize,

    /// Registry of all resources for conflict detection after version changes
    resource_registry: ResourceRegistry,
}

impl<'a> BacktrackingResolver<'a> {
    /// Create a new backtracking resolver with default limits.
    pub fn new(
        core: &'a ResolutionCore,
        version_service: &'a mut VersionResolutionService,
    ) -> Self {
        Self {
            core,
            version_service,
            max_attempts: MAX_ATTEMPTS,
            timeout: MAX_DURATION,
            start_time: Instant::now(),
            attempts: 0,
            change_tracker: TransitiveChangeTracker::new(),
            iteration_history: Vec::new(),
            max_iterations: MAX_ITERATIONS,
            resource_registry: ResourceRegistry::new(),
        }
    }

    /// Populate the resource registry from a ConflictDetector.
    ///
    /// This extracts all requirements from the conflict detector and builds
    /// a complete resource registry for conflict detection during backtracking.
    pub fn populate_from_conflict_detector(&mut self, conflict_detector: &ConflictDetector) {
        let requirements = conflict_detector.requirements();

        let mut skipped_count = 0;
        let mut processed_count = 0;

        for (resource_id, reqs) in requirements {
            if resource_id_to_string(resource_id).is_err() {
                let tool_info =
                    resource_id.tool().map(|t| format!("tool: {}", t)).unwrap_or_default();
                let type_info = format!("type: {}", resource_id.resource_type());

                tracing::warn!(
                    "Skipping resource without source: {} (name: {}, {}, {} requirements: {})",
                    resource_id,
                    resource_id.name(),
                    type_info,
                    tool_info,
                    reqs.len()
                );

                skipped_count += 1;
                continue;
            }

            processed_count += 1;

            for req in reqs {
                self.resource_registry.add_or_update_resource(ResourceParams {
                    resource_id: resource_id.clone(),
                    version: req.requirement.clone(),
                    sha: req.resolved_sha.clone(),
                    version_constraint: req.requirement.clone(),
                    required_by: req.required_by.clone(),
                });
            }
        }

        if skipped_count > 0 {
            tracing::info!(
                "Population complete: processed {} resources, skipped {} without source",
                processed_count,
                skipped_count
            );
        } else {
            tracing::debug!(
                "Population complete: processed {} resources, no local resources skipped",
                processed_count
            );
        }
    }

    /// Create a backtracking resolver with custom limits (for testing).
    #[allow(dead_code)]
    pub fn with_limits(
        core: &'a ResolutionCore,
        version_service: &'a mut VersionResolutionService,
        max_attempts: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            core,
            version_service,
            max_attempts,
            timeout,
            start_time: Instant::now(),
            attempts: 0,
            change_tracker: TransitiveChangeTracker::new(),
            iteration_history: Vec::new(),
            max_iterations: MAX_ITERATIONS,
            resource_registry: ResourceRegistry::new(),
        }
    }

    /// Attempt to resolve conflicts by finding compatible versions.
    pub async fn resolve_conflicts(
        &mut self,
        initial_conflicts: &[VersionConflict],
    ) -> Result<BacktrackingResult> {
        tracing::debug!(
            "Starting iterative backtracking for {} conflict(s), limits: {} iterations, {} attempts, {}s timeout",
            initial_conflicts.len(),
            self.max_iterations,
            self.max_attempts,
            self.timeout.as_secs()
        );

        let mut current_conflicts = initial_conflicts.to_vec();
        let mut all_updates = Vec::new();
        let mut total_transitive = 0;

        for iteration_num in 1..=self.max_iterations {
            tracing::debug!("=== Backtracking iteration {} ===", iteration_num);
            tracing::debug!("Processing {} conflict(s)", current_conflicts.len());

            if self.start_time.elapsed() > self.timeout {
                tracing::warn!("Backtracking timeout after {:?}", self.start_time.elapsed());
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::Timeout,
                ));
            }

            let mut iteration_updates = Vec::new();
            for conflict in &current_conflicts {
                match self.resolve_single_conflict(conflict).await? {
                    Some(update) => {
                        tracing::debug!(
                            "Resolved conflict for {}: {} → {}",
                            conflict.resource,
                            update.old_version,
                            update.new_version
                        );
                        iteration_updates.push(update);
                    }
                    None => {
                        tracing::debug!("Could not resolve conflict for {}", conflict.resource);
                        return Ok(self.build_result(
                            false,
                            all_updates,
                            total_transitive,
                            TerminationReason::NoCompatibleVersion,
                        ));
                    }
                }
            }

            if iteration_updates.is_empty() {
                tracing::debug!("No updates found in iteration {}", iteration_num);
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::NoCompatibleVersion,
                ));
            }

            for update in &iteration_updates {
                self.change_tracker.record_change(
                    &update.resource_id,
                    &update.old_version,
                    &update.new_version,
                    &update.new_sha,
                    update.variant_inputs.clone(),
                );

                self.resource_registry.update_version_and_sha(
                    &update.resource_id,
                    update.new_version.clone(),
                    update.new_sha.clone(),
                );
            }

            all_updates.extend(iteration_updates.clone());

            tracing::debug!(
                "Re-extracting transitive deps for {} changed resource(s)",
                self.change_tracker.get_changed_resources().len()
            );
            let transitive_count = transitive::reextract_transitive_deps(
                self.core,
                self.version_service,
                &mut self.change_tracker,
            )
            .await?;
            total_transitive += transitive_count;

            if transitive_count > 0 {
                tracing::debug!("Re-resolved {} transitive dependency(ies)", transitive_count);
            }

            let new_conflicts = transitive::detect_conflicts_after_changes(&self.resource_registry);
            tracing::debug!(
                "After iteration {}: {} conflict(s) remaining",
                iteration_num,
                new_conflicts.len()
            );

            self.iteration_history.push(BacktrackingIteration {
                iteration: iteration_num,
                conflicts: current_conflicts.clone(),
                updates: iteration_updates,
                transitive_reresolutions: transitive_count,
                made_progress: !new_conflicts.is_empty() || transitive_count > 0,
            });

            if new_conflicts.is_empty() {
                tracing::info!(
                    "✓ Resolved all conflicts after {} iteration(s), {} version update(s), {} transitive re-resolution(s)",
                    iteration_num,
                    all_updates.len(),
                    total_transitive
                );
                return Ok(self.build_result(
                    true,
                    all_updates,
                    total_transitive,
                    TerminationReason::Success,
                ));
            }

            if algorithm::conflicts_equal(&current_conflicts, &new_conflicts) {
                tracing::warn!(
                    "No progress made in iteration {}: same conflicts remain",
                    iteration_num
                );
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::NoProgress,
                ));
            }

            if self.detect_oscillation(&new_conflicts) {
                tracing::warn!("Oscillation detected in iteration {}", iteration_num);
                return Ok(self.build_result(
                    false,
                    all_updates,
                    total_transitive,
                    TerminationReason::Oscillation,
                ));
            }

            current_conflicts = new_conflicts;
        }

        tracing::warn!(
            "Reached max iterations ({}) without resolving all conflicts. {} conflict(s) remaining",
            self.max_iterations,
            current_conflicts.len()
        );
        Ok(self.build_result(
            false,
            all_updates,
            total_transitive,
            TerminationReason::MaxIterations,
        ))
    }

    /// Resolve a single conflict by finding an alternative version.
    async fn resolve_single_conflict(
        &mut self,
        conflict: &VersionConflict,
    ) -> Result<Option<VersionUpdate>> {
        let source_name = conflict
            .resource
            .source()
            .ok_or_else(|| anyhow::anyhow!("Resource {} has no source", conflict.resource))?;

        let mut sha_groups: HashMap<&str, Vec<&crate::version::conflict::ConflictingRequirement>> =
            HashMap::new();
        for req in &conflict.conflicting_requirements {
            sha_groups.entry(req.resolved_sha.as_str()).or_default().push(req);
        }

        let target_sha = algorithm::select_target_sha(&sha_groups)?;

        tracing::debug!(
            "Target SHA for {}: {} ({} requirements)",
            conflict.resource,
            &target_sha[..8.min(target_sha.len())],
            sha_groups.get(target_sha).map_or(0, |v| v.len())
        );

        let requirements_to_update: Vec<&crate::version::conflict::ConflictingRequirement> =
            conflict
                .conflicting_requirements
                .iter()
                .filter(|req| req.resolved_sha != target_sha)
                .collect();

        if requirements_to_update.is_empty() {
            return Ok(None);
        }

        let req_to_update = requirements_to_update[0];

        if req_to_update.required_by == "manifest" {
            algorithm::find_alternative_for_direct_dependency(
                self.core,
                self.version_service,
                source_name,
                req_to_update,
                target_sha,
                &mut self.attempts,
                self.max_attempts,
                self.start_time,
                self.timeout,
            )
            .await
        } else {
            algorithm::find_alternative_for_transitive(
                self.core,
                self.version_service,
                source_name,
                req_to_update,
                target_sha,
                &mut self.attempts,
                self.max_attempts,
                self.start_time,
                self.timeout,
            )
            .await
        }
    }

    /// Detect if we're oscillating between two conflict states.
    fn detect_oscillation(&self, current_conflicts: &[VersionConflict]) -> bool {
        for iteration in &self.iteration_history {
            if algorithm::conflicts_equal(&iteration.conflicts, current_conflicts) {
                tracing::warn!(
                    "Oscillation detected: conflicts match iteration {}",
                    iteration.iteration
                );
                return true;
            }
        }
        false
    }

    fn build_result(
        &self,
        resolved: bool,
        updates: Vec<VersionUpdate>,
        total_transitive: usize,
        termination_reason: TerminationReason,
    ) -> BacktrackingResult {
        BacktrackingResult {
            resolved,
            updates,
            iterations: self.iteration_history.len(),
            attempted_versions: self.attempts,
            iteration_history: self.iteration_history.clone(),
            total_transitive_reresolutions: total_transitive,
            termination_reason,
        }
    }
}
