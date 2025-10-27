//! Helper functions for displaying patch information with original and overridden values.
//!
//! This module provides utilities to extract original field values from resource files
//! in source repositories and format them alongside patched values for display in
//! commands like `agpm list --detailed` and `agpm tree --detailed`.

use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashMap;

use crate::cache::Cache;
use crate::lockfile::LockedResource;
use crate::markdown::MarkdownDocument;

/// Represents a patch with both original and overridden values for display.
#[derive(Debug, Clone)]
pub struct PatchDisplay {
    /// Field name being patched.
    pub field_name: String,
    /// Original value from the source file (if available).
    pub original_value: Option<toml::Value>,
    /// Overridden value from the patch.
    pub overridden_value: toml::Value,
}

impl PatchDisplay {
    /// Format the patch for display as a diff.
    ///
    /// Always uses multi-line diff format with color coding:
    /// - Red `-` line for original value (omitted if no original)
    /// - Green `+` line for overridden value
    ///
    /// ```text
    /// field:
    ///   - "original value"  (red)
    ///   + "overridden value"  (green)
    /// ```
    ///
    /// If there's no original value, only the green `+` line is shown:
    /// ```text
    /// field:
    ///   + "new value"  (green)
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// # use agpm_cli::lockfile::patch_display::PatchDisplay;
    /// let display = PatchDisplay {
    ///     field_name: "model".to_string(),
    ///     original_value: Some(toml::Value::String("opus".to_string())),
    ///     overridden_value: toml::Value::String("haiku".to_string()),
    /// };
    /// let formatted = display.format();
    /// assert!(formatted.contains("model:"));
    /// ```
    pub fn format(&self) -> String {
        let overridden_str = format_toml_value(&self.overridden_value);
        let add_line = format!("  + {}", overridden_str).green().to_string();
        let field_name_colored = self.field_name.blue().to_string();

        if let Some(ref original) = self.original_value {
            let original_str = format_toml_value(original);
            let remove_line = format!("  - {}", original_str).red().to_string();
            format!("{}:\n{}\n{}", field_name_colored, remove_line, add_line)
        } else {
            // No original value - only show the addition (green)
            format!("{}:\n{}", field_name_colored, add_line)
        }
    }
}

/// Extract patch display information for a locked resource.
///
/// This function:
/// 1. Reads the original file from the source worktree
/// 2. Parses it to extract original field values
/// 3. Combines with the overridden values from the applied patches
/// 4. Returns formatted patch display information
///
/// # Arguments
///
/// * `resource` - The locked resource with patch information
/// * `cache` - Repository cache to access source worktrees
///
/// # Returns
///
/// A vector of `PatchDisplay` entries with original and overridden values.
/// If the source file cannot be read or parsed, displays show only overridden values
/// with "(none)" for the original value.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::lockfile::patch_display::extract_patch_displays;
/// use agpm_cli::lockfile::LockedResource;
/// use agpm_cli::cache::Cache;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache = Cache::new()?;
/// let resource = LockedResource::new(
///     // ... resource fields
/// #   "test".to_string(),
/// #   Some("community".to_string()),
/// #   Some("https://example.com/repo.git".to_string()),
/// #   "agents/test.md".to_string(),
/// #   Some("v1.0.0".to_string()),
/// #   Some("abc123".to_string()),
/// #   "sha256:def456".to_string(),
/// #   "agents/test.md".to_string(),
/// #   vec![],
/// #   agpm_cli::core::ResourceType::Agent,
/// #   Some("claude-code".to_string()),
/// #   None,
/// #   std::collections::HashMap::new(),
/// #   None,
/// #   serde_json::Value::Object(serde_json::Map::new()),
/// );
///
/// let displays = extract_patch_displays(&resource, &cache).await;
/// for display in displays {
///     println!("{}", display.format());
/// }
/// # Ok(())
/// # }
/// ```
pub async fn extract_patch_displays(resource: &LockedResource, cache: &Cache) -> Vec<PatchDisplay> {
    // If no patches were applied, return empty
    if resource.applied_patches.is_empty() {
        return Vec::new();
    }

    // Try to extract original values
    let original_values = match extract_original_values(resource, cache).await {
        Ok(values) => {
            tracing::debug!(
                "Successfully extracted {} original values for {}",
                values.len(),
                resource.name
            );
            values
        }
        Err(e) => {
            tracing::warn!(
                "Failed to extract original values for {}: {}. Showing patches without original values.",
                resource.name,
                e
            );
            HashMap::new()
        }
    };

    // Build display entries
    let mut displays = Vec::new();
    for (field_name, overridden_value) in &resource.applied_patches {
        let original_value = original_values.get(field_name).cloned();

        displays.push(PatchDisplay {
            field_name: field_name.clone(),
            original_value,
            overridden_value: overridden_value.clone(),
        });
    }

    // Sort by field name for consistent display
    displays.sort_by(|a, b| a.field_name.cmp(&b.field_name));

    displays
}

