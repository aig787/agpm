// Deadlock prevention tests for lock ordering
//
// Tests that the lock ordering mechanism prevents deadlocks during
// parallel dependency resolution when multiple repositories are involved.
// This is a critical test for the enhanced lock ordering implementation.

use anyhow::Result;
use std::time::Instant;

use crate::common::{ManifestBuilder, TestProject};

/// Test deadlock prevention with multiple repositories in parallel
///
/// This test creates a scenario where multiple dependencies could potentially
/// cause deadlocks if locks weren't acquired in alphabetical order.
#[tokio::test]
async fn test_deadlock_prevention_multiple_repos() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create multiple source repositories in alphabetical order
    let repos = vec!["alpha-repo", "beta-repo", "gamma-repo", "delta-repo"];
    let mut source_repos = Vec::new();

    for repo_name in &repos {
        let repo = project.create_source_repo(repo_name).await?;
        source_repos.push(repo);
    }

    // Add dependencies to each repository that could cause lock conflicts
    for (i, repo) in source_repos.iter().enumerate() {
        // Create agents with cross-repository dependencies
        repo.add_resource(
            "agents",
            &format!("agent-{}", i),
            format!(
                r#"---
name: Agent {} from {}
model: claude-3-sonnet
dependencies:
  agents:
    - path: helper-from-{}
      source: {}
      version: v1.0.0
---
# Agent {} from {}

This agent depends on a helper from another repository.
"#,
                i,
                repos[i],
                repos[(i + 1) % repos.len()], // Next repo in circular manner
                repos[(i + 1) % repos.len()],
                i,
                repos[i]
            )
            .as_str(),
        )
        .await?;

        // Add the helper that other agents depend on
        repo.add_resource(
            "agents",
            "helper",
            format!(
                r#"---
name: Helper from {}
model: claude-3-haiku
---
# Helper from {}

This is a helper agent with no dependencies.
"#,
                repos[i], repos[i]
            )
            .as_str(),
        )
        .await?;
    }

    // Commit and tag all repositories
    for repo in &source_repos {
        repo.commit_all("Initial commit")?;
        repo.tag_version("v1.0.0")?;
    }

    // Create manifest with dependencies from all repositories
    let mut manifest_builder = ManifestBuilder::new();

    // Add sources using bare_file_url
    for (i, repo_name) in repos.iter().enumerate() {
        let source_url = source_repos[i].bare_file_url(project.sources_path())?;
        manifest_builder = manifest_builder.add_source(repo_name, &source_url);
    }

    // Add dependencies from all repositories (potential deadlock scenario)
    for (i, repo_name) in repos.iter().enumerate() {
        manifest_builder = manifest_builder.add_agent(&format!("agent-{}", i), |builder| {
            builder.source(repo_name).path(&format!("agents/agent-{}.md", i))
        });
    }

    let manifest = manifest_builder.build();
    project.write_manifest(&manifest).await?;

    // Measure resolution time
    let start_time = Instant::now();

    // Run install - this should not deadlock
    let output = project.run_agpm(&["install"])?;

    let resolution_time = start_time.elapsed();

    // Verify install completed successfully
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify resolution completed in reasonable time
    assert!(
        resolution_time.as_secs() < 30, // Should complete quickly, not hang
        "Resolution took too long: {} seconds - possible deadlock",
        resolution_time.as_secs()
    );

    // Verify all agents were installed
    for i in 0..repos.len() {
        let agent_path = project.project_path().join(format!(".claude/agents/agent-{}.md", i));
        assert!(
            tokio::fs::metadata(&agent_path).await.is_ok(),
            "Agent {} should be installed at {:?}",
            i,
            agent_path
        );
    }

    println!("✅ Deadlock prevention test passed in {:?}", resolution_time);
    Ok(())
}

/// Test simple concurrent dependency resolution
///
/// This test verifies that multiple dependencies can be resolved concurrently
/// without causing lock conflicts or deadlocks.
#[tokio::test]
async fn test_simple_concurrent_resolution() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a repository with multiple independent agents
    let repo = project.create_source_repo("multi-agent-repo").await?;

    // Add multiple agents with no dependencies (can be resolved concurrently)
    for i in 0..10 {
        repo.add_resource(
            "agents",
            &format!("agent-{}", i),
            format!(
                r#"---
name: Agent {}
model: claude-3-sonnet
---
# Agent {}

This is an independent agent that can be resolved concurrently.
"#,
                i, i
            )
            .as_str(),
        )
        .await?;
    }

    repo.commit_all("Initial commit")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest with all agents
    let source_url = repo.bare_file_url(project.sources_path())?;
    let mut manifest_builder = ManifestBuilder::new().add_source("multi-agent-repo", &source_url);

    for i in 0..10 {
        manifest_builder = manifest_builder.add_agent(&format!("agent-{}", i), |builder| {
            builder.source("multi-agent-repo").path(&format!("agents/agent-{}.md", i))
        });
    }

    let manifest = manifest_builder.build();
    project.write_manifest(&manifest).await?;

    // Measure resolution time
    let start_time = Instant::now();

    // Run install - should resolve all agents concurrently
    let output = project.run_agpm(&["install"])?;
    let resolution_time = start_time.elapsed();

    // Verify install completed successfully
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify all agents were installed
    for i in 0..10 {
        let agent_path = project.project_path().join(format!(".claude/agents/agent-{}.md", i));
        assert!(
            tokio::fs::metadata(&agent_path).await.is_ok(),
            "Agent {} should be installed at {:?}",
            i,
            agent_path
        );
    }

    println!("✅ Simple concurrent resolution test passed in {:?}", resolution_time);
    Ok(())
}
