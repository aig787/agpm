use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test that incremental update only re-resolves specified dependencies
#[tokio::test]
async fn test_incremental_resolves_only_specified_deps() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create remote repository with agent1 at v1.0.0
    let remote = project.create_source_repo("remote").await?;
    remote.add_resource("agents", "agent1", "---\nname: agent1\n---\nOriginal agent1").await?;
    remote.commit_all("Initial agent1")?;
    remote.tag_version("v1.0.0")?;

    // Add agent2 at v1.1.0
    remote.add_resource("agents", "agent2", "---\nname: agent2\n---\nOriginal agent2").await?;
    remote.commit_all("Add agent2")?;
    remote.tag_version("v1.1.0")?;

    // Create manifest with both agents
    let remote_url = remote.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("remote", &remote_url)
        .add_agent("agent1", |d| d.source("remote").path("agents/agent1.md").version("v1.0.0"))
        .add_agent("agent2", |d| d.source("remote").path("agents/agent2.md").version("v1.1.0"))
        .build();

    project.write_manifest(&manifest).await?;

    // Initial install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify both agents installed
    let agent1_path = project.project_path().join(".claude/agents/agpm/agent1.md");
    let agent2_path = project.project_path().join(".claude/agents/agpm/agent2.md");
    assert!(tokio::fs::metadata(&agent1_path).await.is_ok(), "Agent1 should exist");
    assert!(tokio::fs::metadata(&agent2_path).await.is_ok(), "Agent2 should exist");

    // Update agent1 in remote to v1.0.1
    remote.add_resource("agents", "agent1", "---\nname: agent1\n---\nUpdated agent1").await?;
    remote.commit_all("Update agent1")?;
    remote.tag_version("v1.0.1")?;

    // Update agent2 in remote to v1.1.1
    remote.add_resource("agents", "agent2", "---\nname: agent2\n---\nUpdated agent2").await?;
    remote.commit_all("Update agent2")?;
    remote.tag_version("v1.1.1")?;

    // Update the bare repository to include new tags
    // This is necessary because the bare repo was created before we added the new tags
    let bare_path = project.sources_path().join("remote.git");
    std::fs::remove_dir_all(&bare_path)?;
    remote.to_bare_repo(&bare_path).await?;

    // Update manifest to allow patch updates
    // Use tilde (~) constraints to allow only patch-level updates (1.0.x, 1.1.x)
    let manifest = ManifestBuilder::new()
        .add_source("remote", &remote_url)
        .add_agent("agent1", |d| d.source("remote").path("agents/agent1.md").version("~v1.0.0"))
        .add_agent("agent2", |d| d.source("remote").path("agents/agent2.md").version("~v1.1.0"))
        .build();

    project.write_manifest(&manifest).await?;

    // Update only agent1
    let output = project.run_agpm(&["update", "agent1"])?;
    assert!(output.success, "Update should succeed. Stderr: {}", output.stderr);

    // Read lockfile to verify versions
    let lockfile = project.load_lockfile()?;

    // agent1 should be at v1.0.1
    let agent1 =
        lockfile.agents.iter().find(|a| a.name == "agents/agent1").expect("agent1 not found");
    assert_eq!(agent1.version, Some("v1.0.1".to_string()));

    // agent2 should still be at v1.1.0 (unchanged)
    let agent2 =
        lockfile.agents.iter().find(|a| a.name == "agents/agent2").expect("agent2 not found");
    assert_eq!(agent2.version, Some("v1.1.0".to_string()));

    Ok(())
}

