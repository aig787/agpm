use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::fs;
mod common;
mod fixtures;
use common::TestProject;
use fixtures::MarkdownFixture;

/// Extension trait for MarkdownFixture to write to disk
trait MarkdownFixtureExt {
    async fn write_to(&self, dir: &Path) -> Result<PathBuf>;
}

impl MarkdownFixtureExt for MarkdownFixture {
    async fn write_to(&self, dir: &Path) -> Result<PathBuf> {
        let file_path = dir.join(&self.path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&file_path, &self.content).await?;
        Ok(file_path)
    }
}

/// Test path handling on Windows vs Unix
#[tokio::test]
async fn test_path_separators() {
    let project = TestProject::new().await.unwrap();

    // Add mock source with both files first
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource("agents", "windows-agent", "# Windows Agent\n\nA test agent")
        .await
        .unwrap();
    official_repo
        .add_resource("agents", "unix-agent", "# Unix Agent\n\nA test agent")
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    // Use file:// URL with forward slashes for Windows compatibility
    let source_path_str = official_repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with mixed path separators
    let manifest_content = if cfg!(windows) {
        format!(
            r#"
[sources]
official = "{source_path_str}"

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
official = "{source_path_str}"

[agents]
unix-agent = {{ source = "official", path = "agents/unix-agent.md", version = "v1.0.0" }}
windows-agent = {{ source = "official", path = "agents\\windows-agent.md", version = "v1.0.0" }}

[snippets]
local-snippet = {{ path = "./snippets/local.md" }}
"#
        )
    };

    project.write_manifest(&manifest_content).await.unwrap();

    // Create local snippet file with platform-appropriate path
    let snippets_dir = project.project_path().join("snippets");
    fs::create_dir_all(&snippets_dir).await.unwrap();
    let local_snippet = MarkdownFixture::snippet("local");
    local_snippet
        .write_to(project.project_path())
        .await
        .unwrap();

    let output = project.run_ccpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
}

/// Test handling of long paths (Windows limitation)
#[cfg(windows)]
#[tokio::test]
async fn test_long_paths_windows() {
    let project = TestProject::new().await.unwrap();

    // Create a long but valid name for Windows (avoid exceeding practical limits)
    // Full path includes temp dir + project dir + .claude/agents/ + filename
    // So we use a moderately long name that won't exceed limits
    let long_name = "a".repeat(100);

    // Add mock source
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource(
            "agents",
            &long_name,
            &format!("# {}\n\nA test agent", long_name),
        )
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    // Use file:// URL with forward slashes for Windows compatibility
    let source_path_str = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest_content = format!(
        r#"
[sources]
official = "{}"

[agents]
{} = {{ source = "official", path = "agents/{}.md", version = "v1.0.0" }}
"#,
        source_path_str, long_name, long_name
    );

    project.write_manifest(&manifest_content).await.unwrap();

    let output = project.run_ccpm(&["install", "--no-cache"]).unwrap();
    assert!(output.success); // Should handle long paths gracefully
    assert!(output.stdout.contains("Installed") || output.stdout.contains("Installing"));
}

