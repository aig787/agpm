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

use crate::core::file_error::{FileOperation, FileResultExt};
use crate::manifest::patches::{AppliedPatches, PatchData, apply_patches_to_content_with_origin};
use anyhow::Result;
use std::path::Path;
use tokio::fs as async_fs;

/// Maximum size for SKILL.md after patching (10 MB)
/// This prevents patches from inflating content beyond reasonable limits
const MAX_SKILL_MD_SIZE: usize = 10 * 1024 * 1024;

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
/// # async fn example() -> anyhow::Result<()> {
/// let skill_dir = Path::new(".claude/skills/my-skill");
/// let project_patches = BTreeMap::new();
/// let private_patches = BTreeMap::new();
///
/// let (new_content, applied) = apply_skill_patches(
///     skill_dir,
///     &project_patches,
///     &private_patches
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn apply_skill_patches(
    skill_dir: &Path,
    project_patches: &PatchData,
    private_patches: &PatchData,
) -> Result<(String, AppliedPatches)> {
    let skill_md_path = skill_dir.join("SKILL.md");

    // Check if SKILL.md exists using async I/O
    if async_fs::metadata(&skill_md_path).await.is_err() {
        return Err(anyhow::anyhow!(
            "SKILL.md not found in skill directory: {}",
            skill_dir.display()
        ));
    }

    // Read the current content using async I/O
    let content = async_fs::read_to_string(&skill_md_path).await.with_file_context(
        FileOperation::Read,
        &skill_md_path,
        "reading skill for patching",
        "apply_skill_patches",
    )?;

    // Apply patches to the content
    let (new_content, applied_patches) = apply_patches_to_content_with_origin(
        &content,
        "SKILL.md",
        project_patches,
        private_patches,
    )?;

    // Validate size after patching to prevent patches from inflating content beyond limits
    if new_content.len() > MAX_SKILL_MD_SIZE {
        return Err(anyhow::anyhow!(
            "Patched SKILL.md exceeds maximum size of {} MB. \
             Consider reducing patch complexity or splitting into multiple skills.",
            MAX_SKILL_MD_SIZE / (1024 * 1024)
        ));
    }

    // Write the patched content back to the file using async I/O
    async_fs::write(&skill_md_path, &new_content).await.with_file_context(
        FileOperation::Write,
        &skill_md_path,
        "writing patched skill content",
        "apply_skill_patches",
    )?;

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
    let (new_content, applied_patches) = apply_patches_to_content_with_origin(
        skill_content,
        "SKILL.md",
        project_patches,
        private_patches,
    )?;

    // Validate size after patching to prevent patches from inflating content beyond limits
    if new_content.len() > MAX_SKILL_MD_SIZE {
        return Err(anyhow::anyhow!(
            "Patched SKILL.md exceeds maximum size of {} MB. \
             Consider reducing patch complexity or splitting into multiple skills.",
            MAX_SKILL_MD_SIZE / (1024 * 1024)
        ));
    }

    Ok((new_content, applied_patches))
}

#[cfg(test)]
mod tests {
    use super::*;
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

        // Test assertion: valid patches must apply successfully
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

        // Test assertion: valid patches must apply successfully
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

        // Test assertion: valid patches must apply successfully
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

        // Test assertion: valid patches must apply successfully
        let (new_content, applied) =
            apply_skill_patches_preview(content, &patches, &std::collections::BTreeMap::new())
                .unwrap();

        assert_eq!(applied.project.len(), 1);
        assert!(new_content.starts_with("---\n"));
        assert!(new_content.contains("name: My Skill"));
        assert!(new_content.contains("# Test Skill"));
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
        // Test assertion: valid TOML must parse successfully
        let deps_value: toml::Value = toml::from_str(deps_toml).unwrap();
        patches.insert("dependencies".to_string(), deps_value);

        // Test assertion: valid patches must apply successfully
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
