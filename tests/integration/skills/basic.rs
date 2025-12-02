//! Basic skill installation tests.
//!
//! Tests for single skill installation, installation with patches,
//! and pattern-based installation of multiple skills.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use std::fs;

#[tokio::test]
async fn test_install_single_skill() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill in the source repo
    source
        .create_skill(
            "rust-helper",
            r#"---
name: Rust Helper
description: Helps with Rust development
model: claude-3-opus
temperature: "0.5"
---
# Rust Helper

I help with Rust development tasks.
"#,
        )
        .await?;

    // Create a dependency snippet
    source
        .create_file(
            "snippets/rust-patterns.md",
            r#"---
name: Rust Patterns
description: Common Rust patterns
---
# Rust Patterns

Useful Rust patterns and idioms.
"#,
        )
        .await?;

    source.commit_all("Add rust-helper skill and dependency")?;

    // Install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("rust-helper", |d| d.source("test").path("skills/rust-helper").version("HEAD"))
        .with_claude_code_tool()
        .build();
    eprintln!("Manifest:\n{}", manifest_content);
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;
    eprintln!("Install stdout: {}", result.stdout);
    eprintln!("Install stderr: {}", result.stderr);

    // Debug: list all files under .claude
    let claude_dir = project.project_path().join(".claude");
    if claude_dir.exists() {
        eprintln!("Contents of .claude/:");
        for e in walkdir::WalkDir::new(&claude_dir).max_depth(4).into_iter().flatten() {
            eprintln!("  {}", e.path().display());
        }
    } else {
        eprintln!(".claude directory does not exist");
    }

    // Also check the lockfile
    let lockfile_content = project.read_lockfile().await?;
    eprintln!("Lockfile:\n{}", lockfile_content);

    // Verify skill was installed
    let skill_path = project.project_path().join(".claude/skills/agpm/rust-helper");
    eprintln!("Expected skill path: {}, exists: {}", skill_path.display(), skill_path.exists());
    assert!(skill_path.exists());
    assert!(skill_path.join("SKILL.md").exists());

    // Verify content is correct
    // Test assertion: SKILL.md must exist after successful installation (verified by assert above)
    let content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();
    assert!(content.contains("name: Rust Helper"));
    assert!(content.contains("description: Helps with Rust development"));
    Ok(())
}

#[tokio::test]
async fn test_install_skill_with_patches() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill in the source repo
    source
        .create_skill(
            "my-skill",
            r#"---
name: My Test Skill
description: A skill for testing
model: claude-3-opus
temperature: "0.5"
---
# My Test Skill

This is a test skill.
"#,
        )
        .await?;

    source.commit_all("Add my-skill")?;
    let source_url = source.bare_file_url(project.sources_path())?;

    // Create manifest with patches
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("my-skill", |d| d.source("test").path("skills/my-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Create private manifest with patches
    let private_content = r#"
[patch.skills.my-skill]
model = "claude-3-haiku"
temperature = "0.7"
max_tokens = 2000
"#;
    project.write_private_manifest(private_content).await?;

    project.run_agpm(&["install"])?;

    // Verify patches were applied
    let skill_path = project.project_path().join(".claude/skills/agpm/my-skill");
    // Test assertion: SKILL.md must exist after successful installation
    let content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();

    assert!(content.contains("model: claude-3-haiku"));
    assert!(content.contains("temperature: '0.7'"));
    assert!(content.contains("max_tokens: 2000"));
    // Original value should be overridden
    assert!(!content.contains("claude-3-opus"));
    assert!(!content.contains("temperature: \"0.5\""));
    Ok(())
}

#[tokio::test]
async fn test_install_multiple_skills_pattern() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create multiple skills
    source
        .create_skill(
            "skill1",
            r#"---
name: Skill One
description: First test skill
---
# Skill One
"#,
        )
        .await?;

    source
        .create_skill(
            "skill2",
            r#"---
name: Skill Two
description: Second test skill
---
# Skill Two
"#,
        )
        .await?;

    source
        .create_skill(
            "skill3",
            r#"---
name: Skill Three
description: Third test skill
---
# Skill Three
"#,
        )
        .await?;

    source.commit_all("Add multiple skills")?;

    // Install with pattern
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("all", |d| d.source("test").path("skills/*").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify all skills were installed
    assert!(project.project_path().join(".claude/skills/agpm/skill1").exists());
    assert!(project.project_path().join(".claude/skills/agpm/skill2").exists());
    assert!(project.project_path().join(".claude/skills/agpm/skill3").exists());

    // Verify content
    let expected_names =
        [("skill1", "Skill One"), ("skill2", "Skill Two"), ("skill3", "Skill Three")];
    for (skill_name, expected_display_name) in expected_names {
        let skill_path = project.project_path().join(".claude/skills/agpm").join(skill_name);
        assert!(skill_path.exists(), "Skill directory {} does not exist", skill_name);

        let skill_md_path = skill_path.join("SKILL.md");
        assert!(skill_md_path.exists(), "SKILL.md does not exist in {}", skill_name);

        // Test assertion: SKILL.md must exist after successful installation (verified by assert above)
        let content = fs::read_to_string(&skill_md_path).unwrap();
        assert!(content.contains(&format!("name: {}", expected_display_name)));
    }
    Ok(())
}