/// Extract original field values from the source file.
///
/// Reads the file from the source worktree and extracts values for the fields
/// that have patches applied.
async fn extract_original_values(
    resource: &LockedResource,
    cache: &Cache,
) -> Result<HashMap<String, toml::Value>> {
    use std::path::Path;

    // Get source and commit information
    let source = resource.source.as_ref().context("Resource has no source")?;
    let commit = resource.resolved_commit.as_ref().context("Resource has no resolved commit")?;
    let url = resource.url.as_ref().context("Resource has no URL")?;

    tracing::debug!("Attempting to extract original values for resource: {}", resource.name);
    tracing::debug!("Source: {:?}, Commit: {:?}", source, commit);

    // Get worktree path for this SHA
    let worktree_path = cache
        .get_or_create_worktree_for_sha(source, url, commit, Some("patch-display"))
        .await
        .with_context(|| format!("Failed to get worktree for {source}"))?;

    tracing::debug!("Got worktree at: {}", worktree_path.display());

    // Read the file from the worktree
    let file_path = worktree_path.join(&resource.path);
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    tracing::debug!("Read {} bytes from {}", content.len(), file_path.display());

    // Extract values based on file type
    let extension = Path::new(&resource.path).extension().and_then(|s| s.to_str()).unwrap_or("");

    let values = match extension {
        "md" => extract_from_markdown(&content)?,
        "json" => extract_from_json(&content)?,
        _ => HashMap::new(),
    };

    tracing::debug!("Extracted {} original values", values.len());

    Ok(values)
}

/// Extract field values from Markdown file with YAML frontmatter.
fn extract_from_markdown(content: &str) -> Result<HashMap<String, toml::Value>> {
    let doc = MarkdownDocument::parse(content)?;

    let mut values = HashMap::new();

    if let Some(metadata) = &doc.metadata {
        // Extract standard fields
        if let Some(ref title) = metadata.title {
            values.insert("title".to_string(), toml::Value::String(title.clone()));
        }
        if let Some(ref description) = metadata.description {
            values.insert("description".to_string(), toml::Value::String(description.clone()));
        }
        if let Some(ref version) = metadata.version {
            values.insert("version".to_string(), toml::Value::String(version.clone()));
        }
        if let Some(ref author) = metadata.author {
            values.insert("author".to_string(), toml::Value::String(author.clone()));
        }
        if let Some(ref resource_type) = metadata.resource_type {
            values.insert("type".to_string(), toml::Value::String(resource_type.clone()));
        }
        if !metadata.tags.is_empty() {
            let tags: Vec<toml::Value> =
                metadata.tags.iter().map(|s| toml::Value::String(s.clone())).collect();
            values.insert("tags".to_string(), toml::Value::Array(tags));
        }

        // Convert extra fields from JSON value to TOML value
        for (key, json_value) in &metadata.extra {
            if let Ok(toml_value) = json_to_toml_value(json_value) {
                values.insert(key.clone(), toml_value);
            }
        }
    }

    Ok(values)
}

/// Extract field values from JSON file.
fn extract_from_json(content: &str) -> Result<HashMap<String, toml::Value>> {
    let json_value: serde_json::Value = serde_json::from_str(content)?;

    let mut values = HashMap::new();

    if let serde_json::Value::Object(map) = json_value {
        // Skip "dependencies" field as it's not patchable
        for (key, json_val) in map {
            if key == "dependencies" {
                continue;
            }

            if let Ok(toml_value) = json_to_toml_value(&json_val) {
                values.insert(key, toml_value);
            }
        }
    }

    Ok(values)
}

/// Convert serde_json::Value to toml::Value.
pub(crate) fn json_to_toml_value(json: &serde_json::Value) -> Result<toml::Value> {
    match json {
        serde_json::Value::String(s) => Ok(toml::Value::String(s.clone())),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                anyhow::bail!("Unsupported number type")
            }
        }
        serde_json::Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        serde_json::Value::Array(arr) => {
            let toml_arr: Result<Vec<_>> = arr.iter().map(json_to_toml_value).collect();
            Ok(toml::Value::Array(toml_arr?))
        }
        serde_json::Value::Object(map) => {
            let mut toml_map = toml::value::Table::new();
            for (k, v) in map {
                toml_map.insert(k.clone(), json_to_toml_value(v)?);
            }
            Ok(toml::Value::Table(toml_map))
        }
        serde_json::Value::Null => {
            // TOML doesn't have a null type, represent as empty string
            Ok(toml::Value::String(String::new()))
        }
    }
}

