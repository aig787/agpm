//! Tests for manifest dependency hash computation.
//!
//! The dependency hash is used for fast path detection during installs.
//! If the hash matches the lockfile and there are no mutable dependencies,
//! resolution can be skipped entirely.

use crate::manifest::{DetailedDependency, Manifest, ResourceDependency};

/// Helper to create a DetailedDependency with minimal fields
fn detailed_dep(source: Option<&str>, path: &str, version: Option<&str>) -> DetailedDependency {
    DetailedDependency {
        source: source.map(String::from),
        path: path.to_string(),
        version: version.map(String::from),
        branch: None,
        rev: None,
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

fn create_manifest_with_agent(name: &str, source: &str, path: &str) -> Manifest {
    let mut manifest = Manifest::new();
    manifest.sources.insert(source.to_string(), "https://example.com/repo.git".to_string());
    manifest.agents.insert(
        name.to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep(Some(source), path, Some("v1.0.0")))),
    );
    manifest
}

#[test]
fn test_hash_deterministic() {
    let manifest = create_manifest_with_agent("test-agent", "official", "agents/test.md");
    let hash1 = manifest.compute_dependency_hash();
    let hash2 = manifest.compute_dependency_hash();
    assert_eq!(hash1, hash2, "Same manifest should produce same hash");
}

#[test]
fn test_hash_changes_on_dependency_add() {
    let mut manifest = create_manifest_with_agent("test-agent", "official", "agents/test.md");
    let hash1 = manifest.compute_dependency_hash();

    manifest.agents.insert(
        "new-agent".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("official"),
            "agents/new.md",
            Some("v1.0.0"),
        ))),
    );
    let hash2 = manifest.compute_dependency_hash();

    assert_ne!(hash1, hash2, "Hash should change when deps change");
}

#[test]
fn test_hash_independent_of_insertion_order() {
    let mut manifest1 = Manifest::new();
    manifest1.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());
    manifest1.agents.insert(
        "a".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep(Some("repo"), "agents/a.md", None))),
    );
    manifest1.agents.insert(
        "b".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep(Some("repo"), "agents/b.md", None))),
    );

    let mut manifest2 = Manifest::new();
    manifest2.sources.insert("repo".to_string(), "https://example.com/repo.git".to_string());
    manifest2.agents.insert(
        "b".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep(Some("repo"), "agents/b.md", None))),
    );
    manifest2.agents.insert(
        "a".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep(Some("repo"), "agents/a.md", None))),
    );

    assert_eq!(
        manifest1.compute_dependency_hash(),
        manifest2.compute_dependency_hash(),
        "Hash should be independent of insertion order"
    );
}

#[test]
fn test_hash_format() {
    let manifest = create_manifest_with_agent("test-agent", "official", "agents/test.md");
    let hash = manifest.compute_dependency_hash();
    assert!(hash.starts_with("sha256:"), "Hash should have sha256: prefix");
    assert_eq!(hash.len(), 7 + 64, "Hash should be sha256: + 64 hex chars");
}

#[test]
fn test_hash_changes_on_source_change() {
    let manifest1 = create_manifest_with_agent("test-agent", "official", "agents/test.md");
    let hash1 = manifest1.compute_dependency_hash();

    let mut manifest2 = Manifest::new();
    manifest2.sources.insert("official".to_string(), "https://different.com/repo.git".to_string());
    manifest2.agents.insert(
        "test-agent".to_string(),
        ResourceDependency::Detailed(Box::new(detailed_dep(
            Some("official"),
            "agents/test.md",
            Some("v1.0.0"),
        ))),
    );
    let hash2 = manifest2.compute_dependency_hash();

    assert_ne!(hash1, hash2, "Hash should change when source URL changes");
}

#[test]
fn test_empty_manifest_hash() {
    let manifest = Manifest::new();
    let hash = manifest.compute_dependency_hash();
    assert!(hash.starts_with("sha256:"), "Empty manifest should have valid hash");
}
