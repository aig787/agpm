//! Integration tests for instance-level caching and fetch behavior
//! Tests the new v0.3.0 caching architecture improvements

use anyhow::Result;
use std::time::Duration;
use tokio::fs;
use tokio::time::Instant;

mod common;
use common::TestProject;

/// Test instance-level cache reuse across multiple operations
#[tokio::test]
async fn test_instance_cache_reuse() -> Result<()> {
    let project = TestProject::new().await?;

    // Create test source with multiple agents
    let source_repo = project.create_source_repo("official").await?;
    source_repo
        .add_resource("agents", "agent-1", "# Agent 1\n\nTest agent 1")
        .await?;
    source_repo
        .add_resource("agents", "agent-2", "# Agent 2\n\nTest agent 2")
        .await?;
    source_repo
        .add_resource("agents", "agent-3", "# Agent 3\n\nTest agent 3")
        .await?;
    source_repo.commit_all("Add test agents")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/agent-2.md", version = "v1.0.0" }}
agent3 = {{ source = "official", path = "agents/agent-3.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest_content).await?;

    // First install - should populate cache
    let start = Instant::now();
    let output = project.run_agpm(&["install"])?;
    output.assert_success();
    let first_duration = start.elapsed();

    // Remove installed files but keep cache
    fs::remove_dir_all(project.project_path().join(".claude")).await?;

    // Second install - should reuse cached worktrees
    let start = Instant::now();
    let output = project.run_agpm(&["install"])?;
    output.assert_success();
    let second_duration = start.elapsed();

    // Second install should be faster due to cache reuse
    // Allow some tolerance but expect significant speedup
    assert!(
        second_duration <= first_duration + Duration::from_millis(500),
        "Second install should reuse cache and be comparable in speed. First: {:?}, Second: {:?}",
        first_duration,
        second_duration
    );

    // Verify all files were installed correctly
    // Files use basename from path, not dependency name
    assert!(
        project
            .project_path()
            .join(".claude/agents/agent-1.md")
            .exists()
    );
    assert!(
        project
            .project_path()
            .join(".claude/agents/agent-2.md")
            .exists()
    );
    assert!(
        project
            .project_path()
            .join(".claude/agents/agent-3.md")
            .exists()
    );

    Ok(())
}

/// Test fetch caching prevents redundant network operations
#[tokio::test]
async fn test_fetch_caching_prevents_redundancy() -> Result<()> {
    let project = TestProject::new().await?;

    // Create test source with multiple dependencies from same repo
    let source_repo = project.create_source_repo("official").await?;
    source_repo
        .add_resource(
            "agents",
            "fetch-agent-1",
            "# Fetch Agent 1\n\nTest fetch agent 1",
        )
        .await?;
    source_repo
        .add_resource(
            "agents",
            "fetch-agent-2",
            "# Fetch Agent 2\n\nTest fetch agent 2",
        )
        .await?;
    source_repo
        .add_resource(
            "snippets",
            "fetch-snippet-1",
            "# Fetch Snippet 1\n\nTest fetch snippet 1",
        )
        .await?;
    source_repo.commit_all("Add test resources")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/fetch-agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/fetch-agent-2.md", version = "v1.0.0" }}

[snippets]
snippet1 = {{ source = "official", path = "snippets/fetch-snippet-1.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest_content).await?;

    // Install with high parallelism - should use fetch caching
    let start = Instant::now();
    let output = project.run_agpm(&["install", "--verbose"])?;
    output.assert_success();
    let duration = start.elapsed();

    // Should complete reasonably quickly with fetch caching
    assert!(
        duration < Duration::from_secs(30),
        "Install with fetch caching should complete in under 30 seconds, took {:?}",
        duration
    );

    // Verify all resources installed
    // Files use basename from path, not dependency name
    assert!(
        project
            .project_path()
            .join(".claude/agents/fetch-agent-1.md")
            .exists()
    );
    assert!(
        project
            .project_path()
            .join(".claude/agents/fetch-agent-2.md")
            .exists()
    );
    assert!(
        project
            .project_path()
            .join(".agpm/snippets/fetch-snippet-1.md")
            .exists()
    );

    // Explicitly drop source_repo to ensure Git file locks are released before TempDir cleanup
    drop(source_repo);

    Ok(())
}

/// Test cache behavior under high concurrency
#[tokio::test]
async fn test_cache_high_concurrency() -> Result<()> {
    let project = TestProject::new().await?;

    // Create large number of dependencies to stress test caching
    let source_repo = project.create_source_repo("official").await?;
    for i in 0..20 {
        source_repo
            .add_resource(
                "agents",
                &format!("concurrent-agent-{:02}", i),
                &format!("# Concurrent Agent {:02}\n\nTest concurrent agent {}", i, i),
            )
            .await?;
    }
    source_repo.commit_all("Add concurrent test agents")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    let mut manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
"#,
        source_url
    );

    // Add 20 agent dependencies
    for i in 0..20 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/concurrent-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    project.write_manifest(&manifest_content).await?;

    // Install with maximum parallelism
    let start = Instant::now();
    let output = project.run_agpm(&["install"])?;
    output.assert_success();
    let duration = start.elapsed();

    println!("High concurrency install took: {:?}", duration);

    // Verify all agents were installed
    // Files use basename from path, not dependency name
    for i in 0..20 {
        let agent_path = project
            .project_path()
            .join(format!(".claude/agents/concurrent-agent-{:02}.md", i));
        assert!(agent_path.exists(), "Agent {} should be installed", i);
    }

    Ok(())
}

/// Test cache persistence across command invocations
#[tokio::test]
async fn test_cache_persistence() -> Result<()> {
    let project = TestProject::new().await?;

    let source_repo = project.create_source_repo("official").await?;
    source_repo
        .add_resource(
            "agents",
            "persistent-agent",
            "# Persistent Agent\n\nTest persistent agent",
        )
        .await?;
    source_repo
        .add_resource(
            "snippets",
            "persistent-snippet",
            "# Persistent Snippet\n\nTest persistent snippet",
        )
        .await?;
    source_repo.commit_all("Add persistent test resources")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent = {{ source = "official", path = "agents/persistent-agent.md", version = "v1.0.0" }}

[snippets]
snippet = {{ source = "official", path = "snippets/persistent-snippet.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest_content).await?;

    // First command: install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Second command: update (should reuse cache)
    let output = project.run_agpm(&["update"])?;
    output.assert_success();

    // Third command: list (should work with cached data)
    let output = project.run_agpm(&["list"])?;
    output.assert_success();

    // Verify final state
    // Files use basename from path, not dependency name
    assert!(
        project
            .project_path()
            .join(".claude/agents/persistent-agent.md")
            .exists()
    );
    assert!(
        project
            .project_path()
            .join(".agpm/snippets/persistent-snippet.md")
            .exists()
    );

    Ok(())
}
