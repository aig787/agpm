use predicates::prelude::*;
use std::fs;
mod fixtures;
use fixtures::{MarkdownFixture, TestEnvironment};

/// Test path handling on Windows vs Unix
#[test]
fn test_path_separators() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with mixed path separators
    let manifest_content = if cfg!(windows) {
        r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
windows-agent = { source = "official", path = "agents\\windows-agent.md", version = "v1.0.0" }
unix-agent = { source = "official", path = "agents/unix-agent.md", version = "v1.0.0" }

[snippets]
local-snippet = { path = ".\\snippets\\local.md" }
"#
    } else {
        r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
unix-agent = { source = "official", path = "agents/unix-agent.md", version = "v1.0.0" }
windows-agent = { source = "official", path = "agents\\windows-agent.md", version = "v1.0.0" }

[snippets]
local-snippet = { path = "./snippets/local.md" }
"#
    };

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create local snippet file with platform-appropriate path
    let snippets_dir = env.project_path().join("snippets");
    fs::create_dir_all(&snippets_dir).unwrap();
    MarkdownFixture::snippet("local")
        .write_to(env.project_path())
        .unwrap();

    // Add mock source with both files
    let official_files = vec![
        MarkdownFixture::agent("windows-agent"),
        MarkdownFixture::agent("unix-agent"),
    ];
    env.add_mock_source(
        "official",
        "https://github.com/example-org/ccpm-official.git",
        official_files,
    )
    .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"));
}

/// Test handling of long paths (Windows limitation)
#[cfg(windows)]
#[test]
fn test_long_paths_windows() {
    let env = TestEnvironment::new().unwrap();

    // Create a very long path that exceeds Windows' traditional 260 character limit
    let long_name = "a".repeat(200);
    let manifest_content = format!(
        r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
{} = {{ source = "official", path = "agents/{}.md", version = "v1.0.0" }}
"#,
        long_name, long_name
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Add mock source
    let official_files = vec![MarkdownFixture::agent(&long_name)];
    env.add_mock_source(
        "official",
        "https://github.com/example-org/ccpm-official.git",
        official_files,
    )
    .unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .assert()
        .success() // Should handle long paths gracefully
        .stdout(predicate::str::contains("Installing"));
}

/// Test case sensitivity differences between platforms
#[test]
fn test_case_sensitivity() {
    let env = TestEnvironment::new().unwrap();

    // On case-insensitive filesystems, creating two files with names that differ
    // only in case will result in only one file existing (the second overwrites the first)
    // So we need to use different files that actually exist
    let official_files = if cfg!(target_os = "macos") || cfg!(windows) {
        // Case-insensitive filesystem - use only one file
        vec![MarkdownFixture::agent("myagent")]
    } else {
        // Case-sensitive filesystem - can use both
        vec![
            MarkdownFixture::agent("MyAgent"),
            MarkdownFixture::agent("myagent"),
        ]
    };
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Create manifest with file:// URL
    let manifest_content = if cfg!(target_os = "macos") || cfg!(windows) {
        // Case-insensitive filesystem - only reference the file that exists
        format!(
            r#"
[sources]
official = "file://{}"

[agents]
myagent = {{ source = "official", path = "agents/myagent.md", version = "v1.0.0" }}
"#,
            source_path.display()
        )
    } else {
        // Case-sensitive filesystem - can reference both
        format!(
            r#"
[sources]
official = "file://{}"

[agents]
MyAgent = {{ source = "official", path = "agents/MyAgent.md", version = "v1.0.0" }}
myagent = {{ source = "official", path = "agents/myagent.md", version = "v1.0.0" }}
"#,
            source_path.display()
        )
    };

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();

    // The new paradigm uses manifest keys as names, so case sensitivity
    // depends on TOML key handling, not filesystem. TOML keys are case-sensitive,
    // so resources should install successfully.
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .success()
        .stdout(predicate::str::contains("Installing"));

    // Verify files were installed with correct names (in default location)
    let agents_dir = env.project_path().join(".claude").join("agents");
    if cfg!(target_os = "macos") || cfg!(windows) {
        // Case-insensitive filesystem - only one file should exist
        assert!(agents_dir.join("myagent.md").exists());
    } else {
        // Case-sensitive filesystem - both files should exist
        assert!(agents_dir.join("MyAgent.md").exists());
        assert!(agents_dir.join("myagent.md").exists());
    }
}

/// Test home directory expansion across platforms
#[test]
fn test_home_directory_expansion() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with home directory reference
    let manifest_content = if cfg!(windows) {
        r#"
[sources]
local = "~/ccpm-sources/local.git"

[agents]
home-agent = { source = "local", path = "agents/home-agent.md", version = "v1.0.0" }

[snippets]
home-snippet = { path = "~\\Documents\\snippets\\home.md" }
"#
    } else {
        r#"
[sources]
local = "~/ccpm-sources/local.git"

[agents]
home-agent = { source = "local", path = "agents/home-agent.md", version = "v1.0.0" }

[snippets]
home-snippet = { path = "~/Documents/snippets/home.md" }
"#
    };

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success() // Home directory expansion should work and validation should pass
        .stdout(predicate::str::contains("✓")); // Should succeed with valid manifest structure
}

