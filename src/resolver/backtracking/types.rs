//! Public types for backtracking results.
//!
//! This module contains the public types returned by the backtracking resolver.

/// State of a single backtracking iteration.
#[derive(Debug, Clone)]
pub struct BacktrackingIteration {
    /// Iteration number (1-indexed)
    pub iteration: usize,

    /// Conflicts detected at start of this iteration
    pub conflicts: Vec<crate::version::conflict::VersionConflict>,

    /// Updates applied during this iteration
    pub updates: Vec<VersionUpdate>,

    /// Number of transitive deps re-resolved
    pub transitive_reresolutions: usize,

    /// Whether this iteration made progress
    pub made_progress: bool,
}

/// Reason for termination of backtracking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationReason {
    /// All conflicts successfully resolved
    Success,

    /// Reached maximum iteration limit
    MaxIterations,

    /// Reached timeout
    Timeout,

    /// No progress made (same conflicts as previous iteration)
    NoProgress,

    /// Detected oscillation (cycling between states)
    Oscillation,

    /// Failed to find compatible version
    NoCompatibleVersion,
}

/// Result of a backtracking attempt.
#[derive(Debug, Clone)]
pub struct BacktrackingResult {
    /// Whether conflicts were successfully resolved
    pub resolved: bool,

    /// List of ALL version updates made across all iterations
    pub updates: Vec<VersionUpdate>,

    /// Number of backtracking iterations performed
    pub iterations: usize,

    /// Total number of version resolutions attempted
    pub attempted_versions: usize,

    /// History of each iteration (for debugging/logging)
    pub iteration_history: Vec<BacktrackingIteration>,

    /// Total transitive deps re-resolved across all iterations
    pub total_transitive_reresolutions: usize,

    /// Reason for termination
    pub termination_reason: TerminationReason,
}

/// Record of a version update made during backtracking.
#[derive(Debug, Clone)]
pub struct VersionUpdate {
    /// Resource identifier (format: "source:required_by")
    pub resource_id: String,

    /// Original version constraint
    pub old_version: String,

    /// New version selected
    pub new_version: String,

    /// Original resolved SHA
    pub old_sha: String,

    /// New resolved SHA
    pub new_sha: String,

    /// Template variables (variant inputs) for this resource
    pub variant_inputs: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backtracking_result_structure() {
        let result = BacktrackingResult {
            resolved: true,
            updates: vec![VersionUpdate {
                resource_id: "community:test".to_string(),
                old_version: "v1.0.0".to_string(),
                new_version: "v1.0.1".to_string(),
                old_sha: "abc123".to_string(),
                new_sha: "def456".to_string(),
                variant_inputs: None,
            }],
            iterations: 1,
            attempted_versions: 5,
            iteration_history: vec![],
            total_transitive_reresolutions: 0,
            termination_reason: TerminationReason::Success,
        };

        assert!(result.resolved);
        assert_eq!(result.updates.len(), 1);
        assert_eq!(result.iterations, 1);
        assert_eq!(result.attempted_versions, 5);
        assert_eq!(result.termination_reason, TerminationReason::Success);
    }
}
