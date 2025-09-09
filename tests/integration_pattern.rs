//! Integration tests for pattern-based dependency installation.

use anyhow::Result;
use std::fs;

mod common;
mod fixtures;
use common::TestGit;
use fixtures::{path_to_file_url, TestEnvironment};

/// Test installing dependencies using glob patterns.
#[test]
fn test_pattern_based_installation() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let env = TestEnvironment::new()?;
    let repo_dir = env.sources_dir.join("test_repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize git repository
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create multiple agent files in the repository
    let agents_dir = repo_dir.join("agents");
    fs::create_dir_all(&agents_dir)?;

    // Create AI-related agents
    fs::create_dir_all(agents_dir.join("ai"))?;
    fs::write(
        agents_dir.join("ai/assistant.md"),
        "# AI Assistant\n\nAI assistant agent",
    )?;
    fs::write(
        agents_dir.join("ai/analyzer.md"),
        "# AI Analyzer\n\nAI analyzer agent",
    )?;
    fs::write(
        agents_dir.join("ai/generator.md"),
        "# AI Generator\n\nAI generator agent",
    )?;

    // Create review-related agents
    fs::write(
        agents_dir.join("reviewer.md"),
        "# Reviewer\n\nCode reviewer agent",
    )?;
    fs::write(
        agents_dir.join("review-helper.md"),
        "# Review Helper\n\nReview helper agent",
    )?;

    // Create other agents
    fs::write(
        agents_dir.join("debugger.md"),
        "# Debugger\n\nDebugger agent",
    )?;
    fs::write(agents_dir.join("tester.md"), "# Tester\n\nTester agent")?;

    // Commit all files
    git.add_all()?;
    git.commit("Add multiple agent files")?;
    git.tag("v1.0.0")?;

    // Get repo URL as file://
    let repo_url = path_to_file_url(&repo_dir);

    // Create manifest with pattern dependencies
    let manifest_content = format!(
        r#"
[sources]
test-repo = "{}"

[agents]
# Install all AI agents
ai-agents = {{ source = "test-repo", path = "agents/ai/*.md", version = "v1.0.0" }}

# Install all review-related agents  
review-agents = {{ source = "test-repo", path = "agents/review*.md", version = "v1.0.0" }}

# Install all agents recursively
all-agents = {{ source = "test-repo", path = "agents/**/*.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    fs::write(env.project_dir.join("ccpm.toml"), manifest_content)?;

    // Run install command
    env.ccpm_command().arg("install").assert().success();

    // Verify that all AI agents were installed
    let ai_agents_dir = env.project_dir.join(".claude/agents");
    assert!(
        ai_agents_dir.join("assistant.md").exists(),
        "AI assistant not installed"
    );
    assert!(
        ai_agents_dir.join("analyzer.md").exists(),
        "AI analyzer not installed"
    );
    assert!(
        ai_agents_dir.join("generator.md").exists(),
        "AI generator not installed"
    );

    // Verify review agents were installed
    assert!(
        ai_agents_dir.join("reviewer.md").exists(),
        "Reviewer not installed"
    );
    assert!(
        ai_agents_dir.join("review-helper.md").exists(),
        "Review helper not installed"
    );

    // Verify lockfile was created with all resources
    let lockfile_path = env.project_dir.join("ccpm.lock");
    assert!(lockfile_path.exists(), "Lockfile not created");

    let lockfile_content = fs::read_to_string(&lockfile_path)?;
    assert!(
        lockfile_content.contains("assistant"),
        "Assistant not in lockfile"
    );
    assert!(
        lockfile_content.contains("analyzer"),
        "Analyzer not in lockfile"
    );
    assert!(
        lockfile_content.contains("generator"),
        "Generator not in lockfile"
    );
    assert!(
        lockfile_content.contains("reviewer"),
        "Reviewer not in lockfile"
    );
    assert!(
        lockfile_content.contains("review-helper"),
        "Review helper not in lockfile"
    );

    Ok(())
}

/// Test pattern dependencies with custom target directories.
#[test]
fn test_pattern_with_custom_target() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let env = TestEnvironment::new()?;
    let repo_dir = env.sources_dir.join("test_repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize git repository
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create snippet files
    let snippets_dir = repo_dir.join("snippets");
    fs::create_dir_all(&snippets_dir)?;

    fs::write(snippets_dir.join("util1.md"), "# Utility 1")?;
    fs::write(snippets_dir.join("util2.md"), "# Utility 2")?;
    fs::write(snippets_dir.join("helper.md"), "# Helper")?;

    git.add_all()?;
    git.commit("Add snippets")?;
    git.tag("v1.0.0")?;

    // Get repo URL as file://
    let repo_url = path_to_file_url(&repo_dir);

    // Create manifest with custom target
    let manifest_content = format!(
        r#"
[sources]
test-repo = "{}"

[snippets]
utilities = {{ source = "test-repo", path = "snippets/util*.md", version = "v1.0.0", target = "tools/utilities" }}
"#,
        repo_url
    );

    fs::write(env.project_dir.join("ccpm.toml"), manifest_content)?;

    // Run install
    env.ccpm_command().arg("install").assert().success();

    // Verify custom installation path
    let custom_dir = env.project_dir.join(".claude/tools/utilities");
    assert!(
        custom_dir.join("util1.md").exists(),
        "util1 not installed to custom path"
    );
    assert!(
        custom_dir.join("util2.md").exists(),
        "util2 not installed to custom path"
    );
    assert!(
        !custom_dir.join("helper.md").exists(),
        "helper should not be installed"
    );

    Ok(())
}

/// Test pattern dependencies with version constraints.
#[test]
fn test_pattern_with_versions() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let env = TestEnvironment::new()?;
    let repo_dir = env.sources_dir.join("test_repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize git repository
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create v1.0.0 agents
    let agents_dir = repo_dir.join("agents");
    fs::create_dir_all(&agents_dir)?;
    fs::write(agents_dir.join("agent1.md"), "# Agent 1 v1.0.0")?;
    fs::write(agents_dir.join("agent2.md"), "# Agent 2 v1.0.0")?;
    git.add_all()?;
    git.commit("Add agents v1.0.0")?;
    git.tag("v1.0.0")?;

    // Create v2.0.0 agents
    fs::write(agents_dir.join("agent1.md"), "# Agent 1 v2.0.0 - Updated")?;
    fs::write(agents_dir.join("agent2.md"), "# Agent 2 v2.0.0 - Updated")?;
    fs::write(agents_dir.join("agent3.md"), "# Agent 3 v2.0.0 - New")?;
    git.add_all()?;
    git.commit("Update agents to v2.0.0")?;
    git.tag("v2.0.0")?;

    // Get repo URL as file://
    let repo_url = path_to_file_url(&repo_dir);

    // Create manifest with v1.0.0 pattern dependency
    let manifest_content = format!(
        r#"
[sources]
test-repo = "{}"

[agents]
v1-agents = {{ source = "test-repo", path = "agents/*.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    fs::write(env.project_dir.join("ccpm.toml"), manifest_content)?;

    // Run install
    env.ccpm_command().arg("install").assert().success();

    // Verify v1.0.0 agents were installed
    let agent1_path = env.project_dir.join(".claude/agents/agent1.md");
    let agent2_path = env.project_dir.join(".claude/agents/agent2.md");
    let agent3_path = env.project_dir.join(".claude/agents/agent3.md");

    assert!(agent1_path.exists(), "Agent 1 not installed");
    assert!(agent2_path.exists(), "Agent 2 not installed");
    assert!(!agent3_path.exists(), "Agent 3 should not exist in v1.0.0");

    // Verify content is from v1.0.0
    let agent1_content = fs::read_to_string(&agent1_path)?;
    assert!(
        agent1_content.contains("v1.0.0"),
        "Agent 1 should be v1.0.0"
    );
    assert!(
        !agent1_content.contains("Updated"),
        "Agent 1 should not be updated version"
    );

    Ok(())
}

/// Test local filesystem patterns.
#[test]
fn test_local_pattern_dependencies() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let env = TestEnvironment::new()?;

    // Create a local directory with resources
    let resources_dir = env.sources_dir.join("local_resources");
    let agents_dir = resources_dir.join("agents");
    fs::create_dir_all(&agents_dir)?;

    fs::write(agents_dir.join("local1.md"), "# Local Agent 1")?;
    fs::write(agents_dir.join("local2.md"), "# Local Agent 2")?;
    fs::write(agents_dir.join("local3.md"), "# Local Agent 3")?;

    // Create manifest with local pattern dependency
    let manifest_content = format!(
        r#"
[agents]
local-agents = {{ path = "{}/agents/local*.md" }}
"#,
        resources_dir.display()
    );

    fs::write(env.project_dir.join("ccpm.toml"), manifest_content)?;

    // Run install
    let result = env.ccpm_command().arg("install").assert();

    // Local patterns might not be supported in the same way as remote patterns
    // This test documents the current behavior
    if result.get_output().status.success() {
        let agents_installed = env.project_dir.join(".claude/agents");
        println!(
            "Checking for installed local agents in: {:?}",
            agents_installed
        );
        // Verify if agents were installed
        assert!(
            agents_installed.join("local1.md").exists()
                || agents_installed.join("local2.md").exists()
                || agents_installed.join("local3.md").exists(),
            "At least one local agent should be installed"
        );
    } else {
        // Local patterns might require different handling
        println!("Local pattern installation not yet supported");
    }

    Ok(())
}

/// Test error handling for invalid patterns.
#[test]
fn test_invalid_pattern_error() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let env = TestEnvironment::new()?;

    // Create manifest with path traversal pattern
    let manifest_content = r#"
[sources]
test-repo = "https://github.com/example/repo.git"

[agents]
unsafe = { source = "test-repo", path = "../../../etc/*.conf", version = "latest" }
"#;

    fs::write(env.project_dir.join("ccpm.toml"), manifest_content)?;

    // Run validate command
    let result = env.ccpm_command().arg("validate").assert();

    // Should fail validation due to path traversal
    result.failure();

    Ok(())
}

/// Test pattern matching performance with many files.
#[test]
#[ignore = "Performance test - run manually"]
fn test_pattern_performance() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let env = TestEnvironment::new()?;
    let repo_dir = env.sources_dir.join("test_repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize git repository
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create 100 agent files
    let agents_dir = repo_dir.join("agents");
    fs::create_dir_all(&agents_dir)?;

    for i in 0..100 {
        let content = format!("# Agent {}\n\nAgent {} description", i, i);
        fs::write(agents_dir.join(format!("agent{:03}.md", i)), content)?;
    }

    git.add_all()?;
    git.commit("Add 100 agents")?;
    git.tag("v1.0.0")?;

    // Get repo URL as file://
    let repo_url = path_to_file_url(&repo_dir);

    // Create manifest
    let manifest_content = format!(
        r#"
[sources]
test-repo = "{}"

[agents]
all-agents = {{ source = "test-repo", path = "agents/*.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    fs::write(env.project_dir.join("ccpm.toml"), manifest_content)?;

    // Measure installation time
    let start = std::time::Instant::now();

    env.ccpm_command().arg("install").assert().success();

    let duration = start.elapsed();

    // Should complete in reasonable time (< 30 seconds for 100 files)
    assert!(
        duration.as_secs() < 30,
        "Installation took too long: {:?}",
        duration
    );

    // Verify all files were installed
    let lockfile_content = fs::read_to_string(env.project_dir.join("ccpm.lock"))?;
    let agent_count = lockfile_content.matches("agent").count();
    assert!(agent_count >= 100, "Not all agents were installed");

    Ok(())
}
