//! Tests for enhanced template error handling and clarity
//!
//! Tests the improved error handling system that provides:
//! - Clear, actionable error messages
//! - Enhanced context with dependency chains
//! - Proper literal block handling in error scenarios
//! - Cross-platform path handling in errors

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Test literal block preservation with basic template
#[tokio::test]
async fn test_literal_blocks_preserve_template_syntax() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-source").await?;

    // Create a simple agent with literal blocks
    test_repo
        .add_resource(
            "agents",
            "literal-test",
            r#"---
agpm:
  templating: true
---

# Agent with Literal Blocks

This template shows how to preserve template syntax:

```literal
{{ agpm.resource.name }}
{{ agpm.resource.install_path }}
{% for dep in agpm.deps.agents %}
- {{ dep.name }}
{% endfor %}
```

The syntax above should appear literally in the output."#,
        )
        .await?;

    test_repo.commit_all("Add agent with literal blocks")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path()).await?;

    // Create project manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &repo_url)
        .add_agent("literal-agent", |d| {
            d.source("test-source").path("agents/literal-test.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run agpm install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Read the installed agent
    let installed_path = project.project_path().join(".claude/agents/agpm/literal-test.md");
    let content = tokio::fs::read_to_string(&installed_path).await?;

    // Verify literal blocks are preserved in code fences
    assert!(content.contains("```\n{{ agpm.resource.name }}"));
    assert!(content.contains("{{ agpm.resource.install_path }}"));
    assert!(content.contains("{% for dep in agpm.deps.agents %}"));
    assert!(content.contains("{% endfor %}\n```"));

    // Verify no placeholder artifacts remain
    assert!(!content.contains("__AGPM_LITERAL_BLOCK_"));

    Ok(())
}

/// Test that template errors provide clear messages
#[tokio::test]
async fn test_template_error_messages_are_clear() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-source").await?;

    // Create a template with a syntax error
    test_repo
        .add_resource(
            "agents",
            "syntax-error",
            r#"---
agpm:
  templating: true
---

# Agent with Syntax Error

This template has broken syntax:

{{ variable_with_no_closing_brace

Another line here."#,
        )
        .await?;

    test_repo.commit_all("Add agent with syntax error")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path()).await?;

    // Create project manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &repo_url)
        .add_agent("broken-agent", |d| {
            d.source("test-source").path("agents/syntax-error.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run agpm install - should fail with clear error
    let result = project.run_agpm(&["install"])?;

    // Should fail
    assert!(!result.success, "Install should fail due to template syntax error");

    // Error should be helpful
    let stderr = &result.stderr;
    assert!(
        stderr.contains("Template syntax error") || stderr.contains("failed to render"),
        "Error should mention template syntax issue: {}",
        stderr
    );

    // Error should NOT contain internal template names
    assert!(!stderr.contains("__tera_one_off"), "Error should not expose internal template names");

    Ok(())
}

/// Test disabled templating preserves template syntax
#[tokio::test]
async fn test_disabled_templating_preserves_syntax() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-source").await?;

    // Create an agent WITHOUT templating enabled (no agpm.templating: true)
    test_repo
        .add_resource(
            "agents",
            "no-template",
            r#"---
title: No Template Agent
---

# Agent Without Template Processing

This content should not be processed:

{{ agpm.resource.name }}
{{ undefined.variable }}
{% if condition %}{% endif %}

All template syntax should remain exactly as written."#,
        )
        .await?;

    test_repo.commit_all("Add agent without templating")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path()).await?;

    // Create project manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &repo_url)
        .add_agent("no-template-agent", |d| {
            d.source("test-source").path("agents/no-template.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run agpm install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Read the installed agent
    let installed_path = project.project_path().join(".claude/agents/agpm/no-template.md");
    let content = tokio::fs::read_to_string(&installed_path).await?;

    // Template syntax should be preserved exactly as written
    assert!(content.contains("{{ agpm.resource.name }}"));
    assert!(content.contains("{{ undefined.variable }}"));
    assert!(content.contains("{% if condition %}{% endif %}"));

    Ok(())
}
