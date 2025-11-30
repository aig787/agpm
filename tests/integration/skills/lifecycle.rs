//! Skill lifecycle tests.
//!
//! Tests for transitive dependencies, validation, listing, removal,
//! complete removal/reinstallation, and private patches.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use std::fs;

#[tokio::test]
async fn test_skill_with_transitive_dependencies() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create dependency resources
    source
        .add_resource(
            "agents",
            "base-agent",
            r#"---
name: Base Agent
description: A base agent for testing
---
# Base Agent
"#,
        )
        .await?;

    source
        .create_file(
            "snippets/utils.md",
            r#"---
name: Utility Snippets
description: Useful utility snippets
---
# Utility Snippets
"#,
        )
        .await?;

    // Create skill that depends on both
    source
        .create_skill(
            "complex-skill",
            r#"---
name: Complex Skill
description: A skill with dependencies
dependencies:
  agents:
    - path: agents/base-agent.md
  snippets:
    - path: snippets/utils.md
---
# Complex Skill

This skill depends on other resources.
"#,
        )
        .await?;

    source.commit_all("Add skill with dependencies")?;

    // Install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("complex-skill", |d| {
            d.source("test").path("skills/complex-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill and its dependencies were installed
    assert!(project.project_path().join(".claude/skills/complex-skill").exists());
    assert!(project.project_path().join(".claude/agents/base-agent.md").exists());
    assert!(project.project_path().join(".claude/snippets/utils.md").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_validation() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a valid skill
    source
        .create_skill(
            "valid-skill",
            r#"---
name: Valid Skill
description: A properly formatted skill
---
# Valid Skill
"#,
        )
        .await?;

    source.commit_all("Add valid skill")?;

    // Create manifest
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("valid-skill", |d| d.source("test").path("skills/valid-skill").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Install the skill
    project.run_agpm(&["install"])?;

    // Run validation to verify the installation
    let result = project.run_agpm(&["validate", "--paths"])?;
    assert!(result.success);

    // Verify skill was installed correctly
    assert!(project.project_path().join(".claude/skills/valid-skill").exists());
    Ok(())
}

#[tokio::test]
async fn test_skill_list_command() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create skills
    source
        .create_skill(
            "skill-a",
            r#"---
name: Skill A
description: First skill for listing
---
# Skill A
"#,
        )
        .await?;

    source
        .create_skill(
            "skill-b",
            r#"---
name: Skill B
description: Second skill for listing
---
# Skill B
"#,
        )
        .await?;

    source.commit_all("Add skills for listing")?;

    // Create manifest
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("skill-a", |d| d.source("test").path("skills/skill-a").version("HEAD"))
        .add_skill("skill-b", |d| d.source("test").path("skills/skill-b").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Install skills
    project.run_agpm(&["install"])?;

    // List skills (use --type skill since there's no --skills flag)
    let result = project.run_agpm(&["list", "--type", "skill"])?;
    assert!(
        result.success,
        "list command failed: stdout={}, stderr={}",
        result.stdout, result.stderr
    );
    assert!(result.stdout.contains("skill-a"), "skill-a not found in stdout: {}", result.stdout);
    assert!(result.stdout.contains("skill-b"), "skill-b not found in stdout: {}", result.stdout);
    Ok(())
}

#[tokio::test]
async fn test_remove_skill() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill
    source
        .create_skill(
            "removable-skill",
            r#"---
name: Removable Skill
description: A skill that can be removed
---
# Removable Skill
"#,
        )
        .await?;

    source.commit_all("Add removable skill")?;

    // Create manifest and install
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("removable-skill", |d| {
            d.source("test").path("skills/removable-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill is installed
    assert!(project.project_path().join(".claude/skills/removable-skill").exists());

    // Remove skill from manifest
    project.run_agpm(&["remove", "dep", "skill", "removable-skill"])?;

    // Verify skill was removed from manifest
    // Test assertion: agpm.toml must exist after successful agpm command execution
    let manifest_content = fs::read_to_string(project.project_path().join("agpm.toml")).unwrap();
    assert!(!manifest_content.contains("removable-skill"));
    Ok(())
}

#[tokio::test]
async fn test_skill_complete_removal_and_reinstallation() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill with multiple files for comprehensive testing
    source
        .create_skill(
            "comprehensive-skill",
            r#"---
name: Comprehensive Test Skill
description: A skill with multiple files for testing complete removal
model: claude-3-opus
temperature: "0.7"
---
# Comprehensive Test Skill

This skill tests complete removal and reinstallation.
"#,
        )
        .await?;

    // Add additional files to the skill directory
    let skill_source_dir = source.path.join("skills").join("comprehensive-skill");
    fs::write(skill_source_dir.join("config.json"), r#"{"setting": "value"}"#)?;
    fs::write(skill_source_dir.join("script.sh"), "#!/bin/bash\necho 'Hello World'")?;

    // Create a subdirectory with nested content
    fs::create_dir_all(skill_source_dir.join("utils"))?;
    fs::write(skill_source_dir.join("utils/helper.txt"), "Helper content")?;

    source.commit_all("Add comprehensive skill with multiple files")?;

    // Install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("comprehensive-skill", |d| {
            d.source("test").path("skills/comprehensive-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill was installed completely
    let skill_path = project.project_path().join(".claude/skills/comprehensive-skill");
    assert!(skill_path.exists(), "Skill directory should exist after installation");
    assert!(skill_path.is_dir(), "Skill should be a directory");
    assert!(skill_path.join("SKILL.md").exists(), "SKILL.md should exist");
    assert!(skill_path.join("config.json").exists(), "config.json should exist");
    assert!(skill_path.join("script.sh").exists(), "script.sh should exist");
    assert!(skill_path.join("utils/helper.txt").exists(), "Nested file should exist");

    // Verify skill appears in lockfile with checksum
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains("comprehensive-skill"), "Skill should be in lockfile");
    assert!(lockfile_content.contains("checksum = \"sha256:"), "Skill should have checksum");

    // Add an extra file directly to installed directory (should be removed during reinstallation)
    fs::write(skill_path.join("extra-file.txt"), "This should be removed")?;
    assert!(skill_path.join("extra-file.txt").exists(), "Extra file should exist initially");

    // Remove the skill from the manifest
    project.run_agpm(&["remove", "dep", "skill", "comprehensive-skill"])?;

    // Verify complete removal: directory should be gone
    assert!(!skill_path.exists(), "Skill directory should be completely removed after removal");
    assert!(
        !project.project_path().join(".claude/skills/comprehensive-skill").exists(),
        "Skill directory should not exist in any form"
    );

    // Verify skill was removed from manifest
    // Test assertion: agpm.toml must exist after successful agpm remove command
    let updated_manifest = fs::read_to_string(project.project_path().join("agpm.toml")).unwrap();
    assert!(
        !updated_manifest.contains("comprehensive-skill"),
        "Skill should be removed from manifest"
    );

    // Verify skill was removed from lockfile
    let updated_lockfile = project.read_lockfile().await?;
    assert!(
        !updated_lockfile.contains("comprehensive-skill"),
        "Skill should be removed from lockfile"
    );

    // Verify no artifacts remain - the entire skills directory structure for this skill should be gone
    let skills_dir = project.project_path().join(".claude/skills");
    if skills_dir.exists() {
        let entries: Vec<_> = fs::read_dir(skills_dir)?.collect::<Result<Vec<_>, _>>()?;
        assert!(
            !entries.iter().any(|entry| {
                entry.file_name().to_string_lossy().contains("comprehensive-skill")
            }),
            "No skill-related artifacts should remain"
        );
    }

    // Now re-add the skill to the manifest and reinstall
    let reinstallation_manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("comprehensive-skill", |d| {
            d.source("test").path("skills/comprehensive-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&reinstallation_manifest).await?;

    project.run_agpm(&["install"])?;

    // Verify successful reinstallation
    assert!(skill_path.exists(), "Skill directory should exist after reinstallation");
    assert!(skill_path.is_dir(), "Skill should be a directory after reinstallation");
    assert!(skill_path.join("SKILL.md").exists(), "SKILL.md should exist after reinstallation");
    assert!(
        skill_path.join("config.json").exists(),
        "config.json should exist after reinstallation"
    );
    assert!(skill_path.join("script.sh").exists(), "script.sh should exist after reinstallation");
    assert!(
        skill_path.join("utils/helper.txt").exists(),
        "Nested file should exist after reinstallation"
    );

    // Verify extra file was removed during reinstallation (clean reinstall)
    assert!(
        !skill_path.join("extra-file.txt").exists(),
        "Extra file should be removed during clean reinstallation"
    );

    // Verify skill appears back in lockfile with new checksum
    let final_lockfile = project.read_lockfile().await?;
    assert!(final_lockfile.contains("comprehensive-skill"), "Skill should be back in lockfile");
    assert!(
        final_lockfile.contains("checksum = \"sha256:"),
        "Skill should have checksum after reinstallation"
    );

    // Verify content integrity after reinstallation
    // Test assertion: SKILL.md must exist after successful reinstallation (verified by asserts above)
    let skill_content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();
    assert!(skill_content.contains("Comprehensive Test Skill"), "Skill content should be correct");

    // Test assertion: config.json must exist after successful reinstallation (verified by asserts above)
    let config_content = fs::read_to_string(skill_path.join("config.json")).unwrap();
    assert!(config_content.contains("\"setting\""), "Config file should be correct");

    // Test assertion: utils/helper.txt must exist after successful reinstallation (verified by asserts above)
    let helper_content = fs::read_to_string(skill_path.join("utils/helper.txt")).unwrap();
    assert_eq!(helper_content, "Helper content", "Nested file content should be correct");

    Ok(())
}

#[tokio::test]
async fn test_skill_with_private_patches() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill
    source
        .create_skill(
            "patchable-skill",
            r#"---
name: Patchable Skill
description: A skill for testing private patches
model: claude-3-opus
---
# Patchable Skill
"#,
        )
        .await?;

    source.commit_all("Add patchable skill")?;

    // Create manifest with project patches
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("patchable-skill", |d| {
            d.source("test").path("skills/patchable-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    // Create project patches
    let project_patches = r#"
[patch.skills.patchable-skill]
model = "claude-3-sonnet"
temperature = "0.5"
"#;
    fs::write(
        project.project_path().join("agpm.toml"),
        format!("{}\n{}", manifest_content, project_patches),
    )?;

    // Create private patches file
    let private_patches = r#"
[patch.skills.patchable-skill]
temperature = "0.9"
max_tokens = 1000
"#;
    fs::write(project.project_path().join("agpm.private.toml"), private_patches)?;

    // Install with both project and private patches
    project.run_agpm(&["install"])?;

    // Verify patches were applied
    let skill_path = project.project_path().join(".claude/skills/patchable-skill");
    // Test assertion: SKILL.md must exist after successful installation (directory created by install)
    let content = fs::read_to_string(skill_path.join("SKILL.md")).unwrap();

    // Project patch should be overridden by private patch
    assert!(content.contains("model: claude-3-sonnet"));
    assert!(content.contains("temperature: '0.9'")); // Private wins
    assert!(content.contains("max_tokens: 1000")); // Private only
    Ok(())
}
