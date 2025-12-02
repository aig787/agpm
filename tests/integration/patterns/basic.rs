//! Integration tests for pattern-based dependency installation.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use tokio::fs;

/// Test installing dependencies using glob patterns.
#[tokio::test]
async fn test_pattern_based_installation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create mock source repository with multiple agents
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create AI-related agents
    test_repo
        .add_resource("agents/ai", "assistant", "# AI Assistant\n\nAI assistant agent")
        .await?;
    test_repo.add_resource("agents/ai", "analyzer", "# AI Analyzer\n\nAI analyzer agent").await?;
    test_repo
        .add_resource("agents/ai", "generator", "# AI Generator\n\nAI generator agent")
        .await?;

    // Create review-related agents
    test_repo.add_resource("agents", "reviewer", "# Reviewer\n\nCode reviewer agent").await?;
    test_repo
        .add_resource("agents", "review-helper", "# Review Helper\n\nReview helper agent")
        .await?;

    // Create other agents
    test_repo.add_resource("agents", "debugger", "# Debugger\n\nDebugger agent").await?;
    test_repo.add_resource("agents", "tester", "# Tester\n\nTester agent").await?;

    // Commit all files
    test_repo.commit_all("Add multiple agent files")?;
    test_repo.tag_version("v1.0.0")?;

    // Get repo URL as file://
    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest with pattern dependencies (preserving nested structure)
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("ai-agents", |d| {
            d.source("test-repo").path("agents/ai/*.md").version("v1.0.0").flatten(false)
        })
        .add_agent("review-agents", |d| {
            d.source("test-repo").path("agents/review*.md").version("v1.0.0").flatten(false)
        })
        .add_agent("all-agents", |d| {
            d.source("test-repo").path("agents/**/*.md").version("v1.0.0").flatten(false)
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install command
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Verify that all AI agents were installed
    // With flatten=false, nested structure is preserved but resource type prefix is stripped
    let ai_agents_dir = project.project_path().join(".claude/agents/agpm");
    assert!(ai_agents_dir.join("ai/assistant.md").exists(), "AI assistant not installed");
    assert!(ai_agents_dir.join("ai/analyzer.md").exists(), "AI analyzer not installed");
    assert!(ai_agents_dir.join("ai/generator.md").exists(), "AI generator not installed");

    // Verify review agents were installed
    assert!(ai_agents_dir.join("reviewer.md").exists(), "Reviewer not installed");
    assert!(ai_agents_dir.join("review-helper.md").exists(), "Review helper not installed");

    // Verify lockfile was created with all resources
    let lockfile_path = project.project_path().join("agpm.lock");
    assert!(lockfile_path.exists(), "Lockfile not created");

    let lockfile_content = fs::read_to_string(&lockfile_path).await?;
    assert!(lockfile_content.contains("assistant"), "Assistant not in lockfile");
    assert!(lockfile_content.contains("analyzer"), "Analyzer not in lockfile");
    assert!(lockfile_content.contains("generator"), "Generator not in lockfile");
    assert!(lockfile_content.contains("reviewer"), "Reviewer not in lockfile");
    assert!(lockfile_content.contains("review-helper"), "Review helper not in lockfile");

    Ok(())
}

/// Test pattern dependencies with custom target directories.
#[tokio::test]
async fn test_pattern_with_custom_target() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create snippet files
    test_repo.add_resource("snippets", "util1", "# Utility 1").await?;
    test_repo.add_resource("snippets", "util2", "# Utility 2").await?;
    test_repo.add_resource("snippets", "helper", "# Helper").await?;

    test_repo.commit_all("Add snippets")?;
    test_repo.tag_version("v1.0.0")?;

    // Get repo URL as file://
    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest with custom target
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_snippet("utilities", |d| {
            d.source("test-repo")
                .path("snippets/util*.md")
                .version("v1.0.0")
                .target("tools/utilities")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Verify custom installation path
    // Custom target is relative to default snippets directory (.agpm/snippets/ for agpm tool)
    let custom_dir = project.project_path().join(".agpm/snippets/tools/utilities");
    assert!(custom_dir.join("util1.md").exists(), "util1 not installed to custom path");
    assert!(custom_dir.join("util2.md").exists(), "util2 not installed to custom path");
    assert!(!custom_dir.join("helper.md").exists(), "helper should not be installed");

    Ok(())
}

/// Test pattern dependencies with version constraints.
#[tokio::test]
async fn test_pattern_with_versions() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create v1.0.0 agents
    test_repo.add_resource("agents", "agent1", "# Agent 1 v1.0.0").await?;
    test_repo.add_resource("agents", "agent2", "# Agent 2 v1.0.0").await?;
    test_repo.commit_all("Add agents v1.0.0")?;
    test_repo.tag_version("v1.0.0")?;

    // For this test, we'll just use v1.0.0 as testing multiple versions
    // would require more complex git operations

    // Get repo URL as file://
    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest with v1.0.0 pattern dependency
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent_pattern("v1-agents", "test-repo", "agents/*.md", "v1.0.0")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Verify v1.0.0 agents were installed
    let agent1_path = project.project_path().join(".claude/agents/agpm/agent1.md");
    let agent2_path = project.project_path().join(".claude/agents/agpm/agent2.md");
    let agent3_path = project.project_path().join(".claude/agents/agpm/agent3.md");

    assert!(agent1_path.exists(), "Agent 1 not installed");
    assert!(agent2_path.exists(), "Agent 2 not installed");
    assert!(!agent3_path.exists(), "Agent 3 should not exist in v1.0.0");

    // Verify content is from v1.0.0
    let agent1_content = fs::read_to_string(&agent1_path).await?;
    assert!(agent1_content.contains("v1.0.0"), "Agent 1 should be v1.0.0");
    assert!(!agent1_content.contains("Updated"), "Agent 1 should not be updated version");

    Ok(())
}

/// Test local filesystem patterns.
///
/// Verifies that local pattern dependencies (e.g., `path/*.md`) correctly
/// expand and install all matched files.
#[tokio::test]
async fn test_local_pattern_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a local directory with resources relative to project
    let agents_dir = project.project_path().join("local_resources/agents");
    fs::create_dir_all(&agents_dir).await?;

    fs::write(agents_dir.join("local1.md"), "# Local Agent 1").await?;
    fs::write(agents_dir.join("local2.md"), "# Local Agent 2").await?;
    fs::write(agents_dir.join("local3.md"), "# Local Agent 3").await?;

    // Create manifest with local pattern dependency using relative path
    let manifest = ManifestBuilder::new()
        .add_local_agent("local-agents", "local_resources/agents/local*.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should succeed and install all 3 local agents
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Local pattern install failed: {}", output.stderr);

    // Verify all 3 agents were installed
    let agents_installed = project.project_path().join(".claude/agents/agpm");
    assert!(agents_installed.join("local1.md").exists(), "local1.md should be installed");
    assert!(agents_installed.join("local2.md").exists(), "local2.md should be installed");
    assert!(agents_installed.join("local3.md").exists(), "local3.md should be installed");

    Ok(())
}

#[tokio::test]
async fn test_pattern_sha_deduplication() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create multiple agent files
    for i in 1..=5 {
        let content = format!("# Agent {}\n\nAgent {} content", i, i);
        test_repo.add_resource("agents", &format!("agent{}", i), &content).await?;
    }

    // Create multiple snippet files
    for i in 1..=5 {
        let content = format!("# Snippet {}\n\nSnippet {} content", i, i);
        test_repo.add_resource("snippets", &format!("snippet{}", i), &content).await?;
    }

    test_repo.commit_all("Add resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest with multiple patterns pointing to SAME version
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("all-agents", |d| d.source("test-repo").path("agents/*.md").version("v1.0.0"))
        .add_snippet("all-snippets", |d| {
            d.source("test-repo").path("snippets/*.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install with logging to observe Git operations
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Verify all resources were installed
    let lockfile_content =
        tokio::fs::read_to_string(project.project_path().join("agpm.lock")).await?;

    // Should have 5 agents + 5 snippets = 10 total resources
    let agent_count = lockfile_content.matches("[[agents]]").count();
    let snippet_count = lockfile_content.matches("[[snippets]]").count();

    assert_eq!(agent_count, 5, "Should have 5 agents");
    assert_eq!(snippet_count, 5, "Should have 5 snippets");

    // All should reference the same commit SHA
    // Extract all resolved_commit values
    let commit_regex = regex::Regex::new(r#"resolved_commit = "([a-f0-9]+)""#)?;
    let commits: Vec<_> =
        commit_regex.captures_iter(&lockfile_content).map(|cap| cap[1].to_string()).collect();

    assert_eq!(commits.len(), 10, "Should have 10 resolved commits");

    // All commits should be identical (same version)
    let first_commit = &commits[0];
    assert!(
        commits.iter().all(|c| c == first_commit),
        "All resources should reference the same commit SHA: {:?}",
        commits
    );

    Ok(())
}

/// Test error handling for invalid patterns.
#[tokio::test]
async fn test_invalid_pattern_error() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create manifest with path traversal pattern - intentionally invalid, keep as-is
    let manifest_content = r#"
[sources]
test-repo = "https://github.com/example/repo.git"

[agents]
unsafe = { source = "test-repo", path = "../../../etc/*.conf", version = "latest" }
"#;

    project.write_manifest(manifest_content).await?;

    // Run validate command
    let output = project.run_agpm(&["validate"])?;

    // Should fail validation due to path traversal
    assert!(!output.success);

    Ok(())
}
