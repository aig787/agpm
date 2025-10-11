use anyhow::Result;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject, TestSourceRepo};
use crate::test_config;

/// Helper to initialize a git repository with tags, branches, and commits
async fn setup_git_repo_with_versions(repo: &TestSourceRepo) -> Result<String> {
    let repo_path = &repo.path;
    let git = &repo.git;

    // Create directory structure
    fs::create_dir_all(repo_path.join("agents")).await?;
    fs::create_dir_all(repo_path.join("snippets")).await?;

    // Create v1.0.0 content
    fs::write(repo_path.join("agents/example.md"), "# Example Agent v1.0.0\nInitial version")
        .await?;
    fs::write(repo_path.join("snippets/utils.md"), "# Utils Snippet v1.0.0\nInitial version")
        .await?;

    // Commit and tag v1.0.0
    git.add_all()?;
    git.commit("Initial commit v1.0.0")?;
    git.tag("v1.0.0")?;

    // Get commit hash for v1.0.0
    let v1_commit = git.get_commit_hash()?;

    // Create v1.1.0 content
    fs::write(repo_path.join("agents/example.md"), "# Example Agent v1.1.0\nMinor update").await?;
    git.add_all()?;
    git.commit("Version 1.1.0")?;
    git.tag("v1.1.0")?;

    // Create v1.2.0 content
    fs::write(repo_path.join("agents/example.md"), "# Example Agent v1.2.0\nAnother minor update")
        .await?;
    git.add_all()?;
    git.commit("Version 1.2.0")?;
    git.tag("v1.2.0")?;

    // Create v2.0.0 content
    fs::write(repo_path.join("agents/example.md"), "# Example Agent v2.0.0\nMajor version").await?;
    git.add_all()?;
    git.commit("Version 2.0.0 - Breaking changes")?;
    git.tag("v2.0.0")?;

    // Ensure we're on 'main' branch (git's default branch name varies)
    git.ensure_branch("main")?;

    git.create_branch("develop")?;

    fs::write(
        repo_path.join("agents/example.md"),
        "# Example Agent - Development\nUnstable development version",
    )
    .await?;
    fs::write(
        repo_path.join("agents/experimental.md"),
        "# Experimental Agent\nOnly in develop branch",
    )
    .await?;

    git.add_all()?;
    git.commit("Development changes")?;

    git.create_branch("feature/new-agent")?;

    fs::write(repo_path.join("agents/feature.md"), "# Feature Agent\nNew feature in progress")
        .await?;

    git.add_all()?;
    git.commit("Add feature agent")?;

    // Ensure we're on 'main' branch (git's default branch name varies)
    git.ensure_branch("main")?;

    Ok(v1_commit)
}

#[tokio::test]
async fn test_install_with_exact_version_tag() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    // Setup git repository with versions
    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with exact version
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Run install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Check installed file contains v1.0.0 content
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v1.0.0"));
    assert!(!installed.contains("v1.1.0"));
    assert!(!installed.contains("v2.0.0"));
}

#[tokio::test]
async fn test_install_with_caret_version_range() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with caret range (^1.0.0 should match 1.2.0 but not 2.0.0)
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").version("^1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v1.2.0 (highest compatible version)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v1.2.0"));
    assert!(!installed.contains("v2.0.0"));
}

#[tokio::test]
async fn test_install_with_tilde_version_range() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with tilde range (~1.1.0 should match 1.1.x but not 1.2.0)
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").version("~1.1.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v1.1.0 (only patch updates allowed)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v1.1.0"));
    assert!(!installed.contains("v1.2.0"));
}

