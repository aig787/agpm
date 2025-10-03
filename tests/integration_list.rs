use predicates::prelude::*;
use tokio::fs;

mod common;
mod fixtures;
use common::TestProject;
use fixtures::ManifestFixture;

/// Test listing installed resources from lockfile
#[tokio::test]
async fn test_list_installed_resources() {
    let project = TestProject::new().await.unwrap();

    // Create manifest
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile with resources
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list"]).unwrap();
    output
        .assert_success()
        .assert_stdout_contains("my-agent")
        .assert_stdout_contains("helper")
        .assert_stdout_contains("utils");
}

/// Test listing with no lockfile
#[tokio::test]
async fn test_list_no_lockfile() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_ccpm(&["list"]).unwrap();
    output.assert_success();
    assert!(
        output.stdout.contains("No installed resources")
            || output.stdout.contains("ccpm.lock not found"),
        "Expected no resources message, got: {}",
        output.stdout
    );
}

/// Test listing without project
#[tokio::test]
async fn test_list_without_project() {
    let project = TestProject::new().await.unwrap();

    let output = project.run_ccpm(&["list"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("ccpm.toml not found"),
        "Expected manifest not found error, got: {}",
        output.stderr
    );
}

/// Test list with table format
#[tokio::test]
async fn test_list_table_format() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--format", "table"]).unwrap();
    output
        .assert_success()
        .assert_stdout_contains("Name")
        .assert_stdout_contains("Version")
        .assert_stdout_contains("Source")
        .assert_stdout_contains("Type")
        .assert_stdout_contains("my-agent");
}

/// Test list with JSON format
#[tokio::test]
async fn test_list_json_format() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create basic lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--format", "json"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("{"));
    assert!(output.stdout.contains("\"name\""));
    assert!(output.stdout.contains("\"version\""));
    assert!(output.stdout.contains("\"source\""));
    assert!(output.stdout.contains("\"my-agent\""));
}

/// Test list with YAML format
#[tokio::test]
async fn test_list_yaml_format() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create basic lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--format", "yaml"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("name:"));
    assert!(output.stdout.contains("version:"));
    assert!(output.stdout.contains("source:"));
    assert!(output.stdout.contains("my-agent"));
}

/// Test list with compact format
#[tokio::test]
async fn test_list_compact_format() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--format", "compact"]).unwrap();
    output
        .assert_success()
        .assert_stdout_contains("my-agent")
        .assert_stdout_contains("v1.0.0")
        .assert_stdout_contains("official");
}

/// Test filtering by resource type - agents only
#[tokio::test]
async fn test_list_agents_only() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--type", "agents"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("my-agent"));
    assert!(output.stdout.contains("helper"));
    assert!(!output.stdout.contains("utils")); // utils is a snippet
}

/// Test filtering by resource type - snippets only
#[tokio::test]
async fn test_list_snippets_only() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--type", "snippets"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("utils"));
    assert!(!output.stdout.contains("my-agent")); // my-agent is an agent
    assert!(!output.stdout.contains("helper")); // helper is an agent
}

/// Test filtering by source
#[tokio::test]
async fn test_list_filter_by_source() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--source", "official"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("my-agent"));
    assert!(output.stdout.contains("utils"));
    assert!(!output.stdout.contains("helper")); // helper is from community source
}

/// Test listing with search/filter by name
#[tokio::test]
async fn test_list_search_by_name() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--search", "agent"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("my-agent"));
    assert!(!output.stdout.contains("utils"));
}

/// Test listing with detailed/verbose output
#[tokio::test]
async fn test_list_detailed() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--detailed"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("my-agent"));
    assert!(output.stdout.contains("Path:") || output.stdout.contains("agents/my-agent.md"));
    // The detailed output format may vary, so we check for key content
}

