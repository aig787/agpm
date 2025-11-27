//! Tests for manifest validation logic.
//!
//! These tests verify that manifest validation correctly handles:
//! - Patch validation (referencing valid/invalid aliases)
//! - Source validation
//! - Version constraint validation

use crate::manifest::{DetailedDependency, Manifest, ResourceDependency};
use anyhow::Result;
use tempfile::tempdir;

#[test]
fn test_validate_patches_success() -> Result<()> {
    let temp = tempdir()?;
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with valid patches
    let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[agents]
test-agent = { source = "community", path = "agents/test.md", version = "v1.0.0" }

[patch.agents.test-agent]
model = "claude-3-haiku"
temperature = "0.8"
"#;
    std::fs::write(&manifest_path, toml_content)?;

    let manifest = Manifest::load(&manifest_path)?;
    manifest.validate()?;
    Ok(())
}

#[test]
fn test_validate_patches_unknown_dependency() -> Result<()> {
    let temp = tempdir()?;
    let manifest_path = temp.path().join("agpm.toml");

    // Create manifest with patch for non-existent dependency
    let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[agents]
test-agent = { source = "community", path = "agents/test.md", version = "v1.0.0" }

[patch.agents.non-existent]
model = "claude-3-haiku"
"#;
    std::fs::write(&manifest_path, toml_content)?;

    // load() now calls validate() automatically, so it should fail
    let result = Manifest::load(&manifest_path);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Patch references unknown"));
    Ok(())
}

#[test]
fn test_validate_sources() -> Result<()> {
    let mut manifest = Manifest::new();

    // Add dependency without source
    manifest.add_dependency(
        "local".to_string(),
        ResourceDependency::Simple("../local/agent.md".to_string()),
        true,
    );
    manifest.validate()?;

    // Add dependency with undefined source
    manifest.add_dependency(
        "remote".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("undefined".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
        true,
    );
    assert!(manifest.validate().is_err());

    // Add the source
    manifest.add_source("undefined".to_string(), "https://github.com/test/repo.git".to_string());
    manifest.validate()?;
    Ok(())
}

#[test]
fn test_validate_version_constraints() -> Result<()> {
    let mut manifest = Manifest::new();
    manifest.add_source("test".to_string(), "https://github.com/test/repo.git".to_string());

    // Remote dependency without version is now OK (defaults to HEAD)
    manifest.add_dependency(
        "no-version".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
        true,
    );
    manifest.validate()?; // Git deps default to HEAD now

    // Adding version should fix it
    manifest.agents.remove("no-version");
    manifest.add_dependency(
        "with-version".to_string(),
        ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test".to_string()),
            path: "agents/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: Some("claude-code".to_string()),
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        })),
        true,
    );
    manifest.validate()?;
    Ok(())
}
