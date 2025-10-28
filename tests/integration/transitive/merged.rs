//! Integration tests for merged dependencies functionality.
//!
//! Tests that `metadata.dependencies` and `metadata.agpm.dependencies` are properly
//! merged and available in template rendering.

use std::collections::BTreeMap;

use agpm_cli::manifest::DependencySpec;
use agpm_cli::manifest::dependency_spec::{AgpmMetadata, DependencyMetadata};
use agpm_cli::metadata::MetadataExtractor;
use std::path::Path;

#[test]
fn test_markdown_metadata_agpm_extraction() {
    // Test frontmatter with both root-level and nested dependencies
    let content = r#"---
title: "Test Resource"
dependencies:
  agents:
    - path: helper.md
      name: root_helper
agpm:
  templating: true
  dependencies:
    snippets:
      - path: utils.md
        name: nested_utils
    agents:
      - path: another-helper.md
        name: nested_helper
---

# Test Content

This is a test resource with dependencies in both locations.
"#;

    let doc = agpm_cli::markdown::MarkdownDocument::parse(content).unwrap();
    assert!(doc.metadata.is_some());

    let metadata = doc.metadata.unwrap();

    // Check root-level dependencies
    assert!(metadata.dependencies.is_some());
    let root_deps = metadata.dependencies.as_ref().unwrap();
    assert_eq!(root_deps.len(), 1);
    assert!(root_deps.contains_key("agents"));
    assert_eq!(root_deps["agents"].len(), 1);
    assert_eq!(root_deps["agents"][0].path, "helper.md");
    assert_eq!(root_deps["agents"][0].name, Some("root_helper".to_string()));

    // Check nested agpm dependencies
    let agpm_metadata = metadata.get_agpm_metadata();
    assert!(agpm_metadata.is_some());
    let agpm = agpm_metadata.unwrap();
    assert!(agpm.templating.unwrap());
    assert!(agpm.dependencies.is_some());

    let nested_deps = agpm.dependencies.as_ref().unwrap();
    assert_eq!(nested_deps.len(), 2);
    assert!(nested_deps.contains_key("snippets"));
    assert!(nested_deps.contains_key("agents"));

    assert_eq!(nested_deps["snippets"].len(), 1);
    assert_eq!(nested_deps["snippets"][0].path, "utils.md");
    assert_eq!(nested_deps["snippets"][0].name, Some("nested_utils".to_string()));

    assert_eq!(nested_deps["agents"].len(), 1);
    assert_eq!(nested_deps["agents"][0].path, "another-helper.md");
    assert_eq!(nested_deps["agents"][0].name, Some("nested_helper".to_string()));
}

#[test]
fn test_dependency_metadata_merged_view() {
    // Create DependencyMetadata with both root and nested dependencies
    let mut root_deps = BTreeMap::new();
    root_deps.insert(
        "agents".to_string(),
        vec![DependencySpec {
            path: "root-agent.md".to_string(),
            name: Some("root_agent".to_string()),
            version: Some("v1.0.0".to_string()),
            tool: None,
            flatten: None,
            install: None,
        }],
    );

    let mut nested_deps = BTreeMap::new();
    nested_deps.insert(
        "snippets".to_string(),
        vec![DependencySpec {
            path: "nested-snippet.md".to_string(),
            name: Some("nested_snippet".to_string()),
            version: Some("v2.0.0".to_string()),
            tool: None,
            flatten: None,
            install: None,
        }],
    );

    let agpm = AgpmMetadata {
        templating: Some(true),
        dependencies: Some(nested_deps),
    };

    let metadata = DependencyMetadata::new(Some(root_deps), Some(agpm));

    // Test the merged view
    let merged = metadata.get_dependencies().unwrap();
    assert_eq!(merged.len(), 2); // Both resource types
    assert_eq!(merged["agents"].len(), 1);
    assert_eq!(merged["snippets"].len(), 1);

    // Check that root dependencies are included
    assert_eq!(merged["agents"][0].path, "root-agent.md");
    assert_eq!(merged["agents"][0].name, Some("root_agent".to_string()));

    // Check that nested dependencies are included
    assert_eq!(merged["snippets"][0].path, "nested-snippet.md");
    assert_eq!(merged["snippets"][0].name, Some("nested_snippet".to_string()));

    // Check helper methods
    assert!(metadata.has_dependencies());
    assert_eq!(metadata.dependency_count(), 2);
}

