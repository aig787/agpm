//! Constraint set implementation for managing multiple version constraints.

use anyhow::Result;
use semver::Version;

use super::VersionConstraint;
use crate::core::AgpmError;

/// A collection of version constraints that must all be satisfied simultaneously.
///
/// `ConstraintSet` manages multiple [`VersionConstraint`]s for a single dependency,
/// ensuring that all constraints are compatible and can be resolved together.
/// It provides conflict detection, version matching, and best-match selection.
///
/// # Constraint Combination
///
/// When multiple constraints are added to a set, they create an intersection
/// of requirements. For example:
/// - `>=1.0.0` AND `<2.0.0` = versions in range `[1.0.0, 2.0.0)`
/// - `^1.0.0` AND `~1.2.0` = versions compatible with both (e.g., `1.2.x`)
///
/// # Conflict Detection
///
/// The constraint set detects and prevents conflicting constraints:
/// - Multiple exact versions: `1.0.0` AND `2.0.0` (impossible to satisfy)
/// - Conflicting Git refs: `main` AND `develop` (can't be both branches)
///
/// # Resolution Strategy
///
/// When selecting from available versions, the set:
/// 1. Filters versions that satisfy ALL constraints
/// 2. Excludes prereleases unless explicitly allowed
/// 3. Selects the highest remaining version
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust,no_run
/// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
/// use semver::Version;
///
/// let mut set = ConstraintSet::new();
/// set.add(VersionConstraint::parse(">=1.0.0")?)?;
/// set.add(VersionConstraint::parse("<2.0.0")?)?;
///
/// let version = Version::parse("1.5.0")?;
/// assert!(set.satisfies(&version));
///
/// let version = Version::parse("2.0.0")?;
/// assert!(!set.satisfies(&version)); // Outside range
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Best Match Selection
///
/// ```rust,no_run
/// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
/// use semver::Version;
///
/// let mut set = ConstraintSet::new();
/// set.add(VersionConstraint::parse("^1.0.0")?)?;
///
/// let versions = vec![
///     Version::parse("0.9.0")?,  // Too old
///     Version::parse("1.0.0")?,  // Matches
///     Version::parse("1.5.0")?,  // Matches, higher
///     Version::parse("2.0.0")?,  // Too new
/// ];
///
/// let best = set.find_best_match(&versions).unwrap();
/// assert_eq!(best, &Version::parse("1.5.0")?); // Highest compatible
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Conflict Detection
///
/// ```rust,no_run
/// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
/// use semver::Version;
///
/// let mut set = ConstraintSet::new();
/// set.add(VersionConstraint::parse("1.0.0")?)?; // Exact version
///
/// // This will fail - can't have two different exact versions
/// let result = set.add(VersionConstraint::parse("2.0.0")?);
/// assert!(result.is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct ConstraintSet {
    constraints: Vec<VersionConstraint>,
}

impl Default for ConstraintSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintSet {
    /// Creates a new empty constraint set
    ///
    /// # Returns
    ///
    /// Returns a new `ConstraintSet` with no constraints
    #[must_use]
    pub const fn new() -> Self {
        Self {
            constraints: Vec::new(),
        }
    }

    /// Add a constraint to this set with conflict detection.
    ///
    /// This method adds a new constraint to the set after checking for conflicts
    /// with existing constraints. If the new constraint would create an impossible
    /// situation (like requiring two different exact versions), an error is returned.
    ///
    /// # Arguments
    ///
    /// * `constraint` - The [`VersionConstraint`] to add to this set
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the constraint was added successfully, or `Err` if it
    /// conflicts with existing constraints.
    ///
    /// # Conflict Detection
    ///
    /// Current conflict detection covers:
    /// - **Exact version conflicts**: Different exact versions for the same dependency
    /// - **Git ref conflicts**: Different Git references for the same dependency
    ///
    /// Future versions may add more sophisticated conflict detection for semantic
    /// version ranges.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
    ///
    /// let mut set = ConstraintSet::new();
    ///
    /// // These constraints are compatible
    /// set.add(VersionConstraint::parse(">=1.0.0")?)?;
    /// set.add(VersionConstraint::parse("<2.0.0")?)?;
    ///
    /// // This would conflict with exact versions
    /// set.add(VersionConstraint::parse("1.5.0")?)?;
    /// let result = set.add(VersionConstraint::parse("1.6.0")?);
    /// assert!(result.is_err()); // Conflict: can't be both 1.5.0 AND 1.6.0
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn add(&mut self, constraint: VersionConstraint) -> Result<()> {
        // Check for conflicting constraints
        if self.has_conflict(&constraint) {
            return Err(AgpmError::Other {
                message: format!("Constraint {constraint} conflicts with existing constraints"),
            }
            .into());
        }

        self.constraints.push(constraint);
        Ok(())
    }

