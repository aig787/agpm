//! Integration tests for gitignore configuration validation.
//!
//! Tests the new behavior where AGPM:
//! - Validates that required .gitignore entries exist
//! - Warns about missing entries instead of auto-managing them
//! - Checks Claude Code settings configuration

use anyhow::Result;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test that config validation warns about missing gitignore entries
#[tokio::test]
async fn test_config_validation_warns_missing_gitignore() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("helper", "community", "agents/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should succeed but warn about missing gitignore entries
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Should warn about missing gitignore entry
    assert!(
        output.stderr.contains("missing from .gitignore")
            || output.stderr.contains(".claude/agents/agpm/"),
        "Should warn about missing gitignore entries. Stderr:\n{}",
        output.stderr
    );

    Ok(())
}

/// Test that no warning is shown when gitignore entries exist
#[tokio::test]
async fn test_config_validation_no_warning_with_gitignore() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("helper", "community", "agents/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Create .gitignore with required entries
    fs::write(
        project.project_path().join(".gitignore"),
        ".claude/agents/agpm/\n.claude/snippets/agpm/\n.agpm/\nagpm.private.toml\nagpm.private.lock\n",
    )
    .await?;

    // Run install - should succeed without gitignore warning
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Should NOT warn about missing gitignore entries
    assert!(
        !output.stderr.contains("missing from .gitignore"),
        "Should not warn when gitignore entries exist. Stderr:\n{}",
        output.stderr
    );

    Ok(())
}

/// Test config validation for multiple resource types
#[tokio::test]
async fn test_config_validation_multiple_resource_types() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "agent1", "# Agent 1").await?;
    source_repo.add_resource("snippets", "snippet1", "# Snippet 1").await?;
    source_repo.commit_all("Add resources")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("agent1", "community", "agents/agent1.md")
        .add_snippet("snippet1", |d| {
            d.source("community").path("snippets/snippet1.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install without gitignore
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Should warn about multiple missing entries
    let stderr = &output.stderr;
    assert!(
        stderr.contains("missing from .gitignore"),
        "Should warn about missing gitignore. Stderr:\n{}",
        stderr
    );

    Ok(())
}

/// Test that installed files go to /agpm/ subdirectory
#[tokio::test]
async fn test_files_installed_to_agpm_subdirectory() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("helper", "community", "agents/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Agent should be installed in /agpm/ subdirectory
    let agent_path = project.project_path().join(".claude/agents/agpm/helper.md");
    assert!(agent_path.exists(), "Agent should be installed at .claude/agents/agpm/helper.md");

    // Should NOT be at old path without /agpm/
    let old_path = project.project_path().join(".claude/agents/helper.md");
    assert!(!old_path.exists(), "Agent should NOT be at old path .claude/agents/helper.md");

    Ok(())
}

/// Test Claude Code settings warning
#[tokio::test]
async fn test_config_validation_claude_settings_warning() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("helper", "community", "agents/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install without Claude settings
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Should warn about Claude Code settings
    assert!(
        output.stderr.contains("Claude Code settings")
            || output.stderr.contains("respectGitIgnore"),
        "Should warn about Claude Code settings. Stderr:\n{}",
        output.stderr
    );

    Ok(())
}

/// Test no Claude settings warning when properly configured
#[tokio::test]
async fn test_config_validation_no_claude_warning_when_configured() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("helper", "community", "agents/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Create Claude Code settings with respectGitIgnore: false
    let claude_dir = project.project_path().join(".claude");
    fs::create_dir_all(&claude_dir).await?;
    fs::write(claude_dir.join("settings.json"), r#"{"respectGitIgnore": false}"#).await?;

    // Also create gitignore to avoid that warning
    fs::write(
        project.project_path().join(".gitignore"),
        ".claude/agents/agpm/\n.agpm/\nagpm.private.toml\nagpm.private.lock\n",
    )
    .await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Should NOT warn about Claude settings
    assert!(
        !output.stderr.contains("Claude Code settings not configured"),
        "Should not warn when Claude settings are configured. Stderr:\n{}",
        output.stderr
    );

    Ok(())
}

/// Test that gitignore = false in manifest disables validation
#[tokio::test]
async fn test_gitignore_false_disables_validation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create manifest with gitignore = false
    let manifest = format!(
        r#"
gitignore = false

[sources]
community = "{}"

[agents]
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Should NOT warn about gitignore when explicitly disabled
    assert!(
        !output.stderr.contains("missing from .gitignore"),
        "Should not warn about gitignore when disabled. Stderr:\n{}",
        output.stderr
    );

    Ok(())
}

/// Test that gitignore entries can use wildcard patterns
#[tokio::test]
async fn test_gitignore_wildcard_patterns() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "helper", "# Helper Agent").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("helper", "community", "agents/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Create .gitignore with wildcard pattern
    fs::write(
        project.project_path().join(".gitignore"),
        ".claude/*/agpm/\n.agpm/\nagpm.private.*\n",
    )
    .await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Should NOT warn - wildcard should match
    assert!(
        !output.stderr.contains("missing from .gitignore"),
        "Wildcard patterns should be accepted. Stderr:\n{}",
        output.stderr
    );

    Ok(())
}
