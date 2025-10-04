//! Integration tests for update command progress reporting consistency.
//!
//! These tests verify that the update command's progress reporting
//! follows the same pattern as the install command.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use tokio::fs;

/// Test that update command uses proper phases when updating dependencies
#[tokio::test]
async fn test_update_progress_phases() {
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create a manifest with local dependencies
    let manifest_content = r#"
[sources]
test_source = "../test_resources"

[agents]
test_agent = { path = "../test_resources/agent.md" }
"#;
    fs::write(project_dir.join("ccpm.toml"), manifest_content)
        .await
        .unwrap();

    // Create the test resource directory and file
    let resource_dir = project_dir.parent().unwrap().join("test_resources");
    fs::create_dir_all(&resource_dir).await.unwrap();
    fs::write(resource_dir.join("agent.md"), "# Test Agent")
        .await
        .unwrap();

    // Run install first to create lockfile
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    cmd.current_dir(project_dir)
        .arg("install")
        .assert()
        .success();

    // Verify lockfile was created
    assert!(project_dir.join("ccpm.lock").exists());

    // Run update - should show proper phases (or "up to date" message)
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    let output = cmd
        .current_dir(project_dir)
        .arg("update")
        .assert()
        .success();

    // Check for expected output patterns
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Should either show "up to date" or proper phase progression
    assert!(
        stdout.contains("All dependencies are up to date")
            || stdout.contains("Syncing")
            || stdout.contains("Resolving")
            || stdout.contains("Installing"),
        "Update output should show proper status messages: {}",
        stdout
    );
}

/// Test that update handles empty manifest correctly
#[tokio::test]
async fn test_update_empty_manifest() {
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create empty manifest
    fs::write(project_dir.join("ccpm.toml"), "[sources]\n[agents]\n")
        .await
        .unwrap();

    // Run install first
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    cmd.current_dir(project_dir)
        .arg("install")
        .assert()
        .success();

    // Run update - should handle gracefully
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    cmd.current_dir(project_dir)
        .arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("All dependencies are up to date"));
}

/// Test that update without lockfile performs fresh install
#[tokio::test]
async fn test_update_no_lockfile_fresh_install() {
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest without running install
    fs::write(project_dir.join("ccpm.toml"), "[sources]\n[agents]\n")
        .await
        .unwrap();

    // Ensure no lockfile exists
    assert!(!project_dir.join("ccpm.lock").exists());

    // Run update - should perform fresh install
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    let output = cmd
        .current_dir(project_dir)
        .arg("update")
        .assert()
        .success();

    // Check for fresh install indicators
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("No lockfile found")
            || stdout.contains("fresh install")
            || stdout.contains("No dependencies"),
        "Should indicate fresh install: {}",
        stdout
    );

    // Lockfile should be created
    assert!(project_dir.join("ccpm.lock").exists());
}

/// Test that dry-run mode doesn't modify files
#[tokio::test]
async fn test_update_dry_run_no_modifications() {
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest and lockfile
    fs::write(project_dir.join("ccpm.toml"), "[sources]\n[agents]\n")
        .await
        .unwrap();

    fs::write(
        project_dir.join("ccpm.lock"),
        "version = 1\nsources = []\nagents = []\n",
    )
    .await
    .unwrap();

    let original_lock = fs::read_to_string(project_dir.join("ccpm.lock"))
        .await
        .unwrap();

    // Run update with --dry-run
    let mut cmd = Command::cargo_bin("ccpm").unwrap();
    cmd.current_dir(project_dir)
        .arg("update")
        .arg("--dry-run")
        .assert()
        .success();

    // Lockfile should be unchanged
    let current_lock = fs::read_to_string(project_dir.join("ccpm.lock"))
        .await
        .unwrap();
    assert_eq!(original_lock, current_lock);
}
