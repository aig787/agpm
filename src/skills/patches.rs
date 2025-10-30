//! Skill-specific patch support for modifying SKILL.md frontmatter.
//!
//! This module provides functionality to apply patches to skill resources,
//! particularly the SKILL.md file's YAML frontmatter. It builds on the
//! general patch system but adds skill-specific semantics.
//!
//! # Examples
//!
//! ```toml
//! # In agpm.toml or agpm.private.toml
//! [patch.skills.my-skill]
//! model = "claude-3-haiku"
//! temperature = "0.7"
//! allowed-tools = ["claude-code", "opencode"]
//! ```

use crate::manifest::patches::{AppliedPatches, PatchData, apply_patches_to_content_with_origin};
use anyhow::{Context, Result};
use std::path::Path;

/// Apply patches to a skill's SKILL.md file.
///
/// This function applies patches to the SKILL.md file in a skill directory,
/// preserving the structure and applying patches only to the YAML frontmatter.
///
/// # Arguments
///
/// * `skill_dir` - Path to the skill directory
/// * `project_patches` - Patches from project-level configuration
/// * `private_patches` - Patches from private configuration
///
/// # Returns
///
/// A tuple of:
/// - Modified SKILL.md content
/// - `AppliedPatches` struct with separated project and private patches
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::skills::patches::apply_skill_patches;
/// use std::collections::BTreeMap;
/// use std::path::Path;
///
/// let skill_dir = Path::new(".claude/skills/my-skill");
/// let project_patches = BTreeMap::new();
/// let private_patches = BTreeMap::new();
///
/// let (new_content, applied) = apply_skill_patches(
///     skill_dir,
///     &project_patches,
///     &private_patches
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn apply_skill_patches(
    skill_dir: &Path,
    project_patches: &PatchData,
    private_patches: &PatchData,
) -> Result<(String, AppliedPatches)> {
    let skill_md_path = skill_dir.join("SKILL.md");

    if !skill_md_path.exists() {
        return Err(anyhow::anyhow!(
            "SKILL.md not found in skill directory: {}",
            skill_dir.display()
        ));
    }

    // Read the current content
    let content = std::fs::read_to_string(&skill_md_path)
        .with_context(|| format!("Failed to read SKILL.md: {}", skill_md_path.display()))?;

    // Apply patches to the content
    let (new_content, applied_patches) = apply_patches_to_content_with_origin(
        &content,
        "SKILL.md",
        project_patches,
        private_patches,
    )?;

    // Write the patched content back to the file
    std::fs::write(&skill_md_path, &new_content).with_context(|| {
        format!("Failed to write patched SKILL.md: {}", skill_md_path.display())
    })?;

    tracing::info!(
        "Applied {} patches to SKILL.md (project: {}, private: {})",
        applied_patches.total_count(),
        applied_patches.project.len(),
        applied_patches.private.len()
    );

    Ok((new_content, applied_patches))
}

/// Apply patches to a skill's SKILL.md file without writing to disk.
///
/// This function is useful for testing or when you want to preview the
/// changes before applying them.
///
/// # Arguments
///
/// * `skill_content` - The current SKILL.md content
/// * `project_patches` - Patches from project-level configuration
/// * `private_patches` - Patches from private configuration
///
/// # Returns
///
/// A tuple of:
/// - Modified SKILL.md content
/// - `AppliedPatches` struct with separated project and private patches
pub fn apply_skill_patches_preview(
    skill_content: &str,
    project_patches: &PatchData,
    private_patches: &PatchData,
) -> Result<(String, AppliedPatches)> {
    apply_patches_to_content_with_origin(
        skill_content,
        "SKILL.md",
        project_patches,
        private_patches,
    )
}