/// Test environment variable expansion
#[test]
fn test_environment_variable_expansion() {
    // NOTE: This test explicitly tests environment variable expansion functionality
    // It uses std::env::set_var which can cause race conditions in parallel test execution.
    // If this test becomes flaky, run with: cargo test -- --test-threads=1

    let env = TestEnvironment::new().unwrap();

    // Save original value and set test environment variable
    let original = std::env::var("CCPM_TEST_PATH").ok();
    std::env::set_var("CCPM_TEST_PATH", env.temp_path().to_str().unwrap());

    let manifest_content = if cfg!(windows) {
        r#"
[sources]
env_source = "$CCPM_TEST_PATH/sources/repo.git"

[snippets]
env-snippet = { path = "%CCPM_TEST_PATH%\\snippets\\env.md" }
"#
    } else {
        r#"
[sources]
env_source = "$CCPM_TEST_PATH/sources/repo.git"

[snippets]
env-snippet = { path = "$CCPM_TEST_PATH/snippets/env.md" }
"#
    };

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create the referenced file
    let snippets_dir = env.temp_path().join("snippets");
    fs::create_dir_all(&snippets_dir).unwrap();
    fs::write(snippets_dir.join("env.md"), "# Environment snippet").unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--resolve")
        .assert()
        .success() // Should find the expanded path
        .stdout(predicate::str::contains("✓"));

    // Restore original environment variable
    match original {
        Some(val) => std::env::set_var("CCPM_TEST_PATH", val),
        None => std::env::remove_var("CCPM_TEST_PATH"),
    }
}

/// Test different line ending handling
#[test]
fn test_line_endings() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with different line endings
    let manifest_content = if cfg!(windows) {
        "[sources]\r\nofficial = \"https://github.com/example-org/ccpm-official.git\"\r\n\r\n[agents]\r\ntest-agent = { source = \"official\", path = \"agents/test.md\", version = \"v1.0.0\" }\r\n"
    } else {
        "[sources]\nofficial = \"https://github.com/example-org/ccpm-official.git\"\n\n[agents]\ntest-agent = { source = \"official\", path = \"agents/test.md\", version = \"v1.0.0\" }\n"
    };

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"));
}

/// Test git command handling across platforms
#[test]
fn test_git_command_platform() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // This test verifies that git commands work on all platforms
    // The specific git executable name might differ (git vs git.exe)
    // We expect validation to succeed but installation to fail when trying to access remote sources

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .assert()
        .failure() // Remote sources won't be accessible, but git command should be found
        .stderr(
            predicate::str::contains("git command not found")
                .not()
                .and(predicate::str::contains("git.exe not found").not()),
        );
}

/// Test permission handling across platforms
#[cfg(unix)]
#[test]
fn test_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Create a directory with restricted permissions
    let restricted_dir = env.project_path().join("restricted");
    fs::create_dir_all(&restricted_dir).unwrap();

    // Set read-only permissions (no write)
    let mut perms = fs::metadata(&restricted_dir).unwrap().permissions();
    perms.set_mode(0o444); // Read-only
    fs::set_permissions(&restricted_dir, perms).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .env("CCPM_CACHE_DIR", restricted_dir.to_str().unwrap())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Permission denied")
                .or(predicate::str::contains("Access denied")),
        );

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&restricted_dir).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&restricted_dir, perms).unwrap();
}

