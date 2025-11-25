//! Conflict resolution algorithm for backtracking.
//!
//! This module contains the core algorithms for resolving version conflicts:
//! - SHA selection strategies
//! - Version filtering by constraints
//! - Alternative version finding for both direct and transitive dependencies
//! - Oscillation detection

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::resolver::ResolutionCore;
use crate::resolver::version_resolver::VersionResolutionService;
use crate::version::conflict::{ConflictingRequirement, VersionConflict};

use super::types::VersionUpdate;

/// Select the target SHA that other versions should match.
///
/// Strategy: Choose the SHA with the most requirements, breaking ties by:
/// 1. Preferring Version resolution mode over GitRef (semver tags are more stable)
/// 2. Alphabetically by SHA for deterministic ordering
pub fn select_target_sha<'b>(
    sha_groups: &'b HashMap<&str, Vec<&ConflictingRequirement>>,
) -> Result<&'b str> {
    sha_groups
        .iter()
        .max_by(|(sha_a, reqs_a), (sha_b, reqs_b)| {
            // Primary: number of requirements (more is better)
            let count_cmp = reqs_a.len().cmp(&reqs_b.len());
            if count_cmp != std::cmp::Ordering::Equal {
                return count_cmp;
            }

            // Secondary: prefer Version mode over GitRef (semver is more stable)
            // Count how many requirements use Version mode.
            //
            // Note: This detection treats both branch names (e.g., "main") and commit SHAs
            // (e.g., "abc123...") as non-semver since neither parses as a version constraint.
            // This is acceptable because the tertiary SHA comparison provides determinism.
            let version_count_a = reqs_a
                .iter()
                .filter(|r| {
                    // Check if requirement looks like a semver constraint
                    crate::version::constraints::VersionConstraint::parse(&r.requirement).is_ok()
                })
                .count();
            let version_count_b = reqs_b
                .iter()
                .filter(|r| {
                    crate::version::constraints::VersionConstraint::parse(&r.requirement).is_ok()
                })
                .count();

            let mode_cmp = version_count_a.cmp(&version_count_b);
            if mode_cmp != std::cmp::Ordering::Equal {
                return mode_cmp;
            }

            // Tertiary: alphabetically by SHA for deterministic ordering
            sha_a.cmp(sha_b)
        })
        .map(|(sha, _)| *sha)
        .ok_or_else(|| anyhow::anyhow!("No SHA groups found"))
}

/// Filter versions by constraint, returning matching versions in preference order.
///
/// This function implements prefix-aware version filtering, ensuring that prefixed
/// constraints (e.g., `d->=v1.0.0`) only match tags with the same prefix (e.g.,
/// `d-v1.0.0`, `d-v2.0.0`). This prevents cross-contamination from tags with different
/// prefixes that happen to satisfy the version constraint.
///
/// Preference order: highest semantic versions first (with deterministic tag name
/// tie-breaking), excluding pre-releases unless explicitly specified in the constraint.
pub fn filter_by_constraint(versions: &[String], constraint: &str) -> Result<Vec<String>> {
    use crate::resolver::version_resolver::parse_tags_to_versions;
    use crate::version::constraints::{ConstraintSet, VersionConstraint};

    let mut matching = Vec::new();

    // Extract prefix from constraint to filter tags
    let (constraint_prefix, _) = crate::version::split_prefix_and_version(constraint);

    // Filter versions to only those matching the constraint's prefix
    let prefix_filtered_versions: Vec<String> = versions
        .iter()
        .filter(|tag| {
            let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
            tag_prefix == constraint_prefix
        })
        .cloned()
        .collect();

    // Special cases: HEAD, latest, or wildcard
    if constraint == "HEAD" || constraint == "latest" || constraint == "*" {
        let mut tag_versions = parse_tags_to_versions(prefix_filtered_versions.clone());
        if !tag_versions.is_empty() {
            use crate::resolver::version_resolver::sort_versions_deterministic;
            sort_versions_deterministic(&mut tag_versions);
            matching.extend(tag_versions.into_iter().map(|(tag, _)| tag));
        } else {
            matching.extend(prefix_filtered_versions.iter().cloned());
            matching.sort_by(|a, b| b.cmp(a));
        }
    } else {
        // Try to parse as version constraint
        if let Ok(constraint_parsed) = VersionConstraint::parse(constraint) {
            let mut constraint_set = ConstraintSet::new();
            constraint_set.add(constraint_parsed)?;

            let tag_versions = parse_tags_to_versions(prefix_filtered_versions);

            let mut matched_pairs: Vec<(String, semver::Version)> = tag_versions
                .into_iter()
                .filter(|(_, version)| constraint_set.satisfies(version))
                .collect();

            use crate::resolver::version_resolver::sort_versions_deterministic;
            sort_versions_deterministic(&mut matched_pairs);

            matching.extend(matched_pairs.into_iter().map(|(tag, _)| tag));
        } else {
            // Not a constraint, treat as exact ref
            if prefix_filtered_versions.contains(&constraint.to_string()) {
                matching.push(constraint.to_string());
            }
        }
    }

    Ok(matching)
}

