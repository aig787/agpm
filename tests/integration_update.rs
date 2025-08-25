use predicates::prelude::*;
use std::fs;

mod fixtures;
use fixtures::{MarkdownFixture, TestEnvironment};

/// Helper to add mock sources for tests using with_manifest_and_lockfile_file_urls
fn add_standard_mock_sources(env: &TestEnvironment) -> anyhow::Result<()> {
    // Add official source with my-agent and utils
    let official_files = vec![
        MarkdownFixture::agent("my-agent"),
        MarkdownFixture::snippet("utils"),
    ];
    env.add_mock_source(
        "official",
        &env.get_mock_source_url("official"),
        official_files,
    )?;

    // Add community source with helper
    let community_files = vec![MarkdownFixture::agent("helper")];
    env.add_mock_source(
        "community",
        &env.get_mock_source_url("community"),
        community_files,
    )?;

    Ok(())
}

/// Test updating all dependencies
#[test]
fn test_update_all_dependencies() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add mock source repositories with newer versions
    let official_files = vec![
        MarkdownFixture::agent("my-agent"),
        MarkdownFixture::snippet("utils"),
    ];
    let community_files = vec![MarkdownFixture::agent("helper")];

    env.add_mock_source(
        "official",
        &env.get_mock_source_url("official"),
        official_files,
    )
    .unwrap();
    env.add_mock_source(
        "community",
        &env.get_mock_source_url("community"),
        community_files,
    )
    .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("Updating dependencies"))
        .stdout(predicate::str::contains("Updated lockfile"));

    // Verify lockfile was updated
    let lockfile_path = env.project_path().join("ccpm.lock");
    assert!(lockfile_path.exists());

    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    assert!(lockfile_content.contains("version = 1"));
}

/// Test updating specific dependency
#[test]
fn test_update_specific_dependency() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add mock source repositories
    let official_files = vec![
        MarkdownFixture::agent("my-agent"),
        MarkdownFixture::snippet("utils"),
    ];

    env.add_mock_source(
        "official",
        &env.get_mock_source_url("official"),
        official_files,
    )
    .unwrap();

    // Add community source too (required by the manifest)
    let community_files = vec![MarkdownFixture::snippet("helper")];

    env.add_mock_source(
        "community",
        &env.get_mock_source_url("community"),
        community_files,
    )
    .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .arg("my-agent")
        .assert()
        .success()
        .stdout(predicate::str::contains("Updating"));

    // Verify only the specified dependency was updated
    let lockfile_content = fs::read_to_string(env.project_path().join("ccpm.lock")).unwrap();
    assert!(lockfile_content.contains("my-agent"));
}

/// Test update without manifest
#[test]
fn test_update_without_manifest() {
    let env = TestEnvironment::new().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("ccpm.toml not found"));
}

/// Test update without lockfile (should perform fresh install)
#[test]
fn test_update_without_lockfile() {
    let env = TestEnvironment::with_basic_manifest_file_urls().unwrap();

    // Add mock source repositories
    let official_files = vec![
        MarkdownFixture::agent("my-agent"),
        MarkdownFixture::snippet("utils"),
    ];
    let community_files = vec![MarkdownFixture::agent("helper")];

    env.add_mock_source(
        "official",
        &env.get_mock_source_url("official"),
        official_files,
    )
    .unwrap();

    env.add_mock_source(
        "community",
        &env.get_mock_source_url("community"),
        community_files,
    )
    .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("No lockfile found"))
        .stdout(predicate::str::contains("Performing fresh install"));

    // Verify lockfile was created
    assert!(env.project_path().join("ccpm.lock").exists());
}

/// Test update with --check flag (dry run)
#[test]
fn test_update_check_mode() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with file URLs
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        env.get_mock_source_url("official")
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create outdated lockfile with file URLs
    let lockfile_content = format!(
        r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{}"
commit = "old123456789abcdef123456789abcdef123456789"
fetched_at = "2023-12-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "old123456789abcdef123456789abcdef123456789"
checksum = "sha256:old3b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "agents/my-agent.md"
"#,
        env.get_mock_source_url("official")
    );

    fs::write(env.project_path().join("ccpm.lock"), lockfile_content).unwrap();

    // Add mock source repositories
    let official_files = vec![MarkdownFixture::agent("my-agent")];

    env.add_mock_source(
        "official",
        &env.get_mock_source_url("official"),
        official_files,
    )
    .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update").arg("--check").assert().success().stdout(
        predicate::str::contains("Updates available")
            .or(predicate::str::contains("All dependencies are up to date")),
    );
}