/// Test listing installed files (show actual file paths)
#[tokio::test]
async fn test_list_installed_files() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create mock lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "https://github.com/example-org/ccpm-community.git"
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
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    // Create some installed files to match lockfile
    let agents_dir = project.project_path().join("agents");
    let snippets_dir = project.project_path().join("snippets");
    fs::create_dir_all(&agents_dir).await.unwrap();
    fs::create_dir_all(&snippets_dir).await.unwrap();

    fs::write(agents_dir.join("my-agent.md"), "# My Agent")
        .await
        .unwrap();
    fs::write(agents_dir.join("helper.md"), "# Helper Agent")
        .await
        .unwrap();
    fs::write(snippets_dir.join("utils.md"), "# Utils Snippet")
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--files"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("agents/my-agent.md") || output.stdout.contains("my-agent"));
    assert!(output.stdout.contains("agents/helper.md") || output.stdout.contains("helper"));
    assert!(output.stdout.contains("snippets/utils.md") || output.stdout.contains("utils"));
}

/// Test listing with sorting options
#[tokio::test]
async fn test_list_sorted_by_name() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create basic lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--sort", "name"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("my-agent"));
}

/// Test listing sorted by version
#[tokio::test]
async fn test_list_sorted_by_version() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create basic lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--sort", "version"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("my-agent"));
}

/// Test listing sorted by source
#[tokio::test]
async fn test_list_sorted_by_source() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create basic lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--sort", "source"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("my-agent"));
}

/// Test list with local dependencies
#[tokio::test]
async fn test_list_local_dependencies() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::with_local().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create lockfile with local dependencies
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
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

[[agents]]
name = "local-agent"
path = "../local-agents/helper.md"
version = "local"
checksum = "sha256:local123456789abcdef123456789abcdef123456789abcdef"
installed_at = "agents/local-agent.md"

[[snippets]]
name = "local-utils"
path = "./snippets/local-utils.md"
version = "local"
checksum = "sha256:local987654321fedcba987654321fedcba987654321fedcba"
installed_at = "snippets/local-utils.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("local-agent"));
    assert!(output.stdout.contains("local-utils"));
    assert!(output.stdout.contains("local"));
}

/// Test list help command
#[tokio::test]
async fn test_list_help() {
    let mut cmd = assert_cmd::Command::cargo_bin("ccpm").unwrap();
    cmd.arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "List installed Claude Code resources",
        ))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--source"))
        .stdout(predicate::str::contains("--detailed"))
        .stdout(predicate::str::contains("--manifest"))
        .stdout(predicate::str::contains("--agents"))
        .stdout(predicate::str::contains("--snippets"));
}

/// Test list with empty project (no dependencies)
#[tokio::test]
async fn test_list_empty_project() {
    let project = TestProject::new().await.unwrap();

    // Create minimal manifest with no dependencies
    let minimal_manifest = r#"[sources]
official = "https://github.com/example-org/ccpm-official.git"
"#;
    project.write_manifest(minimal_manifest).await.unwrap();

    // Create empty lockfile
    let empty_lockfile = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1
"#;
    fs::write(project.project_path().join("ccpm.lock"), empty_lockfile)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list"]).unwrap();
    assert!(output.success);
    assert!(
        output.stdout.contains("No installed resources") || output.stdout.contains("Empty project"),
        "Expected no resources message, got: {}",
        output.stdout
    );
}

/// Test list with corrupted lockfile
#[tokio::test]
async fn test_list_corrupted_lockfile() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create corrupted lockfile
    fs::write(
        project.project_path().join("ccpm.lock"),
        "corrupted content",
    )
    .await
    .unwrap();

    let output = project.run_ccpm(&["list"]).unwrap();
    assert!(!output.success);
    assert!(
        output.stderr.contains("Invalid lockfile syntax")
            || output.stderr.contains("Failed to parse lockfile"),
        "Expected lockfile error, got: {}",
        output.stderr
    );
}

/// Test list with invalid format option
#[tokio::test]
async fn test_list_invalid_format() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create basic lockfile
    let lockfile_content = r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"
"#;
    fs::write(project.project_path().join("ccpm.lock"), lockfile_content)
        .await
        .unwrap();

    let output = project.run_ccpm(&["list", "--format", "invalid"]).unwrap();
    assert!(!output.success);
    assert!(output.stderr.contains("Invalid format") || output.stderr.contains("invalid"));
}
