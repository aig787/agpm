//! Tests for mutable dependency detection in manifests.
//!
//! Mutable dependencies (local files, branches) can change between installs
//! without manifest changes. The fast path optimization cannot be used when
//! mutable dependencies exist.

use crate::manifest::{DetailedDependency, Manifest, ResourceDependency};

/// Helper to create a DetailedDependency with configurable mutability fields
fn detailed_dep_with_branch(
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

#[test]
fn test_empty_manifest_has_no_mutable_deps() {
    let manifest = Manifest::new();
    assert!(
        !manifest.has_mutable_dependencies(),
        "Empty manifest should have no mutable dependencies"
    );
}

#[test]
fn test_manifest_with_all_immutable_deps() {
    let mut manifest = Manifest::new();
    manifest.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());

    // Add semver-pinned dependency
    manifest.agents.insert(
        "agent1".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "agents/a.md",
            Some("v1.0.0"),
            None,
            None,
        ))),
    );

    // Add SHA-pinned dependency
    manifest.agents.insert(
        "agent2".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "agents/b.md",
            None,
            None,
            Some("abc123def456789012345678901234567890abcd"),
        ))),
    );

    assert!(
        !manifest.has_mutable_dependencies(),
        "Manifest with only semver and SHA-pinned deps should not be mutable"
    );
}

#[test]
fn test_manifest_with_local_dep_is_mutable() {
    let mut manifest = Manifest::new();

    // Local dependency (no source) is always mutable
    manifest.agents.insert(
        "local-agent".to_string(),
        ResourceDependency::Simple("../local/agent.md".to_string()),
    );

    assert!(
        manifest.has_mutable_dependencies(),
        "Manifest with local dependency should be mutable"
    );
}

#[test]
fn test_manifest_with_branch_dep_is_mutable() {
    let mut manifest = Manifest::new();
    manifest.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());

    // Branch reference is mutable
    manifest.agents.insert(
        "agent".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "agents/a.md",
            None,
            Some("main"),
            None,
        ))),
    );

    assert!(
        manifest.has_mutable_dependencies(),
        "Manifest with branch dependency should be mutable"
    );
}

#[test]
fn test_manifest_with_branch_like_version_is_mutable() {
    let mut manifest = Manifest::new();
    manifest.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());

    // Version that looks like a branch name
    manifest.agents.insert(
        "agent".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "agents/a.md",
            Some("develop"),
            None,
            None,
        ))),
    );

    assert!(
        manifest.has_mutable_dependencies(),
        "Manifest with branch-like version should be mutable"
    );
}

#[test]
fn test_manifest_with_mixed_deps_detects_mutable() {
    let mut manifest = Manifest::new();
    manifest.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());

    // Immutable: semver-pinned
    manifest.agents.insert(
        "immutable-agent".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "agents/a.md",
            Some("v1.0.0"),
            None,
            None,
        ))),
    );

    // Mutable: branch reference
    manifest.snippets.insert(
        "mutable-snippet".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "snippets/b.md",
            None,
            Some("main"),
            None,
        ))),
    );

    assert!(
        manifest.has_mutable_dependencies(),
        "Manifest with any mutable dependency should report as mutable"
    );
}

#[test]
fn test_manifest_with_no_version_is_mutable() {
    let mut manifest = Manifest::new();
    manifest.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());

    // No version, branch, or rev - defaults to mutable for safety
    manifest.agents.insert(
        "agent".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "agents/a.md",
            None,
            None,
            None,
        ))),
    );

    assert!(
        manifest.has_mutable_dependencies(),
        "Manifest with undefined version should be mutable (safe default)"
    );
}

#[test]
fn test_manifest_with_prefixed_semver_is_immutable() {
    let mut manifest = Manifest::new();
    manifest.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());

    // Prefixed semver is immutable
    manifest.agents.insert(
        "agent".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep_with_branch(
            Some("repo"),
            "agents/a.md",
            Some("agents-v1.0.0"),
            None,
            None,
        ))),
    );

    assert!(
        !manifest.has_mutable_dependencies(),
        "Manifest with prefixed semver should not be mutable"
    );
}

/// Test that manifest hash computation is deterministic.
///
/// This is critical for the fast path optimization. If hashes are not
/// deterministic, the fast path will never trigger because manifest hashes
/// won't match between runs.
#[test]
fn test_compute_dependency_hash_is_deterministic() {
    use std::collections::HashMap;

    // Create a manifest with multiple dependencies that have nested HashMaps
    let mut manifest = Manifest::new();
    manifest.sources.insert("repo_z".to_string(), "https://example.com/z.git".to_string());
    manifest.sources.insert("repo_a".to_string(), "https://example.com/a.git".to_string());
    manifest.sources.insert("repo_m".to_string(), "https://example.com/m.git".to_string());

    // Add dependencies with transitive dependencies (which contain HashMap)
    let mut transitive_deps = HashMap::new();
    transitive_deps.insert(
        "agents".to_string(),
        vec![
            crate::manifest::dependency_spec::DependencySpec {
                path: "agents/z_helper.md".to_string(),
                name: None,
                version: Some("v1.0.0".to_string()),
                tool: None,
                flatten: None,
                install: None,
            },
            crate::manifest::dependency_spec::DependencySpec {
                path: "agents/a_helper.md".to_string(),
                name: None,
                version: Some("v2.0.0".to_string()),
                tool: None,
                flatten: None,
                install: None,
            },
        ],
    );
    transitive_deps.insert(
        "snippets".to_string(),
        vec![crate::manifest::dependency_spec::DependencySpec {
            path: "snippets/util.md".to_string(),
            name: None,
            version: None,
            tool: None,
            flatten: None,
            install: None,
        }],
    );

    manifest.agents.insert(
        "agent_z".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("repo_a".to_string()),
            path: "agents/test_z.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: Some(transitive_deps.clone()),
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
    );

    manifest.agents.insert(
        "agent_a".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("repo_z".to_string()),
            path: "agents/test_a.md".to_string(),
            version: Some("v2.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: Some(transitive_deps),
            tool: Some("opencode".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
    );

    // Compute hash multiple times and verify they're identical
    let hash1 = manifest.compute_dependency_hash();
    let hash2 = manifest.compute_dependency_hash();
    let hash3 = manifest.compute_dependency_hash();

    assert_eq!(hash1, hash2, "Hash should be identical on consecutive calls (run 1 vs 2)");
    assert_eq!(hash2, hash3, "Hash should be identical on consecutive calls (run 2 vs 3)");

    // Run many more times to catch potential non-determinism
    for i in 0..100 {
        let hash_n = manifest.compute_dependency_hash();
        assert_eq!(hash1, hash_n, "Hash should be identical on iteration {i}");
    }
}
