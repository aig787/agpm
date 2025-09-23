//! Integration tests for --max-parallel flag behavior
//! Tests the new v0.3.0 concurrency control features

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;

mod common;
mod fixtures;
use common::TestProject;

/// Test --max-parallel flag with various values
#[test]
fn test_max_parallel_flag_values() {
    let project = TestProject::new().unwrap();

    // Create a simple test source
    let official_repo = project.create_source_repo("official").unwrap();
    official_repo
        .add_resource("agents", "test-agent-1", "# Test Agent 1\n\nA test agent")
        .unwrap();
    official_repo
        .add_resource("agents", "test-agent-2", "# Test Agent 2\n\nA test agent")
        .unwrap();
    official_repo
        .add_resource("agents", "test-agent-3", "# Test Agent 3\n\nA test agent")
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/test-agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/test-agent-2.md", version = "v1.0.0" }}
agent3 = {{ source = "official", path = "agents/test-agent-3.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest_content).unwrap();

    // Test different --max-parallel values
    for max_parallel in [1, 2, 4, 8] {
        let output = project
            .run_ccpm(&["install", "--max-parallel", &max_parallel.to_string()])
            .unwrap();
        assert!(output.success);
        assert!(output.stdout.contains("Installing") || output.stdout.contains("Installed"));

        // Verify installation worked
        assert!(
            project
                .project_path()
                .join(".claude/agents/agent1.md")
                .exists()
        );
        assert!(
            project
                .project_path()
                .join(".claude/agents/agent2.md")
                .exists()
        );
        assert!(
            project
                .project_path()
                .join(".claude/agents/agent3.md")
                .exists()
        );

        // Clean up for next iteration
        let _ = fs::remove_dir_all(project.project_path().join(".claude"));
    }
}

/// Test --max-parallel with invalid values
#[test]
fn test_max_parallel_invalid_values() {
    let project = TestProject::new().unwrap();

    // Test zero value
    let output = project
        .run_ccpm(&["install", "--max-parallel", "0"])
        .unwrap();
    assert!(!output.success);

    // Test negative value
    let output = project
        .run_ccpm(&["install", "--max-parallel", "-1"])
        .unwrap();
    assert!(!output.success);

    // Test non-numeric value
    let output = project
        .run_ccpm(&["install", "--max-parallel", "abc"])
        .unwrap();
    assert!(!output.success);
}

/// Test default parallelism when --max-parallel is not specified
#[test]
fn test_default_parallelism() {
    let project = TestProject::new().unwrap();

    let official_repo = project.create_source_repo("official").unwrap();
    official_repo
        .add_resource("agents", "test-agent", "# Test Agent\n\nA test agent")
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent = {{ source = "official", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest_content).unwrap();

    // Install without --max-parallel flag (should use default)
    let output = project.run_ccpm(&["install"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("Installing") || output.stdout.contains("Installed"));

    assert!(
        project
            .project_path()
            .join(".claude/agents/agent.md")
            .exists()
    );
}

/// Test --max-parallel flag on install command only (update doesn't support it)
#[test]
fn test_max_parallel_install_only() {
    let project = TestProject::new().unwrap();

    let official_repo = project.create_source_repo("official").unwrap();
    official_repo
        .add_resource("agents", "test-agent", "# Test Agent\n\nA test agent")
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent = {{ source = "official", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        source_url
    );

    project.write_manifest(&manifest_content).unwrap();

    // Install with --max-parallel should work
    let output = project
        .run_ccpm(&["install", "--max-parallel", "2"])
        .unwrap();
    assert!(output.success);

    // Update command should work without --max-parallel
    let output = project.run_ccpm(&["update"]).unwrap();
    assert!(output.success);
}

/// Test --max-parallel in help output for install command
#[test]
fn test_max_parallel_help_coverage() {
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    cmd.arg("install")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--max-parallel"));
}
