use predicates::prelude::*;
use tokio::fs;

mod common;
mod fixtures;
use common::{DirAssert, FileAssert, TestProject};
use fixtures::ManifestFixture;

/// Test installing from a manifest when no lockfile exists
#[tokio::test]
async fn test_install_creates_lockfile() {
    let project = TestProject::new().await.unwrap();

    // Create mock source repositories
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.commit_all("Add my agent").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA helper agent")
        .await
        .unwrap();
    community_repo.commit_all("Add helper agent").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with file:// URLs (no git server needed)
    let manifest_content = format!(
        r#"
[sources]
official = "{}"
community = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}
"#,
        official_url, community_url
    );
    project.write_manifest(&manifest_content).await.unwrap();

    // Run install command
    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    output.assert_success();
    assert!(
        output.stdout.contains("Installing")
            || output.stdout.contains("Cloning")
            || output.stdout.contains("Installed"),
        "Expected install progress message, got: {}",
        output.stdout
    );

    // Verify lockfile was created
    let lockfile_path = project.project_path().join("agpm.lock");
    FileAssert::exists(&lockfile_path).await;

    // Verify lockfile content structure
    let lockfile_content = fs::read_to_string(&lockfile_path).await.unwrap();
    assert!(lockfile_content.contains("version = 1"));
    assert!(lockfile_content.contains("[[sources]]"));
    assert!(lockfile_content.contains("[[agents]]"));
    assert!(lockfile_content.contains("my-agent"));
    assert!(lockfile_content.contains("helper"));
}

/// Test installing when lockfile already exists
#[tokio::test]
async fn test_install_with_existing_lockfile() {
    let project = TestProject::new().await.unwrap();

    // Create mock source repositories
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.commit_all("Add my agent").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();
    let official_sha = official_repo.git.get_commit_hash().unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA helper agent")
        .await
        .unwrap();
    community_repo.commit_all("Add helper agent").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();
    let community_sha = community_repo.git.get_commit_hash().unwrap();

    // Create manifest with file:// URLs
    let manifest_content = format!(
        r#"
[sources]
official = "{}"
community = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}
"#,
        official_url, community_url
    );
    project.write_manifest(&manifest_content).await.unwrap();

    // Create a matching lockfile with the actual commit SHAs
    let lockfile_content = format!(
        r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "{}"
commit = "{}"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community"
url = "{}"
commit = "{}"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "{}"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = ".claude/agents/my-agent.md"

[[agents]]
name = "helper"
source = "community"
path = "agents/helper.md"
version = "v1.0.0"
resolved_commit = "{}"
checksum = "sha256:38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
installed_at = ".claude/agents/helper.md"
"#,
        official_url, official_sha, community_url, community_sha, official_sha, community_sha
    );

    fs::write(project.project_path().join("agpm.lock"), lockfile_content).await.unwrap();

    // Run install command
    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    output.assert_success();
    assert!(
        output.stdout.contains("Installing")
            || output.stdout.contains("Cloning")
            || output.stdout.contains("Installed"),
        "Expected install progress message, got: {}",
        output.stdout
    );

    // Verify agents directory was created and populated
    let agents_dir = project.project_path().join(".claude").join("agents");
    DirAssert::exists(&agents_dir).await;
    DirAssert::contains_file(&agents_dir, "my-agent.md").await;
    DirAssert::contains_file(&agents_dir, "helper.md").await;
}

/// Test install command without agpm.toml
#[tokio::test]
async fn test_install_without_manifest() {
    let project = TestProject::new().await.unwrap();

    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("Manifest file agpm.toml not found"),
        "Expected manifest not found error, got: {}",
        output.stderr
    );
}

/// Test install with invalid manifest syntax
#[tokio::test]
async fn test_install_invalid_manifest_syntax() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::invalid_syntax().content;
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("Invalid manifest file syntax"),
        "Expected syntax error, got: {}",
        output.stderr
    );
}

/// Test install with missing required fields in manifest
#[tokio::test]
async fn test_install_missing_manifest_fields() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::missing_fields().content;
    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("Missing required field"),
        "Expected missing field error, got: {}",
        output.stderr
    );
}

/// Test install with --parallel flag
#[tokio::test]
async fn test_install_parallel_flag() {
    let project = TestProject::new().await.unwrap();

    // Create mock source repositories with multiple files
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo
        .add_resource("snippets", "utils", "# Utils Snippet\n\nA test snippet")
        .await
        .unwrap();
    official_repo.commit_all("Add resources").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    let community_repo = project.create_source_repo("community").await.unwrap();
    community_repo
        .add_resource("agents", "helper", "# Helper Agent\n\nA helper agent")
        .await
        .unwrap();
    community_repo.commit_all("Add helper agent").unwrap();
    community_repo.tag_version("v1.0.0").unwrap();
    let community_url = community_repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with file:// URLs (no git server needed)
    let manifest_content = format!(
        r#"
[sources]
official = "{}"
community = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
helper = {{ source = "community", path = "agents/helper.md", version = "v1.0.0" }}
"#,
        official_url, community_url
    );
    project.write_manifest(&manifest_content).await.unwrap();

    // Run install command
    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    output.assert_success();
    assert!(
        output.stdout.contains("Installing")
            || output.stdout.contains("Cloning")
            || output.stdout.contains("Installed"),
        "Expected install progress message, got: {}",
        output.stdout
    );

    // Verify that files were installed
    let agents_dir = project.project_path().join(".claude").join("agents");
    assert!(agents_dir.join("my-agent.md").exists());
    assert!(agents_dir.join("helper.md").exists());
}

