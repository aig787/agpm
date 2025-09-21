//! Stress tests for new v0.3.0 parallelism features
//! Tests system behavior under various concurrency scenarios

use anyhow::Result;
use std::{fs, time::Duration};
use tokio::time::Instant;

mod fixtures;
use fixtures::{MarkdownFixture, TestEnvironment};

/// Test system stability with very high --max-parallel values
#[tokio::test]
async fn test_extreme_parallelism() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    // Create moderate number of dependencies
    let mut official_files = Vec::new();
    for i in 0..10 {
        official_files.push(MarkdownFixture::agent(&format!("extreme-agent-{:02}", i)));
    }

    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/extreme-test.git",
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

    for i in 0..10 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/extreme-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Test with extremely high parallelism (should be throttled by system)
    env.ccpm_command()
        .arg("install")
        .arg("--max-parallel")
        .arg("100") // Much higher than available work
        .assert()
        .success();

    // Verify all agents installed correctly despite high parallelism
    for i in 0..10 {
        assert!(env
            .project_path()
            .join(format!(".claude/agents/agent{:02}.md", i))
            .exists());
    }

    Ok(())
}

/// Test rapid sequential operations with caching
#[tokio::test]
async fn test_rapid_sequential_operations() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    let official_files = vec![
        MarkdownFixture::agent("rapid-agent-1"),
        MarkdownFixture::agent("rapid-agent-2"),
        MarkdownFixture::snippet("rapid-snippet"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/rapid-test.git",
            official_files,
        )
        .unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/rapid-agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/rapid-agent-2.md", version = "v1.0.0" }}

[snippets]
snippet = {{ source = "official", path = "snippets/rapid-snippet.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Rapid sequence of operations
    let operations = [
        vec!["install", "--max-parallel", "4"],
        vec!["list"],
        vec!["update"],
        vec!["validate"],
        vec!["list", "--detailed"],
    ];

    let start = Instant::now();
    for operation in operations {
        let mut cmd = env.ccpm_command();
        for arg in operation {
            cmd.arg(arg);
        }
        cmd.assert().success();
    }
    let total_duration = start.elapsed();

    // All operations should complete quickly with caching
    assert!(
        total_duration < Duration::from_secs(60),
        "Rapid sequential operations should complete in under 60 seconds, took {:?}",
        total_duration
    );

    // Verify final state
    assert!(env.project_path().join(".claude/agents/agent1.md").exists());
    assert!(env.project_path().join(".claude/agents/agent2.md").exists());
    assert!(env
        .project_path()
        .join(".claude/ccpm/snippets/snippet.md")
        .exists());

    Ok(())
}

/// Test mixed parallelism levels across operations
#[tokio::test]
async fn test_mixed_parallelism_levels() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    let mut official_files = Vec::new();
    for i in 0..8 {
        official_files.push(MarkdownFixture::agent(&format!("mixed-agent-{:02}", i)));
    }

    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/mixed-test.git",
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

    for i in 0..8 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/mixed-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Test various parallelism levels
    let parallelism_levels = [1, 2, 4, 8];

    for &level in &parallelism_levels {
        // Clean slate for each test
        let _ = fs::remove_dir_all(env.project_path().join(".claude"));

        let start = Instant::now();
        env.ccpm_command()
            .arg("install")
            .arg("--max-parallel")
            .arg(level.to_string())
            .assert()
            .success();
        let duration = start.elapsed();

        println!("Parallelism level {}: {:?}", level, duration);

        // Verify installation
        for i in 0..8 {
            assert!(env
                .project_path()
                .join(format!(".claude/agents/agent{:02}.md", i))
                .exists());
        }
    }

    Ok(())
}

/// Test parallelism with resource contention
#[tokio::test]
async fn test_parallelism_resource_contention() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    // Create dependencies that all target the same source repository
    let mut official_files = Vec::new();
    for i in 0..15 {
        official_files.push(MarkdownFixture::agent(&format!(
            "contention-agent-{:02}",
            i
        )));
    }

    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/contention-test.git",
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

    // All dependencies from same source - tests fetch caching and worktree management
    for i in 0..15 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/contention-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // High parallelism with single source should work efficiently
    let start = Instant::now();
    env.ccpm_command()
        .arg("install")
        .arg("--max-parallel")
        .arg("10")
        .assert()
        .success();
    let duration = start.elapsed();

    // Should complete efficiently despite resource contention
    assert!(
        duration < Duration::from_secs(45),
        "Install with resource contention should complete in under 45 seconds, took {:?}",
        duration
    );

    // Verify all installations
    for i in 0..15 {
        assert!(env
            .project_path()
            .join(format!(".claude/agents/agent{:02}.md", i))
            .exists());
    }

    Ok(())
}

/// Test system graceful handling of parallelism limits
#[tokio::test]
async fn test_parallelism_graceful_limits() -> Result<()> {
    let env = TestEnvironment::new().unwrap();

    let official_files = vec![
        MarkdownFixture::agent("limit-agent-1"),
        MarkdownFixture::agent("limit-agent-2"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/limit-test.git",
            official_files,
        )
        .unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/limit-agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/limit-agent-2.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Test various edge cases that should be handled gracefully
    let test_cases = [
        ("1", "minimum parallelism"),
        ("2", "low parallelism"),
        ("50", "higher than work available"),
        ("1000", "extremely high parallelism"),
    ];

    for (max_parallel, description) in test_cases {
        // Clean for each test
        let _ = fs::remove_dir_all(env.project_path().join(".claude"));

        env.ccpm_command()
            .arg("install")
            .arg("--max-parallel")
            .arg(max_parallel)
            .assert()
            .success();

        // Verify installation regardless of parallelism setting
        assert!(
            env.project_path().join(".claude/agents/agent1.md").exists(),
            "Failed with {}: {}",
            max_parallel,
            description
        );
        assert!(
            env.project_path().join(".claude/agents/agent2.md").exists(),
            "Failed with {}: {}",
            max_parallel,
            description
        );
    }

    Ok(())
}