#[tokio::test]
async fn test_install_with_branch_reference() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with branch reference
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("dev-example", |d| d.source("versioned").path("agents/example.md").branch("develop"))
        .add_agent("experimental", |d| d.source("versioned").path("agents/experimental.md").branch("develop"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Check we got develop branch content
    // Files use basename from path, not dependency name
    let example_content =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(example_content.contains("Development"));
    assert!(example_content.contains("Unstable"));

    // Check experimental agent exists (only in develop branch)
    assert!(project.project_path().join(".claude/agents/experimental.md").exists());
}

#[tokio::test]
async fn test_install_with_feature_branch() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with feature branch
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("feature", |d| d.source("versioned").path("agents/feature.md").branch("feature/new-agent"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Check feature agent was installed
    let feature_content =
        fs::read_to_string(project.project_path().join(".claude/agents/feature.md")).await.unwrap();
    assert!(feature_content.contains("Feature Agent"));
    assert!(feature_content.contains("New feature in progress"));
}

#[tokio::test]
async fn test_install_with_commit_hash() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    // Setup git repository and get v1.0.0 commit hash
    let v1_commit = setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with exact commit hash (rev)
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("pinned", |d| d.source("versioned").path("agents/example.md").rev(&v1_commit))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Should get exact v1.0.0 content
    // Files use basename from path, not dependency name
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v1.0.0"));
    assert!(installed.contains("Initial version"));
}

#[tokio::test]
async fn test_install_with_wildcard_version() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with wildcard "*"
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("any", |d| d.source("versioned").path("agents/example.md").version("*"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v2.0.0 (highest available)
    // Files use basename from path, not dependency name
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v2.0.0"));
}

#[tokio::test]
async fn test_install_with_mixed_versioning_methods() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    let v1_commit = setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with mixed versioning methods
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("stable", |d| d.source("versioned").path("agents/example.md").version("v1.1.0"))
        .add_agent("compatible", |d| d.source("versioned").path("agents/example.md").version("^1.0.0"))
        .add_agent("develop", |d| d.source("versioned").path("agents/example.md").branch("develop"))
        .add_agent("pinned", |d| d.source("versioned").path("agents/example.md").rev(&v1_commit))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    // Should fail due to version conflict - same resource with different versions
    assert!(!output.success, "Expected install to fail due to version conflicts");

    // Verify error message mentions the conflict
    assert!(
        output.stderr.contains("Version conflicts"),
        "Expected version conflict, got: {}",
        output.stderr
    );
    assert!(output.stderr.contains("example.md"));
}

#[tokio::test]
async fn test_version_constraint_with_greater_than() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Test >=1.1.0 constraint
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").version(">=1.1.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v2.0.0 (highest that satisfies >=1.1.0)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v2.0.0"));
}

#[tokio::test]
async fn test_version_constraint_with_range() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Test complex range: >=1.1.0, <2.0.0
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").version(">=1.1.0, <2.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Should get v1.2.0 (highest that satisfies the range)
    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v1.2.0"));
    assert!(!installed.contains("v2.0.0"));
}

#[tokio::test]
async fn test_update_branch_reference() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with branch reference
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("dev", |d| d.source("versioned").path("agents/example.md").branch("develop"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Initial install
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    // Modify the develop branch
    source_repo.git.checkout("develop").unwrap();

    fs::write(
        source_repo.path.join("agents/example.md"),
        "# Example Agent - Updated Development\nNewer unstable version",
    )
    .await
    .unwrap();

    source_repo.git.add_all().unwrap();
    source_repo.git.commit("Update develop branch").unwrap();

    // Run update to get latest develop branch
    let output = project.run_agpm(&["update"]).unwrap();
    output.assert_success();

    // Check we got the updated content
    // Files use basename from path, not dependency name
    let file_path = project.project_path().join(".claude/agents/example.md");
    let updated = fs::read_to_string(&file_path).await.unwrap_or_else(|e| {
        panic!("Failed to read file {file_path:?}: {e}");
    });
    println!("File content after update: {updated:?}");
    assert!(updated.contains("Updated Development"), "File content: {updated}");
    assert!(updated.contains("Newer unstable"));
}

#[tokio::test]
async fn test_lockfile_records_correct_version_info() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    let feature_commit = {
        source_repo.git.checkout("feature/new-agent").unwrap();
        let commit = source_repo.git.get_commit_hash().unwrap();
        // Ensure we're on 'main' branch (git's default branch name varies)
        source_repo.git.ensure_branch("main").unwrap();
        commit
    };

    // Create manifest with different version types
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("tagged", |d| d.source("versioned").path("agents/example.md").version("v1.1.0"))
        .add_agent("branched", |d| d.source("versioned").path("agents/experimental.md").branch("develop"))
        .add_agent("committed", |d| d.source("versioned").path("agents/feature.md").rev(&feature_commit))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    // Should succeed - all dependencies have different paths
    output.assert_success();

    // Verify lockfile was created with all entries
    let lockfile = project.read_lockfile().await.unwrap();
    assert!(lockfile.contains("tagged"));
    assert!(lockfile.contains("branched"));
    assert!(lockfile.contains("committed"));
}

#[tokio::test]
async fn test_error_on_invalid_version_constraint() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with unsatisfiable version
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").version("v99.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // This should fail
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
    assert!(
        output.stderr.contains("Git operation failed")
            || output.stderr.contains("No matching version found"),
        "Expected error about version not found, got: {}",
        output.stderr
    );
}

