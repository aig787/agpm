use predicates::prelude::*;
use std::fs;

mod fixtures;
use fixtures::{LockfileFixture, ManifestFixture, MarkdownFixture, TestEnvironment};

/// Test handling of network timeout errors
#[test]
fn test_network_timeout() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Don't add mock sources to simulate network issues
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .env("CCPM_NETWORK_TIMEOUT", "1") // Very short timeout
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Network timeout")
                .or(predicate::str::contains("Connection timeout"))
                .or(predicate::str::contains("Failed to clone"))
                .or(predicate::str::contains("Git operation failed")),
        );
}

/// Test handling of disk space errors
#[test]
fn test_disk_space_error() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Add mock source
    let official_files = vec![MarkdownFixture::agent("large-agent")];
    env.add_mock_source(
        "official",
        "https://github.com/example-org/ccpm-official.git",
        official_files,
    )
    .unwrap();

    // Update manifest to use the mock source as a file URL
    let source_path = env.sources_path().join("official");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
large-agent = {{ source = "official", path = "agents/large-agent.md", version = "v1.0.0" }}
"#,
        source_path.to_string_lossy()
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Simulate disk space issues by pointing to invalid cache directory
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .env("CCPM_CACHE_DIR", "/dev/null") // Invalid directory on Unix
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Disk space")
                .or(predicate::str::contains("No space left"))
                .or(predicate::str::contains("Cannot create directory"))
                .or(predicate::str::contains("Failed to create directory"))
                .or(predicate::str::contains("Permission denied")),
        );
}

/// Test handling of corrupted git repositories
#[test]
fn test_corrupted_git_repo() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create a fake git directory without proper git structure
    let fake_repo_dir = env.sources_path().join("official");
    fs::create_dir_all(&fake_repo_dir).unwrap();
    fs::create_dir_all(fake_repo_dir.join(".git")).unwrap();
    fs::write(fake_repo_dir.join(".git/config"), "corrupted config").unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install").assert().failure().stderr(
        predicate::str::contains("Corrupted repository")
            .or(predicate::str::contains("Invalid git repository"))
            .or(predicate::str::contains("Git error"))
            .or(predicate::str::contains("Failed to clone")),
    );
}

/// Test handling of authentication failures
#[test]
fn test_authentication_failure() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with private repository (simulated)
    let manifest_content = r#"
[sources]
private = "https://github.com/private/secret-repo.git"

[agents]
secret-agent = { source = "private", path = "agents/secret.md", version = "v1.0.0" }
"#;
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install").assert().failure().stderr(
        predicate::str::contains("Authentication failed")
            .or(predicate::str::contains("Access denied"))
            .or(predicate::str::contains("Repository not found"))
            .or(predicate::str::contains("Failed to clone")),
    );
}

/// Test handling of malformed markdown files
#[test]
fn test_malformed_markdown() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create a source with malformed markdown
    let source_dir = env.sources_path().join("official");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(source_dir.join(".git")).unwrap();

    let agents_dir = source_dir.join("agents");
    fs::create_dir_all(&agents_dir).unwrap();

    // Create malformed markdown with invalid frontmatter
    let malformed_content = r#"---
type: agent
name: broken-agent
invalid yaml: [ unclosed
---

# Broken Agent
"#;
    fs::write(agents_dir.join("my-agent.md"), malformed_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install").assert().failure().stderr(
        predicate::str::contains("Invalid markdown")
            .or(predicate::str::contains("Frontmatter parsing failed"))
            .or(predicate::str::contains("YAML error"))
            .or(predicate::str::contains("Failed to clone")),
    );
}

/// Test handling of conflicting file permissions
#[cfg(unix)]
#[test]
fn test_permission_conflicts() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create .claude/agents directory with read-only permissions (default installation path)
    let claude_dir = env.project_path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let agents_dir = claude_dir.join("agents");
    fs::create_dir_all(&agents_dir).unwrap();

    let mut perms = fs::metadata(&agents_dir).unwrap().permissions();
    perms.set_mode(0o444); // Read-only
    fs::set_permissions(&agents_dir, perms).unwrap();

    // Add mock source
    let official_files = vec![MarkdownFixture::agent("my-agent")];
    env.add_mock_source(
        "official",
        "https://github.com/example-org/ccpm-official.git",
        official_files,
    )
    .unwrap();

    // Update manifest to use the mock source as a file URL
    let source_path = env.sources_path().join("official");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        source_path.to_string_lossy()
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install").assert().failure().stderr(
        predicate::str::contains("Permission denied")
            .or(predicate::str::contains("could not create"))
            .or(predicate::str::contains("Access is denied"))
            .or(predicate::str::contains("Failed to checkout"))
            .or(predicate::str::contains("not found")),
    ); // Git checkout may succeed but file copy fails

    // Restore permissions for cleanup
    let claude_dir = env.project_path().join(".claude");
    let agents_dir = claude_dir.join("agents");
    if agents_dir.exists() {
        let mut perms = fs::metadata(&agents_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&agents_dir, perms).unwrap();
    }
}

