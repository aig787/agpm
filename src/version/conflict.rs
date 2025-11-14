//! Version conflict detection and reporting.
//!
//! This module handles detection and reporting of version conflicts that can occur
//! when multiple dependencies require incompatible versions of the same resource.
//! It provides detailed conflict information to help users resolve dependency issues.

use anyhow::Result;
use semver::Version;
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::core::AgpmError;
use crate::lockfile::ResourceId;

/// Represents a version conflict between dependencies
#[derive(Debug, Clone)]
pub struct VersionConflict {
    pub resource: ResourceId,
    pub conflicting_requirements: Vec<ConflictingRequirement>,
}

#[derive(Debug, Clone)]
pub struct ConflictingRequirement {
    /// The parent resource that requires this dependency (e.g., "agents/agent-a")
    pub required_by: String,
    /// The version constraint for the transitive dependency (e.g., "x-v1.0.0")
    pub requirement: String,
    /// The SHA that the transitive dependency resolved to
    pub resolved_sha: String,
    /// The semantic version if applicable
    pub resolved_version: Option<Version>,
    /// The version constraint of the parent resource (e.g., "^1.0.0")
    pub parent_version_constraint: Option<String>,
    /// The SHA that the parent resource resolved to
    pub parent_resolved_sha: Option<String>,
}

impl fmt::Display for VersionConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Version conflict for {}:", self.resource)?;

        // Group by SHA to show which requirements resolve to which commit
        let mut sha_groups: HashMap<&str, Vec<&ConflictingRequirement>> = HashMap::new();
        for req in &self.conflicting_requirements {
            sha_groups.entry(&req.resolved_sha).or_default().push(req);
        }

        for (sha, reqs) in sha_groups {
            let short_sha = &sha[..8.min(sha.len())];
            writeln!(f, "  Commit {short_sha}:")?;
            for req in reqs {
                writeln!(f, "    - {} requires {}", req.required_by, req.requirement)?;
            }
        }

        Ok(())
    }
}

