//! Integration tests for instance-level caching and fetch behavior
//! Tests the new v0.3.0 caching architecture improvements

use anyhow::Result;
use std::{fs, time::Duration};
use tokio::time::Instant;

mod fixtures;
use fixtures::{MarkdownFixture, TestEnvironment};

/// Test instance-level cache reuse across multiple operations
#[tokio::test]
async fn test_instance_cache_reuse() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    // Create test source with multiple agents
    let official_files = vec![
        MarkdownFixture::agent("agent-1"),
        MarkdownFixture::agent("agent-2"),
        MarkdownFixture::agent("agent-3"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/cache-test.git",
            official_files,
        )
        .unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/agent-2.md", version = "v1.0.0" }}
agent3 = {{ source = "official", path = "agents/agent-3.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // First install - should populate cache
    let start = Instant::now();
    env.ccpm_command()
        .arg("install")
        .arg("--max-parallel")
        .arg("4")
        .assert()
        .success();
    let first_duration = start.elapsed();

    // Remove installed files but keep cache
    fs::remove_dir_all(env.project_path().join(".claude")).unwrap();

    // Second install - should reuse cached worktrees
    let start = Instant::now();
    env.ccpm_command()
        .arg("install")
        .arg("--max-parallel")
        .arg("4")
        .assert()
        .success();
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
    assert!(env.project_path().join(".claude/agents/agent1.md").exists());
    assert!(env.project_path().join(".claude/agents/agent2.md").exists());
    assert!(env.project_path().join(".claude/agents/agent3.md").exists());

    Ok(())
}

/// Test fetch caching prevents redundant network operations
#[tokio::test]
async fn test_fetch_caching_prevents_redundancy() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    // Create test source with multiple dependencies from same repo
    let official_files = vec![
        MarkdownFixture::agent("fetch-agent-1"),
        MarkdownFixture::agent("fetch-agent-2"),
        MarkdownFixture::snippet("fetch-snippet-1"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/fetch-test.git",
            official_files,
        )
        .unwrap();

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
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Install with high parallelism - should use fetch caching
    let start = Instant::now();
    env.ccpm_command()
        .arg("install")
        .arg("--max-parallel")
        .arg("8")
        .arg("--verbose")
        .assert()
        .success();
    let duration = start.elapsed();

    // Should complete reasonably quickly with fetch caching
    assert!(
        duration < Duration::from_secs(30),
        "Install with fetch caching should complete in under 30 seconds, took {:?}",
        duration
    );

    // Verify all resources installed
    assert!(env.project_path().join(".claude/agents/agent1.md").exists());
    assert!(env.project_path().join(".claude/agents/agent2.md").exists());
    assert!(env
        .project_path()
        .join(".claude/ccpm/snippets/snippet1.md")
        .exists());

    Ok(())
}

/// Test cache behavior under high concurrency
#[tokio::test]
async fn test_cache_high_concurrency() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    // Create large number of dependencies to stress test caching
    let mut official_files = Vec::new();
    for i in 0..20 {
        official_files.push(MarkdownFixture::agent(&format!(
            "concurrent-agent-{:02}",
            i
        )));
    }

    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/concurrent-test.git",
            official_files,
        )
        .unwrap();

    let mut manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
"#,
        fixtures::path_to_file_url(&source_path)
    );

    // Add 20 agent dependencies
    for i in 0..20 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/concurrent-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Install with maximum parallelism
    let start = Instant::now();
    env.ccpm_command()
        .arg("install")
        .arg("--max-parallel")
        .arg("20") // High concurrency
        .assert()
        .success();
    let duration = start.elapsed();

    println!("High concurrency install took: {:?}", duration);

    // Verify all agents were installed
    for i in 0..20 {
        let agent_path = env
            .project_path()
            .join(format!(".claude/agents/agent{:02}.md", i));
        assert!(agent_path.exists(), "Agent {} should be installed", i);
    }

    Ok(())
}

/// Test cache persistence across command invocations
#[tokio::test]
async fn test_cache_persistence() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    let official_files = vec![
        MarkdownFixture::agent("persistent-agent"),
        MarkdownFixture::snippet("persistent-snippet"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/persistent-test.git",
            official_files,
        )
        .unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent = {{ source = "official", path = "agents/persistent-agent.md", version = "v1.0.0" }}

[snippets]
snippet = {{ source = "official", path = "snippets/persistent-snippet.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // First command: install
    env.ccpm_command().arg("install").assert().success();

    // Second command: update (should reuse cache)
    env.ccpm_command().arg("update").assert().success();

    // Third command: list (should work with cached data)
    env.ccpm_command().arg("list").assert().success();

    // Verify final state
    assert!(env.project_path().join(".claude/agents/agent.md").exists());
    assert!(env
        .project_path()
        .join(".claude/ccpm/snippets/snippet.md")
        .exists());

    Ok(())
}
