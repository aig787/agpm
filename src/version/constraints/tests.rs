//! Tests for version constraints module.

use semver::Version;

use super::VersionConstraint;

#[test]
fn test_version_constraint_parse() {
    // Exact version
    let constraint = VersionConstraint::parse("1.0.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Exact { .. }));

    // Version with v prefix
    let constraint = VersionConstraint::parse("v1.0.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Exact { .. }));

    // Caret requirement
    let constraint = VersionConstraint::parse("^1.0.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Requirement { .. }));

    // Tilde requirement
    let constraint = VersionConstraint::parse("~1.2.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Requirement { .. }));

    // Range requirement
    let constraint = VersionConstraint::parse(">=1.0.0, <2.0.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Requirement { .. }));

    // Git refs (including "latest" - it's just a tag name)
    let constraint = VersionConstraint::parse("latest").unwrap();
    assert!(matches!(constraint, VersionConstraint::GitRef(_)));

    let constraint = VersionConstraint::parse("main").unwrap();
    assert!(matches!(constraint, VersionConstraint::GitRef(_)));
}

#[test]
fn test_constraint_matching() {
    let v100 = Version::parse("1.0.0").unwrap();
    let v110 = Version::parse("1.1.0").unwrap();
    let v200 = Version::parse("2.0.0").unwrap();

    // Exact match
    let exact = VersionConstraint::Exact {
        prefix: None,
        version: v100.clone(),
    };
    assert!(exact.matches(&v100));
    assert!(!exact.matches(&v110));

    // Caret requirement
    let caret = VersionConstraint::parse("^1.0.0").unwrap();
    assert!(caret.matches(&v100));
    assert!(caret.matches(&v110));
    assert!(!caret.matches(&v200));

    // Git refs don't match semantic versions
    let git_ref = VersionConstraint::GitRef("latest".to_string());
    assert!(!git_ref.matches(&v100));
    assert!(!git_ref.matches(&v200));
}

#[test]
fn test_version_constraint_parse_edge_cases() {
    // Test latest-prerelease (just a tag name)
    let constraint = VersionConstraint::parse("latest-prerelease").unwrap();
    assert!(matches!(constraint, VersionConstraint::GitRef(_)));

    // Test asterisk wildcard
    let constraint = VersionConstraint::parse("*").unwrap();
    assert!(matches!(constraint, VersionConstraint::GitRef(_)));

    // Test range operators
    let constraint = VersionConstraint::parse(">=1.0.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Requirement { .. }));

    let constraint = VersionConstraint::parse("<2.0.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Requirement { .. }));

    let constraint = VersionConstraint::parse("=1.0.0").unwrap();
    assert!(matches!(constraint, VersionConstraint::Requirement { .. }));

    // Test git branch names
    let constraint = VersionConstraint::parse("feature/new-feature").unwrap();
    assert!(matches!(constraint, VersionConstraint::GitRef(_)));

    // Test commit hash
    let constraint = VersionConstraint::parse("abc123def456").unwrap();
    assert!(matches!(constraint, VersionConstraint::GitRef(_)));
}

#[test]
fn test_version_constraint_display() {
    let exact = VersionConstraint::Exact {
        prefix: None,
        version: Version::parse("1.0.0").unwrap(),
    };
    assert_eq!(format!("{exact}"), "1.0.0");

    let req = VersionConstraint::parse("^1.0.0").unwrap();
    assert_eq!(format!("{req}"), "^1.0.0");

    let git_ref = VersionConstraint::GitRef("main".to_string());
    assert_eq!(format!("{git_ref}"), "main");

    let latest = VersionConstraint::GitRef("latest".to_string());
    assert_eq!(format!("{latest}"), "latest");
}

#[test]
fn test_version_constraint_matches_ref() {
    let git_ref = VersionConstraint::GitRef("main".to_string());
    assert!(git_ref.matches_ref("main"));
    assert!(!git_ref.matches_ref("develop"));

    // Other constraint types should return false for ref matching
    let exact = VersionConstraint::Exact {
        prefix: None,
        version: Version::parse("1.0.0").unwrap(),
    };
    assert!(!exact.matches_ref("v1.0.0"));

    let latest = VersionConstraint::GitRef("latest".to_string());
    assert!(latest.matches_ref("latest"));
}

#[test]
fn test_version_constraint_to_version_req() {
    let exact = VersionConstraint::Exact {
        prefix: None,
        version: Version::parse("1.0.0").unwrap(),
    };
    let req = exact.to_version_req().unwrap();
    assert!(req.matches(&Version::parse("1.0.0").unwrap()));

    let caret = VersionConstraint::parse("^1.0.0").unwrap();
    let req = caret.to_version_req().unwrap();
    assert!(req.matches(&Version::parse("1.0.0").unwrap()));

    let git_ref = VersionConstraint::GitRef("main".to_string());
    assert!(git_ref.to_version_req().is_none());

    let latest = VersionConstraint::GitRef("latest".to_string());
    assert!(latest.to_version_req().is_none()); // Git ref - cannot convert
}

#[test]
fn test_git_ref_constraint_with_versions() {
    let git_ref = VersionConstraint::GitRef("latest".to_string());

    let v100_pre = Version::parse("1.0.0-alpha.1").unwrap();
    let v100 = Version::parse("1.0.0").unwrap();

    // Git refs don't match semantic versions
    assert!(!git_ref.matches(&v100));
    assert!(!git_ref.matches(&v100_pre));
}

#[test]
fn test_git_ref_allows_prereleases() {
    let git_ref = VersionConstraint::GitRef("latest".to_string());

    // Git refs allow prereleases (they reference commits)
    assert!(git_ref.allows_prerelease());

    let main_ref = VersionConstraint::GitRef("main".to_string());
    assert!(main_ref.allows_prerelease());
}

#[test]
fn test_requirement_constraint_allows_prerelease() {
    let req = VersionConstraint::parse("^1.0.0").unwrap();
    assert!(!req.allows_prerelease());

    let exact = VersionConstraint::Exact {
        prefix: None,
        version: Version::parse("1.0.0").unwrap(),
    };
    assert!(!exact.allows_prerelease());
}

#[test]
fn test_parse_with_whitespace() {
    let constraint = VersionConstraint::parse("  1.0.0  ").unwrap();
    assert!(matches!(constraint, VersionConstraint::Exact { .. }));

    let constraint = VersionConstraint::parse("  latest  ").unwrap();
    assert!(matches!(constraint, VersionConstraint::GitRef(_))); // Just a git ref

    let constraint = VersionConstraint::parse("  ^1.0.0  ").unwrap();
    assert!(matches!(constraint, VersionConstraint::Requirement { .. }));
}

#[test]
fn test_git_ref_to_version_req() {
    let git_ref = VersionConstraint::GitRef("latest".to_string());
    // Git refs cannot be converted to version requirements
    assert!(git_ref.to_version_req().is_none());

    let main_ref = VersionConstraint::GitRef("main".to_string());
    assert!(main_ref.to_version_req().is_none());
}

// ========== Prefix Support Tests ==========

#[test]
fn test_prefixed_constraint_parsing() {
    // Prefixed exact version
    let constraint = VersionConstraint::parse("agents-v1.0.0").unwrap();
    match constraint {
        VersionConstraint::Exact {
            prefix,
            version,
        } => {
            assert_eq!(prefix, Some("agents".to_string()));
            assert_eq!(version, Version::parse("1.0.0").unwrap());
        }
        _ => panic!("Expected Exact constraint"),
    }

    // Prefixed requirement
    let constraint = VersionConstraint::parse("agents-^v1.0.0").unwrap();
    match constraint {
        VersionConstraint::Requirement {
            prefix,
            req,
        } => {
            assert_eq!(prefix, Some("agents".to_string()));
            assert!(req.matches(&Version::parse("1.5.0").unwrap()));
            assert!(!req.matches(&Version::parse("2.0.0").unwrap()));
        }
        _ => panic!("Expected Requirement constraint"),
    }

    // Unprefixed constraint (backward compatible)
    let constraint = VersionConstraint::parse("^1.0.0").unwrap();
    match constraint {
        VersionConstraint::Requirement {
            prefix,
            ..
        } => {
            assert_eq!(prefix, None);
        }
        _ => panic!("Expected Requirement constraint"),
    }
}

#[test]
fn test_prefixed_constraint_display() {
    let prefixed_exact = VersionConstraint::Exact {
        prefix: Some("agents".to_string()),
        version: Version::parse("1.0.0").unwrap(),
    };
    assert_eq!(prefixed_exact.to_string(), "agents-1.0.0");

    let unprefixed_exact = VersionConstraint::Exact {
        prefix: None,
        version: Version::parse("1.0.0").unwrap(),
    };
    assert_eq!(unprefixed_exact.to_string(), "1.0.0");

    let prefixed_req = VersionConstraint::parse("snippets-^v2.0.0").unwrap();
    let display = prefixed_req.to_string();
    assert!(display.starts_with("snippets-"));
}

#[test]
fn test_matches_version_info() {
    use crate::version::VersionInfo;

    // Prefixed constraint matching prefixed version
    let constraint = VersionConstraint::parse("agents-^v1.0.0").unwrap();
    let version_info = VersionInfo {
        prefix: Some("agents".to_string()),
        version: Version::parse("1.2.0").unwrap(),
        tag: "agents-v1.2.0".to_string(),
        prerelease: false,
    };
    assert!(constraint.matches_version_info(&version_info));

    // Prefixed constraint NOT matching different prefix
    let wrong_prefix = VersionInfo {
        prefix: Some("snippets".to_string()),
        version: Version::parse("1.2.0").unwrap(),
        tag: "snippets-v1.2.0".to_string(),
        prerelease: false,
    };
    assert!(!constraint.matches_version_info(&wrong_prefix));

    // Unprefixed constraint matching unprefixed version
    let unprefixed_constraint = VersionConstraint::parse("^1.0.0").unwrap();
    let unprefixed_version = VersionInfo {
        prefix: None,
        version: Version::parse("1.5.0").unwrap(),
        tag: "v1.5.0".to_string(),
        prerelease: false,
    };
    assert!(unprefixed_constraint.matches_version_info(&unprefixed_version));

    // Unprefixed constraint NOT matching prefixed version
    assert!(!unprefixed_constraint.matches_version_info(&version_info));
}

#[test]
fn test_prefix_with_hyphens() {
    // Multiple hyphens in prefix
    let constraint = VersionConstraint::parse("my-cool-agent-v1.0.0").unwrap();
    match constraint {
        VersionConstraint::Exact {
            prefix,
            version,
        } => {
            assert_eq!(prefix, Some("my-cool-agent".to_string()));
            assert_eq!(version, Version::parse("1.0.0").unwrap());
        }
        _ => panic!("Expected Exact constraint"),
    }

    // Prefix ending with 'v'
    let constraint = VersionConstraint::parse("tool-v-v1.0.0").unwrap();
    match constraint {
        VersionConstraint::Exact {
            prefix,
            version,
        } => {
            assert_eq!(prefix, Some("tool-v".to_string()));
            assert_eq!(version, Version::parse("1.0.0").unwrap());
        }
        _ => panic!("Expected Exact constraint"),
    }
}
