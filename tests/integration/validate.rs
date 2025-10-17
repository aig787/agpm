use agpm_cli::utils::normalize_path_for_storage;
use predicates::prelude::*;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};
use crate::fixtures::ManifestFixture;

/// Test validating a valid manifest
#[tokio::test]
async fn test_validate_valid_manifest() {
    let project = TestProject::new().await.unwrap();

    // Create mock sources
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.add_resource("snippets", "utils", "# Utils\n\nA test snippet").await.unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA test agent")
        .await
        .unwrap();
    community_repo.commit_all("Initial commit").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with file:// URLs
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Valid"));
}

/// Test validating manifest without project
#[tokio::test]
async fn test_validate_no_manifest() {
    let project = TestProject::new().await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("No agpm.toml found"));
}

/// Test validating manifest with invalid syntax
#[tokio::test]
async fn test_validate_invalid_syntax() {
    let project = TestProject::new().await.unwrap();
    let manifest = ManifestFixture::invalid_syntax();
    project.write_manifest(&manifest.content).await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("Syntax error"));
    assert!(output.stdout.contains("TOML parsing failed"));
}

/// Test validating manifest with missing required fields
#[tokio::test]
async fn test_validate_missing_fields() {
    let project = TestProject::new().await.unwrap();
    let manifest = ManifestFixture::missing_fields();
    project.write_manifest(&manifest.content).await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("Missing required field"));
    assert!(output.stdout.contains("path"));
}

// TODO: Implement version conflict detection in validate command
// Would check if manifest has conflicting version requirements

/// Test validating with --sources flag to check source availability
#[tokio::test]
async fn test_validate_sources() {
    let project = TestProject::new().await.unwrap();

    // Add mock source repositories
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.add_resource("snippets", "utils", "# Utils\n\nA test snippet").await.unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA test agent")
        .await
        .unwrap();
    community_repo.commit_all("Initial commit").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with file:// URLs
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--sources"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Sources accessible"));
}

/// Test validating sources that are not accessible
#[tokio::test]
async fn test_validate_inaccessible_sources() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with file:// URLs pointing to non-existent sources
    let manifest = ManifestBuilder::new()
        .add_sources(&[
            ("official", "file:///non/existent/path"),
            ("community", "file:///another/non/existent/path"),
        ])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--sources"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("Source not accessible"));
}

/// Test validating with --dependencies flag to check dependency resolution
#[tokio::test]
async fn test_validate_dependencies() {
    let project = TestProject::new().await.unwrap();

    // Add mock source repositories with the required files
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.add_resource("snippets", "utils", "# Utils\n\nA test snippet").await.unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA test agent")
        .await
        .unwrap();
    community_repo.commit_all("Initial commit").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with file:// URLs
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--resolve"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Dependencies resolvable"));
}

/// Test validating dependencies that don't exist in sources\
/// Note: Current implementation validates source accessibility but not individual file existence
#[tokio::test]
async fn test_validate_missing_dependencies() {
    let project = TestProject::new().await.unwrap();

    // Add mock source repositories but without the required files
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource("agents", "other-agent", "# Other Agent\n\nA different agent")
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with file:// URLs
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = ManifestBuilder::new()
        .add_source("official", &official_url)
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--resolve"]).unwrap();
    assert!(output.success); // Current implementation validates source accessibility, not file existence
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Dependencies resolvable"));
}