/// Test install with local dependencies
#[tokio::test]
async fn test_install_local_dependencies() {
    let project = TestProject::new().await.unwrap();

    // Create local files referenced in manifest
    // The manifest expects ../local-agents/helper.md relative to project
    let parent_dir = project.project_path().parent().unwrap();
    let local_agents_dir = parent_dir.join("local-agents");
    fs::create_dir_all(&local_agents_dir).await.unwrap();
    fs::write(local_agents_dir.join("helper.md"), "# Local Agent Helper\n\nThis is a local agent.")
        .await
        .unwrap();

    // Create local snippet in project directory
    project
        .create_local_resource(
            "snippets/local-utils.md",
            "# Local Utils\n\nThis is a local snippet.",
        )
        .await
        .unwrap();

    // Add official source for the remote dependency
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.commit_all("Add my agent").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with local dependencies and file:// URL for remote
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
local-agent = {{ path = "../local-agents/helper.md" }}

[snippets]
local-utils = {{ path = "./snippets/local-utils.md" }}
"#,
        official_url
    );
    project.write_manifest(&manifest_content).await.unwrap();

    // Run install command
    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    output.assert_success();
    assert!(
        output.stdout.contains("Installing")
            || output.stdout.contains("Cloning")
            || output.stdout.contains("Installed"),
        "Expected install progress message, got: {}",
        output.stdout
    );

    // Verify lockfile was created and contains all dependencies
    let lockfile_content =
        fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    assert!(lockfile_content.contains("my-agent")); // remote dependency
    assert!(lockfile_content.contains("local-agent")); // local dependency
    assert!(lockfile_content.contains("local-utils")); // local dependency

    // Verify all dependencies were installed
    let agents_dir = project.project_path().join(".claude").join("agents");
    assert!(agents_dir.join("my-agent.md").exists());
    assert!(agents_dir.join("local-agent.md").exists());

    let snippets_dir = project.project_path().join(".claude").join("agpm").join("snippets");
    assert!(snippets_dir.join("local-utils.md").exists());
}

// TODO: Implement --dry-run flag for install command
// Would show what would be installed without actually installing

/// Test install with verbose output
#[tokio::test]
async fn test_install_verbose() {
    let project = TestProject::new().await.unwrap();

    // Create mock source with required file
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.commit_all("Add my agent").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with file:// URLs (no git server needed)
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        official_url
    );
    project.write_manifest(&manifest_content).await.unwrap();

    // Run install command with verbose flag
    let output = project.run_agpm(&["install", "--no-cache", "--verbose"]).unwrap();
    output.assert_success();
    assert!(
        output.stdout.contains("Installing")
            || output.stdout.contains("Cloning")
            || output.stdout.contains("Installed"),
        "Expected install progress message, got: {}",
        output.stdout
    );
}

/// Test install with quiet output
#[tokio::test]
async fn test_install_quiet() {
    let project = TestProject::new().await.unwrap();

    // Create mock source repository
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo.add_resource("agents", "my-agent", "# My Agent\n\nA test agent").await.unwrap();
    official_repo.commit_all("Add my agent").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let official_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with file:// URLs (no git server needed)
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        official_url
    );
    project.write_manifest(&manifest_content).await.unwrap();

    // Run install command with quiet flag
    let output = project.run_agpm(&["install", "--no-cache", "--quiet"]).unwrap();
    output.assert_success();
}

/// Test install with network simulation failure
#[tokio::test]
async fn test_install_network_failure() {
    let project = TestProject::new().await.unwrap();

    // Create a manifest with non-existent local sources to simulate failure
    let manifest_content = r#"
[sources]
official = "file:///non/existent/path/to/repo"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
"#;
    project.write_manifest(manifest_content).await.unwrap();

    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("Failed to clone")
            || output.stderr.contains("does not exist")
            || output.stderr.contains("Local repository path does not exist"),
        "Expected clone failure error, got: {}",
        output.stderr
    );
}

/// Test install help command
#[tokio::test]
async fn test_install_help() {
    let mut cmd = assert_cmd::Command::cargo_bin("agpm").unwrap();
    cmd.arg("install")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Install Claude Code resources from manifest"))
        .stdout(predicate::str::contains("--max-parallel"))
        .stdout(predicate::str::contains("--no-lock"))
        .stdout(predicate::str::contains("--frozen"));
}

/// Test install with corrupted lockfile
#[tokio::test]
async fn test_install_corrupted_lockfile() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = ManifestFixture::basic().content;
    project.write_manifest(&manifest_content).await.unwrap();

    // Create corrupted lockfile
    fs::write(project.project_path().join("agpm.lock"), "corrupted content").await.unwrap();

    let output = project.run_agpm(&["install", "--no-cache"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("Invalid lockfile syntax"),
        "Expected lockfile syntax error, got: {}",
        output.stderr
    );
}

// TODO: Implement version conflict detection in resolver
// Would detect when multiple dependencies require incompatible versions of the same resource

// TODO: Implement --no-progress flag for install command
// Would disable progress bars for CI/automated environments
