//! Integration tests for private dependency functionality.
//!
//! These tests verify end-to-end behavior of private dependencies:
//! - Private sources and dependencies in agpm.private.toml
//! - Separate lockfile (agpm.private.lock) for private resources
//! - Private dependencies installed to private/ subdirectory
//! - Source merging and shadowing behavior
//! - Validation of private source references

use anyhow::Result;
use tokio::fs;

use crate::common::{FileAssert, TestProject};
use crate::test_config;

/// Helper to create a test repository with a basic agent
async fn create_repo_with_agent(
    project: &TestProject,
    name: &str,
    agent_name: &str,
) -> Result<String> {
    let repo = project.create_source_repo(name).await?;

    let agent_content = format!(
        r#"---
model: gpt-4
---
# {} Agent

This is a test agent from {} repository.
"#,
        agent_name, name
    );

    repo.add_resource("agents", agent_name, &agent_content).await?;
    repo.commit_all("Initial commit")?;
    repo.tag_version("v1.0.0")?;

    let url = repo.bare_file_url(project.sources_path()).await?;
    Ok(url)
}

#[tokio::test]
async fn test_private_deps_install_to_private_subdirectory() -> Result<()> {
    test_config::init_test_env();
    let project = TestProject::new().await?;

    // Create a public source repository
    let public_url = create_repo_with_agent(&project, "public-repo", "public-agent").await?;

    // Create a private source repository
    let private_url = create_repo_with_agent(&project, "private-repo", "private-agent").await?;

    // Create project manifest with public dependency
    let manifest = format!(
        r#"[sources]
public = "{}"

[agents]
public-agent = {{ source = "public", path = "agents/public-agent.md", version = "v1.0.0" }}
"#,
        public_url
    );

    project.write_manifest(&manifest).await?;

    // Create private manifest with private source and dependency
    let private_manifest = format!(
        r#"[sources]
private = "{}"

[agents]
private-agent = {{ source = "private", path = "agents/private-agent.md", version = "v1.0.0" }}
"#,
        private_url
    );

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify public agent is installed to regular path
    let public_agent_path = project.project_path().join(".claude/agents/agpm/public-agent.md");
    FileAssert::exists(&public_agent_path).await;

    // Verify private agent is installed to private/ subdirectory
    let private_agent_path =
        project.project_path().join(".claude/agents/agpm/private/private-agent.md");
    FileAssert::exists(&private_agent_path).await;

    // Verify both lockfiles are created
    let public_lockfile = project.project_path().join("agpm.lock");
    let private_lockfile = project.project_path().join("agpm.private.lock");

    FileAssert::exists(&public_lockfile).await;
    FileAssert::exists(&private_lockfile).await;

    // Verify public lockfile contains only public resources
    let public_lock_content = fs::read_to_string(&public_lockfile).await?;
    assert!(
        public_lock_content.contains("public-agent"),
        "Public lockfile should contain public-agent"
    );
    assert!(
        !public_lock_content.contains("private-agent"),
        "Public lockfile should NOT contain private-agent"
    );

    // Verify private lockfile contains only private resources
    let private_lock_content = fs::read_to_string(&private_lockfile).await?;
    assert!(
        private_lock_content.contains("private-agent"),
        "Private lockfile should contain private-agent"
    );

    Ok(())
}

#[tokio::test]
async fn test_private_source_shadows_public_source() -> Result<()> {
    test_config::init_test_env();
    let project = TestProject::new().await?;

    // Create public source repository
    let public_url = create_repo_with_agent(&project, "public-shared", "shared-agent").await?;

    // Create private source repository with same name
    let private_url = create_repo_with_agent(&project, "private-shared", "different-agent").await?;

    // Create project manifest with source named "shared"
    let manifest = format!(
        r#"[sources]
shared = "{}"

[agents]
from-shared = {{ source = "shared", path = "agents/shared-agent.md", version = "v1.0.0" }}
"#,
        public_url
    );

    project.write_manifest(&manifest).await?;

    // Create private manifest that shadows "shared" source
    let private_manifest = format!(
        r#"[sources]
shared = "{}"
"#,
        private_url
    );

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await?;

    // Run install - should fail because private source shadows public
    // but doesn't have the path referenced by public dependency
    let output = project.run_agpm(&["install"])?;

    // This should fail because the private source doesn't have agents/shared-agent.md
    assert!(
        !output.success,
        "Install should fail when private source shadows public but doesn't have required path"
    );

    Ok(())
}

#[tokio::test]
async fn test_private_deps_validation_for_undefined_source() -> Result<()> {
    test_config::init_test_env();
    let project = TestProject::new().await?;

    // Create project manifest with no sources
    let manifest = r#"# Empty manifest with no sources
"#;

    project.write_manifest(manifest).await?;

    // Create private manifest referencing undefined source
    let private_manifest = r#"[agents]
my-agent = { source = "nonexistent", path = "agents/test.md", version = "v1.0.0" }
"#;

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await?;

    // Run install - should fail validation
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail for undefined source in private manifest");
    assert!(
        output.stderr.contains("nonexistent") || output.stdout.contains("nonexistent"),
        "Error should mention the undefined source name"
    );

    Ok(())
}

