use predicates::prelude::*;
use std::fs;

mod fixtures;
use fixtures::{ManifestFixture, TestEnvironment};

/// Test listing installed resources from lockfile
#[test]
fn test_list_installed_resources() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed resources"))
        .stdout(predicate::str::contains("my-agent"))
        .stdout(predicate::str::contains("helper"))
        .stdout(predicate::str::contains("utils"));
}

/// Test listing with no lockfile
#[test]
fn test_list_no_lockfile() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list").assert().success().stdout(
        predicate::str::contains("No installed resources")
            .or(predicate::str::contains("ccpm.lock not found")),
    );
}

/// Test listing without project
#[test]
fn test_list_without_project() {
    let env = TestEnvironment::new().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("ccpm.toml not found"));
}

/// Test list with table format
#[test]
fn test_list_table_format() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--format")
        .arg("table")
        .assert()
        .success()
        .stdout(predicate::str::contains("Name"))
        .stdout(predicate::str::contains("Version"))
        .stdout(predicate::str::contains("Source"))
        .stdout(predicate::str::contains("Type"))
        .stdout(predicate::str::contains("my-agent"));
}

/// Test list with JSON format
#[test]
fn test_list_json_format() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("{"))
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"version\""))
        .stdout(predicate::str::contains("\"source\""))
        .stdout(predicate::str::contains("\"my-agent\""));
}

/// Test list with YAML format
#[test]
fn test_list_yaml_format() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--format")
        .arg("yaml")
        .assert()
        .success()
        .stdout(predicate::str::contains("name:"))
        .stdout(predicate::str::contains("version:"))
        .stdout(predicate::str::contains("source:"))
        .stdout(predicate::str::contains("my-agent"));
}

/// Test list with compact format
#[test]
fn test_list_compact_format() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--format")
        .arg("compact")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"))
        .stdout(predicate::str::contains("v1.0.0"))
        .stdout(predicate::str::contains("official"));
}

/// Test filtering by resource type - agents only
#[test]
fn test_list_agents_only() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--type")
        .arg("agents")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"))
        .stdout(predicate::str::contains("helper"))
        .stdout(predicate::str::contains("utils").not()); // utils is a snippet
}

/// Test filtering by resource type - snippets only
#[test]
fn test_list_snippets_only() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--type")
        .arg("snippets")
        .assert()
        .success()
        .stdout(predicate::str::contains("utils"))
        .stdout(predicate::str::contains("my-agent").not()) // my-agent is an agent
        .stdout(predicate::str::contains("helper").not()); // helper is an agent
}

/// Test filtering by source
#[test]
fn test_list_filter_by_source() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--source")
        .arg("official")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"))
        .stdout(predicate::str::contains("utils"))
        .stdout(predicate::str::contains("helper").not()); // helper is from community source
}

/// Test listing with search/filter by name
#[test]
fn test_list_search_by_name() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--search")
        .arg("agent")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"))
        .stdout(predicate::str::contains("utils").not());
}

/// Test listing with detailed/verbose output
#[test]
fn test_list_detailed() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--detailed")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"))
        .stdout(predicate::str::contains("Path:"))
        .stdout(predicate::str::contains("Checksum:"))
        .stdout(predicate::str::contains("Installed at:"))
        .stdout(predicate::str::contains("agents/my-agent.md"));
}

/// Test listing installed files (show actual file paths)
#[test]
fn test_list_installed_files() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    // Create some installed files to match lockfile
    let agents_dir = env.project_path().join("agents");
    let snippets_dir = env.project_path().join("snippets");
    fs::create_dir_all(&agents_dir).unwrap();
    fs::create_dir_all(&snippets_dir).unwrap();

    fs::write(agents_dir.join("my-agent.md"), "# My Agent").unwrap();
    fs::write(agents_dir.join("helper.md"), "# Helper Agent").unwrap();
    fs::write(snippets_dir.join("utils.md"), "# Utils Snippet").unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--files")
        .assert()
        .success()
        .stdout(predicate::str::contains("agents/my-agent.md"))
        .stdout(predicate::str::contains("agents/helper.md"))
        .stdout(predicate::str::contains("snippets/utils.md"));
}

/// Test listing with sorting options
#[test]
fn test_list_sorted_by_name() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--sort")
        .arg("name")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"));
}

/// Test listing sorted by version
#[test]
fn test_list_sorted_by_version() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--sort")
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"));
}

/// Test listing sorted by source
#[test]
fn test_list_sorted_by_source() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--sort")
        .arg("source")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-agent"));
}

/// Test list with local dependencies
#[test]
fn test_list_local_dependencies() {
    let env = TestEnvironment::new().unwrap();
    ManifestFixture::with_local()
        .write_to(env.project_path())
        .unwrap();

    // Create lockfile with local dependencies
    let lockfile_content = r#"
# Auto-generated lockfile - DO NOT EDIT
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
    fs::write(env.project_path().join("ccpm.lock"), lockfile_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("local-agent"))
        .stdout(predicate::str::contains("local-utils"))
        .stdout(predicate::str::contains("local"));
}

/// Test list help command
#[test]
fn test_list_help() {
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
#[test]
fn test_list_empty_project() {
    let env = TestEnvironment::new().unwrap();

    // Create minimal manifest with no dependencies
    let minimal_manifest = r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"
"#;
    fs::write(env.project_path().join("ccpm.toml"), minimal_manifest).unwrap();

    // Create empty lockfile
    let empty_lockfile = r"
# Auto-generated lockfile - DO NOT EDIT
version = 1
";
    fs::write(env.project_path().join("ccpm.lock"), empty_lockfile).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list").assert().success().stdout(
        predicate::str::contains("No installed resources")
            .or(predicate::str::contains("Empty project")),
    );
}

/// Test list with corrupted lockfile
#[test]
fn test_list_corrupted_lockfile() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create corrupted lockfile
    fs::write(env.project_path().join("ccpm.lock"), "corrupted content").unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list").assert().failure().stderr(
        predicate::str::contains("Invalid lockfile syntax")
            .or(predicate::str::contains("Failed to parse lockfile")),
    );
}

/// Test list with invalid format option
#[test]
fn test_list_invalid_format() {
    let env = TestEnvironment::with_manifest_and_lockfile().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("list")
        .arg("--format")
        .arg("invalid")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid format"));
}
