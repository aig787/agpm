//! Transitive dependency handling for backtracking.
//!
//! This module handles re-extraction and re-resolution of transitive dependencies
//! after version changes during backtracking.

use anyhow::{Context, Result};

use crate::resolver::ResolutionCore;
use crate::resolver::version_resolver::VersionResolutionService;
use crate::version::conflict::{ConflictDetector, VersionConflict};

use super::registry::{ResourceRegistry, TransitiveChangeTracker, parse_resource_id_string};

/// Get variant inputs (template variables) for a resource from the change tracker.
///
/// Returns None if:
/// - The resource hasn't changed during backtracking
/// - The resource was resolved without template variables
pub fn get_variant_inputs_for_resource(
    change_tracker: &TransitiveChangeTracker,
    resource_id: &str,
) -> Option<serde_json::Value> {
    change_tracker
        .get_changed_resources()
        .get(resource_id)
        .and_then(|(_, _, _, variant_inputs)| variant_inputs.clone())
}

/// Re-extract and re-resolve transitive dependencies for changed resources.
///
/// For each resource whose version changed during backtracking, we need to:
/// 1. Get the worktree for the new version
/// 2. Extract transitive dependencies from the resource file
/// 3. Resolve those dependencies (version → SHA)
/// 4. Update PreparedSourceVersions
///
/// # Returns
///
/// Number of transitive dependencies re-resolved
pub async fn reextract_transitive_deps(
    core: &ResolutionCore,
    version_service: &mut VersionResolutionService,
    change_tracker: &mut TransitiveChangeTracker,
) -> Result<usize> {
    use crate::resolver::transitive_extractor::extract_transitive_deps;

    let mut count = 0;

    // Get all changed resources (need to collect to avoid borrowing issues)
    let changed: Vec<(String, String, String)> = change_tracker
        .get_changed_resources()
        .iter()
        .map(|(id, (_, new_ver, new_sha, _))| (id.clone(), new_ver.clone(), new_sha.clone()))
        .collect();

    for (resource_id, new_version, new_sha) in changed {
        let (source_name, resource_path) = parse_resource_id_string(&resource_id)?;

        tracing::debug!(
            "Re-extracting transitive deps for {}: version={}, sha={}",
            resource_id,
            new_version,
            &new_sha[..8.min(new_sha.len())]
        );

        let source_url = core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let worktree_path = core
            .cache()
            .get_or_create_worktree_for_sha(source_name, &source_url, &new_sha, Some(source_name))
            .await?;

        let variant_inputs = get_variant_inputs_for_resource(change_tracker, &resource_id);

        let transitive_deps =
            extract_transitive_deps(&worktree_path, resource_path, variant_inputs.as_ref()).await?;

        for (_resource_type, specs) in transitive_deps {
            for spec in specs {
                // Skip dependencies with install=false
                if matches!(spec.install, Some(false)) {
                    continue;
                }

                let dep_source = source_name;
                let dep_version = spec.version.as_deref();

                version_service
                    .prepare_additional_version(core, dep_source, dep_version)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to prepare transitive dependency '{}' from {}",
                            spec.path, resource_id
                        )
                    })?;

                count += 1;

                tracing::debug!(
                    "  Re-resolved transitive dep: {} from source {} version {}",
                    spec.path,
                    dep_source,
                    dep_version.unwrap_or("HEAD")
                );
            }
        }
    }

    // Clear change tracker for next iteration
    change_tracker.clear();

    Ok(count)
}

/// Detect conflicts after applying backtracking updates.
///
/// Rebuilds a ConflictDetector from the resource registry to detect
/// conflicts immediately after version changes.
pub fn detect_conflicts_after_changes(
    resource_registry: &ResourceRegistry,
) -> Vec<VersionConflict> {
    tracing::debug!("Detecting conflicts after version changes...");

    let mut detector = ConflictDetector::new();

    for resource in resource_registry.all_resources() {
        for required_by in &resource.required_by {
            detector.add_requirement(
                resource.resource_id.clone(),
                required_by,
                &resource.version_constraint,
                &resource.sha,
            );
        }
    }

    let conflicts = detector.detect_conflicts();

    if conflicts.is_empty() {
        tracing::debug!("No conflicts detected after changes");
    } else {
        tracing::debug!(
            "Detected {} conflict(s) after changes: {:?}",
            conflicts.len(),
            conflicts.iter().map(|c| &c.resource).collect::<Vec<_>>()
        );
    }

    conflicts
}
