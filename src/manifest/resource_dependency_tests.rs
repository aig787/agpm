//! Tests for resource dependency types and implementations.
//!
//! These tests cover the `ResourceDependency` and `DetailedDependency` types,
//! including mutability detection, version constraint parsing, and branch detection.

use super::resource_dependency::{DetailedDependency, ResourceDependency};

/// Helper to create a DetailedDependency with all None fields except those specified
fn detailed_dep(
    source: Option<&str>,
    path: &str,
    version: Option<&str>,
    branch: Option<&str>,
    rev: Option<&str>,
) -> DetailedDependency {
    DetailedDependency {
        source: source.map(String::from),
        path: path.to_string(),
        version: version.map(String::from),
        branch: branch.map(String::from),
        rev: rev.map(String::from),
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: None,
        flatten: None,
        install: None,
        template_vars: None,
    }
}

mod mutability_tests {
    use super::*;

    #[test]
    fn test_is_mutable_simple_local_path() {
        let dep = ResourceDependency::Simple("../local/file.md".to_string());
        assert!(dep.is_mutable(), "Local paths should be mutable");
    }

    #[test]
    fn test_is_mutable_detailed_local_path() {
        let dep = ResourceDependency::Detailed(Box::new(detailed_dep(
            None,
            "../local/file.md",
            None,
            None,
            None,
        )));
        assert!(dep.is_mutable(), "Local paths (no source) should be mutable");
    }

    #[test]
    fn test_is_mutable_detailed_with_branch() {
        let dep = ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("repo"),
            "file.md",
            None,
            Some("main"),
            None,
        )));
        assert!(dep.is_mutable(), "Branch refs should be mutable");
    }

    #[test]
    fn test_is_mutable_detailed_with_rev() {
        let dep = ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("repo"),
            "file.md",
            None,
            None,
            Some("abc123def456789012345678901234567890abcd"),
        )));
        assert!(!dep.is_mutable(), "SHA-pinned deps should be immutable");
    }

    #[test]
    fn test_is_mutable_semver_version() {
        let dep = ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("repo"),
            "file.md",
            Some("^1.0.0"),
            None,
            None,
        )));
        assert!(!dep.is_mutable(), "Semver versions should be immutable");
    }

    #[test]
    fn test_is_mutable_branch_like_version() {
        let dep = ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("repo"),
            "file.md",
            Some("develop"),
            None,
            None,
        )));
        assert!(dep.is_mutable(), "Branch-like versions should be mutable");
    }

    #[test]
    fn test_is_mutable_no_version_or_branch_or_rev() {
        let dep = ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("repo"),
            "file.md",
            None,
            None,
            None,
        )));
        assert!(dep.is_mutable(), "Undefined version should be mutable (safe default)");
    }

    #[test]
    fn test_is_mutable_prefixed_semver() {
        let dep = ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("repo"),
            "file.md",
            Some("agents-v1.0.0"),
            None,
            None,
        )));
        assert!(!dep.is_mutable(), "Prefixed semver should be immutable");
    }
}

mod branch_detection_tests {
    use super::*;

    // Semver patterns (immutable)
    #[test]
    fn test_semver_exact() {
        assert!(!ResourceDependency::is_branch_like_version("v1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("V1.0.0"));
    }

    #[test]
    fn test_semver_caret() {
        assert!(!ResourceDependency::is_branch_like_version("^1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("^v1.0.0"));
    }

    #[test]
    fn test_semver_tilde() {
        assert!(!ResourceDependency::is_branch_like_version("~1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("~v1.0.0"));
    }

    #[test]
    fn test_semver_range_operators() {
        assert!(!ResourceDependency::is_branch_like_version(">=1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("<=1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version(">1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("<1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("=1.0.0"));
    }

    #[test]
    fn test_semver_with_prefix() {
        assert!(!ResourceDependency::is_branch_like_version("agents-v1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("prefix-^v1.0.0"));
        assert!(!ResourceDependency::is_branch_like_version("claude-code-agent-v1.0.0"));
    }

    // Branch patterns (mutable)
    #[test]
    fn test_branch_main() {
        assert!(ResourceDependency::is_branch_like_version("main"));
    }

    #[test]
    fn test_branch_master() {
        assert!(ResourceDependency::is_branch_like_version("master"));
    }

    #[test]
    fn test_branch_develop() {
        assert!(ResourceDependency::is_branch_like_version("develop"));
    }

    #[test]
    fn test_branch_feature() {
        assert!(ResourceDependency::is_branch_like_version("feature/xyz"));
        assert!(ResourceDependency::is_branch_like_version("feature/my-feature"));
    }

    #[test]
    fn test_branch_release_without_version() {
        // "release/1.0" looks like a branch (not semver)
        assert!(ResourceDependency::is_branch_like_version("release/foo"));
    }

    // Edge cases
    #[test]
    fn test_empty_string() {
        assert!(ResourceDependency::is_branch_like_version(""));
        assert!(ResourceDependency::is_branch_like_version("   "));
    }

    #[test]
    fn test_whitespace_trimmed() {
        assert!(!ResourceDependency::is_branch_like_version(" v1.0.0 "));
        assert!(ResourceDependency::is_branch_like_version(" main "));
    }

    // Full SHA hashes are immutable
    #[test]
    fn test_full_sha_hash_immutable() {
        // 40-character hex SHA is immutable (points to specific commit)
        assert!(!ResourceDependency::is_branch_like_version(
            "abc123def456789012345678901234567890abcd"
        ));
        assert!(!ResourceDependency::is_branch_like_version(
            "0000000000000000000000000000000000000000"
        ));
        assert!(!ResourceDependency::is_branch_like_version(
            "ffffffffffffffffffffffffffffffffffffffff"
        ));

        // Short SHAs (not 40 chars) are still treated as branches (could be ambiguous)
        assert!(ResourceDependency::is_branch_like_version("abc1234"));
        assert!(ResourceDependency::is_branch_like_version(
            "abc123def456789012345678901234567890123"
        )); // 39 chars
    }
}
