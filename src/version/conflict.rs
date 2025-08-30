//! Version conflict detection and reporting.
//!
//! This module handles detection and reporting of version conflicts that can occur
//! when multiple dependencies require incompatible versions of the same resource.
//! It provides detailed conflict information to help users resolve dependency issues.

use anyhow::Result;
use semver::{Version, VersionReq};
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::core::CcpmError;

/// Represents a version conflict between dependencies
#[derive(Debug, Clone)]
pub struct VersionConflict {
    pub resource: String,
    pub conflicting_requirements: Vec<ConflictingRequirement>,
}

#[derive(Debug, Clone)]
pub struct ConflictingRequirement {
    pub required_by: String,
    pub requirement: String,
    pub resolved_version: Option<Version>,
}

impl fmt::Display for VersionConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Version conflict for '{}':", self.resource)?;
        for req in &self.conflicting_requirements {
            writeln!(f, "  - {} requires {}", req.required_by, req.requirement)?;
            if let Some(v) = &req.resolved_version {
                writeln!(f, "    (resolved to {})", v)?;
            }
        }
        Ok(())
    }
}

/// Detects and resolves version conflicts
pub struct ConflictDetector {
    requirements: HashMap<String, Vec<(String, String)>>, // resource -> [(requirer, requirement)]
}

impl ConflictDetector {
    pub fn new() -> Self {
        Self {
            requirements: HashMap::new(),
        }
    }

    /// Add a dependency requirement
    pub fn add_requirement(&mut self, resource: &str, required_by: &str, requirement: &str) {
        self.requirements
            .entry(resource.to_string())
            .or_insert_with(Vec::new)
            .push((required_by.to_string(), requirement.to_string()));
    }

    /// Detect conflicts in the current requirements
    pub fn detect_conflicts(&self) -> Vec<VersionConflict> {
        let mut conflicts = Vec::new();

        for (resource, requirements) in &self.requirements {
            if requirements.len() <= 1 {
                continue; // No conflict possible with single requirement
            }

            // Check if requirements are compatible
            if !self.are_requirements_compatible(requirements) {
                let conflict = VersionConflict {
                    resource: resource.clone(),
                    conflicting_requirements: requirements
                        .iter()
                        .map(|(requirer, req)| ConflictingRequirement {
                            required_by: requirer.clone(),
                            requirement: req.clone(),
                            resolved_version: None,
                        })
                        .collect(),
                };
                conflicts.push(conflict);
            }
        }

        conflicts
    }

    /// Check if a set of requirements are compatible
    fn are_requirements_compatible(&self, requirements: &[(String, String)]) -> bool {
        // Parse all requirements
        let parsed_reqs: Vec<_> = requirements
            .iter()
            .filter_map(|(_, req)| {
                if req == "latest" || req == "*" {
                    Some(VersionReq::parse("*").unwrap())
                } else {
                    VersionReq::parse(req).ok()
                }
            })
            .collect();

        if parsed_reqs.len() != requirements.len() {
            // Some requirements couldn't be parsed, might be git refs
            return self.check_git_ref_compatibility(requirements);
        }

        // Check if there's any version that satisfies all requirements
        // This is a simplified check - in reality we'd need available versions
        self.can_satisfy_all(&parsed_reqs)
    }

    /// Check if git references are compatible
    fn check_git_ref_compatibility(&self, requirements: &[(String, String)]) -> bool {
        let refs: HashSet<_> = requirements
            .iter()
            .filter_map(|(_, req)| {
                if !req.starts_with('^')
                    && !req.starts_with('~')
                    && !req.starts_with('>')
                    && !req.starts_with('<')
                    && !req.starts_with('=')
                    && req != "latest"
                    && req != "*"
                {
                    Some(req.as_str())
                } else {
                    None
                }
            })
            .collect();

        // All git refs must be the same
        refs.len() <= 1
    }

    /// Check if all requirements can be satisfied by some version
    fn can_satisfy_all(&self, requirements: &[VersionReq]) -> bool {
        // This is a heuristic - we check common version ranges
        // In a real implementation, we'd check against actual available versions

        let test_versions = vec![
            Version::parse("0.1.0").unwrap(),
            Version::parse("0.5.0").unwrap(),
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.5.0").unwrap(),
            Version::parse("2.0.0").unwrap(),
            Version::parse("2.5.0").unwrap(),
            Version::parse("3.0.0").unwrap(),
        ];

        for version in &test_versions {
            if requirements.iter().all(|req| req.matches(version)) {
                return true;
            }
        }

        false
    }

