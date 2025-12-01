//! Tests for template validation functionality.
//!
//! Covers:
//! - Template validation with --render flag
//! - Error detection and reporting
//! - Syntax validation
//! - Template discovery and counting

use anyhow::Result;
use serde_json::Value as JsonValue;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};
use agpm_cli::utils::normalize_path_for_storage;

/// Test template validation with render flag.
#[tokio::test]
async fn test_validate_render_valid_templates() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a templated agent
    test_repo
        .add_resource(
            "agents",
            "valid-agent",
            r#"---
title: Valid Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent is at: {{ agpm.resource.install_path }}
Version: {{ agpm.resource.version }}
"#,
        )
        .await?;

    // Create a non-templated agent
    test_repo
        .add_resource(
            "agents",
            "plain-agent",
            r#"---
title: Plain Agent
---
# Plain Agent

No templates here.
"#,
        )
        .await?;

    test_repo.commit_all("Add agents")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("valid-agent", |d| {
            d.source("test-repo").path("agents/valid-agent.md").version("v1.0.0")
        })
        .add_agent("plain-agent", |d| {
            d.source("test-repo").path("agents/plain-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install first to generate lockfile
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Validate with render flag
    let output = project.run_agpm(&["validate", "--render"])?;
    assert!(output.success, "Validation should succeed");
    let stdout = &output.stdout;

    // Should report 1 template found (only one has templating enabled)
    assert!(
        stdout.contains("found 1 template")
            || stdout.contains("found 1 templates")
            || stdout.contains("1 template")
            || stdout.contains("rendered successfully")
    );

    Ok(())
}

/// Test template validation with invalid syntax.
#[tokio::test]
async fn test_validate_render_invalid_syntax() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with invalid template syntax
    test_repo
        .add_resource(
            "agents",
            "invalid-agent",
            r#"---
title: Invalid Agent
agpm:
  templating: true
---
# {{ agpm.resource.name

Missing closing braces: {{ broken
Another error: {% if true
"#,
        )
        .await?;

    test_repo.commit_all("Add invalid agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("invalid-agent", |d| {
            d.source("test-repo").path("agents/invalid-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Validate with render flag (should fail)
    let output = project.run_agpm(&["validate", "--render"])?;
    assert!(!output.success, "Validation should fail");
    let stderr = &output.stderr;

    // Should report syntax errors
    assert!(
        stderr.contains("Template error")
            || stderr.contains("template error")
            || stderr.contains("TEMPLATE ERROR")
    );

    Ok(())
}

/// Test template validation with missing variable.
#[tokio::test]
async fn test_validate_render_missing_variable() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent that references a non-existent variable
    test_repo
        .add_resource(
            "agents",
            "missing-var-agent",
            r#"---
title: Missing Var Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This references: {{ agpm.nonexistent.variable }}
And another: {{ missing.too }}
"#,
        )
        .await?;

    test_repo.commit_all("Add agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("missing-var-agent", |d| {
            d.source("test-repo").path("agents/missing-var-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Validate with render flag (should fail)
    let output = project.run_agpm(&["validate", "--render"])?;
    assert!(!output.success, "Validation should fail");
    let stderr = &output.stderr;

    // Should report variable not found error
    assert!(
        stderr.contains("not found")
            || stderr.contains("VARIABLE")
            || stderr.contains("Template error")
    );

    Ok(())
}

/// Test template validation without lockfile.
#[tokio::test]
async fn test_validate_render_no_lockfile() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent
    test_repo
        .add_resource(
            "agents",
            "test-agent",
            r#"---
title: Test Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

Basic template.
"#,
        )
        .await?;

    test_repo.commit_all("Add agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("test-agent", |d| {
            d.source("test-repo").path("agents/test-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Don't install (no lockfile) - this should fail validation
    // Validate with render flag (should fail without lockfile)
    let output = project.run_agpm(&["validate", "--render"])?;
    if !output.success {
        eprintln!("Validation failed without lockfile!");
        eprintln!("stdout: {}", output.stdout);
        eprintln!("stderr: {}", output.stderr);
    }

    // Should fail without lockfile
    assert!(!output.success, "Validation should fail without lockfile");

    Ok(())
}

/// Test template validation with JSON output.
#[tokio::test]
async fn test_validate_render_json_output() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a templated agent
    test_repo
        .add_resource(
            "agents",
            "json-agent",
            r#"---
title: JSON Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent uses templates.
"#,
        )
        .await?;

    test_repo.commit_all("Add agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("json-agent", |d| {
            d.source("test-repo").path("agents/json-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install first to generate lockfile
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Validate with render flag and JSON output
    let output = project.run_agpm(&["validate", "--render", "--format", "json"])?;
    assert!(output.success, "Validation should succeed");
    let stdout = &output.stdout;

    // Should be valid JSON
    let json: JsonValue = serde_json::from_str(stdout)?;

    // Check JSON structure
    assert!(
        json.get("templates_total").is_some() || json.get("templates_rendered").is_some(),
        "Should have templates fields"
    );
    assert!(json.get("errors").is_some(), "Should have errors field");

    Ok(())
}

/// Test that template checksum is enforced for cached templates.
#[tokio::test]
async fn test_templating_checksum_enforced() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a template
    test_repo
        .add_resource(
            "agents",
            "checksum-agent",
            r#"---
title: Checksum Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

Version: {{ agpm.resource.version }}
"#,
        )
        .await?;

    test_repo.commit_all("Add agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("checksum-agent", |d| {
            d.source("test-repo").path("agents/checksum-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install agent
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Check rendered content
    let agent_path = project.project_path().join(".claude/agents/agpm/checksum-agent.md");
    let content1 = fs::read_to_string(&agent_path).await?;

    assert!(content1.contains("# agents/checksum-agent"));
    assert!(content1.contains("Version: v1.0.0"));

    // Now update the template in repo
    test_repo
        .add_resource(
            "agents",
            "checksum-agent",
            r#"---
title: Checksum Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

Updated content!
Version: {{ agpm.resource.version }}
New line here.
"#,
        )
        .await?;

    test_repo.commit_all("Update agent")?;
    test_repo.tag_version("v1.0.1")?;

    // Create a new bare repo with updated tags (use unique name)
    let updated_bare_path = project.sources_path().join("test-repo-updated.git");

    // Clean up any existing bare repo
    if updated_bare_path.exists() {
        fs::remove_dir_all(&updated_bare_path).await?;
    }

    let _repo_url2 = test_repo.to_bare_repo(&updated_bare_path)?;
    let repo_url2 = format!("file://{}", normalize_path_for_storage(&updated_bare_path));

    // Update manifest to use new version
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url2)
        .add_agent("checksum-agent", |d| {
            d.source("test-repo").path("agents/checksum-agent.md").version("v1.0.1")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install again (should detect checksum change and re-render)
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Check new rendered content
    let content2 = fs::read_to_string(&agent_path).await?;

    assert!(content2.contains("# agents/checksum-agent"));
    assert!(content2.contains("Updated content!"));
    assert!(content2.contains("Version: v1.0.1"));
    assert!(content2.contains("New line here."));

    // Content should be different
    assert_ne!(content1, content2, "Content should have changed");

    Ok(())
}

/// Test that --frozen mode detects lockfile corruption via duplicate entries.
#[tokio::test]
async fn test_templating_lockfile_corruption_detection() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with template variables
    let template_content = r#"---
title: Test Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent is installed at: {{ agpm.resource.install_path }}
Version: {{ agpm.resource.version }}
Content: Template content with checksum verification
"#;

    test_repo.add_resource("agents", "test-agent", template_content).await?;

    test_repo.commit_all("Add test agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("test-agent", |d| {
            d.source("test-repo").path("agents/test-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install to generate lockfile and rendered content
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Initial install should succeed");

    // Read the installed file and verify it was rendered correctly
    let installed_path = project.project_path().join(".claude/agents/agpm/test-agent.md");
    let content = fs::read_to_string(&installed_path).await?;
    assert!(content.contains("Content: Template content with checksum verification"));
    assert!(content.contains("# agents/test-agent"));

    // Read the lockfile and manually corrupt it by adding a duplicate entry
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_content = fs::read_to_string(&lockfile_path).await?;

    // Find the agents section and duplicate the first agent entry to simulate corruption
    if let Some(agents_pos) = lockfile_content.find("[[agents]]") {
        let agent_section = &lockfile_content[agents_pos..];

        // Find the end of this agent entry (next [[agents]] or end of file)
        let next_section_start = agent_section.find("\n[[agents]]").unwrap_or(agent_section.len());

        let current_agent_entry = &agent_section[..next_section_start];

        // Create corrupted lockfile with duplicate entry
        let corrupted_lockfile = format!(
            "{}{}{}",
            &lockfile_content[..agents_pos],
            current_agent_entry,
            &lockfile_content[agents_pos..]
        );

        fs::write(&lockfile_path, corrupted_lockfile).await?;
    }

    // Run agpm install --frozen - this should detect the duplicate entry (corruption) and fail
    let output = project.run_agpm(&["install", "--frozen"])?;

    // The install should fail because the lockfile has duplicate entries (corruption)
    assert!(!output.success, "Frozen install should fail with lockfile corruption detected");

    let stderr = &output.stderr;
    assert!(
        stderr.contains("Lockfile has critical issues")
            || stderr.contains("corruption")
            || stderr.contains("duplicate")
            || stderr.contains("Duplicate"),
        "Should report lockfile corruption. Actual stderr: {}",
        stderr
    );

    // Now fix the lockfile by restoring original content and run normal install - should succeed
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Normal install should succeed and fix lockfile");

    // Verify the content is still correct
    let fixed_content = fs::read_to_string(&installed_path).await?;
    assert!(fixed_content.contains("Content: Template content with checksum verification"));

    Ok(())
}