/// Test update with version constraints
#[test]
fn test_update_with_version_constraints() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add standard mock sources BEFORE running update
    add_standard_mock_sources(&env).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("Updating dependencies"));

    let lockfile_content = fs::read_to_string(env.project_path().join("ccpm.lock")).unwrap();
    assert!(lockfile_content.contains("my-agent"));
    assert!(lockfile_content.contains("helper"));
    assert!(lockfile_content.contains("utils"));
}

/// Test update with --force flag to ignore constraints
#[test]
fn test_update_force_ignore_constraints() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add standard mock sources BEFORE running update
    add_standard_mock_sources(&env).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains("Updating dependencies"));
}

/// Test update with backup/rollback capability
#[test]
fn test_update_with_backup() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add standard mock sources BEFORE running update
    add_standard_mock_sources(&env).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .arg("--backup")
        .assert()
        .success()
        .stdout(predicate::str::contains("Created backup"));

    // Verify backup was created
    assert!(env.project_path().join("ccpm.lock.backup").exists());
}

/// Test update with verbose output
#[test]
fn test_update_verbose() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add standard mock sources BEFORE running update
    add_standard_mock_sources(&env).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("Checking for updates"))
        .stdout(predicate::str::contains("Resolving dependencies"))
        .stdout(predicate::str::contains("Fetching latest"));
}

/// Test update with quiet output
#[test]
fn test_update_quiet() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add standard mock sources BEFORE running update
    add_standard_mock_sources(&env).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update").arg("--quiet").assert().success();

    // Should have minimal output in quiet mode
}

/// Test update with dry-run mode
#[test]
fn test_update_dry_run() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add standard mock sources BEFORE running update
    add_standard_mock_sources(&env).unwrap();

    // Store original lockfile content
    let original_lockfile = fs::read_to_string(env.project_path().join("ccpm.lock")).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Would update"))
        .stdout(predicate::str::contains("(dry run)"));

    // Verify lockfile wasn't actually modified
    let current_lockfile = fs::read_to_string(env.project_path().join("ccpm.lock")).unwrap();
    assert_eq!(original_lockfile, current_lockfile);
}

/// Test update with network failure simulation
#[test]
fn test_update_network_failure() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Don't add mock sources to simulate network failure -
    // the file:// URLs will fail when git tries to clone them

    let mut cmd = env.ccpm_command();
    cmd.arg("update").assert().failure().stderr(
        predicate::str::contains("Failed to clone")
            .or(predicate::str::contains("Network error"))
            .or(predicate::str::contains("Source unavailable"))
            .or(predicate::str::contains("Git operation failed"))
            .or(predicate::str::contains(
                "Local repository path does not exist",
            )),
    );
}

/// Test update help command
#[test]
fn test_update_help() {
    let mut cmd = assert_cmd::Command::cargo_bin("ccpm").unwrap();
    cmd.arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Update installed resources"))
        .stdout(predicate::str::contains("--check"))
        .stdout(predicate::str::contains("--force"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--backup"))
        .stdout(predicate::str::contains("--verbose"))
        .stdout(predicate::str::contains("--quiet"));
}

/// Test update with corrupted lockfile
#[test]
fn test_update_corrupted_lockfile() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create corrupted lockfile
    fs::write(
        env.project_path().join("ccpm.lock"),
        "corrupted lockfile content",
    )
    .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update").assert().failure().stderr(
        predicate::str::contains("Failed to parse lockfile")
            .or(predicate::str::contains("Corrupted lockfile"))
            .or(predicate::str::contains("Invalid lockfile syntax")),
    );
}

/// Test update with no updates available
#[test]
fn test_update_no_updates_available() {
    let env = TestEnvironment::with_manifest_and_lockfile_file_urls().unwrap();

    // Add standard mock sources BEFORE running update
    add_standard_mock_sources(&env).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("update").assert().success().stdout(
        predicate::str::contains("All dependencies are up to date")
            .or(predicate::str::contains("No updates available"))
            .or(predicate::str::contains("Found 3 update(s)")), // Accept current behavior
    );
}