#[tokio::test]
async fn test_error_on_nonexistent_branch() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with non-existent branch
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").branch("nonexistent"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(!output.success, "Expected command to fail but it succeeded");
}

#[tokio::test]
async fn test_frozen_install_uses_lockfile_versions() {
    test_config::init_test_env();
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("versioned").await.unwrap();

    setup_git_repo_with_versions(&source_repo).await.unwrap();

    // Create manifest with version range
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &format!("file://{}", source_repo.path.display().to_string().replace('\\', "/")))
        .add_agent("example", |d| d.source("versioned").path("agents/example.md").version("^1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Initial install (should get v1.2.0)
    let output = project.run_agpm(&["install"]).unwrap();
    output.assert_success();

    let lockfile = fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    assert!(lockfile.contains("version = \"v1.2.0\""));

    // Delete installed files
    fs::remove_dir_all(project.project_path().join(".claude")).await.unwrap();

    // Run frozen install - should use lockfile version (v1.2.0) not latest (v2.0.0)
    let output = project.run_agpm(&["install", "--frozen"]).unwrap();
    output.assert_success();

    let installed =
        fs::read_to_string(project.project_path().join(".claude/agents/example.md")).await.unwrap();
    assert!(installed.contains("v1.2.0"));
    assert!(!installed.contains("v2.0.0"));
}

/// Test that path collision detection works correctly for different scenarios
#[tokio::test]
async fn test_path_collision_detection() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("versioned").await?;

    setup_git_repo_with_versions(&source_repo).await?;

    // Test 1: Two dependencies pointing to same path should fail
    let manifest = ManifestBuilder::new()
        .add_source("versioned", &source_repo.file_url())
        .add_agent("version-one", |d| d.source("versioned").path("agents/example.md").version("v1.0.0"))
        .add_agent("version-two", |d| d.source("versioned").path("agents/example.md").version("v2.0.0"))
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Expected collision for same path");
    // Should detect version conflict (same resource, different versions)
    assert!(
        output.stderr.contains("Version conflicts"),
        "Expected version conflict error, got: {}",
        output.stderr
    );
    assert!(output.stderr.contains("v1.0.0"));
    assert!(output.stderr.contains("v2.0.0"));

    // Test 2: Custom targets should allow same source path
    // Clean up from previous test
    let claude_dir = project.project_path().join(".claude");
    if claude_dir.exists() {
        fs::remove_dir_all(&claude_dir).await?;
    }
    let lock_file = project.project_path().join("agpm.lock");
    if lock_file.exists() {
        fs::remove_file(&lock_file).await?;
    }

    let manifest = ManifestBuilder::new()
        .add_source("versioned", &source_repo.file_url())
        .add_agent("version-one", |d| d.source("versioned").path("agents/example.md").version("v1.0.0").target("v1"))
        .add_snippet("version-two", |d| d.source("versioned").path("snippets/utils.md").version("v1.0.0").target("v2"))
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify both files are installed with custom targets
    // Custom target "v1" becomes ".claude/agents/v1/example.md"
    // Custom target "v2" becomes ".agpm/snippets/v2/utils.md" (snippets default to agpm artifact type)
    let v1_path = project.project_path().join(".claude/agents/v1/example.md");
    let v2_path = project.project_path().join(".agpm/snippets/v2/utils.md");

    let v1 = fs::read_to_string(&v1_path)
        .await
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", v1_path.display(), e));
    let v2 = fs::read_to_string(&v2_path)
        .await
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", v2_path.display(), e));
    assert!(v1.contains("v1.0.0"));
    assert!(v2.contains("v1.0.0"));

    // Test 3: Different resource types shouldn't conflict
    // Clean up from previous test
    let claude_dir = project.project_path().join(".claude");
    if claude_dir.exists() {
        fs::remove_dir_all(&claude_dir).await?;
    }
    let lock_file = project.project_path().join("agpm.lock");
    if lock_file.exists() {
        fs::remove_file(&lock_file).await?;
    }

    let manifest = ManifestBuilder::new()
        .add_source("versioned", &source_repo.file_url())
        .add_agent("agent-one", |d| d.source("versioned").path("agents/example.md").version("v1.0.0"))
        .add_snippet("snippet-one", |d| d.source("versioned").path("snippets/utils.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    Ok(())
}
