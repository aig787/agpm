use predicates::prelude::*;
use std::fs;

mod common;
mod fixtures;
use common::{DirAssert, FileAssert};
use fixtures::{ManifestFixture, MarkdownFixture, TestEnvironment};

/// Test installing from a manifest when no lockfile exists
#[tokio::test]
async fn test_install_creates_lockfile() {
    let env = TestEnvironment::new().unwrap();

    // Create mock source repository using local files
    let official_files = vec![MarkdownFixture::agent("my-agent")];
    let community_files = vec![MarkdownFixture::agent("helper")];

    let official_repo = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();
    let community_repo = env
        .add_mock_source(
            "community",
            "https://github.com/example-org/ccpm-community.git",
            community_files,
        )
        .unwrap();

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
        fixtures::path_to_file_url(&official_repo),
        fixtures::path_to_file_url(&community_repo)
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Run install command
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Installing")
                .or(predicate::str::contains("Cloning"))
                .or(predicate::str::contains("Installed")),
        );

    // Verify lockfile was created
    let lockfile_path = env.project_path().join("ccpm.lock");
    FileAssert::exists(&lockfile_path);

    // Verify lockfile content structure
    let lockfile_content = fs::read_to_string(&lockfile_path).unwrap();
    assert!(lockfile_content.contains("version = 1"));
    assert!(lockfile_content.contains("[[sources]]"));
    assert!(lockfile_content.contains("[[agents]]"));
    assert!(lockfile_content.contains("my-agent"));
    assert!(lockfile_content.contains("helper"));
}

/// Test installing when lockfile already exists
#[tokio::test]
async fn test_install_with_existing_lockfile() {
    let env = TestEnvironment::new().unwrap();

    // Create mock source repositories using local files
    let official_files = vec![MarkdownFixture::agent("my-agent")];
    let community_files = vec![MarkdownFixture::agent("helper")];

    let official_repo = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();
    let community_repo = env
        .add_mock_source(
            "community",
            "https://github.com/example-org/ccpm-community.git",
            community_files,
        )
        .unwrap();

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
        fixtures::path_to_file_url(&official_repo),
        fixtures::path_to_file_url(&community_repo)
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Get the HEAD commit SHA for the repos
    let official_commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&official_repo)
        .output()
        .unwrap();
    let official_sha = String::from_utf8_lossy(&official_commit.stdout)
        .trim()
        .to_string();

    let community_commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&community_repo)
        .output()
        .unwrap();
    let community_sha = String::from_utf8_lossy(&community_commit.stdout)
        .trim()
        .to_string();

    // Create a matching lockfile with the actual commit SHAs
    let lockfile_content = format!(
        r#"# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "file://{}"
commit = "{}"
fetched_at = "2024-01-01T00:00:00Z"

[[sources]]
name = "community" 
url = "file://{}"
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
        fixtures::path_to_file_url(&official_repo),
        official_sha,
        fixtures::path_to_file_url(&community_repo),
        community_sha,
        official_sha,
        community_sha
    );

    fs::write(env.project_path().join("ccpm.lock"), lockfile_content).unwrap();

    // Run install command
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Installing")
                .or(predicate::str::contains("Cloning"))
                .or(predicate::str::contains("Installed")),
        );

    // Verify agents directory was created and populated
    let agents_dir = env.project_path().join(".claude").join("agents");
    DirAssert::exists(&agents_dir);
    DirAssert::contains_file(&agents_dir, "my-agent.md");
    DirAssert::contains_file(&agents_dir, "helper.md");
}

/// Test install command without ccpm.toml
#[test]
fn test_install_without_manifest() {
    let env = TestEnvironment::new().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Manifest file ccpm.toml not found",
        ));
}

/// Test install with invalid manifest syntax
#[test]
fn test_install_invalid_manifest_syntax() {
    let env = TestEnvironment::new().unwrap();
    ManifestFixture::invalid_syntax()
        .write_to(env.project_path())
        .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid manifest file syntax"));
}

/// Test install with missing required fields in manifest
#[test]
fn test_install_missing_manifest_fields() {
    let env = TestEnvironment::new().unwrap();
    ManifestFixture::missing_fields()
        .write_to(env.project_path())
        .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .failure()
        .stderr(predicate::str::contains("Missing required field"));
}

/// Test install with --parallel flag
#[tokio::test]
async fn test_install_parallel_flag() {
    let env = TestEnvironment::new().unwrap();

    // Add mock source repositories with multiple files using local files
    let official_files = vec![
        MarkdownFixture::agent("my-agent"),
        MarkdownFixture::snippet("utils"),
    ];
    let community_files = vec![MarkdownFixture::agent("helper")];

    let official_repo = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();
    let community_repo = env
        .add_mock_source(
            "community",
            "https://github.com/example-org/ccpm-community.git",
            community_files,
        )
        .unwrap();

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
        fixtures::path_to_file_url(&official_repo),
        fixtures::path_to_file_url(&community_repo)
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Run install command with parallel flag
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .arg("--max-parallel")
        .arg("2")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Installing")
                .or(predicate::str::contains("Cloning"))
                .or(predicate::str::contains("Installed")),
        );

    // Verify that files were installed
    let agents_dir = env.project_path().join(".claude").join("agents");
    assert!(agents_dir.join("my-agent.md").exists());
    assert!(agents_dir.join("helper.md").exists());
}

