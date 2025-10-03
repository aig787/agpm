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
    // Files use basename from path, not dependency name
    let example_content =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
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
    // Files use basename from path, not dependency name
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
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
    // Files use basename from path, not dependency name
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).unwrap();
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
    // Should fail due to path conflict - all dependencies resolve to same path
    assert!(
        !output.success,
        "Expected install to fail due to path conflicts"
    );

    // Verify error message mentions the conflict
    assert!(output.stderr.contains("Target path conflicts detected"));
    assert!(output.stderr.contains(".claude/agents/example.md"));
    assert!(output.stderr.contains("stable"));
    assert!(output.stderr.contains("compatible"));
    assert!(output.stderr.contains("develop"));
    assert!(output.stderr.contains("pinned"));
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
    // Files use basename from path, not dependency name
    let file_path = project.project_path().join(".claude/agents/example.md");
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
    // Should fail due to path conflict - all dependencies resolve to same path
    assert!(
        !output.success,
        "Expected install to fail due to path conflicts"
    );

    // Verify error message mentions the conflict
    assert!(output.stderr.contains("Target path conflicts detected"));
    assert!(output.stderr.contains(".claude/agents/example.md"));
    assert!(output.stderr.contains("tagged"));
    assert!(output.stderr.contains("branched"));
    assert!(output.stderr.contains("committed"));
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

/// Test that path collision detection works correctly for different scenarios
#[test]
fn test_path_collision_detection() {
    test_config::init_test_env();
    let project = TestProject::new().unwrap();
    let temp_source = TempDir::new().unwrap();
    let source_path = temp_source.path().to_path_buf();

    setup_git_repo_with_versions(&source_path).unwrap();

    // Test 1: Two dependencies pointing to same path should fail
    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
version-one = {{ source = "versioned", path = "agents/example.md", version = "v1.0.0" }}
version-two = {{ source = "versioned", path = "agents/example.md", version = "v2.0.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    assert!(!output.success, "Expected collision for same path");
    assert!(output.stderr.contains("Target path conflicts"));
    assert!(output.stderr.contains("version-one"));
    assert!(output.stderr.contains("version-two"));
    assert!(output.stderr.contains(".claude/agents/example.md"));

    // Test 2: Custom targets should allow same source path
    // Clean up from previous test
    let claude_dir = project.project_path().join(".claude");
    if claude_dir.exists() {
        fs::remove_dir_all(&claude_dir).unwrap();
    }
    let lock_file = project.project_path().join("ccpm.lock");
    if lock_file.exists() {
        fs::remove_file(&lock_file).unwrap();
    }

    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
version-one = {{ source = "versioned", path = "agents/example.md", version = "v1.0.0", target = "v1" }}
version-two = {{ source = "versioned", path = "agents/example.md", version = "v2.0.0", target = "v2" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();

    // Verify both files are installed with the basename from the path
    // Custom target "v1" becomes ".claude/agents/v1/example.md"
    let v1_path = project.project_path().join(".claude/agents/v1/example.md");
    let v2_path = project.project_path().join(".claude/agents/v2/example.md");

    let v1 = fs::read_to_string(&v1_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", v1_path.display(), e));
    let v2 = fs::read_to_string(&v2_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", v2_path.display(), e));
    assert!(v1.contains("v1.0.0"));
    assert!(v2.contains("v2.0.0"));

    // Test 3: Different resource types shouldn't conflict
    // Clean up from previous test
    let claude_dir = project.project_path().join(".claude");
    if claude_dir.exists() {
        fs::remove_dir_all(&claude_dir).unwrap();
    }
    let lock_file = project.project_path().join("ccpm.lock");
    if lock_file.exists() {
        fs::remove_file(&lock_file).unwrap();
    }

    let manifest = format!(
        r#"[sources]
versioned = "file://{}"

[agents]
agent-one = {{ source = "versioned", path = "agents/example.md", version = "v1.0.0" }}

[snippets]
snippet-one = {{ source = "versioned", path = "snippets/utils.md", version = "v1.0.0" }}
"#,
        source_path.display().to_string().replace('\\', "/")
    );
    project.write_manifest(&manifest).unwrap();

    let output = project.run_ccpm(&["install"]).unwrap();
    output.assert_success();
}
