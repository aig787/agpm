//! Integration tests for --max-parallel flag behavior
//! Tests the new v0.3.0 concurrency control features

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;

mod fixtures;
use fixtures::{MarkdownFixture, TestEnvironment};

/// Test --max-parallel flag with various values
#[test]
fn test_max_parallel_flag_values() {
    let env = TestEnvironment::new().unwrap();

    // Create a simple test source
    let official_files = vec![
        MarkdownFixture::agent("test-agent-1"),
        MarkdownFixture::agent("test-agent-2"),
        MarkdownFixture::agent("test-agent-3"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/test.git",
            official_files,
        )
        .unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent1 = {{ source = "official", path = "agents/test-agent-1.md", version = "v1.0.0" }}
agent2 = {{ source = "official", path = "agents/test-agent-2.md", version = "v1.0.0" }}
agent3 = {{ source = "official", path = "agents/test-agent-3.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Test different --max-parallel values
    for max_parallel in [1, 2, 4, 8] {
        let mut cmd = env.ccpm_command();
        cmd.arg("install")
            .arg("--max-parallel")
            .arg(max_parallel.to_string())
            .assert()
            .success()
            .stdout(
                predicate::str::contains("Installing").or(predicate::str::contains("Installed")),
            );

        // Verify installation worked
        assert!(env.project_path().join(".claude/agents/agent1.md").exists());
        assert!(env.project_path().join(".claude/agents/agent2.md").exists());
        assert!(env.project_path().join(".claude/agents/agent3.md").exists());

        // Clean up for next iteration
        let _ = fs::remove_dir_all(env.project_path().join(".claude"));
    }
}

/// Test --max-parallel with invalid values
#[test]
fn test_max_parallel_invalid_values() {
    let env = TestEnvironment::new().unwrap();

    // Test zero value
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--max-parallel")
        .arg("0")
        .assert()
        .failure();

    // Test negative value
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--max-parallel")
        .arg("-1")
        .assert()
        .failure();

    // Test non-numeric value
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--max-parallel")
        .arg("abc")
        .assert()
        .failure();
}

/// Test default parallelism when --max-parallel is not specified
#[test]
fn test_default_parallelism() {
    let env = TestEnvironment::new().unwrap();

    let official_files = vec![MarkdownFixture::agent("test-agent")];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/test.git",
            official_files,
        )
        .unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent = {{ source = "official", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Install without --max-parallel flag (should use default)
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .assert()
        .success()
        .stdout(predicate::str::contains("Installing").or(predicate::str::contains("Installed")));

    assert!(env.project_path().join(".claude/agents/agent.md").exists());
}

/// Test --max-parallel flag on install command only (update doesn't support it)
#[test]
fn test_max_parallel_install_only() {
    let env = TestEnvironment::new().unwrap();

    let official_files = vec![MarkdownFixture::agent("test-agent")];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example/test.git",
            official_files,
        )
        .unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
agent = {{ source = "official", path = "agents/test-agent.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&source_path)
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Install with --max-parallel should work
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--max-parallel")
        .arg("2")
        .assert()
        .success();

    // Update command should work without --max-parallel
    let mut cmd = env.ccpm_command();
    cmd.arg("update").assert().success();
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
