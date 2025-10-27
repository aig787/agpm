//! Tests for cleanup logic with dual checksum system

use agpm_cli::tests::common::TestProject;
use anyhow::Result;
use tokio::fs as fs;

/// Test that cleanup logic works correctly with dual checksum system
#[tokio::test]
async fn test_cleanup_with_dual_checksums() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a template with variables that will generate different context checksums
    test_repo
        .add_resource(
            "agents",
            "templated-agent",
            r#"---
title: "{{ project.title }}"
dependencies:
  agents:
    - path: agents/helper.md
      version: "v1.0.0"
agpm:
  templating: true
---
# {{ project.title }} Agent

This agent uses template variables.
"#,
        )
        .await?;

    // Create the helper dependency
    test_repo
        .add_resource(
            "agents",
            "helper",
            r#"---
title: Helper Agent
---
# Helper Agent

I help with tasks.
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // First install: with template variables
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
templated = {{ source = "test-repo", path = "agents/templated-agent.md", version = "v1.0.0", template_vars = {{ project = {{ title = "Project Alpha" }} }} }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Initial install should succeed. Stderr: {}", output.stderr);

    // Verify the agent and its dependency are installed
    let agent_path = project.project_path().join(".claude/agents/templated.md");
    let helper_path = project.project_path().join(".claude/agents/helper.md");

    assert!(
        fs::metadata(&agent_path).await.is_ok(),
        "Templated agent should be installed at {:?}",
        agent_path
    );
    assert!(
        fs::metadata(&helper_path).await.is_ok(),
        "Helper should be installed at {:?}",
        helper_path
    );

    // Verify lockfile contains context checksum
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("context_checksum"),
        "Lockfile should contain context_checksum. Content:\n{}",
        lockfile_content
    );

    // Second install: change template variables (different context checksum)
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
templated = {{ source = "test-repo", path = "agents/templated-agent.md", version = "v1.0.0", template_vars = {{ project = {{ title = "Project Beta" }} }} }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Second install should succeed. Stderr: {}", output.stderr);

    // Verify both files still exist (only context checksum changed, not paths)
    assert!(
        fs::metadata(&agent_path).await.is_ok(),
        "Agent should still exist (only context checksum changed)"
    );
    assert!(
        fs::metadata(&helper_path).await.is_ok(),
        "Helper should still exist"
    );

    // Verify context checksum changed in lockfile
    let updated_lockfile_content = project.read_lockfile().await?;
    assert!(
        updated_lockfile_content.contains("context_checksum"),
        "Updated lockfile should still contain context_checksum"
    );

    // Third install: remove the templated agent (should trigger cleanup)
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

# No agents - templated agent removed
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Cleanup install should succeed. Stderr: {}", output.stderr);

    // Verify cleanup occurred
    assert!(
        output.stdout.contains("Cleaned up") || output.stdout.contains("moved or removed"),
        "Should report cleanup. Output: {}",
        output.stdout
    );

    // Verify files were removed
    assert!(
        fs::metadata(&agent_path).await.is_err(),
        "Agent should be removed after manifest removal"
    );
    assert!(
        fs::metadata(&helper_path).await.is_err(),
        "Helper should be removed as transitive dependency"
    );

    Ok(())
}

/// Test that cleanup logic works with variant_inputs (renamed from template_vars)
#[tokio::test]
async fn test_cleanup_with_variant_inputs() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a template that uses variant_inputs
    test_repo
        .add_resource(
            "snippets",
            "variant-test",
            r#"---
title: "{{ config.name }}"
version: "{{ config.version }}"
agpm:
  templating: true
---
# {{ config.name }} v{{ config.version }}

This is a test snippet with variant inputs.
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Install with specific variant_inputs
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[snippets]
test = {{ source = "test-repo", path = "snippets/variant-test.md", version = "v1.0.0", template_vars = {{ config = {{ name = "MySnippet", version = "2.0" }} }} }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Initial install should succeed. Stderr: {}", output.stderr);

    // Verify snippet is installed
    let snippet_path = project.project_path().join(".agpm/snippets/test.md");
    assert!(
        fs::metadata(&snippet_path).await.is_ok(),
        "Snippet should be installed at {:?}",
        snippet_path
    );

    // Verify lockfile contains variant_inputs (serialized as template_vars for compatibility)
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("template_vars") || lockfile_content.contains("variant_inputs"),
        "Lockfile should contain variant_inputs (serialized as template_vars). Content:\n{}",
        lockfile_content
    );

    // Remove snippet from manifest
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

# No snippets - test removed
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Cleanup install should succeed. Stderr: {}", output.stderr);

    // Verify snippet was cleaned up
    assert!(
        fs::metadata(&snippet_path).await.is_err(),
        "Snippet should be removed after manifest removal"
    );

    Ok(())
}