/// Test install with local dependencies
#[tokio::test]
async fn test_install_local_dependencies() {
    let env = TestEnvironment::new().unwrap();

    // Create local files referenced in manifest
    // The manifest expects ../local-agents/helper.md relative to project
    let parent_dir = env.project_path().parent().unwrap();
    let local_agents_dir = parent_dir.join("local-agents");
    fs::create_dir_all(&local_agents_dir).unwrap();
    fs::write(
        local_agents_dir.join("helper.md"),
        "# Local Agent Helper\n\nThis is a local agent.",
    )
    .unwrap();

    // Create local snippet in project directory
    let snippets_dir = env.project_path().join("snippets");
    fs::create_dir_all(&snippets_dir).unwrap();
    fs::write(
        snippets_dir.join("local-utils.md"),
        "# Local Utils\n\nThis is a local snippet.",
    )
    .unwrap();

    // Add official source for the remote dependency using local files
    let official_files = vec![MarkdownFixture::agent("my-agent")];
    let official_repo = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

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
        fixtures::path_to_file_url(&official_repo)
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Run install command
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Installing")
                .or(predicate::str::contains("Cloning"))
                .or(predicate::str::contains("Installed")),
        );

    // Verify lockfile was created and contains all dependencies
    let lockfile_content = fs::read_to_string(env.project_path().join("ccpm.lock")).unwrap();
    assert!(lockfile_content.contains("my-agent")); // remote dependency
    assert!(lockfile_content.contains("local-agent")); // local dependency
    assert!(lockfile_content.contains("local-utils")); // local dependency

    // Verify all dependencies were installed
    let agents_dir = env.project_path().join(".claude").join("agents");
    assert!(agents_dir.join("my-agent.md").exists());
    assert!(agents_dir.join("local-agent.md").exists());

    let snippets_dir = env
        .project_path()
        .join(".claude")
        .join("ccpm")
        .join("snippets");
    assert!(snippets_dir.join("local-utils.md").exists());
}

// TODO: Implement --dry-run flag for install command
// Would show what would be installed without actually installing

/// Test install with verbose output
#[tokio::test]
async fn test_install_verbose() {
    let env = TestEnvironment::new().unwrap();

    // Add mock source with required file using local files
    let official_files = vec![MarkdownFixture::agent("my-agent")];
    let official_repo = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Create manifest with file:// URLs (no git server needed)
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&official_repo)
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Run install command with verbose flag
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .arg("--verbose")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Installing")
                .or(predicate::str::contains("Cloning"))
                .or(predicate::str::contains("Installed")),
        );
}

/// Test install with quiet output
#[tokio::test]
async fn test_install_quiet() {
    let env = TestEnvironment::new().unwrap();

    // Create mock source repository using local files
    let official_files = vec![MarkdownFixture::agent("my-agent")];

    let official_repo = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Create manifest with file:// URLs (no git server needed)
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        fixtures::path_to_file_url(&official_repo)
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Run install command with quiet flag
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .arg("--quiet")
        .assert()
        .success();
}

/// Test install with network simulation failure
#[test]
fn test_install_network_failure() {
    let env = TestEnvironment::new().unwrap();

    // Create a manifest with non-existent local sources to simulate failure
    let manifest_content = r#"
[sources]
official = "file:///non/existent/path/to/repo"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
"#;
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Failed to clone")
                .or(predicate::str::contains("does not exist"))
                .or(predicate::str::contains(
                    "Local repository path does not exist",
                )),
        );
}

/// Test install help command
#[test]
fn test_install_help() {
    let mut cmd = assert_cmd::Command::cargo_bin("ccpm").unwrap();
    cmd.arg("install")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Install Claude Code resources from manifest",
        ))
        .stdout(predicate::str::contains("--max-parallel"))
        .stdout(predicate::str::contains("--no-lock"))
        .stdout(predicate::str::contains("--frozen"));
}

/// Test install with corrupted lockfile
#[test]
fn test_install_corrupted_lockfile() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create corrupted lockfile
    fs::write(env.project_path().join("ccpm.lock"), "corrupted content").unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid lockfile syntax"));
}

// TODO: Implement version conflict detection in resolver
// Would detect when multiple dependencies require incompatible versions of the same resource

// TODO: Implement --no-progress flag for install command
// Would disable progress bars for CI/automated environments