/// Detects and resolves version conflicts
pub struct ConflictDetector {
    requirements: HashMap<ResourceId, Vec<ConflictingRequirement>>, // resource -> [requirements]
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ConflictDetector {
    pub fn new() -> Self {
        Self {
            requirements: HashMap::new(),
        }
    }

    /// Get a reference to the internal requirements map.
    ///
    /// This is used by the backtracking resolver to populate its resource registry
    /// for conflict detection during version changes.
    pub fn requirements(&self) -> &HashMap<ResourceId, Vec<ConflictingRequirement>> {
        &self.requirements
    }

    /// Add a dependency requirement with optional parent metadata
    pub fn add_requirement(
        &mut self,
        resource: ResourceId,
        required_by: &str,
        version_constraint: &str,
        resolved_sha: &str,
    ) {
        self.add_requirement_with_parent(
            resource,
            required_by,
            version_constraint,
            resolved_sha,
            None,
            None,
        );
    }

    /// Add a dependency requirement with full parent metadata for backtracking
    pub fn add_requirement_with_parent(
        &mut self,
        resource: ResourceId,
        required_by: &str,
        version_constraint: &str,
        resolved_sha: &str,
        parent_version_constraint: Option<String>,
        parent_resolved_sha: Option<String>,
    ) {
        self.requirements.entry(resource).or_default().push(ConflictingRequirement {
            required_by: required_by.to_string(),
            requirement: version_constraint.to_string(),
            resolved_sha: resolved_sha.to_string(),
            resolved_version: None,
            parent_version_constraint,
            parent_resolved_sha,
        });
    }

    /// Detect conflicts in the current requirements
    pub fn detect_conflicts(&self) -> Vec<VersionConflict> {
        let mut conflicts = Vec::new();

        for (resource_id, requirements) in &self.requirements {
            if requirements.len() <= 1 {
                continue; // No conflict possible with single requirement
            }

            // Group by resolved SHA
            let mut sha_groups: HashMap<&str, Vec<&ConflictingRequirement>> = HashMap::new();
            for req in requirements {
                sha_groups.entry(req.resolved_sha.as_str()).or_default().push(req);
            }

            // Conflict only if multiple different SHAs
            if sha_groups.len() > 1 {
                conflicts.push(VersionConflict {
                    resource: resource_id.clone(),
                    conflicting_requirements: requirements.clone(),
                });
            }
        }

        conflicts
    }

    /// Try to resolve conflicts by finding compatible versions
    pub fn resolve_conflicts(
        &self,
        available_versions: &HashMap<ResourceId, Vec<Version>>,
    ) -> Result<HashMap<ResourceId, Version>> {
        let mut resolved = HashMap::new();
        let conflicts = self.detect_conflicts();

        if !conflicts.is_empty() {
            let conflict_messages: Vec<String> =
                conflicts.iter().map(std::string::ToString::to_string).collect();

            return Err(AgpmError::Other {
                message: format!(
                    "Unable to resolve version conflicts:\n{}",
                    conflict_messages.join("\n")
                ),
            }
            .into());
        }

        // Resolve each resource to its best version
        for (resource_id, requirements) in &self.requirements {
            let versions = available_versions.get(resource_id).ok_or_else(|| AgpmError::Other {
                message: format!("No versions available for resource: {resource_id}"),
            })?;

            let best_version = self.find_best_version(versions, requirements)?;
            resolved.insert(resource_id.clone(), best_version);
        }

        Ok(resolved)
    }

    /// Find the best version that satisfies all requirements
    fn find_best_version(
        &self,
        available: &[Version],
        requirements: &[ConflictingRequirement],
    ) -> Result<Version> {
        let mut candidates = available.to_vec();

        // Filter by each requirement
        for req in requirements {
            let req_str = &req.requirement;
            if req_str == "latest" || req_str == "*" {
                continue; // These match everything
            }

            if let Ok(req) = crate::version::parse_version_req(req_str) {
                candidates.retain(|v| req.matches(v));
            }
        }

        if candidates.is_empty() {
            return Err(AgpmError::Other {
                message: format!("No version satisfies all requirements: {requirements:?}"),
            }
            .into());
        }

        // Sort and return the highest version
        candidates.sort_by(|a, b| b.cmp(a));
        Ok(candidates[0].clone())
    }
}

/// Analyzes dependency graphs for circular dependencies
pub struct CircularDependencyDetector {
    graph: HashMap<String, HashSet<String>>,
}

impl Default for CircularDependencyDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl CircularDependencyDetector {
    pub fn new() -> Self {
        Self {
            graph: HashMap::new(),
        }
    }

    /// Add a dependency edge
    pub fn add_dependency(&mut self, from: &str, to: &str) {
        self.graph.entry(from.to_string()).or_default().insert(to.to_string());
    }

    /// Detect circular dependencies
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node in self.graph.keys() {
            if !visited.contains(node) {
                self.dfs_detect_cycle(node, &mut visited, &mut rec_stack, &mut path, &mut cycles);
            }
        }

        cycles
    }