    /// Try to resolve conflicts by finding compatible versions
    pub fn resolve_conflicts(
        &self,
        available_versions: &HashMap<String, Vec<Version>>,
    ) -> Result<HashMap<String, Version>> {
        let mut resolved = HashMap::new();
        let conflicts = self.detect_conflicts();

        if !conflicts.is_empty() {
            let conflict_messages: Vec<String> = conflicts.iter().map(|c| c.to_string()).collect();

            return Err(CcpmError::Other(format!(
                "Unable to resolve version conflicts:\n{}",
                conflict_messages.join("\n")
            ))
            .into());
        }

        // Resolve each resource to its best version
        for (resource, requirements) in &self.requirements {
            let versions = available_versions.get(resource).ok_or_else(|| {
                CcpmError::Other {
                    message: format!("No versions available for resource: {}", resource),
                }
            })?;

            let best_version = self.find_best_version(versions, requirements)?;
            resolved.insert(resource.clone(), best_version);
        }

        Ok(resolved)
    }

    /// Find the best version that satisfies all requirements
    fn find_best_version(
        &self,
        available: &[Version],
        requirements: &[(String, String)],
    ) -> Result<Version> {
        let mut candidates = available.to_vec();

        // Filter by each requirement
        for (_, req_str) in requirements {
            if req_str == "latest" || req_str == "*" {
                continue; // These match everything
            }

            if let Ok(req) = VersionReq::parse(req_str) {
                candidates.retain(|v| req.matches(v));
            }
        }

        if candidates.is_empty() {
            return Err(CcpmError::Other {
                message: format!(
                    "No version satisfies all requirements: {:?}",
                    requirements
                ),
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

impl CircularDependencyDetector {
    pub fn new() -> Self {
        Self {
            graph: HashMap::new(),
        }
    }

    /// Add a dependency edge
    pub fn add_dependency(&mut self, from: &str, to: &str) {
        self.graph
            .entry(from.to_string())
            .or_insert_with(HashSet::new)
            .insert(to.to_string());
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

    #[test]
    fn test_conflict_detection() {
        let mut detector = ConflictDetector::new();

        // Add compatible requirements
        detector.add_requirement("lib1", "app1", "^1.0.0");
        detector.add_requirement("lib1", "app2", "^1.2.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0); // These are compatible

        // Add incompatible requirements
        detector.add_requirement("lib2", "app1", "^1.0.0");
        detector.add_requirement("lib2", "app2", "^2.0.0");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].resource, "lib2");
    }

    #[test]
    fn test_git_ref_compatibility() {
        let mut detector = ConflictDetector::new();

        // Same git ref - compatible
        detector.add_requirement("lib1", "app1", "main");
        detector.add_requirement("lib1", "app2", "main");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 0);

        // Different git refs - incompatible
        detector.add_requirement("lib2", "app1", "main");
        detector.add_requirement("lib2", "app2", "develop");

        let conflicts = detector.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn test_resolve_conflicts() {
        let mut detector = ConflictDetector::new();
        detector.add_requirement("lib1", "app1", "^1.0.0");
        detector.add_requirement("lib1", "app2", "^1.2.0");

        let mut available = HashMap::new();
        available.insert(
            "lib1".to_string(),
            vec![
                Version::parse("1.0.0").unwrap(),
                Version::parse("1.2.0").unwrap(),
                Version::parse("1.5.0").unwrap(),
                Version::parse("2.0.0").unwrap(),
            ],
        );

        let resolved = detector.resolve_conflicts(&available).unwrap();
        assert_eq!(
            resolved.get("lib1"),
            Some(&Version::parse("1.5.0").unwrap())
        );
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
            resource: "test-lib".to_string(),
            conflicting_requirements: vec![
                ConflictingRequirement {
                    required_by: "app1".to_string(),
                    requirement: "^1.0.0".to_string(),
                    resolved_version: Some(Version::parse("1.5.0").unwrap()),
                },
                ConflictingRequirement {
                    required_by: "app2".to_string(),
                    requirement: "^2.0.0".to_string(),
                    resolved_version: None,
                },
            ],
        };

        let display = format!("{}", conflict);
        assert!(display.contains("test-lib"));
        assert!(display.contains("app1"));
        assert!(display.contains("^1.0.0"));
        assert!(display.contains("1.5.0"));
    }
}
