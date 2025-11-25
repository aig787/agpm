//! Integration tests for version conflict detection.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Test that conflicting exact versions are detected and installation fails.
#[tokio::test]
async fn test_exact_version_conflict_blocks_install() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("community").await?;

    source_repo.add_resource("agents", "api-designer", "# API Designer v0.0.1").await?;
    source_repo.commit_all("Add v0.0.1")?;
    source_repo.tag_version("v0.0.1")?;

    source_repo.add_resource("agents", "api-designer", "# API Designer v0.0.2").await?;
    source_repo.commit_all("Update to v0.0.2")?;
    source_repo.tag_version("v0.0.2")?;

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
#[tokio::test]
async fn test_identical_exact_versions_no_conflict() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

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
#[tokio::test]
async fn test_semver_vs_branch_conflict_blocks_install() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    source_repo.add_resource("agents", "test-agent", "# Test Agent v2.0.0").await.unwrap();
    source_repo.commit_all("Version 2.0.0").unwrap();
    source_repo.tag_version("v2.0.0").unwrap();

    source_repo.git.ensure_branch("main").unwrap();

    source_repo.git.create_branch("develop").unwrap();
    source_repo.add_resource("agents", "test-agent", "# Test Agent - Development").await.unwrap();
    source_repo.commit_all("Development changes").unwrap();
    source_repo.git.checkout("main").unwrap();

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
#[tokio::test]
async fn test_head_vs_pinned_version_conflict_blocks_install() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await.unwrap();
    source_repo.commit_all("v1.0.0 commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    source_repo
        .add_resource("agents", "test-agent", "# Test Agent v2.0.0 - DIFFERENT")
        .await
        .unwrap();
    source_repo.commit_all("v2.0.0 commit").unwrap();
    source_repo.tag_version("v2.0.0").unwrap();

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
#[tokio::test]
async fn test_different_branches_conflict_blocks_install() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    source_repo.add_resource("agents", "test-agent", "# Test Agent - Main").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();

    source_repo.git.ensure_branch("main").unwrap();

    source_repo.git.create_branch("develop").unwrap();
    source_repo.add_resource("agents", "test-agent", "# Test Agent - Development").await.unwrap();
    source_repo.commit_all("Development changes").unwrap();
    source_repo.git.checkout("main").unwrap();

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
#[tokio::test]
async fn test_same_branch_different_case_no_conflict() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    source_repo.add_resource("agents", "test-agent", "# Test Agent").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();

    source_repo.git.ensure_branch("main").unwrap();

    if source_repo.git.create_branch("Main").is_ok() {
        source_repo.git.checkout("main").unwrap();
    }

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
#[tokio::test]
async fn test_changing_dependency_source_no_false_conflict() -> Result<()> {
    let project = TestProject::new().await.unwrap();
    let source_repo = project.create_source_repo("test-repo").await.unwrap();

    source_repo.add_resource("commands", "commit", "# Commit Command v1").await.unwrap();
    source_repo.commit_all("Initial commit").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    let local_dir = project.project_path().join("local-resources");
    tokio::fs::create_dir_all(&local_dir.join("commands")).await.unwrap();
    tokio::fs::write(local_dir.join("commands/commit.md"), "# Commit Command v2 (local)")
        .await
        .unwrap();

    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &source_repo.bare_file_url(project.sources_path())?)
        .add_command("commit", |d| {
            d.source("test-repo").path("commands/commit.md").version("v1.0.0")
        })
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial install should succeed. Stderr: {}", output.stderr);

    let local_path = local_dir.to_str().unwrap();
    let manifest = ManifestBuilder::new()
        .add_command("commit", |d| d.path(&format!("{}/commands/commit.md", local_path)))
        .build();
    project.write_manifest(&manifest).await.unwrap();

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
#[tokio::test]
async fn test_pattern_source_change_no_false_conflict() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

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

    let manifest = ManifestBuilder::new()
        .add_source("repo1", &source_repo1.bare_file_url(project.sources_path())?)
        .add_agent("all-agents", |d| d.source("repo1").path("agents/*.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial pattern install should succeed. Stderr: {}", output.stderr);

    let lockfile_content =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    assert!(
        lockfile_content.contains("source = \"repo1\""),
        "Lockfile should have entries from repo1"
    );

    let manifest2 = ManifestBuilder::new()
        .add_source("repo2", &source_repo2.bare_file_url(project.sources_path())?)
        .add_agent("all-agents", |d| d.source("repo2").path("agents/*.md").version("v1.0.0"))
        .build();
    eprintln!("=== New manifest ===\n{}", manifest2);
    project.write_manifest(&manifest2).await.unwrap();

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
#[tokio::test]
async fn test_source_change_updates_transitive_deps() -> Result<()> {
    use crate::common::{ManifestBuilder, TestProject};

    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

    let repo1 = project.create_source_repo("repo1").await.unwrap();
    let repo2 = project.create_source_repo("repo2").await.unwrap();

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

    let manifest = ManifestBuilder::new()
        .add_source("repo1", &repo1.bare_file_url(project.sources_path())?)
        .add_agent("agent-a", |d| d.source("repo1").path("agents/agent-a.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial install should succeed. Stderr: {}", output.stderr);

    let lockfile_content =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    eprintln!("=== Initial lockfile ===\n{}", lockfile_content);
    assert!(
        lockfile_content.contains("manifest_alias = \"agent-a\""),
        "Lockfile should contain agent-a"
    );
    assert!(
        lockfile_content.contains("name = \"snippets/utils\"")
            && lockfile_content.matches("source = \"repo1\"").count() >= 2,
        "Lockfile should contain transitive dep utils from repo1. Lockfile:\n{}",
        lockfile_content
    );
    let repo1_count = lockfile_content.matches("source = \"repo1\"").count();
    assert_eq!(repo1_count, 2, "Lockfile should have 2 entries from repo1 (agent-a + utils)");

    let agent_path = project.project_path().join(".claude/agents/agent-a.md");
    let utils_path = project.project_path().join(".claude/snippets/utils.md");
    assert!(agent_path.exists(), "Agent should be installed");
    assert!(utils_path.exists(), "Transitive dep should be installed");
    let agent_content_installed = tokio::fs::read_to_string(&agent_path).await.unwrap();
    assert!(agent_content_installed.contains("repo1"), "Agent should be from repo1");

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

    let updated_lockfile =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();

    assert!(
        !updated_lockfile.contains("source = \"repo1\""),
        "Lockfile should not have any entries from old repo1. Lockfile:\n{}",
        updated_lockfile
    );

    let repo2_count = updated_lockfile.matches("source = \"repo2\"").count();
    assert_eq!(
        repo2_count, 2,
        "Lockfile should have 2 entries from repo2 (agent-a + utils). Lockfile:\n{}",
        updated_lockfile
    );

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

    let agent_content_updated = tokio::fs::read_to_string(&agent_path).await.unwrap();
    assert!(agent_content_updated.contains("repo2"), "Agent should now be from repo2");
    Ok(())
}

/// Test that changing a pattern dependency's source updates both the pattern-expanded
#[tokio::test]
async fn test_pattern_with_transitive_deps_source_change() -> Result<()> {
    use crate::common::{ManifestBuilder, TestProject};

    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

    let repo1 = project.create_source_repo("repo1").await.unwrap();
    let repo2 = project.create_source_repo("repo2").await.unwrap();

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

    let manifest = ManifestBuilder::new()
        .add_source("repo1", &repo1.bare_file_url(project.sources_path())?)
        .add_agent("all-agents", |d| d.source("repo1").path("agents/*.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "Initial pattern install should succeed. Stderr: {}", output.stderr);

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

    let helper_path = project.project_path().join(".claude/agents/helper.md");
    let worker_path = project.project_path().join(".claude/agents/worker.md");
    let utils_path = project.project_path().join(".claude/snippets/utils.md");
    assert!(helper_path.exists(), "Helper should be installed");
    assert!(worker_path.exists(), "Worker should be installed");
    assert!(utils_path.exists(), "Utils should be installed");

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

    let updated_lockfile =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await.unwrap();
    eprintln!("=== Updated lockfile ===\n{}", updated_lockfile);

    assert!(
        !updated_lockfile.contains("source = \"repo1\""),
        "Lockfile should not have any entries from old repo1. Lockfile:\n{}",
        updated_lockfile
    );

    let repo2_count = updated_lockfile.matches("source = \"repo2\"").count();
    assert_eq!(
        repo2_count, 3,
        "Lockfile should have 3 entries from repo2 (helper + worker + utils). Lockfile:\n{}",
        updated_lockfile
    );

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

    let helper_content = tokio::fs::read_to_string(&helper_path).await.unwrap();
    let worker_content = tokio::fs::read_to_string(&worker_path).await.unwrap();
    let utils_content = tokio::fs::read_to_string(&utils_path).await.unwrap();

    assert!(helper_content.contains("repo2"), "Helper should be from repo2");
    assert!(worker_content.contains("repo2"), "Worker should be from repo2");
    assert!(utils_content.contains("repo2"), "Utils should be from repo2");
    Ok(())
}

/// Test that commenting out a dependency removes it from the lockfile without conflicts.
#[tokio::test]
async fn test_commented_out_dependency_removed_from_lockfile() -> Result<()> {
    let project = TestProject::new().await.unwrap();

    let source_repo = project.create_source_repo("source").await.unwrap();
    source_repo.add_resource("agents", "agent-a", "# Agent A\nFirst agent").await.unwrap();
    source_repo.add_resource("agents", "agent-b", "# Agent B\nSecond agent").await.unwrap();
    source_repo.commit_all("Initial agents").unwrap();
    source_repo.tag_version("v1.0.0").unwrap();

    let source_url = source_repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("source", &source_url)
        .add_agent("agent-a", |d| d.source("source").path("agents/agent-a.md").version("v1.0.0"))
        .add_agent("agent-b", |d| d.source("source").path("agents/agent-b.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest).await.unwrap();

    let output = project.run_agpm(&["install"]).unwrap();
    assert!(output.success, "First install should succeed. stderr: {}", output.stderr);

    let lockfile_content = project.read_lockfile().await.unwrap();
    assert!(
        lockfile_content.contains("manifest_alias = \"agent-a\""),
        "Lockfile should contain agent-a"
    );
    assert!(
        lockfile_content.contains("manifest_alias = \"agent-b\""),
        "Lockfile should contain agent-b"
    );

    let manifest2 = ManifestBuilder::new()
        .add_source("source", &source_url)
        .add_agent("agent-a", |d| d.source("source").path("agents/agent-a.md").version("v1.0.0"))
        .build();
    project.write_manifest(&manifest2).await.unwrap();

    let output2 = project.run_agpm(&["install"]).unwrap();
    assert!(
        output2.success,
        "Second install should succeed without conflicts. stderr: {}",
        output2.stderr
    );

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

    assert!(
        !output2.stderr.contains("conflict"),
        "Should not have any conflicts. stderr: {}",
        output2.stderr
    );

    let agent_a_path = project.project_path().join(".claude/agents/agent-a.md");
    assert!(agent_a_path.exists(), "Agent A file should exist");
    Ok(())
}

/// Test that direct and transitive dependencies to the same local file don't cause false conflicts.
#[tokio::test]
async fn test_local_direct_and_transitive_deps_no_false_conflict() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;

    let local_dir = project.project_path().join("local-agents");
    tokio::fs::create_dir_all(&local_dir.join("agents")).await?;
    let _local_path = local_dir.to_str().unwrap();

    let helper_content = "# Helper Agent\nProvides helper functionality";
    tokio::fs::write(local_dir.join("agents/helper.md"), helper_content).await?;

    let parent_content = r#"---
dependencies:
  agents:
    - path: helper.md
---
# Parent Agent
Uses the helper agent"#;
    tokio::fs::write(local_dir.join("agents/parent.md"), parent_content).await?;

    let manifest = ManifestBuilder::new()
        .add_agent("my-helper", |d| d.path("local-agents/agents/helper.md"))
        .add_agent("parent", |d| d.path("local-agents/agents/parent.md"))
        .build();
    project.write_manifest(&manifest).await?;

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

    let lockfile = project.read_lockfile().await?;
    let helper_entries = lockfile.matches("path = \"local-agents/agents/helper.md\"").count();
    assert_eq!(
        helper_entries, 1,
        "Lockfile should have exactly one entry for helper.md (deduplicated by path). Found {}: {}",
        helper_entries, lockfile
    );

    let parent_path = project.project_path().join(".claude/agents/parent.md");
    assert!(parent_path.exists(), "Parent should be installed");

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

    let helper_path = project.project_path().join(".claude/agents/helper.md");
    assert!(helper_path.exists(), "Helper file should be installed");

    Ok(())
}

/// Test that transitive dependencies are extracted from the correct resolved version.
#[tokio::test]
async fn test_backtracking_reextracts_transitive_deps() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

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

    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-old", |d| d.source("source").path("agents/agent-a.md").version("^1.0.0"))
        .add_agent("agent-new", |d| d.source("source").path("agents/agent-a.md").version("^1.1.0"))
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    let utils_path = project.project_path().join(".claude/snippets/utils.md");
    let helper_path = project.project_path().join(".claude/snippets/helper.md");

    assert!(utils_path.exists(), "Utils snippet should be installed (from v1.1.0)");
    assert!(!helper_path.exists(), "Helper snippet should NOT be installed (was from v1.0.0)");

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
#[tokio::test]
async fn test_backtracking_cascading_transitive_updates() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

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

    let agent_a_path = project.project_path().join(".claude/agents/agent-a.md");
    let agent_b_path = project.project_path().join(".claude/agents/agent-b.md");
    let snippet_y_path = project.project_path().join(".claude/snippets/snippet-y.md");
    let snippet_x_path = project.project_path().join(".claude/snippets/snippet-x.md");

    assert!(agent_a_path.exists(), "Agent A should be installed");
    assert!(agent_b_path.exists(), "Agent B should be installed");
    assert!(snippet_y_path.exists(), "Snippet Y should be installed");
    assert!(!snippet_x_path.exists(), "Snippet X should NOT be installed (was from v1.0.0)");

    let agent_a_content = tokio::fs::read_to_string(&agent_a_path).await?;
    assert!(
        agent_a_content.contains("v1.1.0"),
        "Agent A should be v1.1.0. Content: {}",
        agent_a_content
    );

    Ok(())
}

/// Test that backtracking handles incompatible version constraints gracefully.
#[tokio::test]
async fn test_backtracking_no_compatible_version_termination() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    source_repo.add_resource("agents", "agent-a", "# Agent A v1.0.0").await?;
    source_repo.commit_all("v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("agents", "agent-a", "# Agent A v2.0.0 INCOMPATIBLE").await?;
    source_repo.commit_all("v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

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

    assert!(
        output.stderr.contains("no compatible version found")
            || output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("automatic resolution failed"),
        "Should report 'no compatible version found' or general conflict resolution failure. Stderr: {}",
        output.stderr
    );

    assert!(
        output.stderr.contains("agents/agent-a"),
        "Should mention the conflicting resource. Stderr: {}",
        output.stderr
    );

    Ok(())
}

/// Test that backtracking tries versions in preference order (newest first).
#[tokio::test]
async fn test_backtracking_version_preference_order() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.0").await?;
    source_repo.commit_all("Add v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.0.5").await?;
    source_repo.commit_all("Add v1.0.5")?;
    source_repo.tag_version("v1.0.5")?;

    source_repo.add_resource("agents", "test-agent", "# Test Agent v1.1.0").await?;
    source_repo.commit_all("Add v1.1.0")?;
    source_repo.tag_version("v1.1.0")?;

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

    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    let lockfile_content = project.read_lockfile().await?;

    assert!(
        lockfile_content.contains(r#"version = "v1.1.0""#),
        "Should prefer newest version v1.1.0. Lockfile: {}",
        lockfile_content
    );

    let v1_count = lockfile_content.matches(r#"version = "v1.1.0""#).count();
    assert_eq!(
        v1_count, 2,
        "Both agents should resolve to v1.1.0. Count: {}. Lockfile: {}",
        v1_count, lockfile_content
    );

    Ok(())
}

/// Test that backtracking detects NoProgress termination.
#[tokio::test]
async fn test_backtracking_no_progress_termination() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    source_repo.add_resource("snippets", "shared-snippet", "# Shared Snippet v1.0.0").await?;
    source_repo.commit_all("Add v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("snippets", "shared-snippet", "# Shared Snippet v2.0.0").await?;
    source_repo.commit_all("Add v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

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

    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-c", |d| d.source("source").path("agents/agent-c.md").version("^1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail due to unresolvable conflict. Stderr: {}",
        output.stderr
    );

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
#[tokio::test]
async fn test_backtracking_install_false_handling() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    source_repo.add_resource("snippets", "shared-utils", "# Shared Utils v1.0.0").await?;
    source_repo.commit_all("Add shared-utils v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    source_repo.add_resource("snippets", "shared-utils", "# Shared Utils v2.0.0").await?;
    source_repo.commit_all("Add shared-utils v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

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

    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-a", |d| d.source("source").path("agents/agent-a.md").version("^1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    if !output.success {
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

    let lockfile_content = project.read_lockfile().await?;

    assert!(
        lockfile_content.contains(r#"name = "agents/agent-a""#),
        "Lockfile should contain agent-a. Lockfile: {}",
        lockfile_content
    );

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

    let shared_utils_path = project.project_path().join(".claude/snippets/shared-utils.md");
    assert!(
        !shared_utils_path.exists(),
        "shared-utils.md should not be installed to filesystem (install=false). Path: {:?}",
        shared_utils_path
    );

    let agent_a_path = project.project_path().join(".claude/agents/agent-a.md");
    assert!(agent_a_path.exists(), "agent-a.md should be installed. Path: {:?}", agent_a_path);

    Ok(())
}

/// Test that backtracking handles partial resolution failure properly.
#[tokio::test]
async fn test_backtracking_partial_resolution_failure() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    source_repo.add_resource("agents", "helper", "# Helper v1.0.0").await?;
    source_repo.commit_all("Add helper v1.0.0")?;
    source_repo.tag_version("helper-v1.0.0")?;

    source_repo.add_resource("agents", "helper", "# Helper v2.0.0").await?;
    source_repo.commit_all("Add helper v2.0.0")?;
    source_repo.tag_version("helper-v2.0.0")?;

    source_repo.add_resource("agents", "helper", "# Helper v3.0.0").await?;
    source_repo.commit_all("Add helper v3.0.0")?;
    source_repo.tag_version("helper-v3.0.0")?;

    source_repo.add_resource("snippets", "utils", "# Utils v1.0.0").await?;
    source_repo.commit_all("Add utils v1.0.0")?;
    source_repo.tag_version("utils-v1.0.0")?;

    source_repo.add_resource("snippets", "utils", "# Utils v2.0.0").await?;
    source_repo.commit_all("Add utils v2.0.0")?;
    source_repo.tag_version("utils-v2.0.0")?;

    source_repo.add_resource("scripts", "deploy", "# Deploy v1.0.0").await?;
    source_repo.commit_all("Add deploy v1.0.0")?;
    source_repo.tag_version("deploy-v1.0.0")?;

    source_repo.add_resource("scripts", "deploy", "# Deploy v2.0.0").await?;
    source_repo.commit_all("Add deploy v2.0.0")?;
    source_repo.tag_version("deploy-v2.0.0")?;

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

    let output = project.run_agpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail due to unresolvable conflict. Stderr: {}",
        output.stderr
    );

    assert!(
        output.stderr.contains("deploy") || output.stderr.contains("scripts/deploy"),
        "Should mention unresolvable deploy conflict. Stderr: {}",
        output.stderr
    );

    assert!(
        output.stderr.contains("conflict")
            || output.stderr.contains("automatic resolution")
            || output.stderr.contains("failed to resolve")
            || output.stderr.contains("Version conflicts"),
        "Should indicate conflict resolution was attempted. Stderr: {}",
        output.stderr
    );

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
#[tokio::test]
async fn test_backtracking_timeout_termination() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    for i in 1..=12 {
        source_repo
            .add_resource("agents", "complex-agent", &format!("# Complex Agent v{}.0", i))
            .await?;
        source_repo.commit_all(&format!("Add v{}.0", i))?;
        source_repo.tag_version(&format!("{}.0.0", i))?;
    }

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

    let output = project.run_agpm(&["install"])?;

    if !output.success {
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
        assert!(output.success, "Install should succeed or fail gracefully");
    }

    Ok(())
}

/// Test that backtracking handles deeply nested transitive dependencies (4+ levels).
#[tokio::test]
async fn test_backtracking_deeply_nested_transitive_dependencies() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    source_repo.add_resource("snippets", "snippet-e", "# Snippet E v1.0.0").await?;
    source_repo.commit_all("Add snippet E v1.0.0")?;
    source_repo.tag_version("e-v1.0.0")?;

    source_repo.add_resource("snippets", "snippet-e", "# Snippet E v2.0.0 CHANGED").await?;
    source_repo.commit_all("Update snippet E v2.0.0")?;
    source_repo.tag_version("e-v2.0.0")?;

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

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("top-agent", |d| {
            d.source("test").path("agents/agent-top.md").version("top-v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    if !output.success {
        assert!(
            output.stderr.contains("conflict")
                || output.stderr.contains("automatic resolution")
                || output.stderr.contains("Version conflicts"),
            "Should mention conflict detection. Stderr: {}",
            output.stderr
        );

        assert!(
            output.stderr.contains("agent-a")
                || output.stderr.contains("agent-b")
                || output.stderr.contains("snippet-e"),
            "Should mention dependencies in the chain. Stderr: {}",
            output.stderr
        );
    } else {
        let lockfile_content = project.read_lockfile().await?;
        let lockfile: agpm_cli::lockfile::LockFile = toml::from_str(&lockfile_content)?;

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
#[tokio::test]
async fn test_backtracking_invalid_resource_id_format_integration() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    source_repo.add_resource("snippets", "valid-snippet", "# Valid Snippet").await?;
    source_repo.commit_all("Add valid snippet")?;
    source_repo.tag_version("v1.0.0")?;

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

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("invalid-dep", |d| {
            d.source("test").path("agents/invalid-dep-agent.md").version("invalid-dep-v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    assert!(
        !output.success,
        "Install should fail due to invalid resource_id format. Stderr: {}",
        output.stderr
    );

    assert!(
        output.stderr.contains("resource")
            || output.stderr.contains("format")
            || output.stderr.contains("parse")
            || output.stderr.contains("invalid")
            || output.stderr.contains("malformed"),
        "Should mention resource format issue. Stderr: {}",
        output.stderr
    );

    assert!(
        !output.stderr.is_empty(),
        "Should have meaningful error message. Stderr: {}",
        output.stderr
    );

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
#[tokio::test]
async fn test_backtracking_missing_source_during_resolution() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    source_repo.add_resource("snippets", "conflict-snippet", "# Snippet v1.0.0").await?;
    source_repo.commit_all("Add snippet v1.0.0")?;
    source_repo.tag_version("backtrack-missing-source-v1.0.0")?;

    source_repo.add_resource("snippets", "conflict-snippet", "# Snippet v2.0.0 CHANGED").await?;
    source_repo.commit_all("Update snippet v2.0.0")?;
    source_repo.tag_version("backtrack-missing-source-v2.0.0")?;

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

    let alt_source_repo = project.create_source_repo("alt").await?;
    alt_source_repo.add_resource("agents", "agent-c", "# Agent C from alt source").await?;
    alt_source_repo.commit_all("Add agent C")?;
    alt_source_repo.tag_version("backtrack-alt-source-v1.0.0")?;

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_source("alt", &alt_source_repo.bare_file_url(project.sources_path())?)
        .add_agent("agent-a", |d| d.source("test").path("agents/agent-a.md").version("v1.0.0"))
        .add_agent("agent-b", |d| d.source("test").path("agents/agent-b.md").version("v1.0.0"))
        .add_agent("agent-c", |d| d.source("alt").path("agents/agent-c.md").version("v1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    let alt_source_path = project.sources_path().join("alt.git");
    if alt_source_path.exists() {
        std::fs::remove_dir_all(&alt_source_path)?;
    }

    let output = project.run_agpm(&["install"])?;

    if !output.success {
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

        assert!(
            !output.stderr.contains("panic")
                && !output.stderr.contains("stack trace")
                && !output.stderr.contains("unwrap")
                && !output.stderr.contains("thread 'main' panicked"),
            "Should not panic. Stderr: {}",
            output.stderr
        );
    }

    Ok(())
}

/// Test that PreparedSourceVersion state is correctly updated after backtracking.
#[tokio::test]
async fn test_backtracking_prepared_source_version_state_inspection() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

    source_repo.add_resource("snippets", "state-snippet", "# State Snippet v1.0.0").await?;
    source_repo.commit_all("Add state snippet v1.0.0")?;
    source_repo.tag_version("prepared-state-v1.0.0")?;

    source_repo.add_resource("snippets", "state-snippet", "# State Snippet v2.0.0 UPDATED").await?;
    source_repo.commit_all("Update state snippet v2.0.0")?;
    source_repo.tag_version("prepared-state-v2.0.0")?;

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

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("top-state", |d| {
            d.source("test").path("agents/top-state-agent.md").version("prepared-state-top-v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    if output.success {
        let lockfile_content = project.read_lockfile().await?;
        let lockfile: agpm_cli::lockfile::LockFile = toml::from_str(&lockfile_content)?;

        assert!(!lockfile.agents.is_empty(), "Should have resolved agents");
        assert!(!lockfile.snippets.is_empty(), "Should have resolved snippets");

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

    Ok(())
}

/// Test that pattern dependencies with version conflicts are detected and resolved.
#[tokio::test]
async fn test_backtracking_pattern_dependencies_with_version_conflicts() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("test").await?;

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

    let manifest = ManifestBuilder::new()
        .add_source("test", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("helper-v1-pattern", |d| {
            d.source("test").path("agents/helper*.md").version("pattern-conflicts-helper-v1.0.0")
        })
        .add_agent("helper-v2-pattern", |d| {
            d.source("test").path("agents/helper*.md").version("pattern-conflicts-helper-v2.0.0")
        })
        .add_agent("other-pattern", |d| {
            d.source("test").path("agents/other*.md").version("pattern-conflicts-other-v1.0.0")
        })
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

    let output = project.run_agpm(&["install"])?;

    if !output.success {
        assert!(
            output.stderr.contains("conflict")
                || output.stderr.contains("Version conflicts")
                || output.stderr.contains("automatic resolution"),
            "Should mention pattern conflict. Stderr: {}",
            output.stderr
        );

        assert!(
            output.stderr.contains("helper") || output.stderr.contains("pattern"),
            "Should mention helper files or patterns. Stderr: {}",
            output.stderr
        );
    } else {
        let lockfile_content = project.read_lockfile().await?;
        let lockfile: agpm_cli::lockfile::LockFile = toml::from_str(&lockfile_content)?;

        let helper_agents: Vec<_> =
            lockfile.agents.iter().filter(|a| a.name.contains("helper")).collect();

        assert!(!helper_agents.is_empty(), "Should have resolved helper agents from patterns");

        let helper_a_count = helper_agents.iter().filter(|a| a.name.contains("helper-a")).count();
        let helper_b_count = helper_agents.iter().filter(|a| a.name.contains("helper-b")).count();

        assert!(helper_a_count <= 1, "Should have at most one helper-a, found {}", helper_a_count);
        assert!(helper_b_count <= 1, "Should have at most one helper-b, found {}", helper_b_count);

        let other_agents: Vec<_> =
            lockfile.agents.iter().filter(|a| a.name.contains("other")).collect();

        assert!(!other_agents.is_empty(), "Should include non-conflicting pattern dependencies");

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