#[test]
fn test_dependency_metadata_duplicate_paths() {
    // Test that duplicate paths are filtered out
    let mut root_deps = BTreeMap::new();
    root_deps.insert(
        "agents".to_string(),
        vec![DependencySpec {
            path: "shared.md".to_string(),
            name: Some("root_name".to_string()),
            version: Some("v1.0.0".to_string()),
            tool: None,
            flatten: None,
            install: None,
        }],
    );

    let mut nested_deps = BTreeMap::new();
    nested_deps.insert(
        "agents".to_string(),
        vec![DependencySpec {
            path: "shared.md".to_string(), // Same path, different name
            name: Some("nested_name".to_string()),
            version: Some("v2.0.0".to_string()),
            tool: None,
            flatten: None,
            install: None,
        }],
    );

    let agpm = AgpmMetadata {
        templating: None,
        dependencies: Some(nested_deps),
    };

    let metadata = DependencyMetadata::new(Some(root_deps), Some(agpm));

    // Only the first occurrence should be kept
    let merged = metadata.get_dependencies().unwrap();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged["agents"].len(), 1);
    assert_eq!(merged["agents"][0].path, "shared.md");
    assert_eq!(merged["agents"][0].name, Some("root_name".to_string())); // Root takes precedence
    assert_eq!(metadata.dependency_count(), 1);
}

#[test]
fn test_end_to_end_merged_dependencies_workflow() {
    // Test the complete workflow with real frontmatter content
    let content = r#"---
title: "Test Merged Dependencies"
description: "A test resource to demonstrate merged dependencies functionality"
dependencies:
  agents:
    - path: root-helper.md
      name: root_helper
      version: v1.0.0
agpm:
  templating: true
  dependencies:
    snippets:
      - path: utils.md
        name: utils_helper
        version: v2.0.0
    agents:
      - path: another-helper.md
        name: another_helper
        version: v2.0.0
---

# Test Merged Dependencies

This file demonstrates how dependencies from both `dependencies` and `agpm.dependencies`
are merged into a single unified view.
"#;

    // Extract metadata from the markdown content
    let metadata = MetadataExtractor::extract(
        Path::new("test.md"),
        content,
        None, // No project config
        None, // No operation context
    )
    .expect("Failed to extract metadata");

    // Verify the merged view contains all dependencies
    let merged = metadata.get_dependencies().expect("Should have merged dependencies");

    // Should have both resource types
    assert_eq!(merged.len(), 2, "Should have both agents and snippets");
    assert!(merged.contains_key("agents"));
    assert!(merged.contains_key("snippets"));

    // Should have 2 agents (1 root + 1 nested)
    assert_eq!(merged["agents"].len(), 2, "Should have 2 agent dependencies");

    // Should have 1 snippet (from nested)
    assert_eq!(merged["snippets"].len(), 1, "Should have 1 snippet dependency");

    // Check specific dependencies
    let agents = &merged["agents"];
    let agent_paths: Vec<&str> = agents.iter().map(|dep| dep.path.as_str()).collect();
    assert!(agent_paths.contains(&"root-helper.md"), "Should contain root-helper.md");
    assert!(agent_paths.contains(&"another-helper.md"), "Should contain another-helper.md");

    let snippets = &merged["snippets"];
    assert_eq!(snippets[0].path, "utils.md", "Should contain utils.md");
    assert_eq!(snippets[0].name, Some("utils_helper".to_string()));

    // Test helper methods
    assert!(metadata.has_dependencies(), "Should have dependencies");
    assert_eq!(metadata.dependency_count(), 3, "Should have 3 total dependencies");

    // Test nested agpm metadata access
    let agpm = metadata.agpm.as_ref().expect("Should have agpm metadata");
    assert!(agpm.templating.unwrap(), "Should have templating enabled");
    assert!(agpm.dependencies.is_some(), "Should have nested dependencies");

    let nested = agpm.dependencies.as_ref().unwrap();
    assert_eq!(nested.len(), 2, "Should have 2 nested resource types");
}
