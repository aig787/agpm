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
/// This function now supports prefixed constraints like `agents-^v1.0.0`.
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
/// use agpm_cli::resolver::version_resolution::is_version_constraint;
/// assert!(is_version_constraint("^1.0.0"));
/// assert!(is_version_constraint("~1.2.0"));
/// assert!(is_version_constraint(">=1.0.0"));
/// assert!(is_version_constraint("*"));
/// assert!(is_version_constraint("agents-^v1.0.0")); // Prefixed constraint
/// assert!(is_version_constraint("agents-*")); // Prefixed wildcard
/// assert!(!is_version_constraint("v1.0.0"));
/// assert!(!is_version_constraint("agents-v1.0.0")); // Exact prefixed tag
/// assert!(!is_version_constraint("main"));
/// assert!(!is_version_constraint("abc123def"));
/// ```
#[must_use]
pub fn is_version_constraint(version: &str) -> bool {
    // Extract prefix first, then check the version part for constraint indicators
    let (_prefix, version_str) = crate::version::split_prefix_and_version(version);

    // Check for wildcard (works with or without prefix)
    if version_str == "*" {
        return true;
    }

    // Check for version constraint operators in the version part
    if version_str.starts_with('^')
        || version_str.starts_with('~')
        || version_str.starts_with('>')
        || version_str.starts_with('<')
        || version_str.starts_with('=')
        || version_str.contains(',')
    // Range constraints like ">=1.0.0, <2.0.0"
    {
        return true;
    }

    false
}

