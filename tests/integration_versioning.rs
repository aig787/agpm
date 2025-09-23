use anyhow::Result;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

mod common;
mod fixtures;
mod test_config;
use common::{TestGit, TestProject};

/// Helper to initialize a git repository with tags, branches, and commits
fn setup_git_repo_with_versions(repo_path: &Path) -> Result<String> {
    // Use TestGit helper for cleaner git operations
    let git = TestGit::new(repo_path);
    git.init()?;
    git.config_user()?;

    // Create directory structure
    fs::create_dir_all(repo_path.join("agents"))?;
    fs::create_dir_all(repo_path.join("snippets"))?;

    // Create v1.0.0 content
    fs::write(
        repo_path.join("agents/example.md"),
        "# Example Agent v1.0.0\nInitial version",
    )?;
    fs::write(
        repo_path.join("snippets/utils.md"),
        "# Utils Snippet v1.0.0\nInitial version",
    )?;

    // Commit and tag v1.0.0
    git.add_all()?;
    git.commit("Initial commit v1.0.0")?;
    git.tag("v1.0.0")?;

    // Get commit hash for v1.0.0
    let v1_commit = git.get_commit_hash()?;

    // Create v1.1.0 content
    fs::write(
        repo_path.join("agents/example.md"),
        "# Example Agent v1.1.0\nMinor update",
    )?;

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", "Version 1.1.0"])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["tag", "v1.1.0"])
        .current_dir(repo_path)
        .output()?;

    // Create v1.2.0 content
    fs::write(
        repo_path.join("agents/example.md"),
        "# Example Agent v1.2.0\nAnother minor update",
    )?;

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", "Version 1.2.0"])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["tag", "v1.2.0"])
        .current_dir(repo_path)
        .output()?;

    // Create v2.0.0 content
    fs::write(
        repo_path.join("agents/example.md"),
        "# Example Agent v2.0.0\nMajor version",
    )?;

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", "Version 2.0.0 - Breaking changes"])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["tag", "v2.0.0"])
        .current_dir(repo_path)
        .output()?;

    // Go back to main/master branch first to ensure we're on the default branch
    // Try main first, fall back to master
    let checkout_result = std::process::Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_path)
        .output()?;

    if !checkout_result.status.success() {
        // Try master if main doesn't exist
        std::process::Command::new("git")
            .args(["checkout", "master"])
            .current_dir(repo_path)
            .output()?;
    }

    // Create develop branch with different content
    std::process::Command::new("git")
        .args(["checkout", "-b", "develop"])
        .current_dir(repo_path)
        .output()?;

    fs::write(
        repo_path.join("agents/example.md"),
        "# Example Agent - Development\nUnstable development version",
    )?;
    fs::write(
        repo_path.join("agents/experimental.md"),
        "# Experimental Agent\nOnly in develop branch",
    )?;

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", "Development changes"])
        .current_dir(repo_path)
        .output()?;

    // Create feature branch
    std::process::Command::new("git")
        .args(["checkout", "-b", "feature/new-agent"])
        .current_dir(repo_path)
        .output()?;

    fs::write(
        repo_path.join("agents/feature.md"),
        "# Feature Agent\nNew feature in progress",
    )?;

    git.add_all()?;
    git.commit("Add feature agent")?;

    // Go back to main/master branch
    let checkout_result = std::process::Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_path)
        .output()?;

    if !checkout_result.status.success() {
        // Try master if main doesn't exist
        std::process::Command::new("git")
            .args(["checkout", "master"])
            .current_dir(repo_path)
            .output()?;
    }

    Ok(v1_commit)
}

#[test]
fn test_install_with_exact_version_tag() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    // Setup git repository with versions
    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with exact version
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", version = "v1.0.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    // Run install
    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Check installed file contains v1.0.0 content
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
    assert!(installed.contains("v1.0.0"));
    assert!(!installed.contains("v1.1.0"));
    assert!(!installed.contains("v2.0.0"));
}

#[test]
fn test_install_with_caret_version_range() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with caret range (^1.0.0 should match 1.2.0 but not 2.0.0)
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", version = "^1.0.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v1.2.0 (highest compatible version)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
    assert!(installed.contains("v1.2.0"));
    assert!(!installed.contains("v2.0.0"));
}

#[test]
fn test_install_with_tilde_version_range() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with tilde range (~1.1.0 should match 1.1.x but not 1.2.0)
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", version = "~1.1.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v1.1.0 (only patch updates allowed)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
    assert!(installed.contains("v1.1.0"));
    assert!(!installed.contains("v1.2.0"));
}

