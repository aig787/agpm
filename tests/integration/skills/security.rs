//! Security tests for skills.
//!
//! Tests for resource size limits, file count limits, frontmatter size limits,
//! and symlink attack rejection.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use std::fs;

#[tokio::test]
async fn test_skill_resource_size_limit() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill with a very large file to test size limits
    let large_content = "x".repeat(200 * 1024 * 1024); // 200MB (exceeds 100MB limit)
    source
        .create_skill(
            "large-skill",
            &format!(
                r#"---
name: Large Skill
description: A skill with oversized content
model: claude-3-opus
---
# Large Skill

This skill contains a large file.

{}

Large content here.
"#,
                large_content
            ),
        )
        .await?;

    source.commit_all("Add oversized skill")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path()).await?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("large-skill", |d| d.source("test").path("skills/large-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    // Tightened assertion: error should mention size or limit exceeded
    assert!(
        (result.stderr.contains("size") || result.stderr.contains("Size"))
            && (result.stderr.contains("limit")
                || result.stderr.contains("exceeds")
                || result.stderr.contains("MB")),
        "Expected error about size limit exceeded, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/agpm/large-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_file_count_limit() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill directory with many files to test file count limit
    let skill_dir = source.path.join("skills").join("many-files-skill");
    fs::create_dir_all(&skill_dir)?;

    // Create SKILL.md
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Many Files Skill
description: A skill with too many files
model: claude-3-opus
---
# Many Files Skill

This skill has too many files.
"#,
    )?;

    // Create many additional files (exceeding 1000 file limit)
    for i in 0..1100 {
        fs::write(skill_dir.join(format!("file_{:04}.txt", i)), format!("Content of file {}", i))?;
    }

    source.commit_all("Add skill with too many files")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path()).await?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("many-files-skill", |d| {
            d.source("test").path("skills/many-files-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    // Tightened assertion: error should mention file count or limit exceeded
    assert!(
        (result.stderr.contains("file") || result.stderr.contains("File"))
            && (result.stderr.contains("limit") || result.stderr.contains("exceeds")),
        "Expected error about file count limit exceeded, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/agpm/many-files-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_frontmatter_size_limit() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill with frontmatter that exceeds MAX_FRONTMATTER_SIZE_BYTES (64KB)
    // Generate a very long description to exceed the limit
    let long_description = "x".repeat(65 * 1024); // 65KB, exceeds 64KB limit
    let skill_content = format!(
        r#"---
name: Oversized Frontmatter Skill
description: {}
model: claude-3-opus
---
# Oversized Frontmatter Skill

This skill has oversized frontmatter.
"#,
        long_description
    );

    source.create_skill("oversized-frontmatter", &skill_content).await?;
    source.commit_all("Add skill with oversized frontmatter")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path()).await?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("oversized-frontmatter", |d| {
            d.source("test").path("skills/oversized-frontmatter").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    // Frontmatter size validation should catch this
    assert!(
        result.stderr.contains("frontmatter")
            && (result.stderr.contains("size")
                || result.stderr.contains("exceeds")
                || result.stderr.contains("KB")),
        "Expected error about frontmatter size exceeding limit, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/agpm/oversized-frontmatter").exists());
    Ok(())
}

#[cfg(unix)] // Symlinks work differently on Windows
#[tokio::test]
async fn test_skill_symlink_attack_rejection() -> Result<()> {
    use std::os::unix::fs::symlink;

    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill directory with a symlink pointing outside
    let skill_dir = source.path.join("skills").join("symlink-skill");
    fs::create_dir_all(&skill_dir)?;

    // Create SKILL.md
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Symlink Skill
description: A skill with a symlink attack attempt
model: claude-3-opus
---
# Symlink Skill

This skill contains a malicious symlink.
"#,
    )?;

    // Create a symlink to /etc/passwd (a sensitive file)
    // The symlink validation should reject this
    let symlink_path = skill_dir.join("sensitive-data.txt");
    symlink("/etc/passwd", &symlink_path)?;

    source.commit_all("Add skill with symlink attack")?;

    // Try to install the skill
    let source_url = source.bare_file_url(project.sources_path()).await?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("symlink-skill", |d| {
            d.source("test").path("skills/symlink-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    assert!(!result.success, "Expected command to fail but it succeeded");
    // Symlink validation should catch this
    assert!(
        result.stderr.contains("symlink")
            || result.stderr.contains("Symlink")
            || result.stderr.contains("not allowed"),
        "Expected error about symlinks not being allowed, got: {}",
        result.stderr
    );

    // Verify nothing was installed
    assert!(!project.project_path().join(".claude/skills/agpm/symlink-skill").exists());
    Ok(())
}
