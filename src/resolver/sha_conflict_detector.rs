//! SHA-based conflict detection for dependency resolution.
//!
//! This module implements conflict detection that only reports true conflicts
//! when different SHAs are required for the same resource, regardless of
//! how those SHAs were specified (version constraint, branch, or rev).

use anyhow::Result;
use std::collections::HashMap;

use super::types::ResolutionMode;

/// A requirement for a resource from a specific dependency.
#[derive(Debug, Clone)]
pub struct ResolvedRequirement {
    /// Source repository name
    pub source: String,
    /// Resource path within the repository
    pub path: String,
    /// Resolved SHA-1 hash
    pub resolved_sha: String,
    /// Original version specification (for display)
    pub requested_version: String,
    /// Parent dependency that requires this resource
    pub required_by: String,
    /// Resolution mode used
    pub resolution_mode: ResolutionMode,
}

/// A SHA-based conflict detected for a resource.
#[derive(Debug, Clone)]
pub struct ShaConflict {
    /// Source repository name
    pub source: String,
    /// Resource path
    pub path: String,
    /// Conflicting requirements grouped by SHA
    pub sha_groups: HashMap<String, Vec<ResolvedRequirement>>,
}

impl ShaConflict {
    /// Format a user-friendly error message for the conflict.
    pub fn format_error(&self) -> String {
        format!(
            "SHA conflict for {}/{}:\n{}",
            self.source,
            self.path,
            self.sha_groups
                .iter()
                .map(|(sha, reqs)| {
                    format!(
                        "  SHA {} required by:\n{}",
                        &sha[..8.min(sha.len())],
                        reqs.iter()
                            .map(|r| format!(
                                "    - {} (via {})",
                                r.required_by,
                                match r.resolution_mode {
                                    ResolutionMode::Version =>
                                        format!("version={}", r.requested_version),
                                    ResolutionMode::GitRef =>
                                        format!("git ref={}", r.requested_version),
                                }
                            ))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

/// SHA-based conflict detector.
///
/// This detector groups requirements by resource and checks if different
/// SHAs are required for the same resource. Only reports conflicts when
/// SHAs actually differ, regardless of the version strings used.
pub struct ShaConflictDetector {
    /// Requirements grouped by (source, path)
    requirements: HashMap<(String, String), Vec<ResolvedRequirement>>,
}

impl Default for ShaConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ShaConflictDetector {
    /// Create a new SHA conflict detector.
    pub fn new() -> Self {
        Self {
            requirements: HashMap::new(),
        }
    }

    /// Add a resolved requirement to the detector.
    pub fn add_requirement(&mut self, requirement: ResolvedRequirement) {
        let key = (requirement.source.clone(), requirement.path.clone());
        self.requirements.entry(key).or_default().push(requirement);
    }

    /// Detect conflicts after all requirements have been added.
    ///
    /// Returns a list of conflicts where different SHAs are required
    /// for the same resource.
    pub fn detect_conflicts(&self) -> Result<Vec<ShaConflict>> {
        let mut conflicts = Vec::new();

        for ((source, path), requirements) in &self.requirements {
            // Group requirements by SHA
            let mut sha_groups: HashMap<String, Vec<ResolvedRequirement>> = HashMap::new();
            for req in requirements {
                sha_groups.entry(req.resolved_sha.clone()).or_default().push(req.clone());
            }

            // If we have multiple different SHAs, that's a conflict
            if sha_groups.len() > 1 {
                conflicts.push(ShaConflict {
                    source: source.clone(),
                    path: path.clone(),
                    sha_groups,
                });
            }
        }

        Ok(conflicts)
    }

    /// Get all requirements for a specific resource.
    pub fn get_requirements(&self, source: &str, path: &str) -> Option<&[ResolvedRequirement]> {
        self.requirements.get(&(source.to_string(), path.to_string())).map(|reqs| reqs.as_slice())
    }

    /// Clear all requirements from the detector.
    pub fn clear(&mut self) {
        self.requirements.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_conflict_same_sha() {
        let mut detector = ShaConflictDetector::new();

        // Two requirements with different version strings but same SHA
        detector.add_requirement(ResolvedRequirement {
            source: "test".to_string(),
            path: "agents/helper.md".to_string(),
            resolved_sha: "abc123def456".to_string(),
            requested_version: "v1.0.0".to_string(),
            required_by: "agent-a".to_string(),
            resolution_mode: ResolutionMode::Version,
        });

        detector.add_requirement(ResolvedRequirement {
            source: "test".to_string(),
            path: "agents/helper.md".to_string(),
            resolved_sha: "abc123def456".to_string(),
            requested_version: "main".to_string(),
            required_by: "agent-b".to_string(),
            resolution_mode: ResolutionMode::GitRef,
        });

        let conflicts = detector.detect_conflicts().unwrap();
        assert_eq!(conflicts.len(), 0);
    }

    #[test]
    fn test_conflict_different_shas() {
        let mut detector = ShaConflictDetector::new();

        // Two requirements with different SHAs
        detector.add_requirement(ResolvedRequirement {
            source: "test".to_string(),
            path: "agents/helper.md".to_string(),
            resolved_sha: "abc123def456".to_string(),
            requested_version: "v1.0.0".to_string(),
            required_by: "agent-a".to_string(),
            resolution_mode: ResolutionMode::Version,
        });

        detector.add_requirement(ResolvedRequirement {
            source: "test".to_string(),
            path: "agents/helper.md".to_string(),
            resolved_sha: "def456abc123".to_string(),
            requested_version: "main".to_string(),
            required_by: "agent-b".to_string(),
            resolution_mode: ResolutionMode::GitRef,
        });

        let conflicts = detector.detect_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);

        let conflict = &conflicts[0];
        assert_eq!(conflict.source, "test");
        assert_eq!(conflict.path, "agents/helper.md");
        assert_eq!(conflict.sha_groups.len(), 2);
    }

    #[test]
    fn test_conflict_formatting() {
        let mut detector = ShaConflictDetector::new();

        detector.add_requirement(ResolvedRequirement {
            source: "test".to_string(),
            path: "agents/helper.md".to_string(),
            resolved_sha: "abc123def456".to_string(),
            requested_version: "v1.0.0".to_string(),
            required_by: "agent-a".to_string(),
            resolution_mode: ResolutionMode::Version,
        });

        detector.add_requirement(ResolvedRequirement {
            source: "test".to_string(),
            path: "agents/helper.md".to_string(),
            resolved_sha: "def456abc123".to_string(),
            requested_version: "main".to_string(),
            required_by: "agent-b".to_string(),
            resolution_mode: ResolutionMode::GitRef,
        });

        let conflicts = detector.detect_conflicts().unwrap();
        let error_msg = conflicts[0].format_error();

        assert!(error_msg.contains("SHA conflict for test/agents/helper.md"));
        assert!(error_msg.contains("abc123de"));
        assert!(error_msg.contains("def456ab"));
        assert!(error_msg.contains("agent-a"));
        assert!(error_msg.contains("agent-b"));
        assert!(error_msg.contains("version=v1.0.0"));
        assert!(error_msg.contains("git ref=main"));
    }
}
