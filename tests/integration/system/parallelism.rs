//! Integration tests for --max-parallel flag behavior
//! Tests the new v0.3.0 concurrency control features

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test --max-parallel flag with various values
#[tokio::test]
async fn test_max_parallel_flag_values() {
    let project = TestProject::new().await.unwrap();

    // Create a simple test source
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource("agents", "test-agent-1", "# Test Agent 1\n\nA test agent")
        .await
        .unwrap();
    official_repo
        .add_resource("agents", "test-agent-2", "# Test Agent 2\n\nA test agent")
        .await
        .unwrap();
    official_repo
        .add_resource("agents", "test-agent-3", "# Test Agent 3\n\nA test agent")
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest = ManifestBuilder::new()
        .add_source("official", &source_url)
        .add_standard_agent("agent1", "official", "agents/test-agent-1.md")
        .add_standard_agent("agent2", "official", "agents/test-agent-2.md")
        .add_standard_agent("agent3", "official", "agents/test-agent-3.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    // Test different --max-parallel values
    for max_parallel in [1, 2, 4, 8] {
        let output =
            project.run_agpm(&["install", "--max-parallel", &max_parallel.to_string()]).unwrap();
        assert!(
            output.success,
            "Install failed with max_parallel={}: {}",
            max_parallel, output.stderr
        );

        // Verify installation worked
        // Files use basename from path, not dependency name
        assert!(project.project_path().join(".claude/agents/test-agent-1.md").exists());
        assert!(project.project_path().join(".claude/agents/test-agent-2.md").exists());
        assert!(project.project_path().join(".claude/agents/test-agent-3.md").exists());

        // Clean up for next iteration
        let _ = fs::remove_dir_all(project.project_path().join(".claude")).await;
    }
}

/// Test --max-parallel with invalid values
#[tokio::test]
async fn test_max_parallel_invalid_values() {
    let project = TestProject::new().await.unwrap();

    // Test zero value
    let output = project.run_agpm(&["install", "--max-parallel", "0"]).unwrap();
    assert!(!output.success);

    // Test negative value
    let output = project.run_agpm(&["install", "--max-parallel", "-1"]).unwrap();
    assert!(!output.success);

    // Test non-numeric value
    let output = project.run_agpm(&["install", "--max-parallel", "abc"]).unwrap();
    assert!(!output.success);
}

/// Test default parallelism when --max-parallel is not specified
#[tokio::test]
async fn test_default_parallelism() {
    let project = TestProject::new().await.unwrap();

    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource("agents", "test-agent", "# Test Agent\n\nA test agent")
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest = ManifestBuilder::new()
        .add_source("official", &source_url)
        .add_standard_agent("agent", "official", "agents/test-agent.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    // Install without --max-parallel flag (should use default)
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("Installing") || output.stdout.contains("Installed"));

    // Files use basename from path, not dependency name
    assert!(project.project_path().join(".claude/agents/test-agent.md").exists());
}

/// Test --max-parallel flag on install command only (update doesn't support it)
#[tokio::test]
async fn test_max_parallel_install_only() {
    let project = TestProject::new().await.unwrap();

    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource("agents", "test-agent", "# Test Agent\n\nA test agent")
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest = ManifestBuilder::new()
        .add_source("official", &source_url)
        .add_standard_agent("agent", "official", "agents/test-agent.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    // Install with --max-parallel should work
    let output = project.run_agpm(&["install", "--max-parallel", "2"]).unwrap();
    assert!(output.success);

    // Update command should work without --max-parallel
    let output = project.run_agpm(&["update"]).unwrap();
    assert!(output.success);
}

/// Test --max-parallel in help output for install command
#[tokio::test]
#[allow(deprecated)]
async fn test_max_parallel_help_coverage() {
    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.arg("install")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--max-parallel"));
}