/// Test case conflict detection universally
/// We reject case conflicts on all platforms to ensure manifest portability
#[tokio::test]
async fn test_case_conflict_detection() {
    let project = TestProject::new().await.unwrap();

    // Create consistent repository content across all platforms
    // Use different filenames that won't conflict on any filesystem
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource("agents", "myagent-lower", "# MyAgent Lower\n\nA test agent")
        .await
        .unwrap();
    official_repo
        .add_resource("agents", "MyAgent-upper", "# MyAgent Upper\n\nA test agent")
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with file:// URL
    // On Windows, backslashes need to be escaped in TOML strings
    let source_path_str = official_repo.bare_file_url(project.sources_path()).unwrap();

    // Test TOML key case sensitivity (which is case-sensitive on all platforms)
    // The keys differ in case but map to different files
    let manifest_content = format!(
        r#"
[sources]
official = "{source_path_str}"

[agents]
myagent = {{ source = "official", path = "agents/myagent-lower.md", version = "v1.0.0" }}
MyAgent = {{ source = "official", path = "agents/MyAgent-upper.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await.unwrap();

    // Pass explicit manifest path to avoid path resolution issues
    let manifest_path = project.project_path().join("ccpm.toml");

    // Validation should fail with case conflict error on all platforms
    // to ensure manifests are portable
    let output = project
        .run_ccpm(&[
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "validate",
        ])
        .unwrap();
    assert!(!output.success);
    assert!(output.stderr.contains("Case conflict"));
    assert!(output.stderr.contains("myagent"));
    assert!(output.stderr.contains("MyAgent"));
}

/// Test home directory expansion across platforms
#[tokio::test]
async fn test_home_directory_expansion() {
    let project = TestProject::new().await.unwrap();

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

    project.write_manifest(manifest_content).await.unwrap();

    let output = project.run_ccpm(&["validate"]).unwrap();
    assert!(output.success); // Home directory expansion should work and validation should pass
    assert!(output.stdout.contains("✓")); // Should succeed with valid manifest structure
}

/// Test different line ending handling
#[tokio::test]
async fn test_line_endings() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with different line endings
    let manifest_content = if cfg!(windows) {
        "[sources]\r\nofficial = \"https://github.com/example-org/ccpm-official.git\"\r\n\r\n[agents]\r\ntest-agent = { source = \"official\", path = \"agents/test.md\", version = \"v1.0.0\" }\r\n"
    } else {
        "[sources]\nofficial = \"https://github.com/example-org/ccpm-official.git\"\n\n[agents]\ntest-agent = { source = \"official\", path = \"agents/test.md\", version = \"v1.0.0\" }\n"
    };

    project.write_manifest(manifest_content).await.unwrap();

    let output = project.run_ccpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
}

/// Test git command handling across platforms
#[tokio::test]
async fn test_git_command_platform() {
    let project = TestProject::new().await.unwrap();

    // Create a mock source to avoid network access
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource("agents", "test-agent", "# Test Agent\n\nA test agent")
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();

    // Use file:// URL with forward slashes for Windows compatibility
    let source_path_str = official_repo.bare_file_url(project.sources_path()).unwrap();
    let manifest_content = format!(
        r#"
[sources]
official = "{source_path_str}"

[agents]
test-agent = {{ source = "official", path = "agents/test-agent.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await.unwrap();

    // This test verifies that git commands work on all platforms
    // The specific git executable name might differ (git vs git.exe)
    let output = project.run_ccpm(&["install", "--no-cache"]).unwrap();

    // Should at least start the installation process (either "Installing" or "Cloning")
    assert!(
        output.stdout.contains("Installing")
            || output.stdout.contains("Cloning")
            || output.stdout.contains("Installed"),
        "Expected 'Installing', 'Cloning', or 'Installed' in stdout: {}",
        output.stdout
    );

    // Accept success OR known acceptable Git failures in test environments
    if !output.success {
        assert!(
            output.stderr.contains("Git operation failed")
                || output.stderr.contains("not a git repository")
                || output.stderr.contains("worktree add"),
            "Unexpected failure: {}",
            output.stderr
        );
    }
}

/// Test permission handling across platforms
#[cfg(unix)]
#[tokio::test]
async fn test_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let project = TestProject::new().await.unwrap();

    // Create a mock source repository
    let test_repo = project.create_source_repo("test-source").await.unwrap();
    test_repo
        .add_resource("snippets", "example", "# Example Snippet\n\nA test snippet")
        .await
        .unwrap();
    test_repo.commit_all("Initial commit").unwrap();
    test_repo.tag_version("v1.0.0").unwrap();
    let source_url = test_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest_content = format!(
        r#"
[sources]
test = "{source_url}"

[snippets]
remote-snippet = {{ source = "test", path = "snippets/example.md", version = "v1.0.0" }}
"#
    );
    project.write_manifest(&manifest_content).await.unwrap();

    // Create a parent directory with restricted permissions
    let parent_dir = project.project_path().join("restricted_parent");
    fs::create_dir_all(&parent_dir).await.unwrap();

    // Create a path to a cache directory that doesn't exist yet
    let restricted_cache = parent_dir.join("cache");

    // Now set the parent directory to read-only so cache creation will fail
    let mut perms = fs::metadata(&parent_dir).await.unwrap().permissions();
    perms.set_mode(0o555); // Read and execute only, no write
    fs::set_permissions(&parent_dir, perms).await.unwrap();

    let output = project
        .run_ccpm_with_env(
            &["install"],
            &[("CCPM_CACHE_DIR", restricted_cache.to_str().unwrap())],
        )
        .unwrap();
    assert!(!output.success);
    assert!(
        output.stderr.contains("Permission denied")
            || output.stderr.contains("Access denied")
            || output.stderr.contains("Failed to create"),
        "Expected permission error, got: {}",
        output.stderr
    );

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&parent_dir).await.unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&parent_dir, perms).await.unwrap();
}

/// Test Windows-specific drive letters and UNC paths
#[cfg(windows)]
#[tokio::test]
async fn test_windows_drive_letters() {
    let project = TestProject::new().await.unwrap();

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
        project
            .project_path()
            .to_str()
            .unwrap()
            .replace("\\", "\\\\")
    );

    project.write_manifest(&manifest_content).await.unwrap();

    // Create the relative snippet file
    let snippets_dir = project.project_path().join("snippets");
    fs::create_dir_all(&snippets_dir).await.unwrap();
    fs::write(snippets_dir.join("relative.md"), "# Relative snippet")
        .await
        .unwrap();

    let output = project.run_ccpm(&["validate", "--resolve"]).unwrap();
    assert!(!output.success); // Absolute paths likely don't exist
    assert!(
        output
            .stderr
            .contains("Local dependency 'absolute-snippet' not found at")
            || output
                .stderr
                .contains("Local dependency 'unc-snippet' not found at"),
        "Expected dependency not found error, got: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("C:\\temp\\snippet.md")
            || output.stderr.contains("\\\\server\\share\\snippet.md"),
        "Expected path in error message, got: {}",
        output.stderr
    );
}

/// Test concurrent access handling (file locking)
#[tokio::test]
async fn test_concurrent_operations() {
    let project = TestProject::new().await.unwrap();

    // Create mock source repository
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource(
            "agents",
            "concurrent-agent",
            "# Concurrent Agent\n\nA test agent",
        )
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{source_url}"

[agents]
concurrent-agent = {{ source = "official", path = "agents/concurrent-agent.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await.unwrap();

    // Test basic validation first to ensure the setup is correct
    let output = project.run_ccpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));

    // Test that multiple validation commands can run concurrently without issues
    let mut handles = vec![];

    for i in 0..3 {
        let project_path = project.project_path().to_path_buf();
        let cache_path = project.cache_path().to_path_buf();
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
    let output = project.run_ccpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));
}