#[test]
fn test_install_with_branch_reference() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with branch reference
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
dev-example = {{ source = "versioned", path = "agents/example.md", branch = "develop" }}
experimental = {{ source = "versioned", path = "agents/experimental.md", branch = "develop" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Check we got develop branch content
    let example_content =
        fs::read_to_string(project.project_path().join(".claude/agents/dev-example.md")).unwrap();
    assert!(example_content.contains("Development"));
    assert!(example_content.contains("Unstable"));

    // Check experimental agent exists (only in develop branch)
    assert!(
        project
            .project_path()
            .join(".claude/agents/experimental.md")
            .exists()
    );
}

#[test]
fn test_install_with_feature_branch() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with feature branch
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
feature = {{ source = "versioned", path = "agents/feature.md", branch = "feature/new-agent" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Check feature agent was installed
    let feature_content =
        fs::read_to_string(project.project_path().join(".claude/agents/feature.md")).unwrap();
    assert!(feature_content.contains("Feature Agent"));
    assert!(feature_content.contains("New feature in progress"));
}

#[test]
fn test_install_with_commit_hash() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    // Setup git repository and get v1.0.0 commit hash
    let v1_commit = setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with exact commit hash (rev)
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
pinned = {{ source = "versioned", path = "agents/example.md", rev = "{}" }}
"#,
        source_path.display().to_string().replace('\\', "/"),
        v1_commit
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Should get exact v1.0.0 content
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/pinned.md")).unwrap();
    assert!(installed.contains("v1.0.0"));
    assert!(installed.contains("Initial version"));
}

#[test]
fn test_install_with_wildcard_version() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with wildcard "*"
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
any = {{ source = "versioned", path = "agents/example.md", version = "*" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v2.0.0 (highest available)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/any.md")).unwrap();
    assert!(installed.contains("v2.0.0"));
}

#[test]
fn test_install_with_mixed_versioning_methods() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    let v1_commit = setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with mixed versioning methods
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
stable = {{ source = "versioned", path = "agents/example.md", version = "v1.1.0" }}
compatible = {{ source = "versioned", path = "agents/example.md", version = "^1.0.0" }}
develop = {{ source = "versioned", path = "agents/example.md", branch = "develop" }}
pinned = {{ source = "versioned", path = "agents/example.md", rev = "{}" }}
"#,
        source_path.display().to_string().replace('\\', "/"),
        v1_commit
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Check each installed file has the expected content
    let stable =
        fs::read_to_string(project.project_path().join(".claude/agents/stable.md")).unwrap();
    assert!(stable.contains("v1.1.0"));

    let compatible =
        fs::read_to_string(project.project_path().join(".claude/agents/compatible.md")).unwrap();
    assert!(compatible.contains("v1.2.0")); // Should get highest 1.x

    let develop =
        fs::read_to_string(project.project_path().join(".claude/agents/develop.md")).unwrap();
    assert!(develop.contains("Development"));

    let pinned =
        fs::read_to_string(project.project_path().join(".claude/agents/pinned.md")).unwrap();
    assert!(pinned.contains("v1.0.0"));
}

#[test]
fn test_version_constraint_with_greater_than() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Test >=1.1.0 constraint
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", version = ">=1.1.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v2.0.0 (highest that satisfies >=1.1.0)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
    assert!(installed.contains("v2.0.0"));
}

#[test]
fn test_version_constraint_with_range() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Test complex range: >=1.1.0, <2.0.0
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", version = ">=1.1.0, <2.0.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v1.2.0 (highest that satisfies the range)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
    assert!(installed.contains("v1.2.0"));
    assert!(!installed.contains("v2.0.0"));
}

#[test]
fn test_update_branch_reference() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with branch reference
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
dev = {{ source = "versioned", path = "agents/example.md", branch = "develop" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    // Initial install
    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Modify the develop branch
    std::process::Command::new("git")
        .args(["checkout", "develop"])
        .current_dir(&source_path)
        .output()
        .unwrap();

    fs::write(
        source_path.join("agents/example.md"),
        "# Example Agent - Updated Development\nNewer unstable version",
    )
    .unwrap();

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&source_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "Update develop branch"])
        .current_dir(&source_path)
        .output()
        .unwrap();

    // Run update to get latest develop branch
    let output = project.run_ccpm(&["update"]).unwrap();
    output.assert_success();

    // Check we got the updated content
    let file_path = project.project_path().join(".claude/agents/dev.md");
    let updated = fs::read_to_string(&file_path).unwrap_or_else(|e| {
        panic!("Failed to read file {file_path:?}: {e}");
    });
    println!("File content after update: {updated:?}");
    assert!(
        updated.contains("Updated Development"),
        "File content: {updated}"
    );
    assert!(updated.contains("Newer unstable"));
}

#[test]
fn test_lockfile_records_correct_version_info() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    let v1_commit = setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with different version types
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
tagged = {{ source = "versioned", path = "agents/example.md", version = "v1.1.0" }}
branched = {{ source = "versioned", path = "agents/example.md", branch = "develop" }}
committed = {{ source = "versioned", path = "agents/example.md", rev = "{}" }}
"#,
        source_path.display().to_string().replace('\\', "/"),
        v1_commit
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Check lockfile contains correct version info
    let lockfile = fs::read_to_string(project.project_path().join("ccpm.lock")).unwrap();

    // Tagged dependency should show version
    assert!(lockfile.contains("name = \"tagged\""));
    assert!(lockfile.contains("version = \"v1.1.0\""));

    // Branch dependency should show git field
    assert!(lockfile.contains("name = \"branched\""));
    // The lockfile will show the git field for branch

    // Commit dependency should show git field
    assert!(lockfile.contains("name = \"committed\""));
    // The lockfile will show the git field for commit hash
}

#[test]
fn test_error_on_invalid_version_constraint() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with unsatisfiable version
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", version = "v99.0.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    // This should fail
    let output = project.run_ccpm(&["install"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("Git operation failed")
            || output.stderr.contains("No matching version found"),
        "Expected error about version not found, got: {}",
        output.stderr
    );
}

#[test]
fn test_error_on_nonexistent_branch() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with non-existent branch
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", branch = "nonexistent" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
}

#[test]
fn test_frozen_install_uses_lockfile_versions() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Create manifest with version range
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
example = {{ source = "versioned", path = "agents/example.md", version = "^1.0.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    // Initial install (should get v1.2.0)
    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    let lockfile = fs::read_to_string(project.project_path().join("ccpm.lock")).unwrap();
    assert!(lockfile.contains("version = \"v1.2.0\""));

    // Delete installed files
    fs::remove_dir_all(project.project_path().join(".claude")).unwrap();

    // Run frozen install - should use lockfile version (v1.2.0) not latest (v2.0.0)
    let output = project.run_ccpm(&["install", "--frozen"]).unwrap();
    output.assert_success();

    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
    assert!(installed.contains("v1.2.0"));
    assert!(!installed.contains("v2.0.0"));
}
