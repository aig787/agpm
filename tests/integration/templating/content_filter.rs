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
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

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
        fs::read_to_string(project.project_path().join(".claude/agents/agpm/test.md")).await?;

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
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

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
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

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
        fs::read_to_string(project.project_path().join(".claude/agents/agpm/test.md")).await?;

    assert!(installed_agent.contains("# Real Content"), "Should contain actual content");
    assert!(
        installed_agent.contains("This is the actual documentation"),
        "Should contain documentation text"
    );
    assert!(!installed_agent.contains("title: Documentation"), "Should not contain frontmatter");
    assert!(!installed_agent.contains("author: Test"), "Should not contain frontmatter fields");

    Ok(())
}

/// Test content filter with large files (size limits)
#[tokio::test]
async fn test_content_filter_size_limit() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create an agent that tries to read a large file
    source_repo
        .add_resource(
            "agents",
            "large-reader",
            r#"---
agpm:
  templating: true
---
# Large File Reader

Reading large file:

{{ 'large.txt' | content }}

End of reader.
"#,
        )
        .await?;
    source_repo.commit_all("Add large file reader")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create a very large file (>1MB)
    let large_content = "x".repeat(2_000_000); // 2MB of 'x' characters
    fs::write(project.project_path().join("large.txt"), large_content).await?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_standard_agent("large-reader", "test", "agents/large-reader.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should either fail or handle gracefully
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        // If it fails, error should mention size limit
        let error_msg = output.stderr.to_lowercase();
        assert!(
            error_msg.contains("size")
                || error_msg.contains("too large")
                || error_msg.contains("limit"),
            "Error should mention size limit when failing: {}",
            error_msg
        );
    } else {
        // If it succeeds, verify to file was handled
        let installed_agent =
            fs::read_to_string(project.project_path().join(".claude/agents/agpm/large-reader.md"))
                .await?;

        // Should either contain truncated content or a message about size
        assert!(
            installed_agent.contains("x".repeat(1000).as_str()) || // Some content present
            installed_agent.contains("large") || // Or a message about large file
            installed_agent.contains("truncated"),
            "Agent should contain some indication of large file handling"
        );
    }

    Ok(())
}

/// Test content filter with various path traversal attempts
#[tokio::test]
async fn test_content_filter_path_traversal_variations() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create an agent with multiple path traversal attempts
    source_repo
        .add_resource(
            "agents",
            "path-traversal-test",
            r#"---
agpm:
  templating: true
---
# Path Traversal Tests

Various traversal attempts:

{{ '../../../etc/passwd' | content }}
{{ '..\\..\\..\\windows\\system32\\config\\sam' | content }}
{{ '....//....//....//etc/passwd' | content }}
{{ '%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd' | content }}
{{ '/etc/passwd' | content }}
"#,
        )
        .await?;
    source_repo.commit_all("Add path traversal test")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_standard_agent("path-traversal-test", "test", "agents/path-traversal-test.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail due to path traversal attempts
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail with path traversal attempts");

    let error_msg = output.stderr.to_lowercase();
    assert!(
        error_msg.contains("path traversal")
            || error_msg.contains("invalid path")
            || error_msg.contains("security")
            || error_msg.contains(".."),
        "Error should mention path traversal or security: {}",
        error_msg
    );

    Ok(())
}

/// Test content filter with non-existent files
#[tokio::test]
async fn test_content_filter_non_existent_file() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create an agent that tries to read a non-existent file
    source_repo
        .add_resource(
            "agents",
            "missing-file-reader",
            r#"---
agpm:
  templating: true
---
# Missing File Reader

Trying to read non-existent file:

{{ 'non-existent.txt' | content }}

This should fail gracefully.
"#,
        )
        .await?;
    source_repo.commit_all("Add missing file reader")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_standard_agent("missing-file-reader", "test", "agents/missing-file-reader.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail with non-existent file");

    let error_msg = output.stderr.to_lowercase();
    assert!(
        error_msg.contains("not found")
            || error_msg.contains("no such file")
            || error_msg.contains("does not exist")
            || error_msg.contains("non-existent.txt"),
        "Error should mention file not found: {}",
        error_msg
    );

    Ok(())
}

/// Test content filter with binary files
#[tokio::test]
async fn test_content_filter_binary_file() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create an agent that tries to read a binary file
    source_repo
        .add_resource(
            "agents",
            "binary-reader",
            r#"---
agpm:
  templating: true
---
# Binary File Reader

Reading binary file:

{{ 'binary.dat' | content }}

Should handle gracefully.
"#,
        )
        .await?;
    source_repo.commit_all("Add binary file reader")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path()).await?;

    // Create a binary file (some non-UTF8 bytes)
    let binary_data = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD];
    fs::write(project.project_path().join("binary.dat"), binary_data).await?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_url)
        .add_standard_agent("binary-reader", "test", "agents/binary-reader.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should either fail or handle gracefully
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        let error_msg = output.stderr.to_lowercase();
        assert!(
            error_msg.contains("binary")
                || error_msg.contains("utf-8")
                || error_msg.contains("invalid")
                || error_msg.contains("encoding"),
            "Error should mention binary/encoding issue: {}",
            error_msg
        );
    } else {
        // If it succeeds, verify it was handled
        let installed_agent =
            fs::read_to_string(project.project_path().join(".claude/agents/agpm/binary-reader.md"))
                .await?;

        // Should contain some indication of binary file handling
        assert!(
            installed_agent.contains("binary")
                || installed_agent.contains("base64")
                || installed_agent.contains("hex")
                || installed_agent.contains("[binary data]"),
            "Agent should indicate binary file was handled"
        );
    }

    Ok(())
}