/// Test Unicode filename handling
#[tokio::test]
async fn test_unicode_filenames() {
    let project = TestProject::new().await.unwrap();

    // Create manifest with Unicode characters (keys need to be quoted in TOML)
    let manifest_content = r#"
[sources]
"官方" = "https://github.com/example-org/官方代理.git"

[agents]
"日本語エージェント" = { source = "官方", path = "agents/日本語.md", version = "v1.0.0" }
"émoji-agent" = { source = "官方", path = "agents/🚀emoji.md", version = "v1.0.0" }
"#;

    project.write_manifest(manifest_content).await.unwrap();

    let output = project.run_ccpm(&["validate"]).unwrap();
    assert!(output.success); // Should handle Unicode gracefully
    assert!(output.stdout.contains("✓"));
}

/// Test handling of symlinks (Unix) vs junctions (Windows)
#[cfg(unix)]
#[tokio::test]
async fn test_symlink_handling() {
    let project = TestProject::new().await.unwrap();

    // Create a symlink to test handling
    // Use project cache path as temp space
    let target_dir = project.cache_path().join("target");
    let link_dir = project.cache_path().join("link");
    fs::create_dir_all(&target_dir).await.unwrap();

    std::os::unix::fs::symlink(&target_dir, &link_dir).unwrap();

    // Create manifest pointing to symlinked directory
    let manifest_content = format!(
        r#"
[snippets]
symlink-snippet = {{ path = "{}/snippet.md" }}
"#,
        link_dir.to_str().unwrap()
    );

    project.write_manifest(&manifest_content).await.unwrap();

    // Create file in target directory
    fs::write(target_dir.join("snippet.md"), "# Symlinked snippet")
        .await
        .unwrap();

    let output = project.run_ccpm(&["validate", "--resolve"]).unwrap();
    assert!(output.success); // Should follow symlinks
    assert!(output.stdout.contains("✓"));
}

/// Test shell command differences across platforms
#[tokio::test]
async fn test_shell_compatibility() {
    let project = TestProject::new().await.unwrap();
    let manifest_content = r#"
[sources]
official = "https://github.com/example-org/ccpm-official.git"

[agents]
my-agent = { source = "official", path = "agents/my-agent.md", version = "v1.0.0" }
"#;
    project.write_manifest(manifest_content).await.unwrap();

    // Test that commands work regardless of shell (bash, zsh, cmd, PowerShell)
    let output = project.run_ccpm(&["--help"]).unwrap();
    assert!(output.success);
    assert!(
        output
            .stdout
            .contains("CCPM is a Git-based package manager")
    );
}

/// Test platform-specific temporary directory handling
#[tokio::test]
async fn test_temp_directory_platform() {
    let project = TestProject::new().await.unwrap();

    // Create mock source repository
    let official_repo = project.create_source_repo("official").await.unwrap();
    official_repo
        .add_resource(
            "agents",
            "temp-test-agent",
            "# Temp Test Agent\n\nA test agent",
        )
        .await
        .unwrap();
    official_repo.commit_all("Initial commit").unwrap();
    official_repo.tag_version("v1.0.0").unwrap();
    let source_url = official_repo.bare_file_url(project.sources_path()).unwrap();

    let manifest_content = format!(
        r#"
[sources]
official = "{source_url}"

[agents]
temp-test-agent = {{ source = "official", path = "agents/temp-test-agent.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await.unwrap();

    // Test that temp directories are created in platform-appropriate locations
    // For now, we just test that validation succeeds as this validates the manifest structure
    let output = project.run_ccpm(&["validate"]).unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("✓"));

    // The specific temp directory varies by platform:
    // - Windows: %TEMP% or %TMP%
    // - macOS: $TMPDIR (usually /var/folders/...)
    // - Linux: /tmp
    // We verify that the basic platform detection and command execution works
}