/// Parses Git tags into semantic versions, filtering out non-semver tags.
///
/// This function handles both prefixed and non-prefixed version tags,
/// including support for monorepo-style prefixes like `agents-v1.0.0`.
/// Tags that don't represent valid semantic versions are filtered out.
///
/// # Arguments
///
/// * `tags` - List of Git tag names from the repository
///
/// # Returns
///
/// A vector of tuples containing the original tag name and its parsed Version,
/// sorted by version (highest first). Tags with different prefixes are treated
/// independently and sorted only by their semver portions.
///
/// # Examples
///
/// ```
/// use agpm_cli::resolver::version_resolution::parse_tags_to_versions;
/// let tags = vec![
///     "v1.0.0".to_string(),
///     "agents-v2.0.0".to_string(),
///     "1.2.0".to_string(),
///     "feature-branch".to_string(),
///     "agents-v2.0.0-beta.1".to_string()
/// ];
/// let versions = parse_tags_to_versions(tags);
/// // Returns: [("agents-v2.0.0", Version), ("agents-v2.0.0-beta.1", Version), ("1.2.0", Version), ("v1.0.0", Version)]
/// ```
#[must_use]
pub fn parse_tags_to_versions(tags: Vec<String>) -> Vec<(String, Version)> {
    let mut versions = Vec::new();

    for tag in tags {
        // Extract prefix and version part (handles both prefixed and unprefixed)
        let (_prefix, version_str) = crate::version::split_prefix_and_version(&tag);

        // Strip 'v' prefix from version part
        let cleaned = version_str.trim_start_matches('v').trim_start_matches('V');

        if let Ok(version) = Version::parse(cleaned) {
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
/// 1. Extracting the prefix from the constraint (if any)
/// 2. Filtering tags to only those with matching prefix
/// 3. Parsing the constraint and matching tags
/// 4. Selecting the best match (usually the highest compatible version)
///
/// # Arguments
///
/// * `constraint_str` - The version constraint string (e.g., "^1.0.0", "agents-^v1.0.0")
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
/// use agpm_cli::resolver::version_resolution::find_best_matching_tag;
///
/// // Unprefixed constraint
/// let tags = vec!["v1.0.0".to_string(), "v1.2.0".to_string(), "v1.5.0".to_string(), "v2.0.0".to_string()];
/// let best = find_best_matching_tag("^1.0.0", tags)?;
/// assert_eq!(best, "v1.5.0"); // Highest 1.x.x version
///
/// // Prefixed constraint (monorepo)
/// let tags = vec!["agents-v1.0.0".to_string(), "agents-v1.2.0".to_string(), "snippets-v1.0.0".to_string()];
/// let best = find_best_matching_tag("agents-^v1.0.0", tags)?;
/// assert_eq!(best, "agents-v1.2.0"); // Highest agents 1.x.x version
/// # Ok(())
/// # }
/// ```
pub fn find_best_matching_tag(constraint_str: &str, tags: Vec<String>) -> Result<String> {
    // Extract prefix from constraint
    let (constraint_prefix, version_str) = crate::version::split_prefix_and_version(constraint_str);

    // Filter tags by prefix first
    let filtered_tags: Vec<String> = tags
        .into_iter()
        .filter(|tag| {
            let (tag_prefix, _) = crate::version::split_prefix_and_version(tag);
            tag_prefix.as_ref() == constraint_prefix.as_ref()
        })
        .collect();

    if filtered_tags.is_empty() {
        return Err(anyhow::anyhow!(
            "No tags found with matching prefix for constraint: {constraint_str}"
        ));
    }

    // Parse filtered tags to versions
    let tag_versions = parse_tags_to_versions(filtered_tags);

    if tag_versions.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid semantic version tags found for constraint: {constraint_str}"
        ));
    }

    // Special case: wildcard (*) matches the highest available version
    if version_str == "*" {
        // tag_versions is already sorted highest first
        return Ok(tag_versions[0].0.clone());
    }

    // Parse constraint using ONLY the version part (prefix already filtered)
    // This ensures semver matching works correctly after prefix filtering
    let constraint = VersionConstraint::parse(version_str)?;

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

    Err(anyhow::anyhow!("No tag found matching constraint: {constraint_str}"))
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

        // Not constraints (including "latest" - it's just a tag name)
        assert!(!is_version_constraint("v1.0.0"));
        assert!(!is_version_constraint("1.0.0"));
        assert!(!is_version_constraint("latest"));
        assert!(!is_version_constraint("latest-prerelease"));
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
        assert!(result.unwrap_err().to_string().contains("No tag found matching"));
    }

    #[test]
    fn test_wildcard_matches_highest_version() {
        let tags = vec![
            "v1.0.0".to_string(),
            "v1.2.0".to_string(),
            "v2.0.0".to_string(),
            "v1.5.0".to_string(),
        ];

        let result = find_best_matching_tag("*", tags).unwrap();
        assert_eq!(result, "v2.0.0", "Wildcard should match highest version");
    }

    #[test]
    fn test_prefixed_wildcard_matches_highest_in_namespace() {
        let tags = vec![
            "agents-v1.0.0".to_string(),
            "agents-v1.2.0".to_string(),
            "agents-v2.0.0".to_string(),
            "snippets-v3.0.0".to_string(), // Higher but different prefix
            "v5.0.0".to_string(),          // Higher but no prefix
        ];

        let result = find_best_matching_tag("agents-*", tags).unwrap();
        assert_eq!(
            result, "agents-v2.0.0",
            "Prefixed wildcard should match highest in that prefix namespace"
        );
    }

    // ========== Prefix Support Tests ==========

    #[test]
    fn test_is_version_constraint_with_prefix() {
        // Prefixed constraints
        assert!(is_version_constraint("agents-^v1.0.0"));
        assert!(is_version_constraint("snippets-~v2.0.0"));
        assert!(is_version_constraint("my-tool->=v1.0.0"));

        // Prefixed wildcards (critical fix)
        assert!(is_version_constraint("agents-*"));
        assert!(is_version_constraint("snippets-*"));
        assert!(is_version_constraint("my-tool-*"));

        // Prefixed exact versions are NOT constraints
        assert!(!is_version_constraint("agents-v1.0.0"));
        assert!(!is_version_constraint("snippets-v2.0.0"));
    }

    #[test]
    fn test_prefixed_wildcards_constraint_detection() {
        // Regression test for prefixed wildcards bug
        assert!(is_version_constraint("*"));
        assert!(is_version_constraint("agents-*"));
        assert!(is_version_constraint("tool123-*"));
        assert!(is_version_constraint("my-cool-tool-*"));

        // Ensure these are still NOT constraints
        assert!(!is_version_constraint("agents-v1.0.0"));
        assert!(!is_version_constraint("main"));
        assert!(!is_version_constraint("develop"));
    }

    #[test]
    fn test_parse_tags_to_versions_with_prefix() {
        let tags = vec![
            "agents-v1.0.0".to_string(),
            "agents-v2.0.0".to_string(),
            "snippets-v1.5.0".to_string(),
            "v1.0.0".to_string(),
            "main".to_string(),
        ];

        let versions = parse_tags_to_versions(tags);

        // Should parse 4 tags (all except "main")
        assert_eq!(versions.len(), 4);

        // Verify prefixed tags are parsed correctly
        assert!(versions.iter().any(|(tag, _)| tag == "agents-v2.0.0"));
        assert!(versions.iter().any(|(tag, _)| tag == "agents-v1.0.0"));
        assert!(versions.iter().any(|(tag, _)| tag == "snippets-v1.5.0"));
        assert!(versions.iter().any(|(tag, _)| tag == "v1.0.0"));
    }

    #[test]
    fn test_find_best_matching_tag_with_prefix() {
        let tags = vec![
            "agents-v1.0.0".to_string(),
            "agents-v1.2.0".to_string(),
            "agents-v2.0.0".to_string(),
            "snippets-v1.5.0".to_string(),
            "snippets-v2.0.0".to_string(),
            "v1.0.0".to_string(),
        ];

        // Prefixed constraint should match only tags with same prefix
        let result = find_best_matching_tag("agents-^v1.0.0", tags.clone()).unwrap();
        assert_eq!(result, "agents-v1.2.0"); // Highest agents 1.x

        // Different prefix
        let result = find_best_matching_tag("snippets-^v1.0.0", tags.clone()).unwrap();
        assert_eq!(result, "snippets-v1.5.0"); // Highest snippets 1.x

        // Unprefixed constraint should only match unprefixed tags
        let result = find_best_matching_tag("^v1.0.0", tags.clone()).unwrap();
        assert_eq!(result, "v1.0.0"); // Only unprefixed tag matching ^1.0
    }

    #[test]
    fn test_prefix_isolation_in_matching() {
        let tags = vec![
            "agents-v1.0.0".to_string(),
            "snippets-v2.0.0".to_string(), // Higher version but different prefix
        ];

        // Should NOT match snippets even though it has higher version
        let result = find_best_matching_tag("agents-^v1.0.0", tags.clone()).unwrap();
        assert_eq!(result, "agents-v1.0.0");
    }

    #[test]
    fn test_find_best_matching_tag_no_matching_prefix() {
        let tags = vec!["agents-v1.0.0".to_string(), "snippets-v1.0.0".to_string()];

        // No tags with "commands" prefix
        let result = find_best_matching_tag("commands-^v1.0.0", tags);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No tags found with matching prefix"));
    }

    #[test]
    fn test_parse_prefixed_tags_with_hyphens() {
        let tags = vec!["my-cool-agent-v1.0.0".to_string(), "tool-v-v2.0.0".to_string()];

        let versions = parse_tags_to_versions(tags);

        assert_eq!(versions.len(), 2);
        // Both should parse correctly despite hyphens in prefix
        assert!(versions.iter().any(|(tag, ver)| {
            tag == "my-cool-agent-v1.0.0" && *ver == Version::parse("1.0.0").unwrap()
        }));
        assert!(versions.iter().any(|(tag, ver)| {
            tag == "tool-v-v2.0.0" && *ver == Version::parse("2.0.0").unwrap()
        }));
    }
}