/// Get all available versions (tags) from a Git repository.
pub async fn get_available_versions(
    version_service: &VersionResolutionService,
    source_name: &str,
) -> Result<Vec<String>> {
    let bare_repo_path = version_service.get_bare_repo_path(source_name).ok_or_else(|| {
        anyhow::anyhow!("Source '{}' not yet synced. Call pre_sync_sources() first.", source_name)
    })?;

    let git_repo = crate::git::GitRepo::new(&bare_repo_path);
    let tags = git_repo
        .list_tags()
        .await
        .with_context(|| format!("Failed to list tags for source '{}'", source_name))?;

    Ok(tags)
}

/// Resolve a version string to its commit SHA.
pub async fn resolve_version_to_sha(
    version_service: &VersionResolutionService,
    source_name: &str,
    version: &str,
) -> Result<String> {
    let bare_repo_path = version_service
        .get_bare_repo_path(source_name)
        .ok_or_else(|| anyhow::anyhow!("Source '{}' not yet synced", source_name))?;

    let git_repo = crate::git::GitRepo::new(&bare_repo_path);

    git_repo.resolve_to_sha(Some(version)).await.context("Failed to resolve version to SHA")
}

/// Find alternative version for a direct dependency (not transitive).
///
/// This searches for versions of the dependency itself.
pub async fn find_alternative_for_direct_dependency(
    _core: &ResolutionCore,
    version_service: &mut VersionResolutionService,
    source_name: &str,
    requirement: &ConflictingRequirement,
    target_sha: &str,
    attempts: &mut usize,
    max_attempts: usize,
    start_time: std::time::Instant,
    timeout: std::time::Duration,
) -> Result<Option<VersionUpdate>> {
    let available_versions = get_available_versions(version_service, source_name).await?;

    tracing::debug!(
        "Searching {} available versions for direct dependency {} matching SHA {}",
        available_versions.len(),
        requirement.requirement,
        &target_sha[..8.min(target_sha.len())]
    );

    let matching_versions = filter_by_constraint(&available_versions, &requirement.requirement)?;

    tracing::debug!(
        "Found {} versions matching constraint {}",
        matching_versions.len(),
        requirement.requirement
    );

    for version in matching_versions {
        *attempts += 1;
        if *attempts >= max_attempts {
            tracing::warn!("Reached max attempts ({})", max_attempts);
            return Ok(None);
        }

        if start_time.elapsed() > timeout {
            tracing::warn!("Backtracking timeout");
            return Ok(None);
        }

        let sha = resolve_version_to_sha(version_service, source_name, &version).await?;

        tracing::trace!(
            "Trying {}: {} → {}",
            version,
            &sha[..8.min(sha.len())],
            if sha == target_sha {
                "MATCH"
            } else {
                "no match"
            }
        );

        if sha == target_sha {
            let resource_id = format!("{}:{}", source_name, requirement.required_by);
            let group_key = format!("{}::{}", source_name, version);
            let variant_inputs: Option<serde_json::Value> =
                version_service.prepared_versions().get(&group_key).and_then(|prepared| {
                    prepared.resource_variants.get(&resource_id).and_then(|opt| opt.clone())
                });

            return Ok(Some(VersionUpdate {
                resource_id,
                old_version: requirement.requirement.clone(),
                new_version: version.clone(),
                old_sha: requirement.resolved_sha.clone(),
                new_sha: sha,
                variant_inputs,
            }));
        }
    }

    Ok(None)
}