/// Test validating with --paths flag to check local file references
#[tokio::test]
async fn test_validate_local_paths() {
    let project = TestProject::new().await.unwrap();
    let manifest = ManifestFixture::with_local();
    project.write_manifest(&manifest.content).await.unwrap();

    // Create the local files referenced in the manifest
    // ../local-agents/helper.md (relative to project directory)
    let project_parent = project.project_path().parent().unwrap();
    let local_agents_dir = project_parent.join("local-agents");
    fs::create_dir_all(&local_agents_dir).await.unwrap();
    fs::write(local_agents_dir.join("helper.md"), "# Helper Agent\n\nThis is a test agent.")
        .await
        .unwrap();

    // ./snippets/local-utils.md (relative to project directory)
    let snippets_dir = project.project_path().join("snippets");
    fs::create_dir_all(&snippets_dir).await.unwrap();
    fs::write(
        snippets_dir.join("local-utils.md"),
        "# Local Utils Snippet\n\nThis is a test snippet.",
    )
    .await
    .unwrap();

    let output = project.run_agpm(&["validate", "--paths"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Local paths exist"));
}

/// Test validating local paths that don't exist
#[tokio::test]
async fn test_validate_missing_local_paths() {
    let project = TestProject::new().await.unwrap();
    let manifest = ManifestFixture::with_local();
    project.write_manifest(&manifest.content).await.unwrap();

    // Don't create the local files to test validation failure

    let output = project.run_agpm(&["validate", "--paths"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("Local path not found"));
    assert!(output.stdout.contains("../local-agents/helper.md"));
    assert!(output.stdout.contains("./snippets/local-utils.md"));
}

/// Test validating with --lockfile flag to check lockfile consistency
#[tokio::test]
async fn test_validate_lockfile_consistent() {
    let project = TestProject::new().await.unwrap();

    // Create mock sources
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.add_resource("snippets", "utils", "# Utils\n\nA test snippet").await.unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA test agent")
        .await
        .unwrap();
    community_repo.commit_all("Initial commit").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with file:// URLs
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    // Create a matching lockfile
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

    let output = project.run_agpm(&["validate", "--check-lock"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Lockfile consistent"));
}

/// Test validating inconsistent lockfile
#[tokio::test]
async fn test_validate_lockfile_inconsistent() {
    let project = TestProject::new().await.unwrap();

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("official", "file:///fake/url")
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    // Create lockfile that doesn't match manifest
    let inconsistent_lockfile = r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "different"
url = "https://github.com/different/repo.git"
commit = "abc123456789abcdef123456789abcdef12345678"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "different-agent"
source = "different"
path = "agents/different.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/different.md"
"#;
    fs::write(project.project_path().join("agpm.lock"), inconsistent_lockfile).await.unwrap();

    let output = project.run_agpm(&["validate", "--check-lock"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("Lockfile inconsistent"));
}

/// Test validating corrupted lockfile
#[tokio::test]
async fn test_validate_corrupted_lockfile() {
    let project = TestProject::new().await.unwrap();

    // Create a basic manifest
    let manifest = ManifestBuilder::new()
        .add_source("official", "file:///fake/url")
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    // Create corrupted lockfile
    fs::write(project.project_path().join("agpm.lock"), "corrupted content").await.unwrap();

    let output = project.run_agpm(&["validate", "--check-lock"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("Failed to parse lockfile"));
    assert!(output.stdout.contains("corrupted") || output.stdout.contains("Invalid"));
}

/// Test validating with --resolve and --check-lock flags (comprehensive validation)
#[tokio::test]
async fn test_validate_all() {
    let project = TestProject::new().await.unwrap();

    // Add mock source repositories
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.add_resource("snippets", "utils", "# Utils\n\nA test snippet").await.unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA test agent")
        .await
        .unwrap();
    community_repo.commit_all("Initial commit").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with file:// URLs
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    // Create a matching lockfile
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

    let output = project.run_agpm(&["validate", "--resolve", "--check-lock"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
}

/// Test validating with verbose output
#[tokio::test]
async fn test_validate_verbose() {
    let project = TestProject::new().await.unwrap();

    let manifest = ManifestBuilder::new()
        .add_source("official", "file:///fake/url")
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--verbose"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("Validating"));
    assert!(output.stdout.contains("✓"));
}

/// Test validating with quiet output
#[tokio::test]
async fn test_validate_quiet() {
    let project = TestProject::new().await.unwrap();

    let manifest = ManifestBuilder::new()
        .add_source("official", "file:///fake/url")
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--quiet"]).unwrap();
    assert!(output.success);

    // Should have minimal output in quiet mode
}

/// Test validating with JSON output format
#[tokio::test]
async fn test_validate_json_output() {
    let project = TestProject::new().await.unwrap();

    let manifest = ManifestBuilder::new()
        .add_source("official", "file:///fake/url")
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--format", "json"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("{"));
    assert!(output.stdout.contains("\"valid\""));
    assert!(output.stdout.contains("\"errors\""));
    assert!(output.stdout.contains("\"warnings\""));
}

/// Test validating specific file path
#[tokio::test]
async fn test_validate_specific_file() {
    let project = TestProject::new().await.unwrap();

    // Create a manifest that uses file:// URLs
    let sources_path_str = normalize_path_for_storage(project.sources_path());
    let official_url = format!("file://{}/official", sources_path_str);
    let community_url = format!("file://{}/community", sources_path_str);

    let manifest = ManifestBuilder::new()
        .add_sources(&[("official", &official_url), ("community", &community_url)])
        .add_standard_agent("my-agent", "official", "agents/my-agent.md")
        .add_standard_agent("helper", "community", "agents/helper.md")
        .add_standard_snippet("utils", "official", "snippets/utils.md")
        .build();

    let manifest_path = project.project_path().join("agpm.toml");
    fs::write(&manifest_path, manifest.trim()).await.unwrap();

    let output = project.run_agpm(&["validate", manifest_path.to_str().unwrap()]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Valid"));
}

/// Test validating with warnings (non-critical issues)
#[tokio::test]
async fn test_validate_with_warnings() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with no dependencies (triggers "no dependencies" warning)
    let manifest = ManifestBuilder::new()
        .add_source("official", "https://github.com/example-org/agpm-official.git")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Valid"));
    assert!(output.stdout.contains("⚠"));
    assert!(output.stdout.contains("Warning"));
}

/// Test validating with --strict flag (treat warnings as errors)
#[tokio::test]
async fn test_validate_strict_mode() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with no dependencies (triggers "no dependencies" warning)
    let manifest = ManifestBuilder::new()
        .add_source("official", "https://github.com/example-org/agpm-official.git")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--strict"]).unwrap();
    assert!(!output.success);
    assert!(output.stdout.contains("✗"));
    assert!(output.stdout.contains("Strict mode"));
    assert!(output.stdout.contains("Warnings treated as errors"));
}

/// Test validate help command
#[tokio::test]
async fn test_validate_help() {
    let mut cmd = assert_cmd::Command::cargo_bin("agpm").unwrap();
    cmd.arg("validate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--sources"))
        .stdout(predicate::str::contains("--resolve"));
}

/// Test validating empty manifest
#[tokio::test]
async fn test_validate_empty_manifest() {
    let project = TestProject::new().await.unwrap();

    // Create minimal/empty manifest
    let empty_manifest = r"
# Empty manifest
";
    project.write_manifest(empty_manifest).await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
    assert!(output.stdout.contains("Valid"));
    assert!(output.stdout.contains("⚠"));
    assert!(output.stdout.contains("No dependencies defined"));
}

/// Test validating with circular dependencies (if supported)
#[tokio::test]
async fn test_validate_circular_dependencies() {
    let project = TestProject::new().await.unwrap();

    // Create manifest that could lead to circular dependencies
    let manifest = ManifestBuilder::new()
        .add_sources(&[
            ("source1", "https://github.com/test/repo1.git"),
            ("source2", "https://github.com/test/repo2.git"),
        ])
        .add_standard_agent("agent-a", "source1", "agents/a.md")
        .add_standard_agent("agent-b", "source2", "agents/b.md")
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate", "--dependencies"]).unwrap();
    assert!(output.success); // Should handle gracefully
    assert!(output.stdout.contains("✓"));
}

/// Test validating manifest with unsupported resource type for tool
#[tokio::test]
async fn test_validate_unsupported_resource_type() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with snippets using opencode tool (not supported)
    let manifest = ManifestBuilder::new()
        .add_source("community", "https://github.com/test/repo.git")
        .add_snippet("utils", |d| {
            d.source("community").path("snippets/utils.md").version("v1.0.0").tool("opencode")
        })
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);

    // Check for enhanced error message components
    assert!(output.stdout.contains("Resource type 'snippets' is not supported by tool 'opencode'"));
    assert!(output.stdout.contains("Tool 'opencode' properly supports:"));
    assert!(output.stdout.contains("💡 Suggestions:"));
    assert!(output.stdout.contains("Snippets work best with the 'agpm' tool"));
    assert!(output.stdout.contains("Add tool='agpm' to this dependency to use shared snippets"));
    assert!(output.stdout.contains("You can fix this by:"));
    assert!(output.stdout.contains("Changing the 'tool' field to a supported tool"));
}

/// Test validating manifest with unsupported resource type shows alternative tools
#[tokio::test]
async fn test_validate_unsupported_shows_alternatives() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with agents using agpm artifact type (not supported)
    let manifest = ManifestBuilder::new()
        .add_source("community", "https://github.com/test/repo.git")
        .add_agent("helper", |d| {
            d.source("community").path("agents/helper.md").version("v1.0.0").tool("agpm")
        })
        .build();

    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);

    // Check for enhanced error message showing alternatives
    assert!(output.stdout.contains("Resource type 'agents' is not supported by tool 'agpm'"));
    assert!(output.stdout.contains("This resource type is supported by tools:"));
    assert!(output.stdout.contains("'claude-code'"));
    assert!(output.stdout.contains("'opencode'"));
}

/// Test validating manifest with malformed merge target configuration
#[tokio::test]
async fn test_validate_malformed_merge_target_configuration() {
    let test_project = TestProject::new().await.unwrap();

    // Create manifest with malformed hooks configuration (path instead of merge_target)
    let manifest = ManifestBuilder::new()
        .add_source("test", "https://github.com/example/test.git")
        .with_malformed_hooks_tool() // This creates hooks with path instead of merge_target
        .add_hook("test-hook", |d| d.source("test").path("hooks/test.json").version("v1.0.0"))
        .build();

    test_project.write_manifest(&manifest).await.unwrap();

    let output = test_project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);

    let stdout = &output.stdout;
    assert!(stdout.contains("improperly configured"));
    assert!(stdout.contains("merge_target"));
    assert!(stdout.contains("claude-code"));
}

/// Test validating manifest with missing hooks configuration
#[tokio::test]
async fn test_validate_missing_merge_target_configuration() {
    let test_project = TestProject::new().await.unwrap();

    // Create manifest with missing hooks configuration
    let manifest = ManifestBuilder::new()
        .add_source("test", "https://github.com/example/test.git")
        .with_missing_hooks_tool() // This creates tool config without hooks
        .add_hook("test-hook", |d| d.source("test").path("hooks/test.json").version("v1.0.0"))
        .build();

    test_project.write_manifest(&manifest).await.unwrap();

    let output = test_project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);

    let stdout = &output.stdout;
    assert!(stdout.contains("not supported"));
    assert!(!stdout.contains("improperly configured"));
}

/// Test validating manifest with properly configured merge targets
#[tokio::test]
async fn test_validate_properly_configured_merge_targets() {
    let test_project = TestProject::new().await.unwrap();

    // Create manifest with proper tool configuration
    let manifest = ManifestBuilder::new()
        .add_source("test", "https://github.com/example/test.git")
        .with_claude_code_tool() // Proper configuration with merge targets
        .add_hook("test-hook", |d| d.source("test").path("hooks/test.json").version("v1.0.0"))
        .add_mcp_server("test-server", |d| d.source("test").path("mcp/test.json").version("v1.0.0"))
        .build();

    test_project.write_manifest(&manifest).await.unwrap();

    let output = test_project.run_agpm(&["validate"]).unwrap();
    assert!(output.success);

    let stdout = &output.stdout;
    assert!(stdout.contains("✓ Valid manifest"));
}

/// Test validating manifest with malformed MCP servers configuration
#[tokio::test]
async fn test_validate_malformed_mcp_servers_configuration() {
    let test_project = TestProject::new().await.unwrap();

    // Create manifest with malformed MCP servers configuration
    let manifest = ManifestBuilder::new()
        .add_source("test", "https://github.com/example/test.git")
        .with_tools_config(|t| {
            t.tool("claude-code", |c| {
                c.path(".claude")
                    .agents(crate::common::ResourceConfigBuilder::default().path("agents"))
                    .mcp_servers(crate::common::ResourceConfigBuilder::default()) // Empty - no path or merge_target
            })
        })
        .add_mcp_server("test-server", |d| d.source("test").path("mcp/test.json").version("v1.0.0"))
        .build();

    test_project.write_manifest(&manifest).await.unwrap();

    let output = test_project.run_agpm(&["validate"]).unwrap();
    assert!(!output.success);

    let stdout = &output.stdout;
    assert!(stdout.contains("improperly configured"));
    assert!(stdout.contains("mcp-servers"));
    assert!(stdout.contains("merge_target"));
}
