//! Error scenario tests for skills.
//!
//! Tests for error handling including missing SKILL.md, invalid frontmatter,
//! missing required fields, path traversal attempts, and installation rollback.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use std::fs;

#[tokio::test]
async fn test_skill_missing_skill_md() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill directory without SKILL.md
    let skill_dir = source.path.join("skills").join("incomplete-skill");
    fs::create_dir_all(&skill_dir)?;
    // Create a different file but not SKILL.md
    fs::write(skill_dir.join("README.md"), "# Readme")?;

    source.commit_all("Add incomplete skill")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("incomplete-skill", |d| {
            d.source("test").path("skills/incomplete-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    // Tightened assertion: error should mention SKILL.md and indicate it's missing/not found
    assert!(
        result.stderr.contains("SKILL.md")
            && (result.stderr.contains("missing")
                || result.stderr.contains("not found")
                || result.stderr.contains("reading")),
        "Expected error about missing SKILL.md, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/incomplete-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_invalid_frontmatter() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill with malformed YAML frontmatter
    source
        .create_skill(
            "invalid-frontmatter",
            r#"---
name: Invalid Frontmatter
description: A skill with bad YAML
model: claude-3-opus
temperature: "0.5"
invalid_yaml: [unclosed array
---
# Invalid Frontmatter

This skill has malformed YAML.
"#,
        )
        .await?;

    source.commit_all("Add skill with invalid frontmatter")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("invalid-frontmatter", |d| {
            d.source("test").path("skills/invalid-frontmatter").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("Failed to parse")
            || result.stderr.contains("YAML")
            || result.stderr.contains("frontmatter"),
        "Expected error about parsing failure, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/invalid-frontmatter").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_missing_required_fields() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill missing required 'name' field
    source
        .create_skill(
            "missing-name",
            r#"---
description: A skill missing the name field
model: claude-3-opus
---
# Missing Name

This skill is missing the required name field.
"#,
        )
        .await?;

    source.commit_all("Add skill missing required field")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("missing-name", |d| d.source("test").path("skills/missing-name").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    assert!(
        result.stderr.contains("missing required field")
            || result.stderr.contains("name")
            || result.stderr.contains("validation"),
        "Expected error about missing required field, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/missing-name").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_path_traversal_attempt() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a normal skill but use malicious path in manifest
    source
        .create_skill(
            "malicious-skill",
            r#"---
name: Malicious Skill
description: A skill trying to escape directory
model: claude-3-opus
---
# Malicious Skill

This skill tries to traverse paths.
"#,
        )
        .await?;

    source.commit_all("Add malicious skill")?;

    // Try to install the skill using a path that tries to traverse directories
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("malicious", |d| {
            d.source("test").path("skills/../../../malicious-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    // Tightened assertion: traversal attempts fail because target path doesn't exist or is invalid
    // The error may reference file access issues, path problems, or installation failure
    assert!(
        result.stderr.contains("SKILL.md")
            || result.stderr.contains("path")
            || result.stderr.contains("directory"),
        "Expected error about path traversal or missing directory, got: {}",
        result.stderr
    );

    // Verify nothing was installed outside the skills directory
    assert!(!project.project_path().join(".claude/malicious-skill").exists());
    assert!(!project.project_path().join("malicious-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_installation_rollback() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a valid skill first
    source
        .create_skill(
            "valid-skill",
            r#"---
name: Valid Skill
description: A valid skill for rollback test
model: claude-3-opus
---
# Valid Skill

This skill should install successfully.
"#,
        )
        .await?;

    source.commit_all("Add valid skill")?;

    // Install the valid skill first
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("valid-skill", |d| d.source("test").path("skills/valid-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify the valid skill was installed
    assert!(project.project_path().join(".claude/skills/valid-skill").exists());

    // Now create an invalid skill and add it to the manifest
    source
        .create_skill(
            "invalid-skill",
            r#"---
description: Missing required name field
model: claude-3-opus
---
# Invalid Skill

This skill should fail.
"#,
        )
        .await?;

    source.commit_all("Add invalid skill")?;

    // Update manifest to include both skills
    let updated_manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("valid-skill", |d| d.source("test").path("skills/valid-skill").version("HEAD"))
        .add_skill("invalid-skill", |d| {
            d.source("test").path("skills/invalid-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&updated_manifest_content).await?;

    // Try to install again - the invalid one should fail but the valid one should remain
    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");

    // Verify the valid skill still exists (AGPM doesn't rollback on partial failures)
    assert!(project.project_path().join(".claude/skills/valid-skill").exists());
    // Verify the invalid skill was not installed
    assert!(!project.project_path().join(".claude/skills/invalid-skill").exists());

    Ok(())
}

#[tokio::test]
async fn test_skill_sensitive_path_validation() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a normal skill but try to install it to a sensitive path via manifest
    source
        .create_skill(
            "sensitive-skill",
            r#"---
name: Sensitive Skill
description: A skill being installed to sensitive path
model: claude-3-opus
---
# Sensitive Skill

This skill is being installed to a sensitive path.
"#,
        )
        .await?;

    source.commit_all("Add skill for sensitive path test")?;

    // Try to install the skill to a sensitive path
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("sensitive", |d| d.source("test").path("skills/.git").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    // Tightened assertion: .git is not a valid skill directory (missing SKILL.md)
    // Error should mention SKILL.md, path, or directory issues
    assert!(
        result.stderr.contains("SKILL.md")
            || result.stderr.contains("path")
            || result.stderr.contains("directory"),
        "Expected error about invalid path or missing SKILL.md, got: {}",
        result.stderr
    );

    // Verify .git directory was not touched
    let git_dir = project.project_path().join(".claude/skills/.git");
    assert!(!git_dir.exists(), "Sensitive .git directory should not exist");
    Ok(())
}