/// Find an alternative version for a transitive dependency.
///
/// This method searches for alternative versions of the **parent resource** (not the
/// transitive dependency that's conflicting). For each alternative parent version,
/// it extracts the transitive dependencies and checks if they resolve to the target SHA.
#[allow(clippy::too_many_arguments)]
pub async fn find_alternative_for_transitive(
    core: &ResolutionCore,
    version_service: &mut VersionResolutionService,
    source_name: &str,
    requirement: &ConflictingRequirement,
    target_sha: &str,
    attempts: &mut usize,
    max_attempts: usize,
    start_time: std::time::Instant,
    timeout: std::time::Duration,
) -> Result<Option<VersionUpdate>> {
    let parent_version_constraint =
        requirement.parent_version_constraint.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Missing parent_version_constraint for transitive dependency required by '{}'",
                requirement.required_by
            )
        })?;

    tracing::debug!(
        "Searching alternative versions of PARENT '{}' (current: {}) to resolve conflict",
        requirement.required_by,
        parent_version_constraint
    );

    let available_versions = get_available_versions(version_service, source_name).await?;
    let matching_versions = filter_by_constraint(&available_versions, parent_version_constraint)?;

    tracing::debug!(
        "Found {} parent versions matching constraint {}",
        matching_versions.len(),
        parent_version_constraint
    );

    for parent_version in matching_versions {
        *attempts += 1;
        if *attempts >= max_attempts {
            tracing::warn!("Reached max attempts ({})", max_attempts);
            return Ok(None);
        }

        if start_time.elapsed() > timeout {
            tracing::warn!("Backtracking timeout");
            return Ok(None);
        }

        let parent_sha =
            resolve_version_to_sha(version_service, source_name, &parent_version).await?;

        tracing::trace!(
            "Trying parent {}: SHA {}",
            parent_version,
            &parent_sha[..8.min(parent_sha.len())]
        );

        let source_url = core
            .source_manager()
            .get_source_url(source_name)
            .ok_or_else(|| anyhow::anyhow!("Source '{}' not found", source_name))?;

        let worktree_path = core
            .cache()
            .get_or_create_worktree_for_sha(
                source_name,
                &source_url,
                &parent_sha,
                Some(source_name),
            )
            .await?;

        let parent_resource_path = if requirement.required_by.ends_with(".md")
            || requirement.required_by.ends_with(".json")
        {
            requirement.required_by.clone()
        } else {
            format!("{}.md", requirement.required_by)
        };

        // Look up variant_inputs from PreparedSourceVersion
        let parent_resource_id = format!("{}:{}", source_name, requirement.required_by);
        let parent_group_key = format!("{}::{}", source_name, parent_version);
        let parent_variant_inputs_cloned: Option<serde_json::Value> =
            version_service.prepared_versions().get(&parent_group_key).and_then(|prepared| {
                prepared.resource_variants.get(&parent_resource_id).and_then(|opt| opt.clone())
            });

        let transitive_deps = match crate::resolver::transitive_extractor::extract_transitive_deps(
            &worktree_path,
            &parent_resource_path,
            parent_variant_inputs_cloned.as_ref(),
        )
        .await
        {
            Ok(deps) => deps,
            Err(e) => {
                tracing::debug!(
                    "Failed to extract transitive deps from parent {} @ {}: {}",
                    parent_resource_path,
                    parent_version,
                    e
                );
                continue;
            }
        };

        for (_resource_type, specs) in transitive_deps {
            for spec in specs {
                let dep_version = spec.version.as_deref().unwrap_or("HEAD");
                let dep_sha =
                    match resolve_version_to_sha(version_service, source_name, dep_version).await {
                        Ok(sha) => sha,
                        Err(_) => continue,
                    };

                tracing::trace!(
                    "  Transitive dep {} @ {}: SHA {} → {}",
                    spec.path,
                    dep_version,
                    &dep_sha[..8.min(dep_sha.len())],
                    if dep_sha == target_sha {
                        "MATCH"
                    } else {
                        "no match"
                    }
                );

                if dep_sha == target_sha {
                    tracing::info!(
                        "Found compatible parent version: {} @ {} (was @ {})",
                        requirement.required_by,
                        parent_version,
                        parent_version_constraint
                    );

                    let resource_id = format!("{}:{}", source_name, requirement.required_by);
                    let group_key = format!("{}::{}", source_name, parent_version);
                    let variant_inputs: Option<serde_json::Value> =
                        version_service.prepared_versions().get(&group_key).and_then(|prepared| {
                            prepared.resource_variants.get(&resource_id).and_then(|opt| opt.clone())
                        });

                    return Ok(Some(VersionUpdate {
                        resource_id,
                        old_version: parent_version_constraint.clone(),
                        new_version: parent_version.clone(),
                        old_sha: requirement
                            .parent_resolved_sha
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                        new_sha: parent_sha,
                        variant_inputs,
                    }));
                }
            }
        }
    }

    Ok(None)
}

