//! Tests for the flatten field in resource dependencies.
//!
//! The flatten field controls whether pattern-matched resources are installed
//! into a flat directory or preserve their subdirectory structure.

use crate::manifest::{DetailedDependency, ResourceDependency};
use anyhow::Result;

#[test]
fn test_flatten_field_agents() -> Result<()> {
    let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some("official".to_string()),
        path: "agents/*.md".to_string(),
        version: Some("v1.0.0".to_string()),
        branch: None,
        rev: None,
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: Some("claude-code".to_string()),
        flatten: Some(false), // Override default
        install: None,
        template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    }));

    assert_eq!(dep.get_flatten(), Some(false));
    Ok(())
}

#[test]
fn test_flatten_field_snippets() -> Result<()> {
    let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
        source: Some("official".to_string()),
        path: "snippets/*.md".to_string(),
        version: Some("v1.0.0".to_string()),
        branch: None,
        rev: None,
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: Some("agpm".to_string()),
        flatten: Some(true), // Override default
        install: None,
        template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    }));

    assert_eq!(dep.get_flatten(), Some(true));
    Ok(())
}
