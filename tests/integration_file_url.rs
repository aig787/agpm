use anyhow::Result;
use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

mod common;
mod fixtures;
use common::TestGit;

/// Convert a path to a file:// URL string, properly handling Windows paths
fn path_to_file_url(path: &std::path::Path) -> String {
    // Convert backslashes to forward slashes for Windows paths in URLs
    let path_str = path.display().to_string().replace('\\', "/");
    format!("file://{path_str}")
}

/// Test that file:// URLs don't modify the source repository's working directory
#[test]
fn test_file_url_source_repo_not_modified() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir)?;

    // Create a source git repository with some commits and tags
    let source_repo_dir = temp_dir.path().join("source_repo");
    fs::create_dir_all(&source_repo_dir)?;

    // Initialize the source repo
    let git = TestGit::new(&source_repo_dir);
    git.init()?;
    git.config_user()?;

    // Create initial commit
    fs::create_dir_all(source_repo_dir.join("agents"))?;
    fs::write(
        source_repo_dir.join("agents").join("test.md"),
        "# Test Agent v1",
    )?;
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    // Create a second commit on main
    fs::write(
        source_repo_dir.join("agents").join("test.md"),
        "# Test Agent v2",
    )?;
    git.add_all()?;
    git.commit("Update to v2")?;
    git.tag("v2.0.0")?;

    // Now checkout v1.0.0 in the source repo (simulating user's working state)
    std::process::Command::new("git")
        .args(["checkout", "v1.0.0"])
        .current_dir(&source_repo_dir)
        .output()?;

    // Verify we're on v1.0.0
    let source_content = fs::read_to_string(source_repo_dir.join("agents").join("test.md"))?;
    assert!(
        source_content.contains("v1"),
        "Source repo should be at v1.0.0"
    );

    // Get the current HEAD of the source repo
    let source_head_before = git.get_commit_hash()?;

    // Create a manifest using file:// URL pointing to v2.0.0
    let file_url = path_to_file_url(&source_repo_dir);
    let manifest = format!(
        r#"
[sources]
local = "{}"

[agents]
test-agent = {{ source = "local", path = "agents/test.md", version = "v2.0.0" }}
"#,
        file_url
    );

    fs::write(project_dir.join("ccpm.toml"), manifest)?;

    // Set CCPM_CACHE_DIR to use temp directory for cache
    let cache_dir = temp_dir.path().join("cache");
    fs::create_dir_all(&cache_dir)?;

    // Run install
    Command::cargo_bin("ccpm")
        .unwrap()
        .current_dir(&project_dir)
        .env("CCPM_CACHE_DIR", cache_dir.display().to_string())
        .arg("install")
        .assert()
        .success();

    // Verify the installed file is from v2.0.0
    let installed_content = fs::read_to_string(
        project_dir
            .join(".claude")
            .join("agents")
            .join("test-agent.md"),
    )?;
    assert!(
        installed_content.contains("v2"),
        "Installed file should be from v2.0.0"
    );

    // CRITICAL: Verify the source repo is still at v1.0.0
    let source_head_after = git.get_commit_hash()?;
    assert_eq!(
        source_head_before.trim(),
        source_head_after.trim(),
        "Source repository HEAD should not have changed"
    );

    let source_content_after = fs::read_to_string(source_repo_dir.join("agents").join("test.md"))?;
    assert!(
        source_content_after.contains("v1"),
        "Source repo working directory should still be at v1.0.0"
    );

    // Also check that the source repo doesn't show any modifications
    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&source_repo_dir)
        .output()?;
    let status = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status.trim().is_empty(),
        "Source repository should have no modifications"
    );

    Ok(())
}

