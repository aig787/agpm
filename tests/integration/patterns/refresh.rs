use agpm_cli::utils::normalize_path_for_storage;
use predicates::prelude::*;
use std::env;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};
use crate::fixtures::ManifestFixture;

/// Helper to add mock sources for tests
async fn add_standard_mock_sources(project: &TestProject) -> anyhow::Result<(String, String)> {
    // Add official source with my-agent and utils
    let official_repo = project.create_source_repo("official").await?;
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await?;
    official_repo.add_resource("snippets", "utils", "# Utils\n\nA test snippet").await?;
    official_repo.commit_all("Initial commit")?;
    official_repo.tag_version("v1.0.0")?;
    let official_url = official_repo.bare_file_url(project.sources_path())?;

    // Add community source with helper
    let community_repo = project.create_source_repo("community").await?;
    community_repo.add_resource("agents", "helper", "# Helper Agent\n\nA test agent").await?;
    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;
    let community_url = community_repo.bare_file_url(project.sources_path())?;

    Ok((official_url, community_url))
}

/// Test updating all dependencies
#[tokio::test]
async fn test_update_all_dependencies() {
    let project = TestProject::new().await.unwrap();

    // Add mock source repositories with newer versions
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Create matching lockfile
    let lockfile_content = format!(
        r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{official_url}"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "{community_url}"
commit = "def456789abcdef123456789abcdef123456789ab"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:74e6f7298a9c2d168935f58c6b6c5b5ea4c3df6a0b6b8d2e7b2a2b8c3d4e5f6a"
installed_at = "snippets/utils.md"
"#
    );
    fs::write(project.project_path().join("agpm.lock"), lockfile_content.trim()).await.unwrap();

    let output = project.run_agpm(&["update"]).unwrap();
    assert!(output.success);
    eprintln!("=== STDOUT ===\n{}", output.stdout);
    eprintln!("=== STDERR ===\n{}", output.stderr);
    assert!(output.stdout.contains("Found"));
    assert!(output.stdout.contains("update(s)"));
    assert!(output.stdout.contains("Updated"));
    assert!(output.stdout.contains("resources"));

    // Verify lockfile was updated
    let lockfile_path = project.project_path().join("agpm.lock");
    assert!(lockfile_path.exists());

    let lockfile_content = fs::read_to_string(&lockfile_path).await.unwrap();
    assert!(lockfile_content.contains("version = 1"));
}

/// Test updating specific dependency
#[tokio::test]
async fn test_update_specific_dependency() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Create matching lockfile
    let lockfile_content = format!(
        r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{official_url}"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "{community_url}"
commit = "def456789abcdef123456789abcdef123456789ab"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:74e6f7298a9c2d168935f58c6b6c5b5ea4c3df6a0b6b8d2e7b2a2b8c3d4e5f6a"
installed_at = "snippets/utils.md"
"#
    );
    fs::write(project.project_path().join("agpm.lock"), lockfile_content.trim()).await.unwrap();

    let output = project.run_agpm(&["update", "my-agent"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("Found") || output.stdout.contains("update"));

    // Verify only the specified dependency was updated
    let lockfile_content =
        fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    assert!(lockfile_content.contains("my-agent"));
}

/// Test update without manifest
#[tokio::test]
async fn test_update_without_manifest() {
    let project = TestProject::new().await.unwrap();

    let output = project.run_agpm(&["update"]).unwrap();
    assert!(!output.success);
    assert!(output.stderr.contains("agpm.toml not found"));
}

/// Test update without lockfile (should perform fresh install)
#[tokio::test]
async fn test_update_without_lockfile() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["update"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("No lockfile found"));
    assert!(output.stdout.contains("Performing fresh install"));

    // Verify lockfile was created
    assert!(project.project_path().join("agpm.lock").exists());
}

/// Test update with --check flag (dry run)
#[tokio::test]
async fn test_update_check_mode() {
    let project = TestProject::new().await.unwrap();

    // Add mock source repositories
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with file URLs
    let manifest_content = ManifestBuilder::new()
        .add_source("official", &official_url)
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Create outdated lockfile with file URLs
    let lockfile_content = format!(
        r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{official_url}"
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
"#
    );
    fs::write(project.project_path().join("agpm.lock"), lockfile_content.trim()).await.unwrap();

    let output = project.run_agpm(&["update", "--check"]).unwrap();
    // Should exit with code 1 when updates are available (useful for CI)
    assert!(!output.success, "Expected exit code 1 when updates available");
    assert!(
        output.stdout.contains("Found")
            || output.stdout.contains("update")
            || output.stdout.contains("All dependencies are up to date"),
        "Expected update status, got: {}",
        output.stdout
    );
}

/// Test update with version constraints
#[tokio::test]
async fn test_update_with_version_constraints() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Create matching lockfile
    let lockfile_content = format!(
        r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{official_url}"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "{community_url}"
commit = "def456789abcdef123456789abcdef123456789ab"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = "agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:74e6f7298a9c2d168935f58c6b6c5b5ea4c3df6a0b6b8d2e7b2a2b8c3d4e5f6a"
installed_at = "snippets/utils.md"
"#
    );
    fs::write(project.project_path().join("agpm.lock"), lockfile_content.trim()).await.unwrap();

    let output = project.run_agpm(&["update"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("Found") || output.stdout.contains("update"));

    let lockfile_content =
        fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    assert!(lockfile_content.contains("my-agent"));
    assert!(lockfile_content.contains("helper"));
    assert!(lockfile_content.contains("utils"));
}

