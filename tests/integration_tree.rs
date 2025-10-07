//! Integration tests for the `agpm tree` command.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use tokio::fs;

async fn create_test_manifest(dir: &std::path::Path) {
    let manifest_content = r#"
[sources]
community = "https://github.com/example/community.git"

[agents]
code-reviewer = { source = "community", path = "agents/reviewer.md", version = "v1.0.0" }
local-agent = { path = "../local/agent.md" }

[snippets]
utils = { source = "community", path = "snippets/utils.md", version = "v1.0.0" }
"#;

    fs::write(dir.join("agpm.toml"), manifest_content).await.unwrap();
}

async fn create_test_lockfile(dir: &std::path::Path) {
    let lockfile_content = r#"
version = 1

[[sources]]
name = "community"
url = "https://github.com/example/community.git"
fetched_at = "2024-01-01T00:00:00Z"

[[agents]]
name = "code-reviewer"
source = "community"
url = "https://github.com/example/community.git"
path = "agents/reviewer.md"
version = "v1.0.0"
resolved_commit = "abc123def456"
checksum = "sha256:abc123"
installed_at = ".claude/agents/code-reviewer.md"
dependencies = ["agent/rust-helper", "snippet/utils"]
resource_type = "agent"

[[agents]]
name = "rust-helper"
source = "community"
url = "https://github.com/example/community.git"
path = "agents/rust-helper.md"
version = "v1.0.0"
resolved_commit = "abc123def456"
checksum = "sha256:def456"
installed_at = ".claude/agents/rust-helper.md"
dependencies = []
resource_type = "agent"

[[agents]]
name = "local-agent"
path = "../local/agent.md"
checksum = "sha256:local123"
installed_at = ".claude/agents/local-agent.md"
dependencies = []
resource_type = "agent"

[[snippets]]
name = "utils"
source = "community"
url = "https://github.com/example/community.git"
path = "snippets/utils.md"
version = "v1.0.0"
resolved_commit = "abc123def456"
checksum = "sha256:ghi789"
installed_at = ".claude/snippets/utils.md"
dependencies = []
resource_type = "snippet"
"#;

    fs::write(dir.join("agpm.lock"), lockfile_content).await.unwrap();
}

#[tokio::test]
async fn test_tree_no_lockfile() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No lockfile found"))
        .stdout(predicate::str::contains("agpm install"));
}

#[tokio::test]
async fn test_tree_basic() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("code-reviewer"))
        .stdout(predicate::str::contains("rust-helper"))
        .stdout(predicate::str::contains("utils"))
        .stdout(predicate::str::contains("local-agent"));
}

#[tokio::test]
async fn test_tree_with_depth() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--depth").arg("1");

    cmd.assert().success();
}

#[tokio::test]
async fn test_tree_json_format() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--format").arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(r#""project":"#))
        .stdout(predicate::str::contains(r#""roots":"#));
}

#[tokio::test]
async fn test_tree_text_format() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--format").arg("text");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("code-reviewer"))
        .stdout(predicate::str::contains("agent/"));
}

#[tokio::test]
async fn test_tree_invalid_format() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--format").arg("invalid");

    cmd.assert().failure().stderr(predicate::str::contains("Invalid format"));
}

#[tokio::test]
async fn test_tree_zero_depth() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--depth").arg("0");

    cmd.assert().failure().stderr(predicate::str::contains("must be at least 1"));
}

#[tokio::test]
async fn test_tree_filter_agents() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--agents");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("agent/"))
        .stdout(predicate::str::contains("code-reviewer"));
}

#[tokio::test]
async fn test_tree_filter_snippets() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--snippets");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("snippet/"))
        .stdout(predicate::str::contains("utils"));
}

#[tokio::test]
async fn test_tree_specific_package() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--package").arg("code-reviewer");

    cmd.assert().success().stdout(predicate::str::contains("code-reviewer"));
}

#[tokio::test]
async fn test_tree_package_not_found() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--package").arg("nonexistent");

    cmd.assert().failure().stderr(predicate::str::contains("not found"));
}

#[tokio::test]
async fn test_tree_with_transitive_deps() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree");

    // Should show code-reviewer with its dependencies
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("code-reviewer"))
        .stdout(predicate::str::contains("rust-helper"))
        .stdout(predicate::str::contains("utils"));
}

#[tokio::test]
async fn test_tree_no_manifest() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree");

    cmd.assert().failure().stderr(predicate::str::contains("Manifest file agpm.toml not found"));
}

#[tokio::test]
async fn test_tree_with_duplicates_flag() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--duplicates");

    cmd.assert().success();
}

#[tokio::test]
async fn test_tree_no_dedupe() {
    let temp = TempDir::new().unwrap();
    create_test_manifest(temp.path()).await;
    create_test_lockfile(temp.path()).await;

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp.path()).arg("tree").arg("--no-dedupe");

    cmd.assert().success();
}