/// Format a toml::Value for display.
///
/// Produces clean, readable output:
/// - Strings: wrapped in quotes `"value"`
/// - Numbers/Booleans: plain text
/// - Arrays/Tables: formatted as TOML syntax
pub fn format_toml_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => format!("\"{}\"", s),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(format_toml_value).collect();
            format!("[{}]", elements.join(", "))
        }
        toml::Value::Table(_) | toml::Value::Datetime(_) => {
            // For complex types, use to_string() as fallback
            value.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_diff() {
        let display = PatchDisplay {
            field_name: "model".to_string(),
            original_value: Some(toml::Value::String("opus".to_string())),
            overridden_value: toml::Value::String("haiku".to_string()),
        };

        let formatted = display.format();
        // Check structure (contains field name and both lines with color codes)
        assert!(formatted.starts_with("model:"));
        assert!(formatted.contains("  - \"opus\""));
        assert!(formatted.contains("  + \"haiku\""));
    }

    #[test]
    fn test_format_long_values() {
        let long_text = "a".repeat(100);
        let display = PatchDisplay {
            field_name: "description".to_string(),
            original_value: Some(toml::Value::String(long_text.clone())),
            overridden_value: toml::Value::String(long_text.clone()),
        };

        let formatted = display.format();
        assert!(formatted.starts_with("description:"));
        assert!(formatted.contains("  -"));
        assert!(formatted.contains("  +"));
        // Verify the long text is preserved
        assert!(formatted.contains(&format!("\"{}\"", long_text)));
    }

    #[test]
    fn test_format_none_original() {
        let display = PatchDisplay {
            field_name: "new_field".to_string(),
            original_value: None,
            overridden_value: toml::Value::String("value".to_string()),
        };

        let formatted = display.format();
        // Should NOT have a - line when there's no original
        assert!(!formatted.contains("  -"));
        // Should have + line
        assert!(formatted.contains("  + \"value\""));
        assert!(formatted.starts_with("new_field:"));
    }

    #[test]
    fn test_format_toml_value_types() {
        // Test various TOML value types
        assert_eq!(format_toml_value(&toml::Value::String("test".into())), r#""test""#);
        assert_eq!(format_toml_value(&toml::Value::Integer(42)), "42");
        assert_eq!(format_toml_value(&toml::Value::Float(2.5)), "2.5");
        assert_eq!(format_toml_value(&toml::Value::Boolean(true)), "true");

        // Array
        let arr = toml::Value::Array(vec![
            toml::Value::String("a".into()),
            toml::Value::String("b".into()),
        ]);
        assert_eq!(format_toml_value(&arr), r#"["a", "b"]"#);
    }

    #[test]
    fn test_format_toml_value_string() {
        let value = toml::Value::String("claude-3-opus".to_string());
        assert_eq!(format_toml_value(&value), "\"claude-3-opus\"");
    }

    #[test]
    fn test_format_toml_value_integer() {
        let value = toml::Value::Integer(42);
        assert_eq!(format_toml_value(&value), "42");
    }

    #[test]
    fn test_format_toml_value_float() {
        let value = toml::Value::Float(0.75);
        assert_eq!(format_toml_value(&value), "0.75");
    }

    #[test]
    fn test_format_toml_value_boolean() {
        let value = toml::Value::Boolean(true);
        assert_eq!(format_toml_value(&value), "true");

        let value = toml::Value::Boolean(false);
        assert_eq!(format_toml_value(&value), "false");
    }

    #[test]
    fn test_format_toml_value_array() {
        let value =
            toml::Value::Array(vec![toml::Value::String("a".to_string()), toml::Value::Integer(1)]);
        assert_eq!(format_toml_value(&value), "[\"a\", 1]");
    }

    #[test]
    fn test_patch_display_format_with_original() {
        let display = PatchDisplay {
            field_name: "model".to_string(),
            original_value: Some(toml::Value::String("claude-3-opus".to_string())),
            overridden_value: toml::Value::String("claude-3-haiku".to_string()),
        };
        let formatted = display.format();
        assert!(formatted.starts_with("model:"));
        assert!(formatted.contains("  - \"claude-3-opus\""));
        assert!(formatted.contains("  + \"claude-3-haiku\""));
    }

    #[test]
    fn test_patch_display_format_without_original() {
        let display = PatchDisplay {
            field_name: "temperature".to_string(),
            original_value: None,
            overridden_value: toml::Value::String("0.8".to_string()),
        };
        let formatted = display.format();
        assert!(formatted.starts_with("temperature:"));
        // Should NOT have a - line
        assert!(!formatted.contains("  -"));
        assert!(formatted.contains("  + \"0.8\""));
    }

    #[test]
    fn test_extract_from_markdown_with_frontmatter() {
        let content = r#"---
model: claude-3-opus
temperature: "0.5"
max_tokens: 4096
custom_field: "custom_value"
---

# Test Agent

Content here."#;

        let values = extract_from_markdown(content).unwrap();

        // Check that we extracted all fields
        assert!(values.contains_key("model"));
        assert!(values.contains_key("temperature"));
        assert!(values.contains_key("max_tokens"));
        assert!(values.contains_key("custom_field"));

        // Check extracted values
        if let Some(toml::Value::String(model)) = values.get("model") {
            assert_eq!(model, "claude-3-opus");
        } else {
            panic!("Expected model to be a string");
        }

        if let Some(toml::Value::String(temp)) = values.get("temperature") {
            assert_eq!(temp, "0.5");
        } else {
            panic!("Expected temperature to be a string");
        }

        if let Some(toml::Value::Integer(tokens)) = values.get("max_tokens") {
            assert_eq!(*tokens, 4096);
        } else {
            panic!("Expected max_tokens to be an integer");
        }

        if let Some(toml::Value::String(custom)) = values.get("custom_field") {
            assert_eq!(custom, "custom_value");
        } else {
            panic!("Expected custom_field to be a string");
        }
    }

    #[test]
    fn test_extract_from_markdown_no_frontmatter() {
        let content = "# Test Agent\n\nNo frontmatter here.";
        let values = extract_from_markdown(content).unwrap();
        assert_eq!(values.len(), 0);
    }

    #[test]
    fn test_extract_from_json() {
        let content = r#"{
  "name": "test-server",
  "command": "npx",
  "timeout": 300,
  "enabled": true
}"#;

        let values = extract_from_json(content).unwrap();
        assert_eq!(values.len(), 4);

        // Check extracted values
        assert!(matches!(
            values.get("name"),
            Some(toml::Value::String(s)) if s == "test-server"
        ));
        assert!(matches!(
            values.get("command"),
            Some(toml::Value::String(s)) if s == "npx"
        ));
        assert!(matches!(values.get("timeout"), Some(toml::Value::Integer(300))));
        assert!(matches!(values.get("enabled"), Some(toml::Value::Boolean(true))));
    }

    #[test]
    fn test_json_to_toml_value_conversions() {
        // String
        let json = serde_json::Value::String("test".to_string());
        let toml = json_to_toml_value(&json).unwrap();
        assert!(matches!(toml, toml::Value::String(s) if s == "test"));

        // Integer
        let json = serde_json::json!(42);
        let toml = json_to_toml_value(&json).unwrap();
        assert!(matches!(toml, toml::Value::Integer(42)));

        // Float
        let json = serde_json::json!(2.5);
        let toml = json_to_toml_value(&json).unwrap();
        assert!(matches!(toml, toml::Value::Float(f) if (f - 2.5).abs() < 0.001));

        // Boolean
        let json = serde_json::Value::Bool(true);
        let toml = json_to_toml_value(&json).unwrap();
        assert!(matches!(toml, toml::Value::Boolean(true)));

        // Array
        let json = serde_json::json!(["a", "b"]);
        let toml = json_to_toml_value(&json).unwrap();
        assert!(matches!(toml, toml::Value::Array(_)));

        // Object
        let json = serde_json::json!({"key": "value"});
        let toml = json_to_toml_value(&json).unwrap();
        assert!(matches!(toml, toml::Value::Table(_)));
    }

    #[test]
    fn test_extract_from_markdown_standard_fields() {
        let content = r#"---
title: "Test Agent"
description: "A test agent for testing"
version: "1.0.0"
author: "Test Author"
type: "agent"
tags:
  - test
  - example
model: "claude-3-opus"
temperature: "0.7"
---

# Test Agent

Content here."#;

        let values = extract_from_markdown(content).unwrap();

        // Check standard metadata fields
        assert!(matches!(
            values.get("title"),
            Some(toml::Value::String(s)) if s == "Test Agent"
        ));
        assert!(matches!(
            values.get("description"),
            Some(toml::Value::String(s)) if s == "A test agent for testing"
        ));
        assert!(matches!(
            values.get("version"),
            Some(toml::Value::String(s)) if s == "1.0.0"
        ));
        assert!(matches!(
            values.get("author"),
            Some(toml::Value::String(s)) if s == "Test Author"
        ));
        assert!(matches!(
            values.get("type"),
            Some(toml::Value::String(s)) if s == "agent"
        ));

        // Check tags array
        if let Some(toml::Value::Array(tags)) = values.get("tags") {
            assert_eq!(tags.len(), 2);
        } else {
            panic!("Expected tags to be an array");
        }

        // Check custom fields
        assert!(matches!(
            values.get("model"),
            Some(toml::Value::String(s)) if s == "claude-3-opus"
        ));
        assert!(matches!(
            values.get("temperature"),
            Some(toml::Value::String(s)) if s == "0.7"
        ));
    }
}