/// Test Windows-specific drive letters and UNC paths
#[cfg(windows)]
#[test]
fn test_windows_drive_letters() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with absolute Windows paths
    let manifest_content = format!(
        r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[snippets]
absolute-snippet = {{ path = "C:\\temp\\snippet.md" }}
unc-snippet = {{ path = "\\\\server\\share\\snippet.md" }}
relative-snippet = {{ path = "{}\\snippets\\relative.md" }}
"#,
        env.project_path().to_str().unwrap().replace("\\", "\\\\")
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create the relative snippet file
    let snippets_dir = env.project_path().join("snippets");
    fs::create_dir_all(&snippets_dir).unwrap();
    fs::write(snippets_dir.join("relative.md"), "# Relative snippet").unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--resolve")
        .assert()
        .failure() // Absolute paths likely don't exist
        .stderr(predicate::str::contains("Local path not found"))
        .stderr(predicate::str::contains("C:\\temp\\snippet.md"));
}

/// Test concurrent access handling (file locking)
#[test]
fn test_concurrent_operations() {
    let env = TestEnvironment::new().unwrap();

    // Add mock source
    let official_files = vec![MarkdownFixture::agent("concurrent-agent")];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Create manifest with file:// URL
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
concurrent-agent = {{ source = "official", path = "agents/concurrent-agent.md", version = "v1.0.0" }}
"#,
        source_path.display()
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Test basic validation first to ensure the setup is correct
    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"));

    // Test that multiple validation commands can run concurrently without issues
    let mut handles = vec![];

    for i in 0..3 {
        let project_path = env.project_path().to_path_buf();
        let cache_path = env.cache_path().to_path_buf();
        let handle = std::thread::spawn(move || {
            let mut cmd = assert_cmd::Command::cargo_bin("ccpm").unwrap();
            cmd.current_dir(&project_path)
                .env("CCPM_CACHE_DIR", &cache_path)
                .arg("validate")
                .env("CCPM_PARALLEL_ID", i.to_string())
                .assert()
                .success();
        });
        handles.push(handle);
    }

    // Wait for all commands to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Test that the manifest remains valid after concurrent access
    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"));
}

/// Test Unicode filename handling
#[test]
fn test_unicode_filenames() {
    let env = TestEnvironment::new().unwrap();

    // Create manifest with Unicode characters (keys need to be quoted in TOML)
    let manifest_content = r#"
[sources]
"官方" = "https://github.com/example-org/官方代理.git"

[agents]
"日本語エージェント" = { source = "官方", path = "agents/日本語.md", version = "v1.0.0" }
"émoji-agent" = { source = "官方", path = "agents/🚀emoji.md", version = "v1.0.0" }
"#;

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success() // Should handle Unicode gracefully
        .stdout(predicate::str::contains("✓"));
}

/// Test handling of symlinks (Unix) vs junctions (Windows)
#[cfg(unix)]
#[test]
fn test_symlink_handling() {
    let env = TestEnvironment::new().unwrap();

    // Create a symlink to test handling
    let target_dir = env.temp_path().join("target");
    let link_dir = env.temp_path().join("link");
    fs::create_dir_all(&target_dir).unwrap();

    std::os::unix::fs::symlink(&target_dir, &link_dir).unwrap();

    // Create manifest pointing to symlinked directory
    let manifest_content = format!(
        r#"
[snippets]
symlink-snippet = {{ path = "{}/snippet.md" }}
"#,
        link_dir.to_str().unwrap()
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create file in target directory
    fs::write(target_dir.join("snippet.md"), "# Symlinked snippet").unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .arg("--resolve")
        .assert()
        .success() // Should follow symlinks
        .stdout(predicate::str::contains("✓"));
}

/// Test shell command differences across platforms
#[test]
fn test_shell_compatibility() {
    let env = TestEnvironment::with_basic_manifest().unwrap();

    // Test that commands work regardless of shell (bash, zsh, cmd, PowerShell)
    let mut cmd = env.ccpm_command();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "CCPM is a Git-based package manager",
        ));
}

/// Test platform-specific temporary directory handling
#[test]
fn test_temp_directory_platform() {
    let env = TestEnvironment::new().unwrap();

    // Add mock source
    let official_files = vec![MarkdownFixture::agent("temp-test-agent")];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Create manifest with file:// URL
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
temp-test-agent = {{ source = "official", path = "agents/temp-test-agent.md", version = "v1.0.0" }}
"#,
        source_path.display()
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Test that temp directories are created in platform-appropriate locations
    // For now, we just test that validation succeeds as this validates the manifest structure
    let mut cmd = env.ccpm_command();
    cmd.arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"));

    // The specific temp directory varies by platform:
    // - Windows: %TEMP% or %TMP%
    // - macOS: $TMPDIR (usually /var/folders/...)
    // - Linux: /tmp
    // We verify that the basic platform detection and command execution works
}
