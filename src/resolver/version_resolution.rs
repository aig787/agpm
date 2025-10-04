//! Version constraint resolution helpers for the dependency resolver.
//!
//! This module provides utilities to bridge between version constraints
//! (like `^1.0.0`, `~1.2.0`) and actual Git tags in repositories.

use anyhow::Result;
use semver::Version;

use crate::version::constraints::{ConstraintSet, VersionConstraint};

/// Checks if a string represents a version constraint rather than a direct reference.
///
/// Version constraints contain operators like `^`, `~`, `>`, `<`, `=`, or special
/// keywords. Direct references are branch names, tag names, or commit hashes.
///
/// # Arguments
///
/// * `version` - The version string to check
///
/// # Returns
///
/// Returns `true` if the string contains constraint operators or keywords,
/// `false` for plain tags, branches, or commit hashes.
///
/// # Examples
///
/// ```
/// use agpm::resolver::version_resolution::is_version_constraint;
/// assert!(is_version_constraint("^1.0.0"));
/// assert!(is_version_constraint("~1.2.0"));
/// assert!(is_version_constraint(">=1.0.0"));
/// assert!(!is_version_constraint("v1.0.0"));
/// assert!(!is_version_constraint("main"));
/// assert!(!is_version_constraint("abc123def"));
/// ```
#[must_use]
pub fn is_version_constraint(version: &str) -> bool {
    // Check for special keywords
    // Note: "latest" is not supported - use explicit version constraints instead
    if version == "*" {
        return true;
    }

    // Check for version constraint operators
    if version.starts_with('^')
        || version.starts_with('~')
        || version.starts_with('>')
        || version.starts_with('<')
        || version.starts_with('=')
        || version.contains(',')
    // Range constraints like ">=1.0.0, <2.0.0"
    {
        return true;
    }

    false
}

/// Parses Git tags into semantic versions, filtering out non-semver tags.
///
/// This function handles both `v`-prefixed and non-prefixed version tags,
/// ignoring tags that don't represent valid semantic versions.
///
/// # Arguments
///
/// * `tags` - List of Git tag names from the repository
///
/// # Returns
///
/// A vector of tuples containing the original tag name and its parsed Version,
/// sorted by version (highest first).
///
/// # Examples
///
/// ```
/// use agpm::resolver::version_resolution::parse_tags_to_versions;
/// let tags = vec!["v1.0.0".to_string(), "1.2.0".to_string(), "feature-branch".to_string(), "v2.0.0-beta.1".to_string()];
/// let versions = parse_tags_to_versions(tags);
/// // Returns: [("v2.0.0-beta.1", Version), ("1.2.0", Version), ("v1.0.0", Version)]
/// ```
#[must_use]
pub fn parse_tags_to_versions(tags: Vec<String>) -> Vec<(String, Version)> {
    let mut versions = Vec::new();

    for tag in tags {
        // Try parsing with and without 'v' prefix
        let version_str = tag.strip_prefix('v').unwrap_or(&tag);

        if let Ok(version) = Version::parse(version_str) {
            versions.push((tag, version));
        }
    }

    // Sort by version, highest first
    versions.sort_by(|a, b| b.1.cmp(&a.1));

    versions
}

/// Finds the best matching tag for a version constraint.
///
/// This function resolves version constraints to actual Git tags by:
/// 1. Parsing the constraint
/// 2. Finding all tags that satisfy the constraint
/// 3. Selecting the best match (usually the highest compatible version)
///
/// # Arguments
///
/// * `constraint_str` - The version constraint string (e.g., "^1.0.0")
/// * `tags` - List of Git tags from the repository
///
/// # Returns
///
/// Returns the best matching tag name, or an error if no tag satisfies the constraint.
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # fn example() -> Result<()> {
/// use agpm::resolver::version_resolution::find_best_matching_tag;
/// let tags = vec!["v1.0.0".to_string(), "v1.2.0".to_string(), "v1.5.0".to_string(), "v2.0.0".to_string()];
/// let best = find_best_matching_tag("^1.0.0", tags)?;
/// assert_eq!(best, "v1.5.0"); // Highest 1.x.x version
/// # Ok(())
/// # }
/// ```
pub fn find_best_matching_tag(constraint_str: &str, tags: Vec<String>) -> Result<String> {
    let constraint = VersionConstraint::parse(constraint_str)?;

    // Parse tags to versions
    let tag_versions = parse_tags_to_versions(tags);

    // Extract just the versions for constraint matching
    let versions: Vec<Version> = tag_versions.iter().map(|(_, v)| v.clone()).collect();

    // Create a constraint set with just this constraint
    let mut constraint_set = ConstraintSet::new();
    constraint_set.add(constraint)?;

    // Find the best match
    if let Some(best_version) = constraint_set.find_best_match(&versions) {
        // Find the original tag name for this version
        for (tag_name, version) in tag_versions {
            if &version == best_version {
                return Ok(tag_name);
            }
        }
    }

    Err(anyhow::anyhow!(
        "No tag found matching constraint: {constraint_str}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_version_constraint() {
        // Constraints
        assert!(is_version_constraint("^1.0.0"));
        assert!(is_version_constraint("~1.2.0"));
        assert!(is_version_constraint(">=1.0.0"));
        assert!(is_version_constraint("<2.0.0"));
        assert!(is_version_constraint(">=1.0.0, <2.0.0"));
        assert!(is_version_constraint("*"));

        // Not constraints
        assert!(!is_version_constraint("v1.0.0"));
        assert!(!is_version_constraint("1.0.0"));
        assert!(!is_version_constraint("main"));
        assert!(!is_version_constraint("develop"));
        assert!(!is_version_constraint("abc123def"));
        assert!(!is_version_constraint("feature/auth"));
    }

    #[test]
    fn test_parse_tags_to_versions() {
        let tags = vec![
            "v1.0.0".to_string(),
            "1.2.0".to_string(),
            "v2.0.0-beta.1".to_string(),
            "main".to_string(),
            "feature-branch".to_string(),
            "v1.5.0".to_string(),
        ];

        let versions = parse_tags_to_versions(tags);

        assert_eq!(versions.len(), 4);
        assert_eq!(versions[0].0, "v2.0.0-beta.1");
        assert_eq!(versions[1].0, "v1.5.0");
        assert_eq!(versions[2].0, "1.2.0");
        assert_eq!(versions[3].0, "v1.0.0");
    }

    #[test]
    fn test_find_best_matching_tag() {
        let tags = vec![
            "v1.0.0".to_string(),
            "v1.2.0".to_string(),
            "v1.5.0".to_string(),
            "v2.0.0".to_string(),
            "v2.1.0".to_string(),
        ];

        // Test caret constraint
        let result = find_best_matching_tag("^1.0.0", tags.clone()).unwrap();
        assert_eq!(result, "v1.5.0");

        // Test tilde constraint
        let result = find_best_matching_tag("~1.2.0", tags.clone()).unwrap();
        assert_eq!(result, "v1.2.0");

        // Test greater than or equal
        let result = find_best_matching_tag(">=2.0.0", tags.clone()).unwrap();
        assert_eq!(result, "v2.1.0");
    }

    #[test]
    fn test_find_best_matching_tag_no_match() {
        let tags = vec!["v1.0.0".to_string(), "v2.0.0".to_string()];

        let result = find_best_matching_tag("^3.0.0", tags);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No tag found matching")
        );
    }
}