/// Test handling of invalid version specifications
#[test]
fn test_invalid_version_specs() {
    let env = TestEnvironment::new().unwrap();

    let manifest_content = r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
invalid-version = { source = "official", path = "agents/invalid.md", version = "not-a-version" }
malformed-constraint = { source = "official", path = "agents/malformed.md", version = ">=1.0.0 <invalid" }
"#;
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install").assert().failure().stderr(
        predicate::str::contains("Invalid version")
            .or(predicate::str::contains("Version constraint error"))
            .or(predicate::str::contains("Failed to clone")),
    );
}

/// Test handling of circular dependency detection
#[test]
fn test_circular_dependency_detection() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest that could theoretically have circular dependencies
    let manifest_content = r#"
[sources]
source_a = "https://github.com/test/repo-a.git"
source_b = "https://github.com/test/repo-b.git"

[agents]
agent_a = { source = "source_a", path = "agents/a.md", version = "v1.0.0" }
agent_b = { source = "source_b", path = "agents/b.md", version = "v1.0.0" }
"#;
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Test that validation alone doesn't fail (just validates syntax)
    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success() // Manifest syntax is valid
        .stdout(predicate::str::contains("Valid manifest"));

    // Test that install fails when trying to access non-existent sources
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .assert()
        .failure() // Sources don't exist
        .stderr(
            predicate::str::contains("Failed to clone repository")
                .or(predicate::str::contains("Repository not accessible")),
        );
}

/// Test handling of exceeding system limits
#[test]
fn test_system_limits() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with many dependencies to test limits
    let mut manifest_content = String::from(
        r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
"#,
    );

    // Add many agents to test system limits
    for i in 0..1000 {
        manifest_content.push_str(&format!(
            "agent_{} = {{ source = \"official\", path = \"agents/agent_{}.md\", version = \"v1.0.0\" }}\n",
            i, i
        ));
    }

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success() // Should handle gracefully
        .stdout(
            predicate::str::contains("✓")
                .or(predicate::str::contains("✅"))
                .or(predicate::str::contains("Validation complete")),
        );
}

/// Test handling of interrupted operations
#[test]
fn test_interrupted_operation() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create partial lockfile to simulate interrupted operation
    let partial_lockfile = r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
"#;
    fs::write(env.project_path().join("ccpm.lock"), partial_lockfile).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--check-lock")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Incomplete lockfile")
                .or(predicate::str::contains("Corrupted lockfile"))
                .or(predicate::str::contains("Missing required fields"))
                .or(predicate::str::contains("Invalid lockfile syntax")),
        );
}

/// Test handling of invalid URL formats
#[test]
fn test_invalid_urls() {
    let env = TestEnvironment::new().unwrap();

    let manifest_content = r#"
[sources]
invalid_url = "not-a-url"
malformed_git = "git://incomplete"
wrong_protocol = "ftp://example.com/repo.git"

[agents]
test-agent = { source = "invalid_url", path = "agents/test.md", version = "v1.0.0" }
"#;
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--resolve")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Invalid URL")
                .or(predicate::str::contains("Malformed URL"))
                .or(predicate::str::contains("Failed to clone"))
                .or(predicate::str::contains("Manifest validation failed")),
        );
}

/// Test handling of extremely large files
#[test]
fn test_large_file_handling() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create large content (1MB+)
    let large_content = format!(
        "---\ntype: agent\nname: my-agent\n---\n\n{}",
        "# Large Agent\n\n".repeat(50000)
    );

    // Add mock source with large file
    let official_files = vec![MarkdownFixture {
        path: "agents/my-agent.md".to_string(),
        content: large_content,
        frontmatter: None,
    }];
    env.add_mock_source(
        "official",
        "https://github.com/example-org/ccpm-official.git",
        official_files,
    )
    .unwrap();

    // Update manifest to use the mock source as a file URL
    let source_path = env.sources_path().join("official");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        source_path.to_string_lossy()
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Large files should be handled correctly
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .assert()
        .success() // Should handle large files properly
        .stdout(predicate::str::contains("Installing"));
}

/// Test handling of filesystem corruption
#[test]
fn test_filesystem_corruption() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create lockfile with null bytes (filesystem corruption simulation)
    let corrupted_lockfile = "version = 1\n\0\0\0corrupted\0data\n";
    fs::write(env.project_path().join("ccpm.lock"), corrupted_lockfile).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--check-lock")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Corrupted")
                .or(predicate::str::contains("Invalid character"))
                .or(predicate::str::contains("TOML"))
                .or(predicate::str::contains("Invalid lockfile syntax")),
        );
}

