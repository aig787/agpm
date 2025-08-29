use predicates::prelude::*;
use std::fs;
mod fixtures;
use fixtures::{MarkdownFixture, TestEnvironment};

/// Test path handling on Windows vs Unix
#[test]
fn test_path_separators() {
    let env = TestEnvironment::new().unwrap();

    // Add mock source with both files first
    let official_files = vec![
        MarkdownFixture::agent("windows-agent"),
        MarkdownFixture::agent("unix-agent"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Use file:// URL with forward slashes for Windows compatibility
    let source_path_str = source_path.display().to_string().replace('\\', "/");

    // Create manifest with mixed path separators
    let manifest_content = if cfg!(windows) {
        format!(
            r#"
[sources]
official = "file://{source_path_str}"

[agents]
windows-agent = {{ source = "official", path = "agents\\windows-agent.md", version = "v1.0.0" }}
unix-agent = {{ source = "official", path = "agents/unix-agent.md", version = "v1.0.0" }}

[snippets]
local-snippet = {{ path = ".\\snippets\\local.md" }}
"#
        )
    } else {
        format!(
            r#"
[sources]
official = "file://{source_path_str}"

[agents]
unix-agent = {{ source = "official", path = "agents/unix-agent.md", version = "v1.0.0" }}
windows-agent = {{ source = "official", path = "agents\\windows-agent.md", version = "v1.0.0" }}

[snippets]
local-snippet = {{ path = "./snippets/local.md" }}
"#
        )
    };

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create local snippet file with platform-appropriate path
    let snippets_dir = env.project_path().join("snippets");
    fs::create_dir_all(&snippets_dir).unwrap();
    MarkdownFixture::snippet("local")
        .write_to(env.project_path())
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

    // Create a long but valid name for Windows (avoid exceeding practical limits)
    // Full path includes temp dir + project dir + .claude/agents/ + filename
    // So we use a moderately long name that won't exceed limits
    let long_name = "a".repeat(100);

    // Add mock source
    let official_files = vec![MarkdownFixture::agent(&long_name)];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Use file:// URL with forward slashes for Windows compatibility
    let source_path_str = source_path.display().to_string().replace('\\', "/");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{}"

[agents]
{} = {{ source = "official", path = "agents/{}.md", version = "v1.0.0" }}
"#,
        source_path_str, long_name, long_name
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache") // Skip cache to avoid network issues
        .assert()
        .success() // Should handle long paths gracefully
        .stdout(predicate::str::contains("Installing"));
}

/// Test case sensitivity differences between platforms
/// This test only runs on Linux because it tests behavior that requires
/// a case-sensitive filesystem. On macOS and Windows, files differing only
/// in case would overwrite each other.
#[test]
#[cfg(target_os = "linux")]
fn test_case_sensitivity() {
    let env = TestEnvironment::new().unwrap();

    // Create consistent repository content across all platforms
    // Use different filenames that won't conflict on any filesystem
    let official_files = vec![
        MarkdownFixture::agent("myagent-lower"),
        MarkdownFixture::agent("MyAgent-upper"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Create manifest with file:// URL
    // On Windows, backslashes need to be escaped in TOML strings
    let source_path_str = source_path.display().to_string().replace('\\', "/");

    // Test TOML key case sensitivity (which is case-sensitive on all platforms)
    // The keys differ in case but map to different files
    let manifest_content = format!(
        r#"
[sources]
official = "file://{source_path_str}"

[agents]
myagent = {{ source = "official", path = "agents/myagent-lower.md", version = "v1.0.0" }}
MyAgent = {{ source = "official", path = "agents/MyAgent-upper.md", version = "v1.0.0" }}
"#
    );

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

    // Both files should be installed with their TOML key names, not source file names
    assert!(
        agents_dir.join("myagent.md").exists(),
        "myagent.md should exist"
    );
    assert!(
        agents_dir.join("MyAgent.md").exists(),
        "MyAgent.md should exist"
    );

    // Verify they have different content (from different source files)
    let lower_content = fs::read_to_string(agents_dir.join("myagent.md")).unwrap();
    let upper_content = fs::read_to_string(agents_dir.join("MyAgent.md")).unwrap();

    // Debug output to see what's actually in the files
    if !lower_content.contains("myagent-lower") {
        eprintln!("ERROR: myagent.md doesn't contain 'myagent-lower'");
        eprintln!(
            "First 200 chars of myagent.md: {:?}",
            &lower_content[..200.min(lower_content.len())]
        );
    }
    if !upper_content.contains("MyAgent-upper") {
        eprintln!("ERROR: MyAgent.md doesn't contain 'MyAgent-upper'");
        eprintln!(
            "First 200 chars of MyAgent.md: {:?}",
            &upper_content[..200.min(upper_content.len())]
        );
    }

    assert!(
        lower_content.contains("myagent-lower"),
        "myagent.md should contain 'myagent-lower'"
    );
    assert!(
        upper_content.contains("MyAgent-upper"),
        "MyAgent.md should contain 'MyAgent-upper'"
    );
}

/// Test case-insensitive filesystem behavior on macOS and Windows
/// This test verifies that when two resources differ only in case,
/// the second one overwrites the first on case-insensitive filesystems.
#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn test_case_insensitive_overwrite() {
    let env = TestEnvironment::new().unwrap();

    // Create repository with two different files
    let official_files = vec![
        MarkdownFixture::agent("resource-lower"),
        MarkdownFixture::agent("resource-upper"),
    ];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Create manifest with file:// URL
    let source_path_str = source_path.display().to_string().replace('\\', "/");

    // Use keys that differ only in case, pointing to different source files
    let manifest_content = format!(
        r#"
[sources]
official = "file://{source_path_str}"

[agents]
myagent = {{ source = "official", path = "agents/resource-lower.md", version = "v1.0.0" }}
MyAgent = {{ source = "official", path = "agents/resource-upper.md", version = "v1.0.0" }}
"#
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    let mut cmd = env.ccpm_command();

    // Installation should succeed
    cmd.arg("install")
        .arg("--no-cache") // Skip cache for tests
        .assert()
        .success()
        .stdout(predicate::str::contains("Installing"));

    // Verify only one file exists (the second overwrites the first)
    let agents_dir = env.project_path().join(".claude").join("agents");

    // On case-insensitive systems, myagent.md and MyAgent.md are the same file
    // The content should be from whichever was written last
    assert!(
        agents_dir.join("myagent.md").exists() || agents_dir.join("MyAgent.md").exists(),
        "At least one file should exist"
    );

    // Count actual files in directory
    let file_count = fs::read_dir(&agents_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|s| s == "md").unwrap_or(false))
        .count();

    assert_eq!(
        file_count, 1,
        "Should have exactly one file due to case-insensitive overwrite"
    );
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
    let env = TestEnvironment::new().unwrap();

    // Create a mock source to avoid network access
    let official_files = vec![MarkdownFixture::agent("test-agent")];
    let source_path = env
        .add_mock_source(
            "official",
            "https://github.com/example-org/ccpm-official.git",
            official_files,
        )
        .unwrap();

    // Use file:// URL with forward slashes for Windows compatibility
    let source_path_str = source_path.display().to_string().replace('\\', "/");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{source_path_str}"

[agents]
test-agent = {{ source = "official", path = "agents/test-agent.md", version = "v1.0.0" }}
"#
    );

    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // This test verifies that git commands work on all platforms
    // The specific git executable name might differ (git vs git.exe)
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .arg("--no-cache")
        .assert()
        .success() // Should succeed with mock source
        .stdout(predicate::str::contains("Installing"));
}

/// Test permission handling across platforms
#[cfg(unix)]
#[test]
fn test_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnvironment::new().unwrap();

    // Create a manifest that requires cache operations (with a mock source)
    let test_files = vec![MarkdownFixture::snippet("example")];
    let source_path = env
        .add_mock_source(
            "test-source",
            "https://github.com/example/test.git",
            test_files,
        )
        .unwrap();

    // Use file:// URL with forward slashes for compatibility
    let source_path_str = source_path.display().to_string().replace('\\', "/");
    let manifest_content = format!(
        r#"
[sources]
test = "file://{source_path_str}"

[snippets]
remote-snippet = {{ source = "test", path = "snippets/example.md", version = "v1.0.0" }}
"#
    );
    fs::write(env.project_path().join("ccpm.toml"), manifest_content).unwrap();

    // Create a parent directory with restricted permissions
    let parent_dir = env.project_path().join("restricted_parent");
    fs::create_dir_all(&parent_dir).unwrap();

    // Create a path to a cache directory that doesn't exist yet
    let restricted_cache = parent_dir.join("cache");

    // Now set the parent directory to read-only so cache creation will fail
    let mut perms = fs::metadata(&parent_dir).unwrap().permissions();
    perms.set_mode(0o555); // Read and execute only, no write
    fs::set_permissions(&parent_dir, perms).unwrap();

    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .env("CCPM_CACHE_DIR", restricted_cache.to_str().unwrap())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Permission denied")
                .or(predicate::str::contains("Access denied"))
                .or(predicate::str::contains("Failed to create")),
        );

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&parent_dir).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&parent_dir, perms).unwrap();
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
        .stderr(
            predicate::str::contains("Local dependency 'absolute-snippet' not found at").or(
                predicate::str::contains("Local dependency 'unc-snippet' not found at"),
            ),
        )
        .stderr(
            predicate::str::contains("C:\\temp\\snippet.md")
                .or(predicate::str::contains("\\\\server\\share\\snippet.md")),
        );
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
    // On Windows, backslashes need to be escaped in TOML strings
    let source_path_str = source_path.display().to_string().replace('\\', "/");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{source_path_str}"

[agents]
concurrent-agent = {{ source = "official", path = "agents/concurrent-agent.md", version = "v1.0.0" }}
"#
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
    // On Windows, backslashes need to be escaped in TOML strings
    let source_path_str = source_path.display().to_string().replace('\\', "/");
    let manifest_content = format!(
        r#"
[sources]
official = "file://{source_path_str}"

[agents]
temp-test-agent = {{ source = "official", path = "agents/temp-test-agent.md", version = "v1.0.0" }}
"#
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
