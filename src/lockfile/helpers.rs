//! Helper utilities for lockfile operations.
//!
//! This module provides small utility functions used throughout the lockfile module.

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item};

/// Check if tool is default ("claude-code") for conditional serialization.
///
/// # Arguments
///
/// * `tool` - Optional tool identifier
///
/// # Returns
///
/// `true` if the tool is "claude-code" (the default), `false` otherwise.
#[allow(dead_code)]
pub(crate) fn is_default_tool(tool: &Option<String>) -> bool {
    // Default tool is claude-code, so always skip serializing when it's Some("claude-code")
    matches!(tool, Some(t) if t == "claude-code")
}

/// Convert lockfile to TOML string with proper formatting for `applied_patches` and `template_vars`.
///
/// Uses `toml_edit` to ensure:
/// 1. `applied_patches` fields are always serialized as inline tables
/// 2. `template_vars` fields are always present as JSON strings (handled by custom serialization)
/// 3. Both fields are always present, even when empty
///
/// Example output:
/// ```toml
/// [[agents]]
/// name = "example"
/// applied_patches = { model = "haiku", temperature = "0.9" }
/// template_vars = "{}"
///
/// [[agents]]
/// name = "nested-example"
/// applied_patches = {}
/// template_vars = "{\"project\": {\"language\": \"rust\", \"framework\": \"axum\"}}"
/// ```
///
/// Note on implementation:
/// - `applied_patches` are always inline tables (simple key-value pairs)
/// - `template_vars` are serialized as JSON strings to allow nested content in inline tables
///   This approach bypasses TOML's limitation where inline tables cannot contain nested tables.
///
/// # Arguments
///
/// * `lockfile` - The lockfile structure to serialize
///
/// # Returns
///
/// * `Ok(String)` - Formatted TOML string
/// * `Err(anyhow::Error)` - Serialization or parsing error
///
/// # Errors
///
/// Returns an error if TOML serialization or document parsing fails.
pub(crate) fn serialize_lockfile_with_inline_patches<T: serde::Serialize>(
    lockfile: &T,
) -> Result<String> {
    // First serialize to a toml_edit document
    let toml_str = toml::to_string_pretty(lockfile).context("Failed to serialize to TOML")?;
    let mut doc: DocumentMut = toml_str.parse().context("Failed to parse TOML document")?;

    // Convert all `applied_patches` and `template_vars` tables to inline tables
    let resource_types = ["agents", "snippets", "commands", "scripts", "hooks", "mcp-servers"];

    for resource_type in &resource_types {
        if let Some(Item::ArrayOfTables(array)) = doc.get_mut(resource_type) {
            for table in array.iter_mut() {
                // Ensure applied_patches is always present as an inline table
                if let Some(Item::Table(patches_table)) = table.get_mut("applied_patches") {
                    // Convert existing table to inline table
                    let mut inline = toml_edit::InlineTable::new();
                    for (key, val) in patches_table.iter() {
                        if let Some(v) = val.as_value() {
                            inline.insert(key, v.clone());
                        }
                    }
                    table.insert("applied_patches", toml_edit::value(inline));
                } else {
                    // Add empty applied_patches if not present
                    let inline = toml_edit::InlineTable::new();
                    table.insert("applied_patches", toml_edit::value(inline));
                }

                // template_vars is now handled by custom serialization at the field level
                // No post-processing needed
            }
        }
    }

    Ok(doc.to_string())
}