/// Check if two conflict sets are equivalent.
///
/// Two conflict sets are equal if they contain the same resources with the same
/// resolved SHAs, regardless of order.
pub fn conflicts_equal(a: &[VersionConflict], b: &[VersionConflict]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut a_state = std::collections::BTreeSet::new();
    let mut b_state = std::collections::BTreeSet::new();

    for conflict in a {
        for req in &conflict.conflicting_requirements {
            a_state.insert((conflict.resource.clone(), req.resolved_sha.clone()));
        }
    }

    for conflict in b {
        for req in &conflict.conflicting_requirements {
            b_state.insert((conflict.resource.clone(), req.resolved_sha.clone()));
        }
    }

    a_state == b_state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ResourceType;
    use crate::lockfile::ResourceId;

    fn test_resource_id(name: &str) -> ResourceId {
        ResourceId::new(
            name,
            Some("test-source"),
            Some("claude-code"),
            ResourceType::Agent,
            crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string(),
        )
    }

    #[test]
    fn test_conflicts_equal_identical_resources_and_shas() {
        let conflict_a = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let conflict_b = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        assert!(conflicts_equal(&[conflict_a], &[conflict_b]));
    }

    #[test]
    fn test_conflicts_equal_same_resources_different_shas() {
        let conflict_a = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "def789abc012".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let conflict_b = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^1.2.0".to_string(),
                    resolved_sha: "999888777666".to_string(), // Different SHA
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        assert!(!conflicts_equal(&[conflict_a], &[conflict_b]));
    }

    #[test]
    fn test_conflicts_equal_different_resources() {
        let conflict_a = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app1".to_string(),
                requirement: "^1.0.0".to_string(),
                resolved_sha: "abc123def456".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        let conflict_b = VersionConflict {
            resource: test_resource_id("lib2"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app1".to_string(),
                requirement: "^1.0.0".to_string(),
                resolved_sha: "abc123def456".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        assert!(!conflicts_equal(&[conflict_a], &[conflict_b]));
    }

    #[test]
    fn test_conflicts_equal_empty_lists() {
        let conflicts_a: Vec<VersionConflict> = vec![];
        let conflicts_b: Vec<VersionConflict> = vec![];
        assert!(conflicts_equal(&conflicts_a, &conflicts_b));
    }

    #[test]
    fn test_conflicts_equal_different_lengths() {
        let conflict1 = VersionConflict {
            resource: test_resource_id("lib1"),
            conflicting_requirements: vec![ConflictingRequirement {
                required_by: "app1".to_string(),
                requirement: "^1.0.0".to_string(),
                resolved_sha: "abc123def456".to_string(),
                resolved_version: None,
                parent_version_constraint: None,
                parent_resolved_sha: None,
            }],
        };

        assert!(!conflicts_equal(&[conflict1.clone()], &[conflict1.clone(), conflict1]));
    }

    #[test]
    fn test_filter_by_constraint_respects_prefix() {
        use crate::resolver::version_resolver::parse_tags_to_versions;
        use crate::version::constraints::{ConstraintSet, VersionConstraint};

        let all_tags = vec![
            "d-v1.0.0".to_string(),
            "d-v2.0.0".to_string(),
            "a-v1.0.0".to_string(),
            "a-v2.0.0".to_string(),
        ];

        let constraint = "d->=v1.0.0";
        let (constraint_prefix, _) = crate::version::split_prefix_and_version(constraint);

        let prefix_filtered: Vec<String> = all_tags
            .iter()
            .filter(|tag| {
                let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
                tag_prefix == constraint_prefix
            })
            .cloned()
            .collect();

        let constraint_parsed = VersionConstraint::parse(constraint).unwrap();
        let mut constraint_set = ConstraintSet::new();
        constraint_set.add(constraint_parsed).unwrap();

        let tag_versions = parse_tags_to_versions(prefix_filtered);

        let matched_tags: Vec<String> = tag_versions
            .into_iter()
            .filter(|(_, version)| constraint_set.satisfies(version))
            .map(|(tag, _)| tag)
            .collect();

        assert_eq!(matched_tags.len(), 2);
        assert!(matched_tags.contains(&"d-v1.0.0".to_string()));
        assert!(matched_tags.contains(&"d-v2.0.0".to_string()));
        assert!(!matched_tags.contains(&"a-v1.0.0".to_string()));
    }

    #[test]
    fn test_filter_by_constraint_unprefixed() {
        use crate::resolver::version_resolver::parse_tags_to_versions;
        use crate::version::constraints::{ConstraintSet, VersionConstraint};

        let all_tags = ["v1.0.0".to_string(), "v2.0.0".to_string(), "d-v1.0.0".to_string()];

        let constraint = ">=v1.0.0";
        let (constraint_prefix, _) = crate::version::split_prefix_and_version(constraint);
        assert!(constraint_prefix.is_none());

        let prefix_filtered: Vec<String> = all_tags
            .iter()
            .filter(|tag| {
                let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
                tag_prefix == constraint_prefix
            })
            .cloned()
            .collect();

        let constraint_parsed = VersionConstraint::parse(constraint).unwrap();
        let mut constraint_set = ConstraintSet::new();
        constraint_set.add(constraint_parsed).unwrap();

        let tag_versions = parse_tags_to_versions(prefix_filtered);

        let matched_tags: Vec<String> = tag_versions
            .into_iter()
            .filter(|(_, version)| constraint_set.satisfies(version))
            .map(|(tag, _)| tag)
            .collect();

        assert_eq!(matched_tags.len(), 2);
        assert!(matched_tags.contains(&"v1.0.0".to_string()));
        assert!(matched_tags.contains(&"v2.0.0".to_string()));
        assert!(!matched_tags.contains(&"d-v1.0.0".to_string()));
    }

    #[test]
    fn test_deterministic_sorting_with_identical_versions() {
        use crate::resolver::version_resolver::{
            parse_tags_to_versions, sort_versions_deterministic,
        };

        let tags = vec![
            "z-v1.0.0".to_string(),
            "a-v1.0.0".to_string(),
            "m-v1.0.0".to_string(),
            "b-v2.0.0".to_string(),
        ];

        let mut result = parse_tags_to_versions(tags);
        assert_eq!(result.len(), 4);

        assert_eq!(result[0].0, "b-v2.0.0");
        assert_eq!(result[1].0, "a-v1.0.0");
        assert_eq!(result[2].0, "m-v1.0.0");
        assert_eq!(result[3].0, "z-v1.0.0");

        sort_versions_deterministic(&mut result);
        assert_eq!(result[0].0, "b-v2.0.0");
        assert_eq!(result[1].0, "a-v1.0.0");
        assert_eq!(result[2].0, "m-v1.0.0");
        assert_eq!(result[3].0, "z-v1.0.0");
    }
}