/// Validate that patch fields are appropriate for skills.
///
/// This function checks that the patch fields make sense for skill resources.
/// It validates field names and values to ensure they can be properly applied
/// to SKILL.md frontmatter.
///
/// # Arguments
///
/// * `patches` - The patch data to validate
///
/// # Returns
///
/// Ok(()) if validation passes, or an error with details about invalid fields
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::skills::patches::validate_skill_patches;
/// use std::collections::HashMap;
/// use toml;
///
/// let mut patches = std::collections::BTreeMap::new();
/// patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
/// patches.insert("allowed-tools".to_string(), toml::Value::Array(vec![
///     toml::Value::String("claude-code".to_string()),
///     toml::Value::String("opencode".to_string()),
/// ]));
///
/// validate_skill_patches(&patches)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn validate_skill_patches(patches: &PatchData) -> Result<()> {
    // Known frontmatter fields for skills
    const VALID_FIELDS: &[&str] = &[
        "name",
        "description",
        "version",
        "allowed-tools",
        "dependencies",
        // Allow extra fields for flexibility
    ];

    for (field_name, value) in patches {
        // Check if the field name looks valid
        if !VALID_FIELDS.contains(&field_name.as_str()) {
            tracing::warn!(
                "Applying patch to potentially invalid field '{}' in SKILL.md. \
                Valid fields are: {}",
                field_name,
                VALID_FIELDS.join(", ")
            );
        }

        // Validate specific field types
        match field_name.as_str() {
            "allowed-tools" => {
                if !value.is_array() {
                    return Err(anyhow::anyhow!(
                        "allowed-tools must be an array of strings, got: {:?}",
                        value
                    ));
                }

                if let Some(array) = value.as_array() {
                    for item in array {
                        if !item.is_str() {
                            return Err(anyhow::anyhow!(
                                "allowed-tools array must contain only strings, got: {:?}",
                                item
                            ));
                        }
                    }
                }
            }
            "dependencies" => {
                // Dependencies should be a valid YAML structure
                // We'll accept any value here since it will be validated when parsed
            }
            _ => {
                // Most fields should be simple values (string, number, boolean)
                // We're permissive here to allow flexibility
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use toml;

    #[test]
    fn test_apply_skill_patches_simple() {
        let content = r#"---
name: Test Skill
description: A test skill
version: "1.0.0"
---
# Test Skill

This is a test skill.
"#;

        let mut patches = std::collections::BTreeMap::new();
        patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));

        let (new_content, applied) =
            apply_skill_patches_preview(content, &patches, &std::collections::BTreeMap::new())
                .unwrap();

        assert_eq!(applied.project.len(), 1);
        assert_eq!(applied.private.len(), 0);
        assert!(new_content.contains("model: claude-3-haiku"));
        assert!(new_content.contains("# Test Skill"));
    }

    #[test]
    fn test_apply_skill_patches_with_private() {
        let content = r#"---
name: Test Skill
description: A test skill
---
# Test Skill
"#;

        let project_patches = std::collections::BTreeMap::from([(
            "model".to_string(),
            toml::Value::String("claude-3-opus".to_string()),
        )]);
        let private_patches = std::collections::BTreeMap::from([(
            "temperature".to_string(),
            toml::Value::String("0.7".to_string()),
        )]);

        let (new_content, applied) =
            apply_skill_patches_preview(content, &project_patches, &private_patches).unwrap();

        assert_eq!(applied.project.len(), 1);
        assert_eq!(applied.private.len(), 1);
        assert!(new_content.contains("model: claude-3-opus"));
        assert!(new_content.contains("temperature:"));
        assert!(new_content.contains("0.7"));
    }

    #[test]
    fn test_apply_skill_patches_private_overrides_project() {
        let content = r#"---
name: Test Skill
model: claude-3-opus
---
# Test Skill
"#;

        let project_patches = std::collections::BTreeMap::from([(
            "model".to_string(),
            toml::Value::String("claude-3-sonnet".to_string()),
        )]);
        let private_patches = std::collections::BTreeMap::from([(
            "model".to_string(),
            toml::Value::String("claude-3-haiku".to_string()),
        )]);

        let (new_content, applied) =
            apply_skill_patches_preview(content, &project_patches, &private_patches).unwrap();

        // Both patches are tracked
        assert_eq!(applied.project.len(), 1);
        assert_eq!(applied.private.len(), 1);

        // Private wins in the content
        assert!(new_content.contains("model: claude-3-haiku"));
        assert!(!new_content.contains("model: claude-3-sonnet"));
    }

    #[test]
    fn test_apply_skill_patches_no_frontmatter() {
        let content = "# Test Skill\n\nThis skill has no frontmatter.";

        let mut patches = std::collections::BTreeMap::new();
        patches.insert("name".to_string(), toml::Value::String("My Skill".to_string()));

        let (new_content, applied) =
            apply_skill_patches_preview(content, &patches, &std::collections::BTreeMap::new())
                .unwrap();

        assert_eq!(applied.project.len(), 1);
        assert!(new_content.starts_with("---\n"));
        assert!(new_content.contains("name: My Skill"));
        assert!(new_content.contains("# Test Skill"));
    }

    #[test]
    fn test_validate_skill_patches_valid() {
        let mut patches = std::collections::BTreeMap::new();
        patches.insert("model".to_string(), toml::Value::String("claude-3-haiku".to_string()));
        patches.insert("temperature".to_string(), toml::Value::String("0.7".to_string()));

        let tools_array = vec![
            toml::Value::String("claude-code".to_string()),
            toml::Value::String("opencode".to_string()),
        ];
        patches.insert("allowed-tools".to_string(), toml::Value::Array(tools_array));

        assert!(validate_skill_patches(&patches).is_ok());
    }

    #[test]
    fn test_validate_skill_patches_invalid_allowed_tools() {
        let mut patches = std::collections::BTreeMap::new();
        patches
            .insert("allowed-tools".to_string(), toml::Value::String("not-an-array".to_string()));

        let result = validate_skill_patches(&patches);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("allowed-tools must be an array"));
    }

    #[test]
    fn test_validate_skill_patches_invalid_allowed_tools_item() {
        let mut patches = std::collections::BTreeMap::new();
        let tools_array = vec![
            toml::Value::String("claude-code".to_string()),
            toml::Value::Integer(123), // Invalid: not a string
        ];
        patches.insert("allowed-tools".to_string(), toml::Value::Array(tools_array));

        let result = validate_skill_patches(&patches);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must contain only strings"));
    }

    #[test]
    fn test_validate_skill_patches_empty() {
        let patches = BTreeMap::new();
        assert!(validate_skill_patches(&patches).is_ok());
    }

    #[test]
    fn test_apply_skill_patches_complex() {
        let content = r#"---
name: Test Skill
description: A test skill
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
---
# Test Skill

This skill has dependencies.
"#;

        let mut patches = std::collections::BTreeMap::new();

        // Update a simple field
        patches.insert(
            "description".to_string(),
            toml::Value::String("Updated description".to_string()),
        );

        // Update dependencies
        let deps_toml = r#"
[dependencies]
agents = [
    { path = "agents/helper.md", version = "v2.0.0" },
    { path = "agents/reviewer.md" }
]
snippets = [
    { path = "snippets/utils.md" }
]
"#;
        let deps_value: toml::Value = toml::from_str(deps_toml).unwrap();
        patches.insert("dependencies".to_string(), deps_value);

        let (new_content, applied) =
            apply_skill_patches_preview(content, &patches, &std::collections::BTreeMap::new())
                .unwrap();

        assert_eq!(applied.project.len(), 2);
        assert!(new_content.contains("description: Updated description"));
        assert!(new_content.contains("version: v2.0.0"));
        assert!(new_content.contains("reviewer.md"));
        assert!(new_content.contains("snippets:"));
    }
}
