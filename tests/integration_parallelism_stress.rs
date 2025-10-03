//! Stress tests for new v0.3.0 parallelism features
//! Tests system behavior under various concurrency scenarios

use anyhow::Result;
use std::{fs, time::Duration};
use tokio::time::Instant;

mod common;
mod fixtures;
use common::TestProject;

/// Test system stability with very high --max-parallel values
#[tokio::test]
async fn test_extreme_parallelism() -> Result<()> {
    let project = TestProject::new().unwrap();

    // Create moderate number of dependencies
    let official_repo = project.create_source_repo("official").unwrap();
    for i in 0..10 {
        official_repo
            .add_resource(
                "agents",
                &format!("extreme-agent-{:02}", i),
                &format!("# Extreme Agent {:02}\n\nA test agent", i),
            )
            .unwrap();
    }
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let mut manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
"#,
        source_url
    );

    for i in 0..10 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/extreme-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    project.write_manifest(&manifest_content).unwrap();

    // Test with extremely high parallelism (should be throttled by system)
    let output = project
        .run_ccpm(&["install", "--max-parallel", "100"])
        .unwrap();
    assert!(output.success);

    // Verify all agents installed correctly despite high parallelism
    // Files use basename from path, not dependency name
    for i in 0..10 {
        assert!(
            project
                .project_path()
                .join(format!(".claude/agents/extreme-agent-{:02}.md", i))
                .exists()
        );
    }

    Ok(())
}

/// Test rapid sequential operations with caching
#[tokio::test]
async fn test_rapid_sequential_operations() -> Result<()> {
    let project = TestProject::new().unwrap();

    let official_repo = project.create_source_repo("official").unwrap();
    official_repo
        .add_resource("agents", "rapid-agent-1", "# Rapid Agent 1\n\nA test agent")
        .unwrap();
    official_repo
        .add_resource("agents", "rapid-agent-2", "# Rapid Agent 2\n\nA test agent")
        .unwrap();
    official_repo
        .add_resource(
            "snippets",
            "rapid-snippet",
            "# Rapid Snippet\n\nA test snippet",
        )
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
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
        source_url
    );

    project.write_manifest(&manifest_content).unwrap();

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
        let output = project.run_ccpm(&operation).unwrap();
        assert!(output.success);
    }
    let total_duration = start.elapsed();

    // All operations should complete quickly with caching
    assert!(
        total_duration < Duration::from_secs(60),
        "Rapid sequential operations should complete in under 60 seconds, took {:?}",
        total_duration
    );

    // Verify final state
    // Files use basename from path, not dependency name
    assert!(
        project
            .project_path()
            .join(".claude/agents/rapid-agent-1.md")
            .exists()
    );
    assert!(
        project
            .project_path()
            .join(".claude/agents/rapid-agent-2.md")
            .exists()
    );
    assert!(
        project
            .project_path()
            .join(".claude/ccpm/snippets/rapid-snippet.md")
            .exists()
    );

    Ok(())
}

/// Test mixed parallelism levels across operations
#[tokio::test]
async fn test_mixed_parallelism_levels() -> Result<()> {
    let project = TestProject::new().unwrap();

    let official_repo = project.create_source_repo("official").unwrap();
    for i in 0..8 {
        official_repo
            .add_resource(
                "agents",
                &format!("mixed-agent-{:02}", i),
                &format!("# Mixed Agent {:02}\n\nA test agent", i),
            )
            .unwrap();
    }
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let mut manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
"#,
        source_url
    );

    for i in 0..8 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/mixed-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    project.write_manifest(&manifest_content).unwrap();

    // Test various parallelism levels
    let parallelism_levels = [1, 2, 4, 8];

    for &level in &parallelism_levels {
        // Clean slate for each test
        let _ = fs::remove_dir_all(project.project_path().join(".claude"));

        let start = Instant::now();
        let output = project
            .run_ccpm(&["install", "--max-parallel", &level.to_string()])
            .unwrap();
        assert!(output.success);
        let duration = start.elapsed();

        println!("Parallelism level {}: {:?}", level, duration);

        // Verify installation
        // Files use basename from path, not dependency name
        for i in 0..8 {
            assert!(
                project
                    .project_path()
                    .join(format!(".claude/agents/mixed-agent-{:02}.md", i))
                    .exists()
            );
        }
    }

    Ok(())
}

/// Test parallelism with resource contention
#[tokio::test]
async fn test_parallelism_resource_contention() -> Result<()> {
    let project = TestProject::new().unwrap();

    // Create a single source repository with many agents
    let official_repo = project.create_source_repo("official").unwrap();
    for i in 0..15 {
        official_repo
            .add_resource(
                "agents",
                &format!("contention-agent-{:02}", i),
                &format!("# Contention Agent {:02}\n\nA test agent", i),
            )
            .unwrap();
    }
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let mut manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
"#,
        source_url
    );

    // All dependencies from same source - tests fetch caching and worktree management
    for i in 0..15 {
        manifest_content.push_str(&format!(
            "agent{:02} = {{ source = \"official\", path = \"agents/contention-agent-{:02}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    project.write_manifest(&manifest_content).unwrap();

    // High parallelism with single source should work efficiently
    let start = Instant::now();
    let output = project
        .run_ccpm(&["install", "--max-parallel", "10"])
        .unwrap();
    assert!(output.success);
    let duration = start.elapsed();

    // Should complete efficiently despite resource contention
    assert!(
        duration < Duration::from_secs(45),
        "Install with resource contention should complete in under 45 seconds, took {:?}",
        duration
    );

    // Verify all installations
    // Files use basename from path, not dependency name
    for i in 0..15 {
        assert!(
            project
                .project_path()
                .join(format!(".claude/agents/contention-agent-{:02}.md", i))
                .exists()
        );
    }

    Ok(())
}

/// Test system graceful handling of parallelism limits
#[tokio::test]
async fn test_parallelism_graceful_limits() -> Result<()> {
    let project = TestProject::new().unwrap();

    let official_repo = project.create_source_repo("official").unwrap();
    official_repo
        .add_resource("agents", "limit-agent-1", "# Limit Agent 1\n\nA test agent")
        .unwrap();
    official_repo
        .add_resource("agents", "limit-agent-2", "# Limit Agent 2\n\nA test agent")
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/limit-agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/limit-agent-2.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest_content).unwrap();

    // Test various edge cases that should be handled gracefully
    let test_cases = [
        ("1", "minimum parallelism"),
        ("2", "low parallelism"),
        ("50", "higher than work available"),
        ("1000", "extremely high parallelism"),
    ];

    for (max_parallel, description) in test_cases {
        // Clean for each test
        let _ = fs::remove_dir_all(project.project_path().join(".claude"));

        let output = project
            .run_ccpm(&["install", "--max-parallel", max_parallel])
            .unwrap();
        assert!(
            output.success,
            "Failed with {}: {}",
            max_parallel, description
        );

        // Verify installation regardless of parallelism setting
        // Files use basename from path, not dependency name
        assert!(
            project
                .project_path()
                .join(".claude/agents/limit-agent-1.md")
                .exists(),
            "Failed with {}: {}",
            max_parallel,
            description
        );
        assert!(
            project
                .project_path()
                .join(".claude/agents/limit-agent-2.md")
                .exists(),
            "Failed with {}: {}",
            max_parallel,
            description
        );
    }

    Ok(())
}
