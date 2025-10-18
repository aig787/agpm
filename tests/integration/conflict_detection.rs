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
        output.stderr.contains("Version conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("api-designer.md"),
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
        output.stderr.contains("Version conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("test-agent.md"),
        "Should mention conflicting resource. Stderr: {}",
        output.stderr
    );
    Ok(())
}

/// Test that HEAD (unspecified version) mixed with a pinned version is detected as a conflict.
///
/// This verifies the conflict detector identifies when the same resource is requested
/// both with and without a version specification (HEAD means "use whatever is current").
#[tokio::test]
async fn test_head_vs_pinned_version_conflict_blocks_install() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    // Create v1.0.0
    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    // Create manifest with same resource, one unspecified (HEAD), one pinned
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
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
        output.stderr.contains("Version conflicts detected"),
        "Should contain conflict message. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("test-agent.md"),
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

    // Verify lockfile has only ONE entry for "commit"
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_content = tokio::fs::read_to_string(&lockfile_path).await.unwrap();
    let commit_count = lockfile_content.matches("name = \"commit\"").count();
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
    let helper_count = updated_lockfile.matches("name = \"helper\"").count();
    let worker_count = updated_lockfile.matches("name = \"worker\"").count();
    assert_eq!(
        helper_count, 1,
        "Lockfile should have exactly one 'helper' entry, found {}: {}",
        helper_count, updated_lockfile
    );
    assert_eq!(
        worker_count, 1,
        "Lockfile should have exactly one 'worker' entry, found {}: {}",
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
    assert!(lockfile_content.contains("name = \"agent-a\""), "Lockfile should contain agent-a");
    assert!(
        lockfile_content.contains("name = \"utils\""),
        "Lockfile should contain transitive dep utils. Lockfile:\n{}",
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
    let agent_count = updated_lockfile.matches("name = \"agent-a\"").count();
    let utils_count = updated_lockfile.matches("name = \"utils\"").count();
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

    assert!(lockfile_content.contains("name = \"helper\""), "Should have helper");
    assert!(lockfile_content.contains("name = \"worker\""), "Should have worker");
    assert!(
        lockfile_content.contains("name = \"utils\""),
        "Should have transitive utils. Lockfile:\n{}",
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
    let helper_count = updated_lockfile.matches("name = \"helper\"").count();
    let worker_count = updated_lockfile.matches("name = \"worker\"").count();
    let utils_count = updated_lockfile.matches("name = \"utils\"").count();

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
    assert!(lockfile_content.contains("name = \"agent-a\""), "Lockfile should contain agent-a");
    assert!(lockfile_content.contains("name = \"agent-b\""), "Lockfile should contain agent-b");

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
        updated_lockfile.contains("name = \"agent-a\""),
        "Lockfile should still contain agent-a"
    );
    assert!(
        !updated_lockfile.contains("name = \"agent-b\""),
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
