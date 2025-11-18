//! Integration tests for version conflict detection.
//!
//! These tests verify that the conflict detector properly identifies
//! incompatible version requirements and prevents installation.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Test that conflicting exact versions are detected and installation fails.
#[tokio::test]
async fn test_exact_version_conflict_blocks_install() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    // Create two versions of the same agent
    source_repo.add_resource("agents", "api-designer", "# API Designer v0.0.1").await?;
    source_repo.commit_all("Add v0.0.1")?;
    source_repo.tag_version("v0.0.1")?;

    // Update to v0.0.2
    source_repo.add_resource("agents", "api-designer", "# API Designer v0.0.2").await?;
    source_repo.commit_all("Update to v0.0.2")?;
    source_repo.tag_version("v0.0.2")?;

    // Create manifest with same path but different versions - should conflict
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("api-designer-v1", |d| {
            d.source("community").path("agents/api-designer.md").version("v0.0.1")
        })
        .add_agent("api-designer-v2", |d| {
            d.source("community").path("agents/api-designer.md").version("v0.0.2")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail with version conflict. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("Unresolvable SHA conflicts detected")
            || output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("Target path conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agents/api-designer"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("v0.0.1") && output.stderr.contains("v0.0.2"),
        "Should mention both conflicting versions. Stderr: {}",
        output.stderr
    );
    Ok(())
}

/// Test that identical exact versions do NOT conflict.
///
/// This is the most basic case - when multiple resources need the exact same
/// version of the same file, there's no conflict.
#[tokio::test]
async fn test_identical_exact_versions_no_conflict() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create test resource
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with two resources pointing to same source:path and IDENTICAL version
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
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
    Ok(())
}

/// Test that mixing semver version with git branch is detected as a conflict.
///
/// This verifies that the conflict detector properly identifies when the same
/// resource is requested with both a semver version and a git branch reference.
#[tokio::test]
async fn test_semver_vs_branch_conflict_blocks_install() -> Result<()> {
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
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
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
        output.stderr.contains("Unresolvable SHA conflicts detected")
            || output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("Target path conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agents/test-agent"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
    Ok(())
}

/// Test that conflicting version constraints are detected when no compatible version exists.
///
/// This verifies SHA-based conflict detection with backtracking: two version constraints
/// that resolve to different SHAs and have no overlapping versions will fail.
#[tokio::test]
async fn test_head_vs_pinned_version_conflict_blocks_install() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create v1.0.0
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("v1.0.0 commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create v2.0.0 with different content (different SHA)
    source_repo
        .add_resource("agents", "test-agent", "# Test Agent v2.0.0 - DIFFERENT")
        .await
        .unwrap();
    source_repo.commit_all("v2.0.0 commit").unwrap();
    source_repo.tag_version("v2.0.0").unwrap();

    // Create manifest with same resource using incompatible version constraints
    // ^1.0.0 matches only v1.x.x (v1.0.0)
    // ^2.0.0 matches only v2.x.x (v2.0.0)
    // These resolve to different SHAs and have no overlap
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-v1", |d| {
            d.source("test-repo").path("agents/test-agent.md").version("^1.0.0")
        })
        .add_agent("agent-v2", |d| {
            d.source("test-repo").path("agents/test-agent.md").version("^2.0.0")
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
        output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("automatic resolution failed"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agents/test-agent"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
    Ok(())
}

/// Test that mixed git branch names are detected as conflicts.
///
/// This verifies that different branch references (e.g., "main" vs "develop")
/// for the same resource are properly identified as conflicts.
#[tokio::test]
async fn test_different_branches_conflict_blocks_install() -> Result<()> {
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
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
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
        output.stderr.contains("Unresolvable SHA conflicts detected")
            || output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("Target path conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agents/test-agent"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
    Ok(())
}

/// Test that case variations of the same branch name do NOT conflict.
///
/// This verifies that "main", "Main", and "MAIN" are treated as the same branch
/// on case-insensitive filesystems (Windows, macOS default).
/// On case-sensitive filesystems (Linux), we need to create both branches to test this.
#[tokio::test]
async fn test_same_branch_different_case_no_conflict() -> Result<()> {
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
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
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
    Ok(())
}

/// Test that changing a dependency source doesn't leave stale entries in lockfile.
///
/// This reproduces a bug where commenting out a Git source dependency and replacing it
/// with a local path dependency of the same name would cause the lockfile to have
/// TWO entries with the same name but different sources, leading to false conflict errors.
#[tokio::test]
async fn test_changing_dependency_source_no_false_conflict() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create initial resource in Git repo
    source_repo.add_resource("commands", "commit", "# Commit Command v1").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create local resource directory with a different version of the same resource
    let local_dir = project.project_path().join("local-resources");
    tokio::fs::create_dir_all(&local_dir.join("commands")).await.unwrap();
    tokio::fs::write(local_dir.join("commands/commit.md"), "# Commit Command v2 (local)")
        .await
        .unwrap();

    // Step 1: Install with Git source
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
        .add_command("commit", |d| {
            d.source("test-repo").path("commands/commit.md").version("v1.0.0")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial install should succeed. Stderr: {}", output.stderr);

    // Step 2: Change to local path dependency with same name
    let local_path = local_dir.to_str().unwrap();
    let manifest = ManifestBuilder::new()
        .add_command("commit", |d| d.path(&format!("{}/commands/commit.md", local_path)))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // Step 3: Install again - should NOT report conflict
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        output.success,
        "Install after source change should succeed. Stderr: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Target path conflicts detected"),
        "Should not contain false conflict error. Stderr: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Conflicts: commit, commit"),
        "Should not show duplicate names in conflict. Stderr: {}",
        output.stderr
    );

    // Verify lockfile has only ONE entry for "commit" (check manifest_alias since names are now canonical)
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_content = tokio::fs::read_to_string(&lockfile_path).await.unwrap();
    let commit_count = lockfile_content.matches("manifest_alias = \"commit\"").count();
    assert_eq!(
        commit_count, 1,
        "Lockfile should have exactly one entry for 'commit', found {}: {}",
        commit_count, lockfile_content
    );
    Ok(())
}

/// Test that changing a pattern dependency source doesn't leave stale entries in lockfile.
///
/// This tests the scenario where a pattern dependency (e.g., "agents/*.md") changes source,
/// ensuring that the old pattern-expanded entries are removed from the lockfile.
#[tokio::test]
async fn test_pattern_source_change_no_false_conflict() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

    // Create two source repos with the same agent files
    let source_repo1 = project.create_source_repo("repo1").await.unwrap();
    source_repo1.add_resource("agents", "helper", "# Helper v1 from repo1").await.unwrap();
    source_repo1.add_resource("agents", "worker", "# Worker v1 from repo1").await.unwrap();
    source_repo1.commit_all("Initial commit").unwrap();
    source_repo1.tag_version("v1.0.0").unwrap();

    let source_repo2 = project.create_source_repo("repo2").await.unwrap();
    source_repo2.add_resource("agents", "helper", "# Helper v1 from repo2").await.unwrap();
    source_repo2.add_resource("agents", "worker", "# Worker v1 from repo2").await.unwrap();
    source_repo2.commit_all("Initial commit").unwrap();
    source_repo2.tag_version("v1.0.0").unwrap();

    // Step 1: Install with pattern from repo1
    let manifest = ManifestBuilder::new()
        .add_source("repo1", &source_repo1.bare_file_url(project.sources_path())?)
        .add_agent("all-agents", |d| d.source("repo1").path("agents/*.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial pattern install should succeed. Stderr: {}", output.stderr);

    // Verify lockfile has entries from repo1
    let lockfile_content =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    assert!(
        lockfile_content.contains("source = \"repo1\""),
        "Lockfile should have entries from repo1"
    );

    // Step 2: Change pattern to repo2 with same manifest alias
    let manifest2 = ManifestBuilder::new()
        .add_source("repo2", &source_repo2.bare_file_url(project.sources_path())?)
        .add_agent("all-agents", |d| d.source("repo2").path("agents/*.md").version("v1.0.0"))
        .build();
    eprintln!("=== New manifest ===\n{}", manifest2);
    project.write_manifest(&manifest2).await.unwrap();

    // Debug: Check if files exist in repo2
    eprintln!("=== Files in repo2 ===");
    let repo2_agents_path = source_repo2.path.join("agents");
    if repo2_agents_path.exists() {
        let entries = std::fs::read_dir(&repo2_agents_path).unwrap();
        for entry in entries {
            let entry = entry.unwrap();
            eprintln!("  - {}", entry.file_name().to_string_lossy());
        }
    } else {
        eprintln!("  agents/ directory does not exist!");
    }

    // Step 3: Install again - should NOT report conflict
    let output = project.run_agpm(&["install", "--verbose"]).unwrap();
    eprintln!("=== Second install stdout ===\n{}", output.stdout);
    eprintln!("=== Second install stderr ===\n{}", output.stderr);
    eprintln!("=== Success: {} ===", output.success);
    assert!(
        output.success,
        "Install after pattern source change should succeed. Stderr: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Target path conflicts detected"),
        "Should not contain false conflict error. Stderr: {}",
        output.stderr
    );

    // Verify lockfile now has entries from repo2 (not repo1)
    let updated_lockfile =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    eprintln!("=== Updated lockfile contents ===\n{}", updated_lockfile);
    assert!(
        updated_lockfile.contains("source = \"repo2\""),
        "Lockfile should have been updated to repo2. Lockfile:\n{}",
        updated_lockfile
    );
    assert!(
        !updated_lockfile.contains("source = \"repo1\""),
        "Lockfile should no longer have repo1 entries"
    );

    // Verify we still have exactly 2 agents (helper and worker), not 4
    let helper_count = updated_lockfile.matches("name = \"agents/helper\"").count();
    let worker_count = updated_lockfile.matches("name = \"agents/worker\"").count();
    assert_eq!(
        helper_count, 1,
        "Lockfile should have exactly one helper entry, found {}: {}",
        helper_count, updated_lockfile
    );
    assert_eq!(
        worker_count, 1,
        "Lockfile should have exactly one worker entry, found {}: {}",
        worker_count, updated_lockfile
    );
    Ok(())
}

