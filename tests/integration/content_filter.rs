//! Integration tests for the content filter (`{{ 'path' | content }}`)
//!
//! Tests the content filter which allows embedding project files in templates.

use anyhow::Result;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test basic content filter usage - embedding a project README
#[tokio::test]
async fn test_content_filter_basic() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create an agent that uses the content filter
    source_repo
        .add_resource(
            "agents",
            "test",
            r#"---
agpm:
  templating: true
---
# Test Agent

## Project Readme

{{ 'README.md' | content }}
"#,
        )
        .await?;
    source_repo.commit_all("Add test agent")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create a README in the project
    fs::write(
        project.project_path().join("README.md"),
        "# My Project\n\nThis is the project README.",
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_standard_agent("test", "test", "agents/test.md")
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify the rendered output contains the README content
    let installed_agent =
        fs::read_to_string(project.project_path().join(".claude/agents/test.md")).await?;

    assert!(installed_agent.contains("# My Project"), "Should contain embedded README title");
    assert!(
        installed_agent.contains("This is the project README"),
        "Should contain embedded README content"
    );
    assert!(!installed_agent.contains("{{ '"), "Template syntax should be replaced");

    Ok(())
}

/// Test path traversal attempts should be rejected
#[tokio::test]
async fn test_content_filter_security_path_traversal() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create an agent that tries path traversal
    source_repo
        .add_resource(
            "agents",
            "malicious",
            r#"---
agpm:
  templating: true
---
# Malicious Agent

{{ '../../../etc/passwd' | content }}
"#,
        )
        .await?;
    source_repo.commit_all("Add malicious agent")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_standard_agent("malicious", "test", "agents/malicious.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail due to path traversal
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail with path traversal");
    assert!(
        output.stderr.contains("Path traversal detected") || output.stderr.contains(".."),
        "Error should mention path traversal: {}",
        output.stderr
    );

    Ok(())
}

/// Test Markdown frontmatter stripping
#[tokio::test]
async fn test_content_filter_strips_frontmatter() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create an agent that embeds a file
    source_repo
        .add_resource(
            "agents",
            "test",
            r#"---
agpm:
  templating: true
---
# Agent

{{ 'doc.md' | content }}
"#,
        )
        .await?;
    source_repo.commit_all("Add test agent")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create a Markdown file with frontmatter
    let doc_with_frontmatter = r#"---
title: Documentation
author: Test
---
# Real Content

This is the actual documentation."#;

    fs::write(project.project_path().join("doc.md"), doc_with_frontmatter).await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_standard_agent("test", "test", "agents/test.md")
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify frontmatter is stripped
    let installed_agent =
        fs::read_to_string(project.project_path().join(".claude/agents/test.md")).await?;

    assert!(installed_agent.contains("# Real Content"), "Should contain actual content");
    assert!(
        installed_agent.contains("This is the actual documentation"),
        "Should contain documentation text"
    );
    assert!(!installed_agent.contains("title: Documentation"), "Should not contain frontmatter");
    assert!(!installed_agent.contains("author: Test"), "Should not contain frontmatter fields");

    Ok(())
}