/// Test that incremental update with multiple dependencies works correctly
#[tokio::test]
async fn test_incremental_multiple_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create remote repository with initial resources at v1.0.0
    let remote = project.create_source_repo("remote").await?;
    remote.add_resource("agents", "agent1", "---\nname: agent1\n---\nOriginal agent1").await?;
    remote.add_resource("agents", "agent2", "---\nname: agent2\n---\nOriginal agent2").await?;
    remote.add_resource("snippets", "snippet1", "# Snippet 1\nOriginal snippet1").await?;
    remote.commit_all("Initial resources")?;
    remote.tag_version("v1.0.0")?;

    // Create manifest
    let remote_url = remote.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("remote", &remote_url)
        .add_standard_agent("agent1", "remote", "agents/agent1.md")
        .add_standard_agent("agent2", "remote", "agents/agent2.md")
        .add_standard_snippet("snippet1", "remote", "snippets/snippet1.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Initial install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Update all resources to v1.1.0
    remote.add_resource("agents", "agent1", "---\nname: agent1\n---\nUpdated agent1").await?;
    remote.add_resource("agents", "agent2", "---\nname: agent2\n---\nUpdated agent2").await?;
    remote.add_resource("snippets", "snippet1", "# Snippet 1\nUpdated snippet1").await?;
    remote.commit_all("Update all")?;
    remote.tag_version("v1.1.0")?;

    // Update the bare repository to include new tags
    let bare_path = project.sources_path().join("remote.git");
    std::fs::remove_dir_all(&bare_path)?;
    remote.to_bare_repo(&bare_path).await?;

    // Update manifest to allow minor version updates
    // Use caret (^) to allow minor updates (1.0.0 -> 1.1.0)
    // For incremental update test, only agent1 and snippet1 should update
    let manifest = ManifestBuilder::new()
        .add_source("remote", &remote_url)
        .add_agent("agent1", |d| d.source("remote").path("agents/agent1.md").version("^v1.0.0"))
        .add_agent("agent2", |d| d.source("remote").path("agents/agent2.md").version("^v1.0.0"))
        .add_snippet("snippet1", |d| {
            d.source("remote").path("snippets/snippet1.md").version("^v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Update agent1 and snippet1, but not agent2
    let output = project.run_agpm(&["update", "agent1", "snippet1"])?;
    assert!(
        output.success,
        "Update should succeed. Stderr: {}\nStdout: {}",
        output.stderr, output.stdout
    );

    // Read lockfile to verify versions
    let lockfile_content = project.read_lockfile().await?;
    let lockfile = project.load_lockfile()?;

    // agent1 should be updated
    let agent1 =
        lockfile.agents.iter().find(|a| a.name == "agents/agent1").expect("agent1 not found");
    assert_eq!(
        agent1.version,
        Some("v1.1.0".to_string()),
        "agent1 should be v1.1.0, got {:?}. Lockfile:\n{}",
        agent1.version,
        lockfile_content
    );

    // agent2 should be unchanged
    let agent2 =
        lockfile.agents.iter().find(|a| a.name == "agents/agent2").expect("agent2 not found");
    assert_eq!(agent2.version, Some("v1.0.0".to_string()));

    // snippet1 should be updated
    let snippet1 = lockfile
        .snippets
        .iter()
        .find(|s| s.name == "snippets/snippet1")
        .expect("snippet1 not found");
    assert_eq!(snippet1.version, Some("v1.1.0".to_string()));

    Ok(())
}

/// Test that incremental update with non-existent dependency is handled gracefully
///
/// Current behavior: Does full resolution and returns existing lockfile unchanged.
/// Future enhancement: Could show user-friendly warning about non-existent dependency.
#[tokio::test]
async fn test_incremental_nonexistent_dependency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create remote repository with agent1 at v1.0.0
    let remote = project.create_source_repo("remote").await?;
    remote.add_resource("agents", "agent1", "---\nname: agent1\n---\nAgent content").await?;
    remote.commit_all("Initial commit")?;
    remote.tag_version("v1.0.0")?;

    // Create manifest
    let remote_url = remote.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("remote", &remote_url)
        .add_standard_agent("agent1", "remote", "agents/agent1.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Initial install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Get initial lockfile
    let initial_lockfile = project.read_lockfile().await?;

    // Try to update a non-existent dependency
    // Current behavior: Should succeed and return unchanged lockfile
    let output = project.run_agpm(&["update", "nonexistent"])?;
    assert!(output.success, "Update should succeed. Stderr: {}", output.stderr);

    // Lockfile should be unchanged
    let final_lockfile = project.read_lockfile().await?;
    assert_eq!(
        initial_lockfile, final_lockfile,
        "Lockfile should be unchanged when updating non-existent dependency"
    );

    Ok(())
}