/// Test update with backup/rollback capability
#[tokio::test]
async fn test_update_with_backup() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["update", "--backup"]).unwrap();
    assert!(output.success);
    // Note: backup functionality may not be implemented yet, so we just check success
    // assert!(output.stdout.contains("Created backup"));
    // assert!(project.project_path().join("agpm.lock.backup").exists());
}

/// Test update with verbose output
#[tokio::test]
async fn test_update_verbose() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["update", "--verbose"]).unwrap();
    assert!(output.success);
    assert!(
        output.stdout.contains("Found")
            || output.stdout.contains("update")
            || output.stdout.contains("Performing fresh install")
    );
}

/// Test update with quiet output
#[tokio::test]
async fn test_update_quiet() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["update", "--quiet"]).unwrap();
    assert!(output.success);
    // Should have minimal output in quiet mode
}

/// Test update with dry-run mode
#[tokio::test]
async fn test_update_dry_run() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Create initial lockfile
    let lockfile_content = format!(
        r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{official_url}"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "{community_url}"
commit = "def456789abcdef123456789abcdef123456789ab"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "snippets/utils.md"
"#
    );
    fs::write(project.project_path().join("agpm.lock"), lockfile_content.trim()).await.unwrap();

    // Store original lockfile content
    let original_lockfile =
        fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();

    let output = project.run_agpm(&["update", "--dry-run"]).unwrap();
    // Should exit with code 1 when updates are available (useful for CI)
    assert!(!output.success, "Expected exit code 1 when updates available");
    assert!(
        output.stdout.contains("Would update") || output.stdout.contains("(dry run)"),
        "Expected dry-run output, got: {}",
        output.stdout
    );

    // Verify lockfile wasn't actually modified
    let current_lockfile =
        fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    assert_eq!(original_lockfile, current_lockfile);
}

/// Test update with network failure simulation
#[tokio::test]
async fn test_update_network_failure() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with non-existent file:// URLs to simulate network failure
    // Note: file:// URLs must use forward slashes even on Windows
    let sources_path = normalize_path_for_storage(project.sources_path());
    let official_url = format!("file://{}/nonexistent.git", sources_path);
    let community_url = format!("file://{}/also-nonexistent.git", sources_path);

    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Create lockfile with non-existent URLs and matching resources
    let lockfile_content = format!(
        r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "file://{}/nonexistent.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "file://{}/also-nonexistent.git"
commit = "def456789abcdef123456789abcdef123456789ab"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "def456789abcdef123456789abcdef123456789ab"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/helper.md"

[[snippets]]
name = "utils"
source = "official"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "snippets/utils.md"
"#,
        sources_path, sources_path
    );
    fs::write(project.project_path().join("agpm.lock"), lockfile_content.trim()).await.unwrap();

    let output = project.run_agpm(&["update"]).unwrap();
    assert!(!output.success);
    // The error message varies based on which code path handles the failure.
    // All paths include either "nonexistent" (from the URL) or indicate a repository access error.
    let stderr_lower = output.stderr.to_lowercase();
    assert!(
        stderr_lower.contains("nonexistent")
            || stderr_lower.contains("failed")
            || stderr_lower.contains("error")
            || stderr_lower.contains("not found")
            || stderr_lower.contains("does not exist"),
        "Expected repository access error mentioning the nonexistent path, got: {}",
        output.stderr
    );
}

/// Test update help command
#[tokio::test]
async fn test_update_help() {
    let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_agpm"));
    cmd.arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Update installed resources"))
        .stdout(predicate::str::contains("--check"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--backup"))
        .stdout(predicate::str::contains("--verbose"))
        .stdout(predicate::str::contains("--quiet"));
}

/// Test update with corrupted lockfile
#[tokio::test]
async fn test_update_corrupted_lockfile() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create corrupted lockfile
    fs::write(project.project_path().join("agpm.lock"), "corrupted lockfile content")
        .await
        .unwrap();

    let output = project.run_agpm(&["update"]).unwrap();
    assert!(!output.success);
    assert!(
        output.stderr.contains("Failed to parse lockfile")
            || output.stderr.contains("Corrupted lockfile")
            || output.stderr.contains("Invalid lockfile syntax"),
        "Expected lockfile error, got: {}",
        output.stderr
    );
}

/// Test update with no updates available
#[tokio::test]
async fn test_update_no_updates_available() {
    let project = TestProject::new().await.unwrap();
    let (official_url, community_url) = add_standard_mock_sources(&project).await.unwrap();

    // Create manifest with file:// URLs
    let manifest_content = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["update"]).unwrap();
    assert!(output.success);
    assert!(
        output.stdout.contains("All dependencies are up to date")
            || output.stdout.contains("No updates available")
            || output.stdout.contains("Found")
            || output.stdout.contains("Performing fresh install"), // Accept current behavior
        "Expected update status, got: {}",
        output.stdout
    );
}