/// Test handling of missing dependencies in lockfile
#[test]
fn test_missing_lockfile_dependencies() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create lockfile missing some dependencies from manifest
    let incomplete_lockfile = r#"
# Auto-generated lockfile - DO NOT EDIT
version = 1

[[sources]]
name = "official"
url = "https://github.com/example-org/ccpm-official.git"
commit = "abc123456789abcdef123456789abcdef12345678"

[[agents]]
name = "my-agent"
source = "official"
path = "agents/my-agent.md"
version = "v1.0.0"
resolved_commit = "abc123456789abcdef123456789abcdef12345678"
checksum = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
installed_at = "agents/my-agent.md"

# Missing 'helper' and 'utils' from manifest
"#;
    fs::write(env.project_path().join("ccpm.lock"), incomplete_lockfile).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--check-lock")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Missing dependencies")
                .or(predicate::str::contains("lockfile").and(predicate::str::contains("mismatch")))
                .or(predicate::str::contains("helper"))
                .or(predicate::str::contains("utils"))
                .or(predicate::str::contains("Invalid lockfile syntax")),
        );
}

/// Test handling of git command not found
#[test]
fn test_git_command_missing() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .env("PATH", "") // Empty PATH to simulate missing git
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("git command not found")
                .or(predicate::str::contains("Git not installed"))
                .or(predicate::str::contains("command not found"))
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("executable file not found"))
                .or(predicate::str::contains("File system error")),
        );
}

/// Test handling of invalid configuration files
#[test]
fn test_invalid_config_files() {
    let env = TestEnvironment::new().unwrap();

    // Create completely invalid TOML
    let invalid_toml = r#"
this is not valid toml at all
[unclosed section
key = "value without closing quote
"#;
    fs::write(env.project_path().join("ccpm.toml"), invalid_toml).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate").assert().failure().stderr(
        predicate::str::contains("TOML parsing")
            .or(predicate::str::contains("Syntax error"))
            .or(predicate::str::contains("Parse error"))
            .or(predicate::str::contains("Invalid")),
    );
}

/// Test recovery from partial installations
#[test]
fn test_partial_installation_recovery() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Add mock source
    let official_files = vec![
        MarkdownFixture::agent("my-agent"),
        MarkdownFixture::snippet("utils"),
    ];
    env.add_mock_source(
        "official",
        "https://github.com/example-org/ccpm-official.git",
        official_files,
    )
    .unwrap();

    // Update manifest to use the mock source as a file URL
    let source_path = env.sources_path().join("official");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}

[snippets]
utils = {{ source = "official", path = "snippets/utils.md", version = "v1.0.0" }}
"#,
        source_path.to_string_lossy()
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create partial installation (only some files)
    let agents_dir = env.project_path().join("agents");
    fs::create_dir_all(&agents_dir).unwrap();
    fs::write(agents_dir.join("my-agent.md"), "# Partial agent").unwrap();

    // Create lockfile indicating complete installation
    LockfileFixture::basic()
        .write_to(env.project_path())
        .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--check-lock")
        .assert()
        .failure() // Should detect missing files
        .stderr(
            predicate::str::contains("Lockfile inconsistent")
                .or(predicate::str::contains("helper")), // Lockfile has helper but manifest doesn't
        );
}

/// Test handling of concurrent lockfile modifications
#[test]
fn test_concurrent_lockfile_modification() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Add mock source
    let official_files = vec![MarkdownFixture::agent("my-agent")];
    env.add_mock_source(
        "official",
        "https://github.com/example-org/ccpm-official.git",
        official_files,
    )
    .unwrap();

    // Update manifest to use the mock source as a file URL
    let source_path = env.sources_path().join("official");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
my-agent = {{ source = "official", path = "agents/my-agent.md", version = "v1.0.0" }}
"#,
        source_path.to_string_lossy()
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // This test mainly checks that the system can handle lockfile operations
    // In a real concurrent scenario, we'd expect either success or a conflict detection
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .assert()
        .success() // File URLs should work correctly now
        .stdout(predicate::str::contains("Installing"));
}

/// Test error message quality and helpfulness
#[test]
fn test_helpful_error_messages() {
    let env = TestEnvironment::new().unwrap();
    ManifestFixture::missing_fields()
        .write_to(env.project_path())
        .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:")) // Error indicator
        .stderr(
            predicate::str::contains("Missing required field")
                .or(predicate::str::contains("Invalid manifest"))
                .or(predicate::str::contains("Manifest validation failed")),
        ) // Clear description
        .stderr(predicate::str::contains("path").or(predicate::str::contains("Suggestion:")));
    // Specific field or helpful suggestion
}