    /// Check if a version satisfies all constraints in this set.
    ///
    /// This method tests whether a given version passes all the constraints
    /// in this set. For a version to be acceptable, it must satisfy every
    /// single constraint - this represents a logical AND operation.
    ///
    /// # Arguments
    ///
    /// * `version` - The semantic version to test against all constraints
    ///
    /// # Returns
    ///
    /// Returns `true` if the version satisfies ALL constraints, `false` if it
    /// fails to satisfy any constraint.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
    /// use semver::Version;
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse(">=1.0.0")?)?; // Must be at least 1.0.0
    /// set.add(VersionConstraint::parse("<2.0.0")?)?;  // Must be less than 2.0.0
    /// set.add(VersionConstraint::parse("^1.0.0")?)?;  // Must be compatible with 1.0.0
    ///
    /// assert!(set.satisfies(&Version::parse("1.5.0")?)); // Satisfies all three
    /// assert!(!set.satisfies(&Version::parse("0.9.0")?)); // Fails >=1.0.0
    /// assert!(!set.satisfies(&Version::parse("2.0.0")?)); // Fails <2.0.0
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Performance Note
    ///
    /// This method short-circuits on the first constraint that fails, making it
    /// efficient even with many constraints.
    #[must_use]
    pub fn satisfies(&self, version: &Version) -> bool {
        self.constraints.iter().all(|c| c.matches(version))
    }

    /// Find the best matching version from a list of available versions.
    ///
    /// This method filters provided versions to find those that satisfy all
    /// constraints, then selects the "best" match according to AGPM's resolution
    /// strategy. The selection prioritizes newer versions while respecting prerelease
    /// preferences.
    ///
    /// # Resolution Strategy
    ///
    /// 1. **Filter candidates**: Keep only versions that satisfy all constraints
    /// 2. **Sort by version**: Order candidates from highest to lowest version
    /// 3. **Apply prerelease policy**: Remove prereleases unless explicitly allowed
    /// 4. **Select best**: Return the highest remaining version
    ///
    /// # Arguments
    ///
    /// * `versions` - Slice of available versions to choose from
    ///
    /// # Returns
    ///
    /// Returns `Some(&Version)` with the best matching version, or `None` if no
    /// version satisfies all constraints.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
    /// use semver::Version;
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse("^1.0.0")?)?;
    ///
    /// let versions = vec![
    ///     Version::parse("0.9.0")?,    // Too old
    ///     Version::parse("1.0.0")?,    // Compatible
    ///     Version::parse("1.2.0")?,    // Compatible, newer
    ///     Version::parse("1.5.0")?,    // Compatible, newest
    ///     Version::parse("2.0.0")?,    // Too new
    /// ];
    ///
    /// let best = set.find_best_match(&versions).unwrap();
    /// assert_eq!(best, &Version::parse("1.5.0")?); // Highest compatible version
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// ## Prerelease Handling
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
    /// use semver::Version;
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse("^1.0.0")?)?; // Doesn't allow prereleases
    ///
    /// let versions = vec![
    ///     Version::parse("1.0.0")?,
    ///     Version::parse("1.1.0-alpha.1")?,  // Prerelease
    ///     Version::parse("1.1.0")?,           // Stable
    /// ];
    ///
    /// let best = set.find_best_match(&versions).unwrap();
    /// assert_eq!(best, &Version::parse("1.1.0")?); // Stable version preferred
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub fn find_best_match<'a>(&self, versions: &'a [Version]) -> Option<&'a Version> {
        let mut candidates: Vec<&Version> = versions.iter().filter(|v| self.satisfies(v)).collect();

        // Sort by version (highest first) with deterministic tie-breaking
        // Note: Version comparison itself is deterministic, but this protects against potential future issues
        candidates.sort_by(|a, b| b.cmp(a));

        // If we don't allow prereleases, filter them out
        if !self.allows_prerelease() {
            candidates.retain(|v| v.pre.is_empty());
        }

        candidates.first().copied()
    }

    /// Check if any constraint in this set allows prerelease versions.
    ///
    /// This method determines the prerelease policy for the entire constraint set.
    /// If ANY constraint in the set allows prereleases, the entire set is considered
    /// to allow prereleases. This ensures that explicit prerelease constraints
    /// (like `latest-prerelease` or Git refs) are respected.
    ///
    /// # Returns
    ///
    /// Returns `true` if any constraint allows prereleases, `false` if all constraints
    /// exclude prereleases.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
    ///
    /// let mut stable_set = ConstraintSet::new();
    /// stable_set.add(VersionConstraint::parse("^1.0.0")?)?;
    /// stable_set.add(VersionConstraint::parse("~1.2.0")?)?;
    /// assert!(!stable_set.allows_prerelease()); // All constraints exclude prereleases
    ///
    /// let mut prerelease_set = ConstraintSet::new();
    /// prerelease_set.add(VersionConstraint::parse("^1.0.0")?)?;
    /// prerelease_set.add(VersionConstraint::parse("main")?)?; // Git ref allows prereleases
    /// assert!(prerelease_set.allows_prerelease()); // One constraint allows prereleases
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Impact on Resolution
    ///
    /// This setting affects [`find_best_match`](Self::find_best_match) behavior:
    /// - If `false`: Prerelease versions are filtered out before selection
    /// - If `true`: Prerelease versions are included in selection
    #[must_use]
    pub fn allows_prerelease(&self) -> bool {
        self.constraints.iter().any(VersionConstraint::allows_prerelease)
    }

    /// Check if a new constraint would conflict with existing constraints.
    ///
    /// This method performs conflict detection to prevent adding incompatible
    /// constraints to the same set. It currently detects basic conflicts but
    /// could be enhanced with more sophisticated analysis in the future.
    ///
    /// # Current Conflict Detection
    ///
    /// - **Exact version conflicts**: Two different exact versions (`1.0.0` vs `2.0.0`)
    /// - **Git reference conflicts**: Two different Git refs (`main` vs `develop`)
    ///
    /// # Arguments
    ///
    /// * `new_constraint` - The constraint to test for conflicts
    ///
    /// # Returns
    ///
    /// Returns `true` if the constraint conflicts with existing ones, `false` if
    /// it's compatible.
    ///
    /// # Future Enhancements
    ///
    /// Future versions could detect more sophisticated conflicts:
    /// - Impossible version ranges (e.g., `>2.0.0` AND `<1.0.0`)
    /// - Contradictory semver requirements
    /// - Mixed version and Git reference constraints
    ///
    /// # Examples
    ///
    /// ```rust,no_run,ignore
    /// use agpm_cli::version::constraints::{ConstraintSet, VersionConstraint};
    ///
    /// let mut set = ConstraintSet::new();
    /// set.add(VersionConstraint::parse("1.0.0")?)?;
    ///
    /// // This would conflict (different exact versions)
    /// let conflicting = VersionConstraint::parse("2.0.0")?;
    /// assert!(set.has_conflict(&conflicting));
    ///
    /// // This would not conflict (same exact version)
    /// let compatible = VersionConstraint::parse("1.0.0")?;
    /// assert!(!set.has_conflict(&compatible));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    fn has_conflict(&self, new_constraint: &VersionConstraint) -> bool {
        // Simple conflict detection - can be enhanced
        for existing in &self.constraints {
            match (existing, new_constraint) {
                (
                    VersionConstraint::Exact {
                        prefix: p1,
                        version: v1,
                    },
                    VersionConstraint::Exact {
                        prefix: p2,
                        version: v2,
                    },
                ) => {
                    // Different prefixes = different namespaces, no conflict
                    if p1 != p2 {
                        continue;
                    }
                    // Same prefix (or both None), conflict if different versions
                    if v1 != v2 {
                        return true;
                    }
                }
                (VersionConstraint::GitRef(r1), VersionConstraint::GitRef(r2)) => {
                    if r1 != r2 {
                        return true;
                    }
                }
                // For Requirement constraints, different prefixes = no conflict
                (
                    VersionConstraint::Exact {
                        prefix: p1,
                        ..
                    }
                    | VersionConstraint::Requirement {
                        prefix: p1,
                        ..
                    },
                    VersionConstraint::Requirement {
                        prefix: p2,
                        ..
                    },
                )
                | (
                    VersionConstraint::Requirement {
                        prefix: p1,
                        ..
                    },
                    VersionConstraint::Exact {
                        prefix: p2,
                        ..
                    },
                ) => {
                    // Different prefixes = different namespaces, no conflict
                    if p1 != p2 {
                        // Continue to next pair
                    }
                    // Same prefix - could do more sophisticated conflict detection here
                }
                _ => {
                    // More sophisticated conflict detection could be added here
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[test]
    fn test_constraint_set() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse(">=1.0.0").unwrap()).unwrap();
        set.add(VersionConstraint::parse("<2.0.0").unwrap()).unwrap();

        let v090 = Version::parse("0.9.0").unwrap();
        let v100 = Version::parse("1.0.0").unwrap();
        let v150 = Version::parse("1.5.0").unwrap();
        let v200 = Version::parse("2.0.0").unwrap();

        assert!(!set.satisfies(&v090));
        assert!(set.satisfies(&v100));
        assert!(set.satisfies(&v150));
        assert!(!set.satisfies(&v200));
    }

    #[test]
    fn test_find_best_match() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse("^1.0.0").unwrap()).unwrap();

        let versions = vec![
            Version::parse("0.9.0").unwrap(),
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.2.0").unwrap(),
            Version::parse("1.5.0").unwrap(),
            Version::parse("2.0.0").unwrap(),
        ];

        let best = set.find_best_match(&versions).unwrap();
        assert_eq!(best, &Version::parse("1.5.0").unwrap());
    }

    #[test]
    fn test_constraint_conflicts() -> Result<()> {
        let mut set = ConstraintSet::new();

        // Add first exact version
        set.add(VersionConstraint::Exact {
            prefix: None,
            version: Version::parse("1.0.0").unwrap(),
        })
        .unwrap();

        // Try to add conflicting exact version
        let result = set.add(VersionConstraint::Exact {
            prefix: None,
            version: Version::parse("2.0.0").unwrap(),
        });
        assert!(result.is_err());

        // Adding the same version should be ok
        let result = set.add(VersionConstraint::Exact {
            prefix: None,
            version: Version::parse("1.0.0").unwrap(),
        });
        result?;
        Ok(())
    }

    #[test]
    fn test_allows_prerelease() {
        assert!(VersionConstraint::GitRef("main".to_string()).allows_prerelease());
        assert!(VersionConstraint::GitRef("latest".to_string()).allows_prerelease()); // Git ref
        assert!(
            !VersionConstraint::Exact {
                prefix: None,
                version: Version::parse("1.0.0").unwrap()
            }
            .allows_prerelease()
        );
    }

    #[test]
    fn test_constraint_set_with_prereleases() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::GitRef("main".to_string())).unwrap();

        let v100_pre = Version::parse("1.0.0-alpha.1").unwrap();
        let v100 = Version::parse("1.0.0").unwrap();

        assert!(set.allows_prerelease());

        // Git refs don't match semver versions
        let versions = vec![v100_pre.clone(), v100.clone()];
        let best = set.find_best_match(&versions);
        assert!(best.is_none()); // Git refs don't match semver
    }

    #[test]
    fn test_constraint_set_no_matches() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse(">=2.0.0").unwrap()).unwrap();

        let versions = vec![Version::parse("1.0.0").unwrap(), Version::parse("1.5.0").unwrap()];

        let best = set.find_best_match(&versions);
        assert!(best.is_none());
    }

    #[test]
    fn test_constraint_set_git_ref_conflicts() -> Result<()> {
        let mut set = ConstraintSet::new();

        // Add first git ref
        set.add(VersionConstraint::GitRef("main".to_string())).unwrap();

        // Try to add conflicting git ref
        let result = set.add(VersionConstraint::GitRef("develop".to_string()));
        assert!(result.is_err());

        // Adding the same ref should be ok
        let result = set.add(VersionConstraint::GitRef("main".to_string()));
        result?;
        Ok(())
    }

    #[test]
    fn test_constraint_set_prerelease_filtering() {
        let mut set = ConstraintSet::new();
        set.add(VersionConstraint::parse("^1.0.0").unwrap()).unwrap();

        let versions = vec![
            Version::parse("1.0.0-alpha.1").unwrap(),
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.1.0-beta.1").unwrap(),
            Version::parse("1.1.0").unwrap(),
        ];

        let best = set.find_best_match(&versions).unwrap();
        assert_eq!(best, &Version::parse("1.1.0").unwrap()); // Should pick highest stable
    }

    #[test]
    fn test_constraint_set_no_conflict_different_types() {
        let mut set = ConstraintSet::new();

        // These shouldn't conflict as they are different types
        set.add(VersionConstraint::parse("^1.0.0").unwrap()).unwrap();
        set.add(VersionConstraint::GitRef("main".to_string())).unwrap();

        // Should have 2 constraints
        assert_eq!(set.constraints.len(), 2);
    }

    // ========== Prefix Support Tests ==========

    #[test]
    fn test_prefixed_constraint_conflicts() -> Result<()> {
        let mut set = ConstraintSet::new();

        // Add prefixed constraint
        set.add(VersionConstraint::parse("agents-^v1.0.0").unwrap()).unwrap();

        // Different prefix should not conflict
        let result = set.add(VersionConstraint::parse("snippets-^v1.0.0").unwrap());
        result?;

        // Same prefix but compatible constraints should not conflict
        let result = set.add(VersionConstraint::parse("agents-~v1.2.0").unwrap());
        result?;

        // Different prefixes for Exact constraints
        let mut exact_set = ConstraintSet::new();
        exact_set.add(VersionConstraint::parse("agents-v1.0.0").unwrap()).unwrap();

        // Different prefix, same version - should not conflict
        let result = exact_set.add(VersionConstraint::parse("snippets-v1.0.0").unwrap());
        result?;
        Ok(())
    }
}
