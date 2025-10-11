//! Integration tests for version conflict detection.
//!
//! These tests verify that the conflict detector properly identifies
//! incompatible version requirements and prevents installation.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test that conflicting exact versions are detected and installation fails.
#[tokio::test]
async fn test_exact_version_conflict_blocks_install() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("agpm.toml");

    // Create manifest with two resources pointing to same source:path but different versions
    fs::write(
        &manifest_path,
        r#"
[sources]
community = "https://github.com/aig787/agpm-community.git"

[agents]
# Same path, different versions - should conflict
api-designer-v1 = { source = "community", path = "agents/awesome-claude-code-subagents/categories/01-core-development/api-designer.md", version = "v0.0.1" }
api-designer-v2 = { source = "community", path = "agents/awesome-claude-code-subagents/categories/01-core-development/api-designer.md", version = "v0.0.2" }
"#,
    )
    .await
    .unwrap();

    let mut cmd = Command::cargo_bin("agpm").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("install")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Version conflicts detected"))
        .stderr(predicate::str::contains("api-designer.md"))
        .stderr(predicate::str::contains("v0.0.1"))
        .stderr(predicate::str::contains("v0.0.2"));
}

/// Test that identical exact versions do NOT conflict.
///
/// This is the most basic case - when multiple resources need the exact same
/// version of the same file, there's no conflict.
#[tokio::test]
async fn test_identical_exact_versions_no_conflict() {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create test resource
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with two resources pointing to same source:path and IDENTICAL version
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.file_url())
        .add_standard_agent("test-agent-1", "test-repo", "agents/test-agent.md")
        .add_standard_agent("test-agent-2", "test-repo", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);
    assert!(
        !output.stderr.contains("Version conflicts detected"),
        "Should not contain conflict message. Stderr: {}",
        output.stderr
    );
}

/// Test that mixing semver version with git branch is detected as a conflict.
///
/// This verifies that the conflict detector properly identifies when the same
/// resource is requested with both a semver version and a git branch reference.
#[tokio::test]
async fn test_semver_vs_branch_conflict_blocks_install() {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create v1.0.0
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create v2.0.0
    source_repo.add_resource("agents", "test-agent", "# Test Agent v2.0.0").await.unwrap();
    source_repo.commit_all("Version 2.0.0").unwrap();
    source_repo.tag_version("v2.0.0").unwrap();

    // Ensure we're on 'main' branch (git's default branch name varies)
    source_repo.git.ensure_branch("main").unwrap();

    // Create develop branch
    source_repo.git.create_branch("develop").unwrap();
    source_repo.add_resource("agents", "test-agent", "# Test Agent - Development").await.unwrap();
    source_repo.commit_all("Development changes").unwrap();
    source_repo.git.checkout("main").unwrap();

    // Create manifest with same resource using semver version and git branch
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.file_url())
        .add_standard_agent("agent-stable", "test-repo", "agents/test-agent.md")
        .add_agent("agent-dev", |d| {
            d.source("test-repo").path("agents/test-agent.md").branch("main")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        !output.success,
        "Install should fail with version conflict. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("Version conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("test-agent.md"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
}

/// Test that HEAD (unspecified version) mixed with a pinned version is detected as a conflict.
///
/// This verifies the conflict detector identifies when the same resource is requested
/// both with and without a version specification (HEAD means "use whatever is current").
#[tokio::test]
async fn test_head_vs_pinned_version_conflict_blocks_install() {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create v1.0.0
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with same resource, one unspecified (HEAD), one pinned
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.file_url())
        .add_agent("agent-head", |d| d.source("test-repo").path("agents/test-agent.md"))
        .add_standard_agent("agent-pinned", "test-repo", "agents/test-agent.md")
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        !output.success,
        "Install should fail with version conflict. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("Version conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("test-agent.md"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
}

/// Test that mixed git branch names are detected as conflicts.
///
/// This verifies that different branch references (e.g., "main" vs "develop")
/// for the same resource are properly identified as conflicts.
#[tokio::test]
async fn test_different_branches_conflict_blocks_install() {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create initial commit
    source_repo.add_resource("agents", "test-agent", "# Test Agent - Main").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();

    // Ensure we're on 'main' branch (git's default branch name varies)
    source_repo.git.ensure_branch("main").unwrap();

    // Create develop branch with different content
    source_repo.git.create_branch("develop").unwrap();
    source_repo.add_resource("agents", "test-agent", "# Test Agent - Development").await.unwrap();
    source_repo.commit_all("Development changes").unwrap();
    source_repo.git.checkout("main").unwrap();

    // Create manifest with same resource using different branches
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.file_url())
        .add_agent("agent-main", |d| {
            d.source("test-repo").path("agents/test-agent.md").branch("main")
        })
        .add_agent("agent-dev", |d| {
            d.source("test-repo").path("agents/test-agent.md").branch("develop")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        !output.success,
        "Install should fail with version conflict. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("Version conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("test-agent.md"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
}

/// Test that case variations of the same branch name do NOT conflict.
///
/// This verifies that "main", "Main", and "MAIN" are treated as the same branch
/// on case-insensitive filesystems (Windows, macOS default).
/// On case-sensitive filesystems (Linux), we need to create both branches to test this.
#[tokio::test]
async fn test_same_branch_different_case_no_conflict() {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create initial commit
    source_repo.add_resource("agents", "test-agent", "# Test Agent").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();

    // Ensure we're on 'main' branch (git's default branch name varies)
    source_repo.git.ensure_branch("main").unwrap();

    // On case-sensitive filesystems (Linux), Git allows branches with different case.
    // On case-insensitive filesystems (macOS, Windows), "main" and "Main" are the same.
    // Try to create "Main" branch - if it succeeds, we're on a case-sensitive filesystem.
    if source_repo.git.create_branch("Main").is_ok() {
        // We successfully created "Main" - we're on case-sensitive filesystem (Linux)
        // The new branch is already created from main's current commit, so we're good
        // Just go back to main
        source_repo.git.checkout("main").unwrap();
    }
    // If create_branch failed, we're on case-insensitive (macOS/Windows) and "Main" == "main"

    // Create manifest with same resource using different case for branch name
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.file_url())
        .add_agent("agent-1", |d| d.source("test-repo").path("agents/test-agent.md").branch("main"))
        .add_agent("agent-2", |d| d.source("test-repo").path("agents/test-agent.md").branch("Main"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);
    assert!(
        !output.stderr.contains("Version conflicts detected"),
        "Should not contain conflict message. Stderr: {}",
        output.stderr
    );
}
