//! Integration tests for project template variables in transitive dependencies

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Test that project variables can be used in transitive dependency paths
#[tokio::test]
async fn test_project_template_vars_in_transitive_deps() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo
    let community_repo = project.create_source_repo("community").await?;

    // Add a simple helper agent (no deps)
    community_repo
        .add_resource("agents", "rust-helper", "# Rust Helper\nHelps with Rust code")
        .await?;

    // Add main agent that depends on template-resolved path
    community_repo
        .add_resource(
            "agents",
            "main-agent",
            r#"---
agpm:
  templating: true
dependencies:
  agents:
    - path: ./{{ agpm.project.language }}-helper.md
      version: v1.0.0
---

# Main Agent
Uses language-specific helper.
"#,
        )
        .await?;

    community_repo.commit_all("Add resources with templated deps")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest with project variables
    let source_url = community_repo.bare_file_url(project.sources_path()).await?;
    let mut manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("main", "community", "agents/main-agent.md")
        .build();

    // Add [project] section manually since ManifestBuilder doesn't support it yet
    manifest = format!(
        r#"[project]
language = "rust"

{}
"#,
        manifest
    );

    project.write_manifest(&manifest).await?;

    // Run install - should resolve template variable
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify both agents were installed
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains("main-agent"), "Main agent should be in lockfile");

    // Verify transitive dependency with resolved template path
    assert!(
        lockfile_content.contains("rust-helper"),
        "Lockfile should contain transitive agent with resolved template (rust-helper). Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        !lockfile_content.contains("{{"),
        "Lockfile should not contain template syntax. Lockfile:\n{}",
        lockfile_content
    );

    // Verify both files exist
    let main_path = project.project_path().join(".claude/agents/agpm/main-agent.md");
    let helper_path = project.project_path().join(".claude/agents/agpm/rust-helper.md");
    assert!(tokio::fs::metadata(&main_path).await.is_ok(), "Main agent should be installed");
    assert!(
        tokio::fs::metadata(&helper_path).await.is_ok(),
        "Helper agent with template-resolved path should be installed at {:?}",
        helper_path
    );

    Ok(())
}

/// Test that undefined template variables fail with helpful error
#[tokio::test]
async fn test_undefined_template_var_error() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo
    let community_repo = project.create_source_repo("community").await?;

    // Add agent with undefined template variable
    community_repo
        .add_resource(
            "agents",
            "test-agent",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: ../snippets/{{ agpm.project.undefined_var }}-guide.md
---

# Test Agent
"#,
        )
        .await?;

    community_repo.commit_all("Add agent with undefined var")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest WITHOUT defining the variable
    let source_url = community_repo.bare_file_url(project.sources_path()).await?;
    let mut manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("test", "community", "agents/test-agent.md")
        .build();

    // Add [project] section manually
    manifest = format!(
        r#"[project]
language = "rust"

{}
"#,
        manifest
    );

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail with undefined template variable");

    // Should have helpful error message
    let combined_output = format!("{}{}", output.stdout, output.stderr);
    assert!(
        combined_output.contains("Failed to render frontmatter template")
            || combined_output.contains("not found in context"),
        "Should have template error message. Output:\n{}",
        combined_output
    );

    Ok(())
}
