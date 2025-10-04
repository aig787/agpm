use tokio::fs;

mod common;
mod fixtures;
use common::TestProject;
use fixtures::ManifestFixture;

/// Test outdated command with up-to-date dependencies
#[tokio::test]
async fn test_outdated_all_up_to_date() {
    let project = TestProject::new().await.unwrap();

    // Create manifest
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile with up-to-date resources
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/agpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v2.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
resource_type = "agent"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile_content)
        .await
        .unwrap();

    // Run outdated command
    let output = project.run_agpm(&["outdated", "--no-fetch"]).unwrap();
    output
        .assert_success()
        .assert_stdout_contains("All dependencies are up to date!");
}

/// Test outdated command with outdated dependencies
#[tokio::test]
async fn test_outdated_with_updates_available() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with version constraints
    let manifest_content = r#"[sources]
official = "https://github.com/example-org/agpm-official.git"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "^1.0.0" }
"#;
    project.write_manifest(manifest_content).await.unwrap();

    // Create mock lockfile with older version
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/agpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
resource_type = "agent"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile_content)
        .await
        .unwrap();

    // Note: In a real test, we'd need to mock the Git repository to return available versions
    // For now, this test would need actual network access or mocked Git operations
    // The test is structured to show the expected behavior
}

/// Test outdated command with no lockfile
#[tokio::test]
async fn test_outdated_no_lockfile() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["outdated"]).unwrap();
    assert!(!output.success, "Expected command to fail without lockfile");
    assert!(
        output.stderr.contains("agpm.lock") || output.stderr.contains("Run 'agpm install' first"),
        "Expected lockfile error message, got: {}",
        output.stderr
    );
}

/// Test outdated command without project
#[tokio::test]
async fn test_outdated_without_project() {
    let project = TestProject::new().await.unwrap();

    let output = project.run_agpm(&["outdated"]).unwrap();
    assert!(!output.success, "Expected command to fail without project");
    assert!(
        output.stderr.contains("agpm.toml not found"),
        "Expected manifest not found error, got: {}",
        output.stderr
    );
}

/// Test outdated command with JSON format
#[tokio::test]
async fn test_outdated_json_format() {
    let project = TestProject::new().await.unwrap();

    // Create manifest
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/agpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
resource_type = "agent"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile_content)
        .await
        .unwrap();

    // Run outdated command with JSON format
    let output = project
        .run_agpm(&["outdated", "--format", "json", "--no-fetch"])
        .unwrap();
    output.assert_success();

    // Check that output is valid JSON
    assert!(
        output.stdout.contains("{") && output.stdout.contains("}"),
        "Expected JSON output, got: {}",
        output.stdout
    );
    assert!(
        output.stdout.contains("\"outdated\"") && output.stdout.contains("\"summary\""),
        "Expected JSON structure with outdated and summary fields, got: {}",
        output.stdout
    );
}

/// Test outdated command with --check flag
#[tokio::test]
async fn test_outdated_check_flag() {
    let project = TestProject::new().await.unwrap();

    // Create manifest
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile with all up-to-date
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/agpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v2.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
resource_type = "agent"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile_content)
        .await
        .unwrap();

    // Run outdated command with --check flag
    let output = project
        .run_agpm(&["outdated", "--check", "--no-fetch"])
        .unwrap();

    // Should succeed when all dependencies are up to date
    output.assert_success();
}

/// Test outdated command with specific dependencies
#[tokio::test]
async fn test_outdated_specific_dependencies() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with multiple dependencies
    let manifest_content = r#"[sources]
official = "https://github.com/example-org/agpm-official.git"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "^1.0.0" }
helper = { source = "official", path = "agents/helper.md", version = "^1.0.0" }
"#;
    project.write_manifest(manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/agpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
resource_type = "agent"

[[agents]]
name = "helper"
source = "official"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/helper.md"
resource_type = "agent"
"#;
    fs::write(project.project_path().join("agpm.lock"), lockfile_content)
        .await
        .unwrap();

    // Run outdated command for specific dependency
    let output = project
        .run_agpm(&["outdated", "--no-fetch", "my-agent"])
        .unwrap();
    output.assert_success();
}