    fn dfs_detect_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = self.graph.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    self.dfs_detect_cycle(neighbor, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(neighbor) {
                    // Found a cycle
                    let cycle_start = path.iter().position(|n| n == neighbor).unwrap();
                    let cycle = path[cycle_start..].to_vec();
                    cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test helper to create a ResourceId easily
    fn test_resource_id(name: &str) -> ResourceId {
        ResourceId::new(
            name,
            Some("test-source"),
            Some("claude-code"),
            crate::core::ResourceType::Agent,
            crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string(),
        )
    }

    #[test]
    fn test_conflict_detection() {
        let mut detector = ConflictDetector::new();

        // Add compatible requirements (same SHA)
        detector.add_requirement(test_resource_id("lib1"), "app1", "^1.0.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "^1.2.0", "abc123def456");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0); // These are compatible (same SHA)

        // Add incompatible requirements (different SHAs)
        detector.add_requirement(test_resource_id("lib2"), "app1", "^1.0.0", "111222333444");
        detector.add_requirement(test_resource_id("lib2"), "app2", "^2.0.0", "555666777888");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].resource.to_string().contains("lib2"));
    }

    #[test]
    fn test_git_ref_compatibility() {
        let mut detector = ConflictDetector::new();

        // Same git ref resolving to same SHA - compatible
        detector.add_requirement(test_resource_id("lib1"), "app1", "main", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "main", "abc123def456");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0);

        // Different git refs resolving to different SHAs - incompatible
        detector.add_requirement(test_resource_id("lib2"), "app1", "main", "abc123def456");
        detector.add_requirement(test_resource_id("lib2"), "app2", "develop", "999888777666");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn test_git_ref_case_insensitive() {
        let mut detector = ConflictDetector::new();

        // Git refs differing only by case should be treated as the same
        // (important for case-insensitive filesystems like Windows and macOS)
        detector.add_requirement(test_resource_id("lib1"), "app1", "main", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "Main", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app3", "MAIN", "abc123def456");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "Git refs differing only by case should be compatible (case-insensitive filesystems)"
        );

        // Mixed case with different branch names should still conflict
        let mut detector2 = ConflictDetector::new();
        detector2.add_requirement(test_resource_id("lib2"), "app1", "Main", "abc123def456");
        detector2.add_requirement(test_resource_id("lib2"), "app2", "Develop", "999888777666");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(
            conflicts2.len(),
            1,
            "Different branch names should conflict regardless of case"
        );
    }

    #[test]
    fn test_resolve_conflicts() {
        let mut detector = ConflictDetector::new();
        let lib1_id = test_resource_id("lib1");
        detector.add_requirement(lib1_id.clone(), "app1", "^1.0.0", "abc123def456");
        detector.add_requirement(lib1_id.clone(), "app2", "^1.2.0", "abc123def456");

        let mut available = HashMap::new();
        available.insert(
            lib1_id.clone(),
            vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("1.2.0").unwrap(),
                Version::parse("1.5.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ],
        );

        let resolved = detector.resolve_conflicts(&available).unwrap();
        assert_eq!(resolved.get(&lib1_id), Some(&Version::parse("1.5.0").unwrap()));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut detector = CircularDependencyDetector::new();

        // Create a cycle: A -> B -> C -> A
        detector.add_dependency("A", "B");
        detector.add_dependency("B", "C");
        detector.add_dependency("C", "A");

        let cycles = detector.detect_cycles();
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].contains(&"A".to_string()));
        assert!(cycles[0].contains(&"B".to_string()));
        assert!(cycles[0].contains(&"C".to_string()));
    }

    #[test]
    fn test_no_circular_dependencies() {
        let mut detector = CircularDependencyDetector::new();

        // Create a DAG: A -> B -> C
        detector.add_dependency("A", "B");
        detector.add_dependency("B", "C");
        detector.add_dependency("A", "C");

        let cycles = detector.detect_cycles();
        assert_eq!(cycles.len(), 0);
    }

