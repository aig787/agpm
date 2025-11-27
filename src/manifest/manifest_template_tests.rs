//! Tests for template variable functionality in manifests.
//!
//! These tests verify that template variables can be:
//! - Declared in dependencies
//! - Serialized/deserialized correctly from TOML
//! - Used in various formats (inline tables, nested objects)

use crate::manifest::{DetailedDependency, Manifest, ResourceDependency};
use anyhow::Result;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_template_vars_in_dependency() {
    let vars = json!({
        "project": {
            "language": "python"
        }
    });

    let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some("official".to_string()),
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
        template_vars: Some(vars.clone()),
    }));

    assert_eq!(dep.get_template_vars(), Some(&vars));
}

#[test]
fn test_template_vars_serialization() -> Result<()> {
    let temp = tempdir()?;
    let manifest_path = temp.path().join("agpm.toml");

    let toml_content = r#"
[sources]
community = "https://github.com/example/agpm-community.git"

[agents.python-dev]
source = "community"
path = "agents/generic-dev.md"
version = "v1.0.0"
tool = "claude-code"

[agents.python-dev.template_vars]
project = { language = "python", framework = "fastapi" }
"#;
    std::fs::write(&manifest_path, toml_content)?;

    let manifest = Manifest::load(&manifest_path)?;
    let dep = manifest.agents.get("python-dev").unwrap();

    let vars = dep.get_template_vars().unwrap();
    assert_eq!(
        vars.get("project").and_then(|p| p.get("language")).and_then(|l| l.as_str()),
        Some("python")
    );
    assert_eq!(
        vars.get("project").and_then(|p| p.get("framework")).and_then(|f| f.as_str()),
        Some("fastapi")
    );
    Ok(())
}

#[test]
fn test_template_vars_empty() {
    let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some("official".to_string()),
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
        template_vars: None,
    }));

    assert_eq!(dep.get_template_vars(), None);
}

#[test]
fn test_template_vars_inline_table_with_multiple_keys() -> Result<()> {
    let temp = tempdir()?;
    let manifest_path = temp.path().join("agpm.toml");

    // Test inline table format with multiple top-level keys (project AND config)
    let toml_content = r#"
[sources]
test-repo = "https://example.com/repo.git"

[agents]
templated = { source = "test-repo", path = "agents/templated-agent.md", version = "v1.0.0", template_vars = { project = { name = "Production" }, config = { model = "claude-3-opus", temperature = 0.5 } } }
"#;
    std::fs::write(&manifest_path, toml_content)?;

    let manifest = Manifest::load(&manifest_path)?;
    let dep = manifest.agents.get("templated").unwrap();

    let vars = dep.get_template_vars().unwrap();

    // Debug: print the vars
    println!("Parsed template_vars: {}", serde_json::to_string_pretty(vars)?);

    // Verify project key
    assert!(vars.get("project").is_some(), "project should be present");
    assert_eq!(
        vars.get("project").and_then(|p| p.get("name")).and_then(|n| n.as_str()),
        Some("Production")
    );

    // Verify config key
    assert!(vars.get("config").is_some(), "config should be present in template_vars");
    assert_eq!(
        vars.get("config").and_then(|c| c.get("model")).and_then(|m| m.as_str()),
        Some("claude-3-opus")
    );
    assert_eq!(
        vars.get("config").and_then(|c| c.get("temperature")).and_then(|t| t.as_f64()),
        Some(0.5)
    );
    Ok(())
}