/// Test that updates from file:// repos work correctly
#[test]
fn test_file_url_updates_work() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir)?;

    // Create a source git repository
    let source_repo_dir = temp_dir.path().join("source_repo");
    fs::create_dir_all(&source_repo_dir)?;

    // Initialize the source repo
    let git = TestGit::new(&source_repo_dir);
    git.init()?;
    git.config_user()?;

    // Create initial commit
    fs::create_dir_all(source_repo_dir.join("agents"))?;
    fs::write(
        source_repo_dir.join("agents").join("test.md"),
        "# Test Agent v1",
    )?;
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    // Create manifest using file:// URL
    let file_url = path_to_file_url(&source_repo_dir);
    let manifest = format!(
        r#"
[sources]
local = "{}"

[agents]
test-agent = {{ source = "local", path = "agents/test.md", version = "v1.0.0" }}
"#,
        file_url
    );

    fs::write(project_dir.join("ccpm.toml"), manifest)?;

    // Set CCPM_CACHE_DIR to use temp directory for cache
    let cache_dir = temp_dir.path().join("cache");
    fs::create_dir_all(&cache_dir)?;

    // Initial install
    Command::cargo_bin("ccpm")
        .unwrap()
        .current_dir(&project_dir)
        .env("CCPM_CACHE_DIR", cache_dir.display().to_string())
        .arg("install")
        .assert()
        .success();

    // Verify v1 is installed
    let installed_content = fs::read_to_string(
        project_dir
            .join(".claude")
            .join("agents")
            .join("test-agent.md"),
    )?;
    assert!(installed_content.contains("v1"), "Should have v1 installed");

    // Now add a new version in the source repo
    fs::write(
        source_repo_dir.join("agents").join("test.md"),
        "# Test Agent v2",
    )?;
    git.add_all()?;
    git.commit("Update to v2")?;
    git.tag("v2.0.0")?;

    // Update manifest to use v2.0.0
    let manifest_v2 = format!(
        r#"
[sources]
local = "{}"

[agents]
test-agent = {{ source = "local", path = "agents/test.md", version = "v2.0.0" }}
"#,
        file_url
    );

    fs::write(project_dir.join("ccpm.toml"), manifest_v2)?;

    // Run install again (should fetch updates and install v2)
    Command::cargo_bin("ccpm")
        .unwrap()
        .current_dir(&project_dir)
        .env("CCPM_CACHE_DIR", cache_dir.display().to_string())
        .arg("install")
        .assert()
        .success();

    // Verify v2 is now installed
    let installed_content_v2 = fs::read_to_string(
        project_dir
            .join(".claude")
            .join("agents")
            .join("test-agent.md"),
    )?;
    assert!(
        installed_content_v2.contains("v2"),
        "Should have v2 installed after update"
    );

    Ok(())
}

/// Test that file:// URLs with uncommitted changes in source don't cause issues
#[test]
fn test_file_url_with_uncommitted_changes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir)?;

    // Create a source git repository
    let source_repo_dir = temp_dir.path().join("source_repo");
    fs::create_dir_all(&source_repo_dir)?;

    // Initialize the source repo
    let git = TestGit::new(&source_repo_dir);
    git.init()?;
    git.config_user()?;

    // Create initial commit
    fs::create_dir_all(source_repo_dir.join("agents"))?;
    fs::write(
        source_repo_dir.join("agents").join("test.md"),
        "# Test Agent v1",
    )?;
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    // Make uncommitted changes in the source repo
    fs::write(
        source_repo_dir.join("agents").join("test.md"),
        "# Test Agent - Work in Progress",
    )?;
    fs::write(source_repo_dir.join("new_file.txt"), "Uncommitted work")?;

    // Verify there are uncommitted changes
    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&source_repo_dir)
        .output()?;
    let status_before = String::from_utf8_lossy(&status_output.stdout).to_string();
    assert!(
        !status_before.trim().is_empty(),
        "Should have uncommitted changes"
    );

    // Create manifest using file:// URL
    let file_url = path_to_file_url(&source_repo_dir);
    let manifest = format!(
        r#"
[sources]
local = "{}"

[agents]
test-agent = {{ source = "local", path = "agents/test.md", version = "v1.0.0" }}
"#,
        file_url
    );

    fs::write(project_dir.join("ccpm.toml"), manifest)?;

    // Set CCPM_CACHE_DIR to use temp directory for cache
    let cache_dir = temp_dir.path().join("cache");
    fs::create_dir_all(&cache_dir)?;

    // Run install
    Command::cargo_bin("ccpm")
        .unwrap()
        .current_dir(&project_dir)
        .env("CCPM_CACHE_DIR", cache_dir.display().to_string())
        .arg("install")
        .assert()
        .success();

    // Verify the installed file is from the committed v1.0.0, not the uncommitted changes
    let installed_content = fs::read_to_string(
        project_dir
            .join(".claude")
            .join("agents")
            .join("test-agent.md"),
    )?;
    assert!(
        installed_content.contains("v1"),
        "Should install committed v1, not uncommitted changes"
    );
    assert!(
        !installed_content.contains("Work in Progress"),
        "Should not include uncommitted changes"
    );

    // Verify the source repo still has its uncommitted changes
    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&source_repo_dir)
        .output()?;
    let status_after = String::from_utf8_lossy(&status_output.stdout).to_string();
    assert_eq!(
        status_before.trim(),
        status_after.trim(),
        "Source repo uncommitted changes should be preserved"
    );

    let source_content = fs::read_to_string(source_repo_dir.join("agents").join("test.md"))?;
    assert!(
        source_content.contains("Work in Progress"),
        "Source repo working directory should still have uncommitted changes"
    );

    Ok(())
}