    #[test]
    fn test_conflict_display() {
        let conflict = VersionConflict {
            resource: ResourceId::new(
                "test-lib",
                Some("test-source"),
                Some("claude-code"),
                crate::core::ResourceType::Agent,
                crate::utils::EMPTY_VARIANT_INPUTS_HASH.to_string(),
            ),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_sha: "abc123def456".to_string(),
                    resolved_version: Some(Version::parse("1.5.0").unwrap()),
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^2.0.0".to_string(),
                    resolved_sha: "999888777666".to_string(),
                    resolved_version: None,
                    parent_version_constraint: None,
                    parent_resolved_sha: None,
                },
            ],
        };

        let display = format!("{}", conflict);
        // Check that the conflict is displayed with SHA-based grouping
        assert!(display.contains("test-lib"));
        assert!(display.contains("app1"));
        assert!(display.contains("app2"));
        assert!(display.contains("^1.0.0"));
        assert!(display.contains("^2.0.0"));
        // Check that SHAs are displayed (first 8 chars)
        assert!(display.contains("abc123de"));
        assert!(display.contains("99988877"));
    }

    #[test]
    fn test_head_with_specific_version_conflict() {
        let mut detector = ConflictDetector::new();

        // HEAD (unspecified) mixed with specific version should conflict
        detector.add_requirement(test_resource_id("lib1"), "app1", "HEAD", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "^1.0.0", "999888777666");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1, "HEAD mixed with specific version should conflict");

        // "*" with any specific range is compatible (intersection is non-empty)
        let mut detector2 = ConflictDetector::new();
        detector2.add_requirement(test_resource_id("lib2"), "app1", "*", "abc123def456");
        detector2.add_requirement(test_resource_id("lib2"), "app2", "^1.0.0", "abc123def456");

        let conflicts = detector2.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "* should be compatible with ^1.0.0 (intersection is [1.0.0, 2.0.0))"
        );

        // "*" with ~2.1.0 is also compatible (intersection is [2.1.0, 2.2.0))
        let mut detector3 = ConflictDetector::new();
        detector3.add_requirement(test_resource_id("lib3"), "app1", "*", "abc123def456");
        detector3.add_requirement(test_resource_id("lib3"), "app2", "~2.1.0", "abc123def456");

        let conflicts = detector3.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "* should be compatible with ~2.1.0 (intersection is [2.1.0, 2.2.0))"
        );
    }

    #[test]
    fn test_mixed_semver_and_git_refs() {
        let mut detector = ConflictDetector::new();

        // Mix of semver and git branch - should be incompatible
        detector.add_requirement(test_resource_id("lib1"), "app1", "^1.0.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "main", "999888777666");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1, "Mixed semver and git ref should be detected as conflict");

        // Test with exact version and git tag
        let mut detector2 = ConflictDetector::new();
        detector2.add_requirement(test_resource_id("lib2"), "app1", "v1.0.0", "abc123def456");
        detector2.add_requirement(test_resource_id("lib2"), "app2", "develop", "999888777666");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "Exact version with git branch should conflict");
    }

    #[test]
    fn test_duplicate_requirements_same_version() {
        let mut detector = ConflictDetector::new();

        // Multiple resources requiring the same exact version
        detector.add_requirement(test_resource_id("lib1"), "app1", "v1.0.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "v1.0.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app3", "v1.0.0", "abc123def456");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0, "Same version requirements should not conflict");
    }

    #[test]
    fn test_exact_version_conflicts() {
        let mut detector = ConflictDetector::new();

        // Different exact versions - definitely incompatible
        detector.add_requirement(test_resource_id("lib1"), "app1", "v1.0.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "v2.0.0", "999888777666");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1, "Different exact versions must conflict");
        assert_eq!(conflicts[0].conflicting_requirements.len(), 2);
    }

    #[test]
    fn test_resolve_conflicts_missing_resource() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement(test_resource_id("lib1"), "app1", "^1.0.0", "abc123def456");

        let available = HashMap::new(); // Empty - missing lib1

        let result = detector.resolve_conflicts(&available);
        assert!(result.is_err(), "Should error when resource not in available versions");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No versions available"), "Error should mention missing versions");
    }

    #[test]
    fn test_resolve_conflicts_with_incompatible_ranges() {
        let mut detector = ConflictDetector::new();
        let lib1_id = test_resource_id("lib1");
        detector.add_requirement(lib1_id.clone(), "app1", "^1.0.0", "abc123def456");
        detector.add_requirement(lib1_id.clone(), "app2", "^2.0.0", "999888777666");

        let mut available = HashMap::new();
        available.insert(
            lib1_id,
            vec![Version::parse("1.5.0").unwrap(), Version::parse("2.3.0").unwrap()],
        );

        let result = detector.resolve_conflicts(&available);
        assert!(result.is_err(), "Should error when requirements are incompatible");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Unable to resolve version conflicts"),
            "Error should mention conflict resolution failure"
        );
    }

    #[test]
    fn test_resolve_conflicts_no_matching_version() {
        let mut detector = ConflictDetector::new();
        let lib1_id = test_resource_id("lib1");
        detector.add_requirement(lib1_id.clone(), "app1", "^3.0.0", "abc123def456"); // Requires 3.x

        let mut available = HashMap::new();
        available.insert(
            lib1_id,
            vec![Version::parse("1.0.0").unwrap(), Version::parse("2.0.0").unwrap()],
        );

        let result = detector.resolve_conflicts(&available);
        assert!(result.is_err(), "Should error when no version satisfies requirement");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("No version satisfies"),
            "Error should mention no matching version: {}",
            err_msg
        );
    }

    #[test]
    fn test_conflict_aggregated_error_message() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement(test_resource_id("lib1"), "app1", "^1.0.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "^2.0.0", "999888777666");
        detector.add_requirement(test_resource_id("lib2"), "app1", "main", "111222333444");
        detector.add_requirement(test_resource_id("lib2"), "app3", "develop", "555666777888");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 2, "Should detect both conflicts");

        // Verify the conflicts contain proper information
        let lib1_conflict = conflicts.iter().find(|c| c.resource.to_string().contains("lib1"));
        assert!(lib1_conflict.is_some(), "Should have lib1 conflict");
        assert_eq!(
            lib1_conflict.unwrap().conflicting_requirements.len(),
            2,
            "lib1 should have 2 conflicting requirements"
        );

        let lib2_conflict = conflicts.iter().find(|c| c.resource.to_string().contains("lib2"));
        assert!(lib2_conflict.is_some(), "Should have lib2 conflict");
        assert_eq!(
            lib2_conflict.unwrap().conflicting_requirements.len(),
            2,
            "lib2 should have 2 conflicting requirements"
        );
    }

    #[test]
    fn test_multi_comparator_compatible() {
        let mut detector = ConflictDetector::new();

        // ">=5.0.0, <6.0.0" should be compatible with ">=5.5.0"
        // Intersection is [5.5.0, 6.0.0)
        detector.add_requirement(
            test_resource_id("lib1"),
            "app1",
            ">=5.0.0, <6.0.0",
            "abc123def456",
        );
        detector.add_requirement(test_resource_id("lib1"), "app2", ">=5.5.0", "abc123def456");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "Multi-comparator ranges with non-empty intersection should be compatible"
        );
    }

    #[test]
    fn test_multi_comparator_incompatible() {
        let mut detector = ConflictDetector::new();

        // ">=5.0.0, <6.0.0" should conflict with ">=7.0.0"
        // Intersection is empty
        detector.add_requirement(
            test_resource_id("lib1"),
            "app1",
            ">=5.0.0, <6.0.0",
            "abc123def456",
        );
        detector.add_requirement(test_resource_id("lib1"), "app2", ">=7.0.0", "999888777666");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            1,
            "Multi-comparator ranges with empty intersection should conflict"
        );
    }

    #[test]
    fn test_tilde_operator_variants() {
        let mut detector1 = ConflictDetector::new();

        // ~1 means [1.0.0, 2.0.0) - should be compatible with ^1.5.0 [1.5.0, 2.0.0)
        detector1.add_requirement(test_resource_id("lib1"), "app1", "~1", "abc123def456");
        detector1.add_requirement(test_resource_id("lib1"), "app2", "^1.5.0", "abc123def456");

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            0,
            "~1 should be compatible with ^1.5.0 (intersection is [1.5.0, 2.0.0))"
        );

        let mut detector2 = ConflictDetector::new();

        // ~1.2 means [1.2.0, 1.3.0) - should conflict with ^1.5.0 [1.5.0, 2.0.0)
        detector2.add_requirement(test_resource_id("lib2"), "app1", "~1.2", "abc123def456");
        detector2.add_requirement(test_resource_id("lib2"), "app2", "^1.5.0", "999888777666");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "~1.2 should conflict with ^1.5.0 (disjoint ranges)");

        let mut detector3 = ConflictDetector::new();

        // ~1.2.3 means [1.2.3, 1.3.0) - should be compatible with >=1.2.0
        detector3.add_requirement(test_resource_id("lib3"), "app1", "~1.2.3", "abc123def456");
        detector3.add_requirement(test_resource_id("lib3"), "app2", ">=1.2.0", "abc123def456");

        let conflicts3 = detector3.detect_conflicts();
        assert_eq!(conflicts3.len(), 0, "~1.2.3 should be compatible with >=1.2.0");
    }

    #[test]
    fn test_caret_zero_zero_patch() {
        let mut detector1 = ConflictDetector::new();

        // ^0.0.3 means [0.0.3, 0.0.4) - should be compatible with >=0.0.3, <0.0.5
        detector1.add_requirement(test_resource_id("lib1"), "app1", "^0.0.3", "abc123def456");
        detector1.add_requirement(
            test_resource_id("lib1"),
            "app2",
            ">=0.0.3, <0.0.5",
            "abc123def456",
        );

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            0,
            "^0.0.3 should be compatible with >=0.0.3, <0.0.5 (intersection is [0.0.3, 0.0.4))"
        );

        let mut detector2 = ConflictDetector::new();

        // ^0.0.3 means [0.0.3, 0.0.4) - should conflict with ^0.0.5 [0.0.5, 0.0.6)
        detector2.add_requirement(test_resource_id("lib2"), "app1", "^0.0.3", "abc123def456");
        detector2.add_requirement(test_resource_id("lib2"), "app2", "^0.0.5", "999888777666");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "^0.0.3 should conflict with ^0.0.5 (disjoint ranges)");
    }

    #[test]
    fn test_caret_zero_variants() {
        let mut detector1 = ConflictDetector::new();

        // ^0 means [0.0.0, 1.0.0) - should be compatible with ^0.5.0 [0.5.0, 0.6.0)
        detector1.add_requirement(test_resource_id("lib1"), "app1", "^0", "abc123def456");
        detector1.add_requirement(test_resource_id("lib1"), "app2", "^0.5.0", "abc123def456");

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            0,
            "^0 should be compatible with ^0.5.0 (intersection is [0.5.0, 0.6.0))"
        );

        let mut detector2 = ConflictDetector::new();

        // ^0.0 means [0.0.0, 0.1.0) - should conflict with ^0.5.0 [0.5.0, 0.6.0)
        detector2.add_requirement(test_resource_id("lib2"), "app1", "^0.0", "abc123def456");
        detector2.add_requirement(test_resource_id("lib2"), "app2", "^0.5.0", "999888777666");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "^0.0 should conflict with ^0.5.0 (disjoint ranges)");
    }

    #[test]
    fn test_prerelease_versions() {
        let mut detector1 = ConflictDetector::new();

        // =1.0.0-beta.1 should conflict with =1.0.0 (different versions)
        detector1.add_requirement(
            test_resource_id("lib1"),
            "app1",
            "=1.0.0-beta.1",
            "abc123def456",
        );
        detector1.add_requirement(test_resource_id("lib1"), "app2", "=1.0.0", "999888777666");

        let conflicts1 = detector1.detect_conflicts();
        assert_eq!(
            conflicts1.len(),
            1,
            "=1.0.0-beta.1 should conflict with =1.0.0 (different prerelease)"
        );

        let mut detector2 = ConflictDetector::new();

        // =1.0.0-beta.1 should be compatible with itself
        detector2.add_requirement(
            test_resource_id("lib2"),
            "app1",
            "=1.0.0-beta.1",
            "abc123def456",
        );
        detector2.add_requirement(
            test_resource_id("lib2"),
            "app2",
            "=1.0.0-beta.1",
            "abc123def456",
        );

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 0, "Same prerelease version should be compatible");

        let mut detector3 = ConflictDetector::new();

        // >=1.0.0-beta should be compatible with >=1.0.0-alpha (intersection exists)
        detector3.add_requirement(test_resource_id("lib3"), "app1", ">=1.0.0-beta", "abc123def456");
        detector3.add_requirement(
            test_resource_id("lib3"),
            "app2",
            ">=1.0.0-alpha",
            "abc123def456",
        );

        let conflicts3 = detector3.detect_conflicts();
        assert_eq!(conflicts3.len(), 0, ">=1.0.0-beta should be compatible with >=1.0.0-alpha");
    }

    #[test]
    fn test_high_version_ranges() {
        let mut detector = ConflictDetector::new();

        // Test ranges well above typical test versions (>3.0.0)
        detector.add_requirement(
            test_resource_id("lib1"),
            "app1",
            ">=5.0.0, <10.0.0",
            "abc123def456",
        );
        detector.add_requirement(test_resource_id("lib1"), "app2", "^7.5.0", "abc123def456");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "High version ranges should work correctly (intersection is [7.5.0, 8.0.0))"
        );

        let mut detector2 = ConflictDetector::new();

        // Test conflicting high version ranges
        detector2.add_requirement(test_resource_id("lib2"), "app1", ">=100.0.0", "abc123def456");
        detector2.add_requirement(test_resource_id("lib2"), "app2", "<50.0.0", "999888777666");

        let conflicts2 = detector2.detect_conflicts();
        assert_eq!(conflicts2.len(), 1, "Disjoint high version ranges should conflict");
    }

    #[test]
    fn test_cross_prefix_same_sha_no_conflict() {
        let mut detector = ConflictDetector::new();

        // Different version prefixes resolving to same SHA should NOT conflict
        detector.add_requirement(test_resource_id("lib1"), "app1", "agents-v1.0.0", "abc123def456");
        detector.add_requirement(
            test_resource_id("lib1"),
            "app2",
            "snippets-v1.0.0",
            "abc123def456",
        );

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "Different version prefixes resolving to same SHA should not conflict"
        );
    }

    #[test]
    fn test_cross_prefix_different_sha_conflicts() {
        let mut detector = ConflictDetector::new();

        // Different version prefixes resolving to different SHAs SHOULD conflict
        detector.add_requirement(test_resource_id("lib1"), "app1", "agents-v1.0.0", "abc123def456");
        detector.add_requirement(
            test_resource_id("lib1"),
            "app2",
            "snippets-v1.0.0",
            "999888777666",
        );

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            1,
            "Different version prefixes resolving to different SHAs should conflict"
        );
    }

    #[test]
    fn test_many_requirements_same_sha_no_conflict() {
        let mut detector = ConflictDetector::new();

        // Multiple requirements with same SHA should NOT conflict
        detector.add_requirement(test_resource_id("lib1"), "app1", "^1.0.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app2", "^1.2.0", "abc123def456");
        detector.add_requirement(test_resource_id("lib1"), "app3", "~1.5.0", "abc123def456");
        detector.add_requirement(
            test_resource_id("lib1"),
            "app4",
            ">=1.0.0, <2.0.0",
            "abc123def456",
        );
        detector.add_requirement(test_resource_id("lib1"), "app5", "v1.8.0", "abc123def456");

        let conflicts = detector.detect_conflicts();
        assert_eq!(
            conflicts.len(),
            0,
            "Multiple requirements with same SHA should not conflict, regardless of version constraints"
        );
    }
}