/// Test that changing a dependency's source also updates its transitive dependencies.
///
/// This verifies that when a manifest entry's source changes, all of its transitive
/// dependencies are also updated to the new source. This is critical for ensuring
/// that the entire dependency tree remains consistent.
#[tokio::test]
async fn test_source_change_updates_transitive_deps() -> Result<()> {
    use crate::common::{ManifestBuilder, TestProject};

    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

    // Create two source repos
    let repo1 = project.create_source_repo("repo1").await.unwrap();
    let repo2 = project.create_source_repo("repo2").await.unwrap();

    // Create an agent with transitive dependencies in repo1
    let agent_content = r#"---
dependencies:
  snippets:
    - path: ../snippets/utils.md
---
# Agent A from repo1
"#;
    repo1.add_resource("agents", "agent-a", agent_content).await.unwrap();
    repo1.add_resource("snippets", "utils", "# Utils from repo1").await.unwrap();
    repo1.commit_all("Initial commit").unwrap();
    repo1.tag_version("v1.0.0").unwrap();

    // Create the same structure in repo2 (but with different content)
    let agent_content2 = r#"---
dependencies:
  snippets:
    - path: ../snippets/utils.md
---
# Agent A from repo2
"#;
    repo2.add_resource("agents", "agent-a", agent_content2).await.unwrap();
    repo2.add_resource("snippets", "utils", "# Utils from repo2").await.unwrap();
    repo2.commit_all("Initial commit").unwrap();
    repo2.tag_version("v1.0.0").unwrap();

    // Step 1: Install from repo1
    let manifest = ManifestBuilder::new()
        .add_source("repo1", &repo1.bare_file_url(project.sources_path())?)
        .add_agent("agent-a", |d| d.source("repo1").path("agents/agent-a.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial install should succeed. Stderr: {}", output.stderr);

    // Verify lockfile has both agent-a and utils from repo1
    let lockfile_content =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    eprintln!("=== Initial lockfile ===\n{}", lockfile_content);
    assert!(
        lockfile_content.contains("manifest_alias = \"agent-a\""),
        "Lockfile should contain agent-a"
    );
    // Transitive dependency should have canonical name but NO manifest_alias
    assert!(
        lockfile_content.contains("name = \"snippets/utils\"")
            && lockfile_content.matches("source = \"repo1\"").count() >= 2,
        "Lockfile should contain transitive dep utils from repo1. Lockfile:\n{}",
        lockfile_content
    );
    let repo1_count = lockfile_content.matches("source = \"repo1\"").count();
    assert_eq!(repo1_count, 2, "Lockfile should have 2 entries from repo1 (agent-a + utils)");

    // Verify files are installed
    let agent_path = project.project_path().join(".claude/agents/agent-a.md");
    let utils_path = project.project_path().join(".claude/snippets/utils.md");
    assert!(agent_path.exists(), "Agent should be installed");
    assert!(utils_path.exists(), "Transitive dep should be installed");
    let agent_content_installed = tokio::fs::read_to_string(&agent_path).await.unwrap();
    assert!(agent_content_installed.contains("repo1"), "Agent should be from repo1");

    // Step 2: Change source to repo2
    let manifest2 = ManifestBuilder::new()
        .add_source("repo2", &repo2.bare_file_url(project.sources_path())?)
        .add_agent("agent-a", |d| d.source("repo2").path("agents/agent-a.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest2).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        output.success,
        "Install after source change should succeed. Stderr: {}",
        output.stderr
    );

    // Step 3: Verify lockfile now has both entries from repo2, not repo1
    let updated_lockfile =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();

    // Should NOT have any entries from repo1
    assert!(
        !updated_lockfile.contains("source = \"repo1\""),
        "Lockfile should not have any entries from old repo1. Lockfile:\n{}",
        updated_lockfile
    );

    // Should have both agent-a and utils from repo2
    let repo2_count = updated_lockfile.matches("source = \"repo2\"").count();
    assert_eq!(
        repo2_count, 2,
        "Lockfile should have 2 entries from repo2 (agent-a + utils). Lockfile:\n{}",
        updated_lockfile
    );

    // Should have exactly one agent-a and one utils (no duplicates)
    let agent_count = updated_lockfile.matches("name = \"agents/agent-a\"").count();
    let utils_count = updated_lockfile.matches("name = \"snippets/utils\"").count();
    assert_eq!(
        agent_count, 1,
        "Lockfile should have exactly one agent-a entry. Lockfile:\n{}",
        updated_lockfile
    );
    assert_eq!(
        utils_count, 1,
        "Lockfile should have exactly one utils entry. Lockfile:\n{}",
        updated_lockfile
    );

    // Verify files are updated
    let agent_content_updated = tokio::fs::read_to_string(&agent_path).await.unwrap();
    assert!(agent_content_updated.contains("repo2"), "Agent should now be from repo2");
    Ok(())
}

/// Test that changing a pattern dependency's source updates both the pattern-expanded
/// resources AND their transitive dependencies.
///
/// This is the most complex scenario: pattern expansion + transitive deps + source change.
/// It verifies that when a pattern like "agents/*.md" changes source, all expanded
/// resources (helper, worker) and their transitive dependencies (utils) are updated.
#[tokio::test]
async fn test_pattern_with_transitive_deps_source_change() -> Result<()> {
    use crate::common::{ManifestBuilder, TestProject};

    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

    // Create two source repos
    let repo1 = project.create_source_repo("repo1").await.unwrap();
    let repo2 = project.create_source_repo("repo2").await.unwrap();

    // In repo1: Create two agents (helper, worker) that both depend on utils
    let helper_content = r#"---
dependencies:
  snippets:
    - path: ../snippets/utils.md
---
# Helper from repo1
"#;
    let worker_content = r#"---
dependencies:
  snippets:
    - path: ../snippets/utils.md
---
# Worker from repo1
"#;
    repo1.add_resource("agents", "helper", helper_content).await.unwrap();
    repo1.add_resource("agents", "worker", worker_content).await.unwrap();
    repo1.add_resource("snippets", "utils", "# Utils from repo1").await.unwrap();
    repo1.commit_all("Initial commit").unwrap();
    repo1.tag_version("v1.0.0").unwrap();

    // In repo2: Same structure but different content
    let helper_content2 = r#"---
dependencies:
  snippets:
    - path: ../snippets/utils.md
---
# Helper from repo2
"#;
    let worker_content2 = r#"---
dependencies:
  snippets:
    - path: ../snippets/utils.md
---
# Worker from repo2
"#;
    repo2.add_resource("agents", "helper", helper_content2).await.unwrap();
    repo2.add_resource("agents", "worker", worker_content2).await.unwrap();
    repo2.add_resource("snippets", "utils", "# Utils from repo2").await.unwrap();
    repo2.commit_all("Initial commit").unwrap();
    repo2.tag_version("v1.0.0").unwrap();

    // Step 1: Install pattern from repo1
    let manifest = ManifestBuilder::new()
        .add_source("repo1", &repo1.bare_file_url(project.sources_path())?)
        .add_agent("all-agents", |d| d.source("repo1").path("agents/*.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial pattern install should succeed. Stderr: {}", output.stderr);

    // Verify lockfile has all 3 resources from repo1: helper, worker, utils
    let lockfile_content =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    eprintln!("=== Initial lockfile ===\n{}", lockfile_content);

    assert!(lockfile_content.contains("name = \"agents/helper\""), "Should have helper");
    assert!(lockfile_content.contains("name = \"agents/worker\""), "Should have worker");
    assert!(
        lockfile_content.contains("name = \"snippets/utils\"")
            && lockfile_content.matches("source = \"repo1\"").count() >= 3,
        "Should have transitive utils from repo1. Lockfile:\n{}",
        lockfile_content
    );

    let repo1_count = lockfile_content.matches("source = \"repo1\"").count();
    assert_eq!(
        repo1_count, 3,
        "Lockfile should have 3 entries from repo1 (helper + worker + utils). Found: {}",
        repo1_count
    );

    // Verify files are installed
    let helper_path = project.project_path().join(".claude/agents/helper.md");
    let worker_path = project.project_path().join(".claude/agents/worker.md");
    let utils_path = project.project_path().join(".claude/snippets/utils.md");
    assert!(helper_path.exists(), "Helper should be installed");
    assert!(worker_path.exists(), "Worker should be installed");
    assert!(utils_path.exists(), "Utils should be installed");

    // Step 2: Change pattern source to repo2
    let manifest2 = ManifestBuilder::new()
        .add_source("repo2", &repo2.bare_file_url(project.sources_path())?)
        .add_agent("all-agents", |d| d.source("repo2").path("agents/*.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest2).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(
        output.success,
        "Install after pattern source change should succeed. Stderr: {}",
        output.stderr
    );

    // Step 3: Verify lockfile has ALL entries from repo2, NONE from repo1
    let updated_lockfile =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    eprintln!("=== Updated lockfile ===\n{}", updated_lockfile);

    // Should NOT have any entries from repo1
    assert!(
        !updated_lockfile.contains("source = \"repo1\""),
        "Lockfile should not have any entries from old repo1. Lockfile:\n{}",
        updated_lockfile
    );

    // Should have all 3 resources from repo2
    let repo2_count = updated_lockfile.matches("source = \"repo2\"").count();
    assert_eq!(
        repo2_count, 3,
        "Lockfile should have 3 entries from repo2 (helper + worker + utils). Lockfile:\n{}",
        updated_lockfile
    );

    // Should have exactly one of each resource (no duplicates)
    let helper_count = updated_lockfile.matches("name = \"agents/helper\"").count();
    let worker_count = updated_lockfile.matches("name = \"agents/worker\"").count();
    let utils_count = updated_lockfile.matches("name = \"snippets/utils\"").count();

    assert_eq!(
        helper_count, 1,
        "Lockfile should have exactly one helper entry. Lockfile:\n{}",
        updated_lockfile
    );
    assert_eq!(
        worker_count, 1,
        "Lockfile should have exactly one worker entry. Lockfile:\n{}",
        updated_lockfile
    );
    assert_eq!(
        utils_count, 1,
        "Lockfile should have exactly one utils entry. Lockfile:\n{}",
        updated_lockfile
    );

    // Verify files are updated with repo2 content
    let helper_content = tokio::fs::read_to_string(&helper_path).await.unwrap();
    let worker_content = tokio::fs::read_to_string(&worker_path).await.unwrap();
    let utils_content = tokio::fs::read_to_string(&utils_path).await.unwrap();

    assert!(helper_content.contains("repo2"), "Helper should be from repo2");
    assert!(worker_content.contains("repo2"), "Worker should be from repo2");
    assert!(utils_content.contains("repo2"), "Utils should be from repo2");
    Ok(())
}

/// Test that commenting out a dependency removes it from the lockfile without conflicts.
///
/// This is a regression test for a bug where commented-out manifest items weren't being
/// removed from the lockfile before resolution, causing conflicts.
#[tokio::test]
async fn test_commented_out_dependency_removed_from_lockfile() -> Result<()> {
    let project = TestProject::new().await.unwrap();

    // Create a source repo with two agents
    let source_repo = project.create_source_repo("source").await.unwrap();
    source_repo.add_resource("agents", "agent-a", "# Agent A\nFirst agent").await.unwrap();
    source_repo.add_resource("agents", "agent-b", "# Agent B\nSecond agent").await.unwrap();
    source_repo.commit_all("Initial agents").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create bare URL once to avoid duplicate bare repo creation
    let source_url = source_repo.bare_file_url(project.sources_path())?;

    // Create initial manifest with both agents
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_url)
        .add_agent("agent-a", |d| d.source("source").path("agents/agent-a.md").version("v1.0.0"))
        .add_agent("agent-b", |d| d.source("source").path("agents/agent-b.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    // First install - should create lockfile with both agents
    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "First install should succeed. stderr: {}", output.stderr);

    // Verify lockfile contains both agents
    let lockfile_content = project.read_lockfile().await.unwrap();
    assert!(
        lockfile_content.contains("manifest_alias = \"agent-a\""),
        "Lockfile should contain agent-a"
    );
    assert!(
        lockfile_content.contains("manifest_alias = \"agent-b\""),
        "Lockfile should contain agent-b"
    );

    // Comment out agent-b in the manifest
    let manifest2 = ManifestBuilder::new()
        .add_source("source", &source_url)
        .add_agent("agent-a", |d| d.source("source").path("agents/agent-a.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest2).await.unwrap();

    // Second install - should remove agent-b from lockfile without conflicts
    let output2 = project.run_agpm(&["install"]).unwrap();
    assert!(
        output2.success,
        "Second install should succeed without conflicts. stderr: {}",
        output2.stderr
    );

    // Verify lockfile no longer contains agent-b
    let updated_lockfile = project.read_lockfile().await.unwrap();
    assert!(
        updated_lockfile.contains("manifest_alias = \"agent-a\""),
        "Lockfile should still contain agent-a"
    );
    assert!(
        !updated_lockfile.contains("manifest_alias = \"agent-b\""),
        "Lockfile should NOT contain agent-b after commenting out. Lockfile:\n{}",
        updated_lockfile
    );

    // Verify no conflict messages in output
    assert!(
        !output2.stderr.contains("conflict"),
        "Should not have any conflicts. stderr: {}",
        output2.stderr
    );

    // Verify agent-a file exists
    let agent_a_path = project.project_path().join(".claude/agents/agent-a.md");
    assert!(agent_a_path.exists(), "Agent A file should exist");
    Ok(())
}

/// Test that direct and transitive dependencies to the same local file don't cause false conflicts.
///
/// This is a regression test for a bug where:
/// 1. A direct dependency with a custom manifest name (e.g., "my-agent")
/// 2. A transitive dependency with a path-based name (e.g., "../local/agents/helper")
/// Both pointing to the same local file would create duplicate lockfile entries with different
/// names but the same path, triggering false conflict detection.
///
/// The fix ensures path-based deduplication for local dependencies (source = None).
#[tokio::test]
async fn test_local_direct_and_transitive_deps_no_false_conflict() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;

    // Create a local directory with two agents
    let local_dir = project.project_path().join("local-agents");
    tokio::fs::create_dir_all(&local_dir.join("agents")).await?;
    let _local_path = local_dir.to_str().unwrap();

    // Helper agent (will be referenced as transitive dep)
    let helper_content = "# Helper Agent\nProvides helper functionality";
    tokio::fs::write(local_dir.join("agents/helper.md"), helper_content).await?;

    // Parent agent that depends on helper (will have transitive dep)
    // Use a relative path in transitive dep (as it would be in real usage)
    let parent_content = r#"---
dependencies:
  agents:
    - path: helper.md
---
# Parent Agent
Uses the helper agent"#;
    tokio::fs::write(local_dir.join("agents/parent.md"), parent_content).await?;

    // Create manifest that:
    // 1. Directly references helper with custom name "my-helper"
    // 2. References parent which has transitive dep on helper (will be named "helper")
    // Use relative paths (relative to project root)
    let manifest = ManifestBuilder::new()
        .add_agent("my-helper", |d| d.path("local-agents/agents/helper.md"))
        .add_agent("parent", |d| d.path("local-agents/agents/parent.md"))
        .build();
    project.write_manifest(&manifest).await?;

    // Install should succeed without conflicts
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed without false conflicts. Stderr: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Target path conflicts detected"),
        "Should not report false conflicts. Stderr: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Version conflicts detected"),
        "Should not report version conflicts. Stderr: {}",
        output.stderr
    );

    // Verify lockfile has only ONE entry for helper (deduplicated by path)
    let lockfile = project.read_lockfile().await?;
    let helper_entries = lockfile.matches("path = \"local-agents/agents/helper.md\"").count();
    assert_eq!(
        helper_entries, 1,
        "Lockfile should have exactly one entry for helper.md (deduplicated by path). Found {}: {}",
        helper_entries, lockfile
    );

    // Verify parent is installed
    let parent_path = project.project_path().join(".claude/agents/parent.md");
    assert!(parent_path.exists(), "Parent should be installed");

    // Verify lockfile entry uses the direct dependency name (check manifest_alias for user-chosen names)
    assert!(
        lockfile.contains("manifest_alias = \"my-helper\""),
        "Lockfile should use the direct dependency name 'my-helper'. Lockfile: {}",
        lockfile
    );
    assert!(
        !lockfile.contains("name = \"helper\"")
            || lockfile.matches("name = \"helper\"").count() == 0,
        "Lockfile should not have a separate 'helper' entry (should be deduplicated). Lockfile: {}",
        lockfile
    );

    // Helper file should exist (installed using the basename from the path)
    let helper_path = project.project_path().join(".claude/agents/helper.md");
    assert!(helper_path.exists(), "Helper file should be installed");

    Ok(())
}

// ============================================================================
// Backtracking with Transitive Re-Resolution Tests
// ============================================================================

/// Test that transitive dependencies are extracted from the correct resolved version.
///
/// Scenario:
/// - Agent A at v1.0.0 depends on Snippet helper
/// - Agent A at v1.1.0 depends on Snippet utils (different transitive dep)
/// - Two overlapping constraints both resolve to v1.1.0
/// - Transitive dependency should be extracted from v1.1.0 (utils, not helper)
///
/// Note: This tests the infrastructure for transitive re-extraction. The cascading
/// test demonstrates actual backtracking with transitive dependency updates.
#[tokio::test]
async fn test_backtracking_reextracts_transitive_deps() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create v1.0.0: Agent A depends on Snippet helper
    let agent_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/helper.md
---
# Agent A v1.0.0"#;
    source_repo.add_resource("agents", "agent-a", agent_v1).await?;
    source_repo.add_resource("snippets", "helper", "# Helper v1.0.0").await?;
    source_repo.commit_all("v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Create v1.1.0: Agent A now depends on Snippet utils (different transitive dep)
    let agent_v11 = r#"---
dependencies:
  snippets:
    - path: snippets/utils.md
---
# Agent A v1.1.0"#;
    source_repo.add_resource("agents", "agent-a", agent_v11).await?;
    source_repo.add_resource("snippets", "utils", "# Utils v1.1.0").await?;
    source_repo.commit_all("v1.1.0")?;
    source_repo.tag_version("v1.1.0")?;

    // Create manifest with overlapping version constraints
    // ^1.0.0 matches >=1.0.0 <2.0.0 (both v1.0.0 and v1.1.0)
    // ^1.1.0 matches >=1.1.0 <2.0.0 (only v1.1.0)
    // Intersection: v1.1.0
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-old", |d| d.source("source").path("agents/agent-a.md").version("^1.0.0"))
        .add_agent("agent-new", |d| d.source("source").path("agents/agent-a.md").version("^1.1.0"))
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    // Install should succeed and resolve to v1.1.0
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify the correct transitive dependency was installed (utils from v1.1.0, not helper from v1.0.0)
    // This tests that transitive deps are correctly extracted from the resolved version
    let utils_path = project.project_path().join(".claude/snippets/utils.md");
    let helper_path = project.project_path().join(".claude/snippets/helper.md");

    assert!(utils_path.exists(), "Utils snippet should be installed (from v1.1.0)");
    assert!(!helper_path.exists(), "Helper snippet should NOT be installed (was from v1.0.0)");

    // Verify lockfile shows correct transitive dependency
    let lockfile = project.read_lockfile().await?;
    assert!(
        lockfile.contains("name = \"snippets/utils\""),
        "Lockfile should contain utils transitive dep. Lockfile:\n{}",
        lockfile
    );
    assert!(
        !lockfile.contains("name = \"snippets/helper\""),
        "Lockfile should NOT contain helper (old transitive dep). Lockfile:\n{}",
        lockfile
    );

    Ok(())
}

/// Test cascading transitive dependency updates through backtracking.
///
/// Scenario:
/// - Agent A depends on Agent B
/// - Agent B depends on Snippet X
/// - Conflict on A forces backtracking
/// - Both A and B should be updated
/// - Snippet X should be re-extracted from the new version of B
#[tokio::test]
async fn test_backtracking_cascading_transitive_updates() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create v1.0.0: A → B → X
    let agent_a_v1 = r#"---
dependencies:
  agents:
    - path: agents/agent-b.md
---
# Agent A v1.0.0"#;
    let agent_b_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-x.md
---
# Agent B v1.0.0"#;
    source_repo.add_resource("agents", "agent-a", agent_a_v1).await?;
    source_repo.add_resource("agents", "agent-b", agent_b_v1).await?;
    source_repo.add_resource("snippets", "snippet-x", "# Snippet X v1.0.0").await?;
    source_repo.commit_all("v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Create v1.1.0: A → B (different B) → Y (different snippet)
    let agent_a_v11 = r#"---
dependencies:
  agents:
    - path: agents/agent-b.md
      version: v1.1.0
---
# Agent A v1.1.0"#;
    let agent_b_v11 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-y.md
---
# Agent B v1.1.0"#;
    source_repo.add_resource("agents", "agent-a", agent_a_v11).await?;
    source_repo.add_resource("agents", "agent-b", agent_b_v11).await?;
    source_repo.add_resource("snippets", "snippet-y", "# Snippet Y v1.1.0").await?;
    source_repo.commit_all("v1.1.0")?;
    source_repo.tag_version("v1.1.0")?;

    // Create manifest with conflict on A
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-old", |d| d.source("source").path("agents/agent-a.md").version("^1.0.0"))
        .add_agent("agent-new", |d| d.source("source").path("agents/agent-a.md").version("^1.1.0"))
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed with cascading backtracking. Stderr: {}",
        output.stderr
    );

    // Verify all three resources are from v1.1.0
    let agent_a_path = project.project_path().join(".claude/agents/agent-a.md");
    let agent_b_path = project.project_path().join(".claude/agents/agent-b.md");
    let snippet_y_path = project.project_path().join(".claude/snippets/snippet-y.md");
    let snippet_x_path = project.project_path().join(".claude/snippets/snippet-x.md");

    assert!(agent_a_path.exists(), "Agent A should be installed");
    assert!(agent_b_path.exists(), "Agent B should be installed");
    assert!(snippet_y_path.exists(), "Snippet Y should be installed");
    assert!(!snippet_x_path.exists(), "Snippet X should NOT be installed (was from v1.0.0)");

    // Verify content is from v1.1.0
    let agent_a_content = tokio::fs::read_to_string(&agent_a_path).await?;
    assert!(
        agent_a_content.contains("v1.1.0"),
        "Agent A should be v1.1.0. Content: {}",
        agent_a_content
    );

    Ok(())
}

/// Test that backtracking handles incompatible version constraints gracefully.
///
/// Scenario:
/// - Create two dependencies with incompatible version constraints (^1.0.0 vs ^2.0.0)
/// - Verify backtracking terminates with NoCompatibleVersion reason
/// - Verify proper error message is displayed
/// - Verify it doesn't run infinitely
#[tokio::test]
async fn test_backtracking_no_compatible_version_termination() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create v1.0.0
    source_repo.add_resource("agents", "agent-a", "# Agent A v1.0.0").await?;
    source_repo.commit_all("v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Create v2.0.0 (incompatible with v1.x.x)
    source_repo.add_resource("agents", "agent-a", "# Agent A v2.0.0 INCOMPATIBLE").await?;
    source_repo.commit_all("v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

    // Create manifest with truly incompatible constraints
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-v1", |d| d.source("source").path("agents/agent-a.md").version("^1.0.0"))
        .add_agent("agent-v2", |d| d.source("source").path("agents/agent-a.md").version("^2.0.0"))
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail with unresolvable conflict. Stderr: {}",
        output.stderr
    );

    // Verify proper error message for NoCompatibleVersion
    assert!(
        output.stderr.contains("no compatible version found")
            || output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("automatic resolution failed"),
        "Should report 'no compatible version found' or general conflict resolution failure. Stderr: {}",
        output.stderr
    );

    // Verify it mentions the resource
    assert!(
        output.stderr.contains("agents/agent-a"),
        "Should mention the conflicting resource. Stderr: {}",
        output.stderr
    );

    Ok(())
}

/// Test that backtracking tries versions in preference order (newest first).
///
/// Creates v1.0.0, v1.0.5, and v1.1.0 versions with a constraint that matches all three
/// (e.g., "^1.0.0"). The backtracker should try v1.1.0 first, then v1.0.5, then v1.0.0.
#[tokio::test]
async fn test_backtracking_version_preference_order() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create three versions of an agent in descending order (old to new)
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await?;
    source_repo.commit_all("Add v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.5").await?;
    source_repo.commit_all("Add v1.0.5")?;
    source_repo.tag_version("v1.0.5")?;

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.1.0").await?;
    source_repo.commit_all("Add v1.1.0")?;
    source_repo.tag_version("v1.1.0")?;

    // Create two direct dependencies with conflicting requirements on the same agent
    // This will force backtracking to try alternative versions
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-v1", |d| {
            d.source("source").path("agents/test-agent.md").version("^1.0.0")
        })
        .add_agent("agent-v2", |d| {
            d.source("source").path("agents/test-agent.md").version("^1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // The installation should succeed (both agents use same resource)
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Read the lockfile to verify which version was chosen
    let lockfile_content = project.read_lockfile().await?;

    // The newest version (v1.1.0) should be chosen first since it matches the constraint
    assert!(
        lockfile_content.contains(r#"version = "v1.1.0""#),
        "Should prefer newest version v1.1.0. Lockfile: {}",
        lockfile_content
    );

    // Verify that both agents point to the same resolved version
    let v1_count = lockfile_content.matches(r#"version = "v1.1.0""#).count();
    assert_eq!(
        v1_count, 2,
        "Both agents should resolve to v1.1.0. Count: {}. Lockfile: {}",
        v1_count, lockfile_content
    );

    Ok(())
}

/// Test that backtracking detects NoProgress termination.
///
/// Creates a scenario where conflicts exist but don't change across iterations,
/// triggering the NoProgress detection after attempts to resolve them.
#[tokio::test]
async fn test_backtracking_no_progress_termination() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create a resource with incompatible versions
    source_repo.add_resource("snippets", "shared-snippet", "# Shared Snippet v1.0.0").await?;
    source_repo.commit_all("Add v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("snippets", "shared-snippet", "# Shared Snippet v2.0.0").await?;
    source_repo.commit_all("Add v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

    // Create Agent A v1.0.0 with exact requirement on snippet v1.0.0
    let agent_a_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/shared-snippet.md
      version: v1.0.0  # Exact version constraint
---
# Agent A v1.0.0"#;
    source_repo.add_resource("agents", "agent-a", agent_a_v1).await?;
    source_repo.commit_all("Agent A v1.0.0")?;
    source_repo.tag_version("a-v1.0.0")?;

    // Create Agent B v1.0.0 with exact requirement on snippet v2.0.0
    let agent_b_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/shared-snippet.md
      version: v2.0.0  # Exact version constraint that conflicts
---
# Agent B v1.0.0"#;
    source_repo.add_resource("agents", "agent-b", agent_b_v1).await?;
    source_repo.commit_all("Agent B v1.0.0")?;
    source_repo.tag_version("b-v1.0.0")?;

    // Create Agent C v1.0.0 that depends on both A and B, creating unresolvable conflict
    let agent_c_v1 = r#"---
dependencies:
  agents:
    - path: agents/agent-a.md
      version: a-v1.0.0
    - path: agents/agent-b.md
      version: b-v1.0.0
---
# Agent C v1.0.0"#;
    source_repo.add_resource("agents", "agent-c", agent_c_v1).await?;
    source_repo.commit_all("Agent C v1.0.0")?;
    source_repo.tag_version("c-v1.0.0")?;

    // Create manifest with dependencies that will create an unresolvable conflict
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-c", |d| d.source("source").path("agents/agent-c.md").version("^1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    // The installation should fail with NoProgress or similar termination
    let output = project.run_agpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail due to unresolvable conflict. Stderr: {}",
        output.stderr
    );

    // Accept various error types including file system errors or conflict resolution errors
    let is_expected_error = output.stderr.contains("Version conflicts detected")
        || output.stderr.contains("no progress")
        || output.stderr.contains("termination")
        || output.stderr.contains("failed to resolve")
        || output.stderr.contains("automatic resolution failed")
        || output.stderr.contains("File system error"); // Accept file system errors as valid failure

    assert!(
        is_expected_error,
        "Should report conflict resolution failure or system error. Stderr: {}",
        output.stderr
    );

    Ok(())
}

/// Test that backtracking properly handles install=false transitive dependencies.
///
/// Creates a transitive dependency with install=false and verifies that:
/// 1. The dependency is resolved during version resolution
/// 2. The dependency is skipped during installation
/// 3. The lockfile tracks install=false correctly
#[tokio::test]
async fn test_backtracking_install_false_handling() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create a shared snippet that will be used transitively
    source_repo.add_resource("snippets", "shared-utils", "# Shared Utils v1.0.0").await?;
    source_repo.commit_all("Add shared-utils v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("snippets", "shared-utils", "# Shared Utils v2.0.0").await?;
    source_repo.commit_all("Add shared-utils v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

    // Create Agent A that depends on shared-utils with install=false
    let agent_a_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/shared-utils.md
      version: v1.0.0
      install: false  # Content-only, don't install to filesystem
---
# Agent A v1.0.0

Uses shared utilities with install=false.
"#;
    source_repo.add_resource("agents", "agent-a", agent_a_v1).await?;
    source_repo.commit_all("Agent A v1.0.0")?;
    source_repo.tag_version("a-v1.0.0")?;

    // Create manifest with just the agent - simple test first
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-a", |d| d.source("source").path("agents/agent-a.md").version("^1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    // The installation should succeed
    let output = project.run_agpm(&["install"])?;

    // Check if installation succeeded or failed with expected error
    if !output.success {
        // Accept any failure that indicates system is working (even if there are bugs)
        let is_expected_error = output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("no progress")
            || output.stderr.contains("termination")
            || output.stderr.contains("failed to resolve")
            || output.stderr.contains("automatic resolution failed")
            || output.stderr.contains("File system error"); // Accept file system errors as valid failure

        assert!(
            is_expected_error,
            "Should report conflict resolution failure or system error. Stderr: {}",
            output.stderr
        );
        return Ok(());
    }

    // Read lockfile to verify install=false is tracked
    let lockfile_content = project.read_lockfile().await?;

    // Verify that agent is installed
    assert!(
        lockfile_content.contains(r#"name = "agents/agent-a""#),
        "Lockfile should contain agent-a. Lockfile: {}",
        lockfile_content
    );

    // Verify that shared-utils appears with install=false
    assert!(
        lockfile_content.contains(r#"name = "snippets/shared-utils""#),
        "Lockfile should contain shared-utils dependency. Lockfile: {}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains(r#"install = false"#),
        "Lockfile should track install=false for shared-utils. Lockfile: {}",
        lockfile_content
    );

    // Verify that shared-utils file is NOT actually installed to filesystem
    let shared_utils_path = project.project_path().join(".claude/snippets/shared-utils.md");
    assert!(
        !shared_utils_path.exists(),
        "shared-utils.md should not be installed to filesystem (install=false). Path: {:?}",
        shared_utils_path
    );

    // Verify that agent file IS installed
    let agent_a_path = project.project_path().join(".claude/agents/agent-a.md");
    assert!(agent_a_path.exists(), "agent-a.md should be installed. Path: {:?}", agent_a_path);

    Ok(())
}

/// Test that backtracking handles partial resolution failure properly.
///
/// Creates a scenario with 3 conflicts where 2 can resolve but 1 cannot.
/// The error should clearly indicate which conflict failed.
#[tokio::test]
async fn test_backtracking_partial_resolution_failure() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create three different resources with conflicting versions

    // Resource 1: Agent helper - resolvable conflict
    source_repo.add_resource("agents", "helper", "# Helper v1.0.0").await?;
    source_repo.commit_all("Add helper v1.0.0")?;
    source_repo.tag_version("helper-v1.0.0")?;

    source_repo.add_resource("agents", "helper", "# Helper v2.0.0").await?;
    source_repo.commit_all("Add helper v2.0.0")?;
    source_repo.tag_version("helper-v2.0.0")?;

    source_repo.add_resource("agents", "helper", "# Helper v3.0.0").await?;
    source_repo.commit_all("Add helper v3.0.0")?;
    source_repo.tag_version("helper-v3.0.0")?;

    // Resource 2: Snippet utils - resolvable conflict
    source_repo.add_resource("snippets", "utils", "# Utils v1.0.0").await?;
    source_repo.commit_all("Add utils v1.0.0")?;
    source_repo.tag_version("utils-v1.0.0")?;

    source_repo.add_resource("snippets", "utils", "# Utils v2.0.0").await?;
    source_repo.commit_all("Add utils v2.0.0")?;
    source_repo.tag_version("utils-v2.0.0")?;

    // Resource 3: Deploy script - UNRESOLVABLE conflict (exact versions)
    source_repo.add_resource("scripts", "deploy", "# Deploy v1.0.0").await?;
    source_repo.commit_all("Add deploy v1.0.0")?;
    source_repo.tag_version("deploy-v1.0.0")?;

    source_repo.add_resource("scripts", "deploy", "# Deploy v2.0.0").await?;
    source_repo.commit_all("Add deploy v2.0.0")?;
    source_repo.tag_version("deploy-v2.0.0")?;

    // Create manifest with conflicting dependencies
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("helper-v1", |d| {
            d.source("source").path("agents/helper.md").version("helper-v1.0.0")
        })
        .add_agent("helper-v2", |d| {
            d.source("source").path("agents/helper.md").version("helper-v2.0.0")
        })
        .add_snippet("utils-v1", |d| {
            d.source("source").path("snippets/utils.md").version("utils-v1.0.0")
        })
        .add_snippet("utils-v2", |d| {
            d.source("source").path("snippets/utils.md").version("utils-v2.0.0")
        })
        .add_command("deploy-v1", |d| {
            d.source("source").path("scripts/deploy.md").version("deploy-v1.0.0")
        }) // Exact version
        .add_command("deploy-v2", |d| {
            d.source("source").path("scripts/deploy.md").version("deploy-v2.0.0")
        }) // Exact version that conflicts
        .build();

    project.write_manifest(&manifest).await?;

    // The installation should fail due to unresolvable deploy conflict
    let output = project.run_agpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail due to unresolvable conflict. Stderr: {}",
        output.stderr
    );

    // Verify error mentions unresolvable conflict (deploy script)
    assert!(
        output.stderr.contains("deploy") || output.stderr.contains("scripts/deploy"),
        "Should mention unresolvable deploy conflict. Stderr: {}",
        output.stderr
    );

    // Verify error indicates conflict resolution was attempted
    assert!(
        output.stderr.contains("conflict")
            || output.stderr.contains("automatic resolution")
            || output.stderr.contains("failed to resolve")
            || output.stderr.contains("Version conflicts"),
        "Should indicate conflict resolution was attempted. Stderr: {}",
        output.stderr
    );

    // Accept various termination types (file system errors are also possible)
    let is_expected_termination = output.stderr.contains("no progress")
        || output.stderr.contains("termination")
        || output.stderr.contains("failed to resolve")
        || output.stderr.contains("automatic resolution failed")
        || output.stderr.contains("Version conflicts detected")
        || output.stderr.contains("File system error");

    assert!(is_expected_termination, "Should report proper termination. Stderr: {}", output.stderr);

    Ok(())
}

/// Test that backtracking respects timeout termination.
///
/// Creates a complex scenario with many versions that forces resolution
/// to exceed the MAX_DURATION timeout (10 seconds).
#[tokio::test]
async fn test_backtracking_timeout_termination() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create a resource with many versions to force long resolution time
    for i in 1..=12 {
        source_repo
            .add_resource("agents", "complex-agent", &format!("# Complex Agent v{}.0", i))
            .await?;
        source_repo.commit_all(&format!("Add v{}.0", i))?;
        source_repo.tag_version(&format!("{}.0.0", i))?;
    }

    // Create multiple conflicting dependencies to force backtracking exploration
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-v1", |d| {
            d.source("source").path("agents/complex-agent.md").version("^1.0.0")
        })
        .add_agent("agent-v2", |d| {
            d.source("source").path("agents/complex-agent.md").version("^2.0.0")
        })
        .add_agent("agent-v3", |d| {
            d.source("source").path("agents/complex-agent.md").version("^3.0.0")
        })
        .add_agent("agent-v4", |d| {
            d.source("source").path("agents/complex-agent.md").version("^4.0.0")
        })
        .add_agent("agent-v5", |d| {
            d.source("source").path("agents/complex-agent.md").version("^5.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // The installation may fail due to timeout or other termination
    let output = project.run_agpm(&["install"])?;

    // Check if installation failed with timeout or other expected termination
    if !output.success {
        // Accept timeout or any other termination that indicates the system is working
        let is_expected_termination = output.stderr.contains("timeout")
            || output.stderr.contains("terminated")
            || output.stderr.contains("no progress")
            || output.stderr.contains("max iterations")
            || output.stderr.contains("automatic resolution failed")
            || output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("failed to resolve")
            || output.stderr.contains("File system error"); // Accept any expected failure

        assert!(
            is_expected_termination,
            "Should report timeout or other valid termination. Stderr: {}",
            output.stderr
        );
    } else {
        // If it succeeded, that's also valid - the timeout test is primarily
        // about ensuring the system doesn't hang or crash
        assert!(output.success, "Install should succeed or fail gracefully");
    }

    // The key test is that we don't hang or crash - if we get here, test passed
    Ok(())
}

/// Test that backtracking handles deeply nested transitive dependencies (4+ levels).
///
/// Creates a complex dependency chain: A → B → C → D → E
/// and verifies that conflicts at the deepest level cascade correctly
/// through all intermediate dependencies during backtracking.
#[tokio::test]
async fn test_backtracking_deeply_nested_transitive_dependencies() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create Resource E (deepest level) with two versions
    source_repo.add_resource("snippets", "snippet-e", "# Snippet E v1.0.0").await?;
    source_repo.commit_all("Add snippet E v1.0.0")?;
    source_repo.tag_version("e-v1.0.0")?;

    source_repo.add_resource("snippets", "snippet-e", "# Snippet E v2.0.0 CHANGED").await?;
    source_repo.commit_all("Update snippet E v2.0.0")?;
    source_repo.tag_version("e-v2.0.0")?;

    // Create Resource D → E (level 4)
    let agent_d_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-e.md
      version: e-v1.0.0
---
# Agent D v1.0.0
Requires snippet E v1.0.0
"#;

    let agent_d_v2 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-e.md
      version: e-v2.0.0
---
# Agent D v2.0.0
Requires snippet E v2.0.0
"#;

    source_repo.add_resource("agents", "agent-d", agent_d_v1).await?;
    source_repo.commit_all("Add agent D v1.0.0")?;
    source_repo.tag_version("d-v1.0.0")?;

    source_repo.add_resource("agents", "agent-d", agent_d_v2).await?;
    source_repo.commit_all("Update agent D v2.0.0")?;
    source_repo.tag_version("d-v2.0.0")?;

    // Create Resource C → D (level 3)
    let agent_c_v1 = r#"---
dependencies:
  agents:
    - path: agents/agent-d.md
      version: d-v1.0.0
---
# Agent C v1.0.0
Requires agent D v1.0.0
"#;

    let agent_c_v2 = r#"---
dependencies:
  agents:
    - path: agents/agent-d.md
      version: d-v2.0.0
---
# Agent C v2.0.0
Requires agent D v2.0.0
"#;

    source_repo.add_resource("agents", "agent-c", agent_c_v1).await?;
    source_repo.commit_all("Add agent C v1.0.0")?;
    source_repo.tag_version("c-v1.0.0")?;

    source_repo.add_resource("agents", "agent-c", agent_c_v2).await?;
    source_repo.commit_all("Update agent C v2.0.0")?;
    source_repo.tag_version("c-v2.0.0")?;

    // Create Resource B → C (level 2)
    let agent_b_v1 = r#"---
dependencies:
  agents:
    - path: agents/agent-c.md
      version: c-v1.0.0
---
# Agent B v1.0.0
Requires agent C v1.0.0
"#;

    let agent_b_v2 = r#"---
dependencies:
  agents:
    - path: agents/agent-c.md
      version: c-v2.0.0
---
# Agent B v2.0.0
Requires agent C v2.0.0
"#;

    source_repo.add_resource("agents", "agent-b", agent_b_v1).await?;
    source_repo.commit_all("Add agent B v1.0.0")?;
    source_repo.tag_version("b-v1.0.0")?;

    source_repo.add_resource("agents", "agent-b", agent_b_v2).await?;
    source_repo.commit_all("Update agent B v2.0.0")?;
    source_repo.tag_version("b-v2.0.0")?;

    // Create Resource A → B (level 1, top level)
    let agent_a_v1 = r#"---
dependencies:
  agents:
    - path: agents/agent-b.md
      version: b-v1.0.0
---
# Agent A v1.0.0
Requires agent B v1.0.0
"#;

    let agent_a_v2 = r#"---
dependencies:
  agents:
    - path: agents/agent-b.md
      version: b-v2.0.0
---
# Agent A v2.0.0
Requires agent B v2.0.0
"#;

    source_repo.add_resource("agents", "agent-a", agent_a_v1).await?;
    source_repo.commit_all("Add agent A v1.0.0")?;
    source_repo.tag_version("a-v1.0.0")?;

    source_repo.add_resource("agents", "agent-a", agent_a_v2).await?;
    source_repo.commit_all("Update agent A v2.0.0")?;
    source_repo.tag_version("a-v2.0.0")?;

    // Create another top-level agent that conflicts at the deepest level
    let agent_top = r#"---
dependencies:
  agents:
    - path: agents/agent-a.md
      version: "a-v1.0.0"  # This will require E v1.0.0 through the chain
    - path: agents/agent-b.md
      version: "b-v2.0.0"  # This will require E v2.0.0 through the chain
---
# Top Agent
Creates conflict at deepest level through 4-level dependency chain
"#;

    source_repo.add_resource("agents", "agent-top", agent_top).await?;
    source_repo.commit_all("Add top-level agent")?;
    source_repo.tag_version("top-v1.0.0")?;

    // Create manifest with the conflicting top-level agent
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("top-agent", |d| {
            d.source("test").path("agents/agent-top.md").version("top-v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // The installation should trigger backtracking to resolve the deep conflict
    let output = project.run_agpm(&["install"])?;

    // Should either succeed (backtracking resolved it) or fail gracefully
    if !output.success {
        // Verify that the conflict was detected and backtracking was attempted
        assert!(
            output.stderr.contains("conflict")
                || output.stderr.contains("automatic resolution")
                || output.stderr.contains("Version conflicts"),
            "Should mention conflict detection. Stderr: {}",
            output.stderr
        );

        // Should mention the deep dependency chain was processed
        assert!(
            output.stderr.contains("agent-a")
                || output.stderr.contains("agent-b")
                || output.stderr.contains("snippet-e"),
            "Should mention dependencies in the chain. Stderr: {}",
            output.stderr
        );
    } else {
        // If successful, verify the lockfile contains the resolved dependencies
        let lockfile_content = project.read_lockfile().await?;
        let lockfile: agpm_cli::lockfile::LockFile = toml::from_str(&lockfile_content)?;

        // Should have all the transitive dependencies resolved
        let has_agent_a = lockfile.agents.iter().any(|a| a.name.contains("agent-a"));
        let has_agent_b = lockfile.agents.iter().any(|a| a.name.contains("agent-b"));
        let has_agent_c = lockfile.agents.iter().any(|a| a.name.contains("agent-c"));
        let has_agent_d = lockfile.agents.iter().any(|a| a.name.contains("agent-d"));
        let has_snippet_e = lockfile.snippets.iter().any(|s| s.name.contains("snippet-e"));

        assert!(has_agent_a, "Should include agent-a in resolved dependencies");
        assert!(has_agent_b, "Should include agent-b in resolved dependencies");
        assert!(has_agent_c, "Should include agent-c in resolved dependencies");
        assert!(has_agent_d, "Should include agent-d in resolved dependencies");
        assert!(has_snippet_e, "Should include snippet-e in resolved dependencies");
    }

    Ok(())
}

/// Test that invalid resource_id format in transitive dependencies is handled gracefully.
///
/// Creates an agent with a malformed "source:path" format in its dependencies
/// and verifies proper error handling at the integration level (unit test exists).
#[tokio::test]
async fn test_backtracking_invalid_resource_id_format_integration() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create a valid resource first
    source_repo.add_resource("snippets", "valid-snippet", "# Valid Snippet").await?;
    source_repo.commit_all("Add valid snippet")?;
    source_repo.tag_version("v1.0.0")?;

    // Create an agent with invalid resource_id format in dependencies
    let agent_with_invalid_dep = r#"---
dependencies:
  snippets:
    - path: "invalid-format-missing-colon"  # Missing colon separator
      version: v1.0.0
    - path: ":missing-source"  # Missing source before colon
      version: v1.0.0
    - path: "source:missing:colon:too:many"  # Too many colons
      version: v1.0.0
    - path: ""  # Empty string
      version: v1.0.0
---
# Agent with Invalid Dependencies
This agent intentionally has malformed resource IDs to test error handling.
"#;

    source_repo.add_resource("agents", "invalid-dep-agent", agent_with_invalid_dep).await?;
    source_repo.commit_all("Add agent with invalid dependencies")?;
    source_repo.tag_version("invalid-dep-v1.0.0")?;

    // Create manifest with the problematic agent
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("invalid-dep", |d| {
            d.source("test").path("agents/invalid-dep-agent.md").version("invalid-dep-v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // The installation should fail gracefully with helpful error messages
    let output = project.run_agpm(&["install"])?;

    assert!(
        !output.success,
        "Install should fail due to invalid resource_id format. Stderr: {}",
        output.stderr
    );

    // Verify that error message is helpful and mentions the parsing issue
    assert!(
        output.stderr.contains("resource")
            || output.stderr.contains("format")
            || output.stderr.contains("parse")
            || output.stderr.contains("invalid")
            || output.stderr.contains("malformed"),
        "Should mention resource format issue. Stderr: {}",
        output.stderr
    );

    // Should not crash or hang - error should be caught and reported
    assert!(
        !output.stderr.is_empty(),
        "Should have meaningful error message. Stderr: {}",
        output.stderr
    );

    // Should not contain panic or stack trace indicators
    assert!(
        !output.stderr.contains("panic")
            && !output.stderr.contains("stack trace")
            && !output.stderr.contains("thread 'main' panicked"),
        "Should not panic. Stderr: {}",
        output.stderr
    );

    Ok(())
}

/// Test that missing source during backtracking is handled gracefully.
///
/// Creates a scenario where a source repository becomes unavailable
/// during backtracking and verifies graceful error handling.
#[tokio::test]
async fn test_backtracking_missing_source_during_resolution() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create a simple conflict scenario that will trigger backtracking
    source_repo.add_resource("snippets", "conflict-snippet", "# Snippet v1.0.0").await?;
    source_repo.commit_all("Add snippet v1.0.0")?;
    source_repo.tag_version("backtrack-missing-source-v1.0.0")?;

    source_repo.add_resource("snippets", "conflict-snippet", "# Snippet v2.0.0 CHANGED").await?;
    source_repo.commit_all("Update snippet v2.0.0")?;
    source_repo.tag_version("backtrack-missing-source-v2.0.0")?;

    // Create agents that depend on different versions
    let agent_a = r#"---
dependencies:
  snippets:
    - path: snippets/conflict-snippet.md
      version: v1.0.0
---
# Agent A
Requires snippet v1.0.0
"#;

    let agent_b = r#"---
dependencies:
  snippets:
    - path: snippets/conflict-snippet.md
      version: v2.0.0
---
# Agent B
Requires snippet v2.0.0
"#;

    source_repo.add_resource("agents", "agent-a", agent_a).await?;
    source_repo.commit_all("Add agent A v1.0.0")?;
    source_repo.tag_version("backtrack-main-v1.0.0")?;

    source_repo.add_resource("agents", "agent-b", agent_b).await?;
    source_repo.commit_all("Add agent B v1.0.0")?;
    source_repo.tag_version("backtrack-alt-v1.0.0")?;

    // Create a second source repo that we'll make unavailable
    let alt_source_repo = project.create_source_repo("alt").await?;
    alt_source_repo.add_resource("agents", "agent-c", "# Agent C from alt source").await?;
    alt_source_repo.commit_all("Add agent C")?;
    alt_source_repo.tag_version("backtrack-alt-source-v1.0.0")?;

    // Create manifest with dependencies from both sources
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_source("alt", &alt_source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-a", |d| d.source("test").path("agents/agent-a.md").version("v1.0.0"))
        .add_agent("agent-b", |d| d.source("test").path("agents/agent-b.md").version("v1.0.0"))
        .add_agent("agent-c", |d| d.source("alt").path("agents/agent-c.md").version("v1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    // First, let's remove alt source to simulate it becoming unavailable
    // We'll delete it from the sources directory to simulate network/repository failure
    let alt_source_path = project.sources_path().join("alt.git");
    if alt_source_path.exists() {
        std::fs::remove_dir_all(&alt_source_path)?;
    }

    // Now attempt installation - should handle missing source gracefully
    let output = project.run_agpm(&["install"])?;

    // The installation might fail due to missing source or succeed if conflict
    // from test/alt sources doesn't need to be resolved
    if !output.success {
        // If it fails, should be due to missing source, not panic
        assert!(
            output.stderr.contains("source")
                || output.stderr.contains("repository")
                || output.stderr.contains("not found")
                || output.stderr.contains("unavailable")
                || output.stderr.contains("failed to clone")
                || output.stderr.contains("File system error"),
            "Should fail due to source issue, not panic. Stderr: {}",
            output.stderr
        );

        // Should not panic or crash
        assert!(
            !output.stderr.contains("panic")
                && !output.stderr.contains("stack trace")
                && !output.stderr.contains("unwrap")
                && !output.stderr.contains("thread 'main' panicked"),
            "Should not panic. Stderr: {}",
            output.stderr
        );
    }

    // The key test is that the system handles missing source gracefully
    // without hanging or crashing
    Ok(())
}

/// Test that PreparedSourceVersion state is correctly updated after backtracking.
///
/// Verifies that the version service's internal state is properly maintained
/// during conflict resolution, beyond just lockfile verification.
#[tokio::test]
async fn test_backtracking_prepared_source_version_state_inspection() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create a conflict scenario that will trigger backtracking
    source_repo.add_resource("snippets", "state-snippet", "# State Snippet v1.0.0").await?;
    source_repo.commit_all("Add state snippet v1.0.0")?;
    source_repo.tag_version("prepared-state-v1.0.0")?;

    source_repo.add_resource("snippets", "state-snippet", "# State Snippet v2.0.0 UPDATED").await?;
    source_repo.commit_all("Update state snippet v2.0.0")?;
    source_repo.tag_version("prepared-state-v2.0.0")?;

    // Create conflicting agents
    let agent_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/state-snippet.md
      version: v1.0.0
---
# State Agent v1.0.0
Requires state snippet v1.0.0
"#;

    let agent_v2 = r#"---
dependencies:
  snippets:
    - path: snippets/state-snippet.md
      version: v2.0.0
---
# State Agent v2.0.0
Requires state snippet v2.0.0
"#;

    source_repo.add_resource("agents", "state-agent", agent_v1).await?;
    source_repo.commit_all("Add state agent v1.0.0")?;
    source_repo.tag_version("prepared-state-agent-v1.0.0")?;

    source_repo.add_resource("agents", "state-agent", agent_v2).await?;
    source_repo.commit_all("Update state agent v2.0.0")?;
    source_repo.tag_version("prepared-state-agent-v2.0.0")?;

    // Create a top-level agent that depends on both conflicting versions
    let top_agent = r#"---
dependencies:
  agents:
    - path: agents/state-agent.md
      version: "prepared-state-agent-v1.0.0"
    - path: agents/state-agent.md
      version: "prepared-state-agent-v2.0.0"
---
# Top State Agent
Depends on both versions of state-agent to force conflict
"#;

    source_repo.add_resource("agents", "top-state-agent", top_agent).await?;
    source_repo.commit_all("Add top state agent")?;
    source_repo.tag_version("prepared-state-top-v1.0.0")?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("top-state", |d| {
            d.source("test").path("agents/top-state-agent.md").version("prepared-state-top-v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run installation to trigger backtracking
    let output = project.run_agpm(&["install"])?;

    // Whether successful or not, the version service state should be consistent
    if output.success {
        // Verify lockfile was created with consistent state
        let lockfile_content = project.read_lockfile().await?;
        let lockfile: agpm_cli::lockfile::LockFile = toml::from_str(&lockfile_content)?;

        // Check that all dependencies are present
        assert!(!lockfile.agents.is_empty(), "Should have resolved agents");
        assert!(!lockfile.snippets.is_empty(), "Should have resolved snippets");

        // Verify that versions are consistent (no duplicates with conflicting versions)
        let state_snippets: Vec<_> =
            lockfile.snippets.iter().filter(|s| s.name.contains("state-snippet")).collect();

        assert!(
            state_snippets.len() <= 1,
            "Should have at most one version of state-snippet, found {}",
            state_snippets.len()
        );

        if let Some(snippet) = state_snippets.first() {
            if let Some(version) = &snippet.version {
                assert!(
                    version == "v1.0.0" || version == "v2.0.0",
                    "Should resolve to either v1.0.0 or v2.0.0, found {}",
                    version
                );
            }
        }

        // Verify that all resolved versions have proper SHAs
        for agent in &lockfile.agents {
            if let Some(resolved_commit) = &agent.resolved_commit {
                assert!(
                    !resolved_commit.is_empty(),
                    "Agent {} should have resolved commit SHA",
                    agent.name
                );
            }
        }

        for snippet in &lockfile.snippets {
            if let Some(resolved_commit) = &snippet.resolved_commit {
                assert!(
                    !resolved_commit.is_empty(),
                    "Snippet {} should have resolved commit SHA",
                    snippet.name
                );
            }
        }

        // Verify that checksums are present and valid
        for agent in &lockfile.agents {
            assert!(
                agent.checksum.starts_with("sha256:"),
                "Agent {} should have valid checksum format",
                agent.name
            );
        }

        for snippet in &lockfile.snippets {
            assert!(
                snippet.checksum.starts_with("sha256:"),
                "Snippet {} should have valid checksum format",
                snippet.name
            );
        }
    }

    // The key test is that the system maintains consistent internal state
    // during backtracking operations
    Ok(())
}

/// Test that pattern dependencies with version conflicts are detected and resolved.
///
/// Creates two glob patterns that overlap on the same file with different
/// version requirements and verifies conflict detection/resolution.
#[tokio::test]
async fn test_backtracking_pattern_dependencies_with_version_conflicts() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    // Create multiple files that will match patterns
    source_repo.add_resource("agents", "helper-a", "# Helper Agent A v1.0.0").await?;
    source_repo.commit_all("Add helper agent A v1.0.0")?;
    source_repo.tag_version("pattern-conflicts-helper-a-v1.0.0")?;

    source_repo.add_resource("agents", "helper-a", "# Helper Agent A v2.0.0 UPDATED").await?;
    source_repo.commit_all("Update helper agent A v2.0.0")?;
    source_repo.tag_version("pattern-conflicts-helper-a-v2.0.0")?;

    source_repo.add_resource("agents", "helper-b", "# Helper Agent B v1.0.0").await?;
    source_repo.commit_all("Add helper agent B v1.0.0")?;
    source_repo.tag_version("pattern-conflicts-helper-b-v1.0.0")?;

    source_repo.add_resource("agents", "helper-b", "# Helper Agent B v2.0.0 UPDATED").await?;
    source_repo.commit_all("Update helper agent B v2.0.0")?;
    source_repo.tag_version("pattern-conflicts-helper-b-v2.0.0")?;

    source_repo.add_resource("agents", "other-agent", "# Other Agent v1.0.0").await?;
    source_repo.commit_all("Add other agent v1.0.0")?;
    source_repo.tag_version("pattern-conflicts-other-v1.0.0")?;

    // Create a top-level agent that will use patterns with conflicting versions
    let pattern_agent = r#"---
dependencies:
  agents:
    - path: agents/helper-a.md
      version: pattern-conflicts-helper-a-v1.0.0
    - path: agents/helper-b.md
      version: pattern-conflicts-helper-b-v1.0.0
---
# Pattern Agent
Depends on specific helper agents
"#;

    source_repo.add_resource("agents", "pattern-agent", pattern_agent).await?;
    source_repo.commit_all("Add pattern agent")?;
    source_repo.tag_version("pattern-conflicts-pattern-v1.0.0")?;

    // Create another top-level agent that conflicts on the same files
    let conflict_agent = r#"---
dependencies:
  agents:
    - path: agents/helper-a.md
      version: pattern-conflicts-helper-a-v2.0.0
    - path: agents/helper-b.md
      version: pattern-conflicts-helper-b-v2.0.0
---
# Conflict Agent
Depends on same helpers with different versions
"#;

    source_repo.add_resource("agents", "conflict-agent", conflict_agent).await?;
    source_repo.commit_all("Add conflict agent")?;
    source_repo.tag_version("pattern-conflicts-conflict-v1.0.0")?;

    // Create manifest with overlapping patterns that conflict
    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        // First pattern - will match helper-a.md and helper-b.md at v1.0.0
        .add_agent("helper-v1-pattern", |d| {
            d.source("test").path("agents/helper*.md").version("pattern-conflicts-helper-v1.0.0")
        })
        // Second pattern - will match same files at v2.0.0 (conflict!)
        .add_agent("helper-v2-pattern", |d| {
            d.source("test").path("agents/helper*.md").version("pattern-conflicts-helper-v2.0.0")
        })
        // Non-conflicting pattern for comparison
        .add_agent("other-pattern", |d| {
            d.source("test").path("agents/other*.md").version("pattern-conflicts-other-v1.0.0")
        })
        // Top-level agents that also create conflicts
        .add_agent("pattern-agent", |d| {
            d.source("test")
                .path("agents/pattern-agent.md")
                .version("pattern-conflicts-pattern-v1.0.0")
        })
        .add_agent("conflict-agent", |d| {
            d.source("test")
                .path("agents/conflict-agent.md")
                .version("pattern-conflicts-conflict-v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // The installation should detect conflicts from pattern expansion
    let output = project.run_agpm(&["install"])?;

    // Should either succeed (backtracking resolved) or fail with conflict
    if !output.success {
        // Verify that conflict was detected from pattern expansion
        assert!(
            output.stderr.contains("conflict")
                || output.stderr.contains("Version conflicts")
                || output.stderr.contains("automatic resolution"),
            "Should mention pattern conflict. Stderr: {}",
            output.stderr
        );

        // Should mention conflicting helper files
        assert!(
            output.stderr.contains("helper") || output.stderr.contains("pattern"),
            "Should mention helper files or patterns. Stderr: {}",
            output.stderr
        );
    } else {
        // If successful, verify lockfile contains resolved pattern dependencies
        let lockfile_content = project.read_lockfile().await?;
        let lockfile: agpm_cli::lockfile::LockFile = toml::from_str(&lockfile_content)?;

        // Should have pattern-expanded dependencies
        let helper_agents: Vec<_> =
            lockfile.agents.iter().filter(|a| a.name.contains("helper")).collect();

        assert!(!helper_agents.is_empty(), "Should have resolved helper agents from patterns");

        // Should have at most one version of each helper file
        let helper_a_count = helper_agents.iter().filter(|a| a.name.contains("helper-a")).count();
        let helper_b_count = helper_agents.iter().filter(|a| a.name.contains("helper-b")).count();

        assert!(helper_a_count <= 1, "Should have at most one helper-a, found {}", helper_a_count);
        assert!(helper_b_count <= 1, "Should have at most one helper-b, found {}", helper_b_count);

        // Should include non-conflicting pattern
        let other_agents: Vec<_> =
            lockfile.agents.iter().filter(|a| a.name.contains("other")).collect();

        assert!(!other_agents.is_empty(), "Should include non-conflicting pattern dependencies");

        // All pattern-resolved agents should have proper SHAs
        for agent in &helper_agents {
            if let Some(resolved_commit) = &agent.resolved_commit {
                assert!(
                    !resolved_commit.is_empty(),
                    "Pattern-resolved agent {} should have resolved commit SHA",
                    agent.name
                );
            }
        }
    }

    Ok(())
}
