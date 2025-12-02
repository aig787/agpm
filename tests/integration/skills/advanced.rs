//! Advanced skill tests.
//!
//! Tests for install:false dependencies and circular skill dependency detection.
//! Note: Template variables in SKILL.md frontmatter are not supported because
//! the frontmatter must be valid YAML before template rendering occurs.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

#[tokio::test]
async fn test_skill_with_install_false_dependency() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a helper resource that should NOT be installed
    source
        .create_file(
            "snippets/internal-helper.md",
            r#"---
name: Internal Helper
description: An internal helper that should not be installed
---
# Internal Helper

This is only used for content embedding, not installation.
"#,
        )
        .await?;

    // Create a skill that references the helper with install: false
    source
        .create_skill(
            "referencing-skill",
            r#"---
name: Referencing Skill
description: A skill that references but doesn't install a dependency
dependencies:
  snippets:
    - path: snippets/internal-helper.md
      install: false
      name: helper
---
# Referencing Skill

This skill references content from internal-helper but doesn't install it.
"#,
        )
        .await?;

    source.commit_all("Add skill with install:false dependency")?;

    // Install the skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("referencing-skill", |d| {
            d.source("test").path("skills/referencing-skill").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    project.run_agpm(&["install"])?;

    // Verify skill was installed
    let skill_path = project.project_path().join(".claude/skills/agpm/referencing-skill");
    assert!(skill_path.exists(), "Skill directory should exist");

    // Verify the internal helper was NOT installed (install: false)
    let helper_path = project.project_path().join(".claude/snippets/internal-helper.md");
    assert!(
        !helper_path.exists(),
        "Helper with install:false should NOT be installed at {}",
        helper_path.display()
    );

    Ok(())
}

#[tokio::test]
async fn test_skill_circular_dependency_detection() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create skill A that depends on skill B
    source
        .create_skill(
            "skill-a",
            r#"---
name: Skill A
description: First skill in circular dependency
dependencies:
  skills:
    - path: skills/skill-b
---
# Skill A

This skill depends on Skill B.
"#,
        )
        .await?;

    // Create skill B that depends on skill A (circular!)
    source
        .create_skill(
            "skill-b",
            r#"---
name: Skill B
description: Second skill in circular dependency
dependencies:
  skills:
    - path: skills/skill-a
---
# Skill B

This skill depends on Skill A, creating a circular dependency.
"#,
        )
        .await?;

    source.commit_all("Add skills with circular dependency")?;

    // Try to install skill A (which should detect the cycle)
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("skill-a", |d| d.source("test").path("skills/skill-a").version("HEAD"))
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;

    // The system should either:
    // 1. Detect the cycle and fail with an error
    // 2. Or handle it gracefully by not infinitely recursing

    // Check if it failed due to cycle detection
    if !result.success {
        assert!(
            result.stderr.contains("cycle")
                || result.stderr.contains("circular")
                || result.stderr.contains("dependency")
                || result.stderr.contains("recursive"),
            "Expected error about circular dependency, got: {}",
            result.stderr
        );
    } else {
        // If it succeeded, verify it didn't install infinitely
        // (the skill directory should exist but without infinite nesting)
        let skill_a_path = project.project_path().join(".claude/skills/agpm/skill-a");
        assert!(skill_a_path.exists(), "Skill A should be installed");

        // Verify no deeply nested circular installation
        let deeply_nested = project
            .project_path()
            .join(".claude/skills/agpm/skill-a/skills/skill-b/skills/skill-a");
        assert!(
            !deeply_nested.exists(),
            "Should not have deeply nested circular skill installation"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_skill_self_circular_dependency() -> Result<()> {
    let project = TestProject::new().await?;
    let source = project.create_source_repo("test").await?;

    // Create a skill that depends on itself
    source
        .create_skill(
            "self-referential",
            r#"---
name: Self Referential Skill
description: A skill that references itself
dependencies:
  skills:
    - path: skills/self-referential
---
# Self Referential Skill

This skill depends on itself!
"#,
        )
        .await?;

    source.commit_all("Add self-referential skill")?;

    // Try to install the self-referential skill
    let source_url = source.bare_file_url(project.sources_path())?;
    let manifest_content = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_skill("self-referential", |d| {
            d.source("test").path("skills/self-referential").version("HEAD")
        })
        .with_claude_code_tool()
        .build();
    project.write_manifest(&manifest_content).await?;

    let result = project.run_agpm(&["install"])?;

    // Should detect self-reference and either fail or handle gracefully
    if !result.success {
        assert!(
            result.stderr.contains("cycle")
                || result.stderr.contains("circular")
                || result.stderr.contains("self")
                || result.stderr.contains("recursive"),
            "Expected error about self-referential dependency, got: {}",
            result.stderr
        );
    } else {
        // If it succeeds, verify no infinite nesting
        let skill_path = project.project_path().join(".claude/skills/agpm/self-referential");
        assert!(skill_path.exists(), "Skill should be installed");

        // Verify no deeply nested self-installation
        let nested = skill_path.join("skills/self-referential/skills/self-referential");
        assert!(!nested.exists(), "Should not have deeply nested self-installation");
    }

    Ok(())
}