#[tokio::test]
async fn test_private_lockfile_roundtrip() -> Result<()> {
    test_config::init_test_env();
    let project = TestProject::new().await?;

    // Create a private source repository
    let private_url = create_repo_with_agent(&project, "private-repo", "my-agent").await?;

    // Create empty project manifest
    let manifest = r#"# Empty manifest
"#;
    project.write_manifest(manifest).await?;

    // Create private manifest with dependency
    let private_manifest = format!(
        r#"[sources]
private = "{}"

[agents]
my-private-agent = {{ source = "private", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        private_url
    );

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await?;

    // First install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify private lockfile is created
    let private_lockfile = project.project_path().join("agpm.private.lock");
    FileAssert::exists(&private_lockfile).await;

    // Read the private lockfile content
    let first_private_lock = fs::read_to_string(&private_lockfile).await?;

    // Run install again (should be fast due to lockfile)
    let output2 = project.run_agpm(&["install"])?;
    output2.assert_success();

    // Verify private lockfile is unchanged
    let second_private_lock = fs::read_to_string(&private_lockfile).await?;
    assert_eq!(
        first_private_lock, second_private_lock,
        "Private lockfile should be stable between runs"
    );

    Ok(())
}

#[tokio::test]
async fn test_mixed_public_and_private_deps() -> Result<()> {
    test_config::init_test_env();
    let project = TestProject::new().await?;

    // Create public source repository
    let public_url = create_repo_with_agent(&project, "public-repo", "public-agent").await?;

    // Create private source repository
    let private_url = create_repo_with_agent(&project, "private-repo", "private-agent").await?;

    // Create project manifest with public dependency
    let manifest = format!(
        r#"[sources]
public = "{}"

[agents]
team-agent = {{ source = "public", path = "agents/public-agent.md", version = "v1.0.0" }}
"#,
        public_url
    );

    project.write_manifest(&manifest).await?;

    // Create private manifest with private dependency
    let private_manifest = format!(
        r#"[sources]
private = "{}"

[agents]
my-agent = {{ source = "private", path = "agents/private-agent.md", version = "v1.0.0" }}
"#,
        private_url
    );

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify both agents are installed
    let public_agent = project.project_path().join(".claude/agents/agpm/public-agent.md");
    let private_agent = project.project_path().join(".claude/agents/agpm/private/private-agent.md");

    FileAssert::exists(&public_agent).await;
    FileAssert::exists(&private_agent).await;

    // Verify list command shows both
    let list_output = project.run_agpm(&["list"])?;
    list_output.assert_success();

    // Both agents should appear in the list
    assert!(
        list_output.stdout.contains("team-agent") || list_output.stdout.contains("public-agent"),
        "List should show public agent"
    );
    assert!(
        list_output.stdout.contains("my-agent") || list_output.stdout.contains("private-agent"),
        "List should show private agent"
    );

    Ok(())
}

#[tokio::test]
async fn test_private_manifest_cannot_have_tools() -> Result<()> {
    test_config::init_test_env();
    let project = TestProject::new().await?;

    // Create project manifest
    let manifest = r#"[sources]
test = "https://github.com/example/test.git"
"#;
    project.write_manifest(manifest).await?;

    // Create private manifest with tools section - should fail
    let private_manifest = r#"
[tools.claude-code]
path = ".claude"
enabled = true

[tools.claude-code.resources.agents]
path = "agents"
"#;

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await?;

    // Run install - should fail because private manifest has tools
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail when private manifest has tools");
    assert!(
        output.stderr.contains("tools") || output.stdout.contains("tools"),
        "Error should mention 'tools' are not allowed in private manifest"
    );

    Ok(())
}

#[tokio::test]
async fn test_frozen_install_respects_private_lockfile() -> Result<()> {
    test_config::init_test_env();
    let project = TestProject::new().await?;

    // Create private source repository
    let private_url = create_repo_with_agent(&project, "private-repo", "my-agent").await?;

    // Create empty project manifest
    let manifest = r#"# Empty manifest
"#;
    project.write_manifest(manifest).await?;

    // Create private manifest
    let private_manifest = format!(
        r#"[sources]
private = "{}"

[agents]
my-agent = {{ source = "private", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        private_url
    );

    let private_path = project.project_path().join("agpm.private.toml");
    fs::write(&private_path, private_manifest).await?;

    // First install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Delete the installed file to simulate need for reinstall
    let agent_path = project.project_path().join(".claude/agents/agpm/private/my-agent.md");
    if agent_path.exists() {
        fs::remove_file(&agent_path).await?;
    }

    // Run install --frozen (should reinstall from lockfile)
    let output2 = project.run_agpm(&["install", "--frozen"])?;
    output2.assert_success();

    // Verify agent is reinstalled
    FileAssert::exists(&agent_path).await;

    Ok(())
}
