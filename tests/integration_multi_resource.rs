use anyhow::Result;
use std::fs;
use std::path::Path;
use tracing::debug;

mod common;
mod fixtures;
use common::TestGit;
use fixtures::{path_to_file_url, TestEnvironment};

#[test]
fn test_install_multiple_resources_with_versions() -> Result<()> {
    // Initialize test logging
    ccpm::test_utils::init_test_logging();

    let env = TestEnvironment::new()?;
    let repo_dir = env.sources_dir.join("test_repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize git repository with file:// URL
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create initial resources and commit (v1.0.0)
    create_v1_resources(&repo_dir)?;
    git.add_all()?;
    git.commit("Initial resources v1.0.0")?;
    git.tag("v1.0.0")?;

    // Create v1.1.0 with updated snippets
    update_snippets_v1_1(&repo_dir)?;
    git.add_all()?;
    git.commit("Update snippets v1.1.0")?;
    git.tag("v1.1.0")?;

    // Create v2.0.0 with updated agents
    update_agents_v2(&repo_dir)?;
    git.add_all()?;
    git.commit("Update agents v2.0.0")?;
    git.tag("v2.0.0")?;

    // Create v2.1.0 with updated command
    update_command_v2_1(&repo_dir)?;
    git.add_all()?;
    git.commit("Update command v2.1.0")?;
    git.tag("v2.1.0")?;

    // Create main branch pointing to latest
    git.create_branch("main")?;

    // Create ccpm.toml with mixed versions
    let repo_url = path_to_file_url(&repo_dir);
    let manifest_content = format!(
        r#"[sources]
test_repo = "{}"

[snippets]
snippet-one = {{ source = "test_repo", path = "snippets/snippet1.md", version = "v1.0.0" }}
snippet-two = {{ source = "test_repo", path = "snippets/snippet2.md", version = "v1.1.0" }}

[commands]
deploy-cmd = {{ source = "test_repo", path = "commands/deploy.md", version = "v2.1.0" }}

[agents]
agent-alpha = {{ source = "test_repo", path = "agents/alpha.md", version = "v1.0.0" }}
agent-beta = {{ source = "test_repo", path = "agents/beta.md", version = "v2.0.0" }}
agent-gamma = {{ source = "test_repo", path = "agents/gamma.md", version = "main" }}
"#,
        repo_url
    );

    fs::write(env.project_dir.join("ccpm.toml"), &manifest_content)?;

    // Log the manifest content and working directory for debugging
    debug!("Generated manifest content:\n{}", manifest_content);
    debug!("Running ccpm from directory: {:?}", env.project_dir);

    // Run install
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .env("CCPM_CACHE_DIR", env.cache_path())
        .assert()
        .success();

    // Verify all resources are installed with correct versions

    // Check snippets
    assert!(env
        .project_dir
        .join(".claude/ccpm/snippets/snippet-one.md")
        .exists());
    let snippet1_content =
        fs::read_to_string(env.project_dir.join(".claude/ccpm/snippets/snippet-one.md"))?;
    assert!(
        snippet1_content.contains("Snippet 1 v1.0.0"),
        "snippet-one should be v1.0.0"
    );

    assert!(env
        .project_dir
        .join(".claude/ccpm/snippets/snippet-two.md")
        .exists());
    let snippet2_content =
        fs::read_to_string(env.project_dir.join(".claude/ccpm/snippets/snippet-two.md"))?;
    assert!(
        snippet2_content.contains("Snippet 2 v1.1.0"),
        "snippet-two should be v1.1.0"
    );

    // Check command
    assert!(env
        .project_dir
        .join(".claude/commands/deploy-cmd.md")
        .exists());
    let command_content =
        fs::read_to_string(env.project_dir.join(".claude/commands/deploy-cmd.md"))?;
    assert!(
        command_content.contains("Deploy Command v2.1.0"),
        "deploy-cmd should be v2.1.0"
    );

    // Check agents
    assert!(env
        .project_dir
        .join(".claude/agents/agent-alpha.md")
        .exists());
    let alpha_content = fs::read_to_string(env.project_dir.join(".claude/agents/agent-alpha.md"))?;
    assert!(
        alpha_content.contains("Agent Alpha v1.0.0"),
        "agent-alpha should be v1.0.0"
    );

    assert!(env
        .project_dir
        .join(".claude/agents/agent-beta.md")
        .exists());
    let beta_content = fs::read_to_string(env.project_dir.join(".claude/agents/agent-beta.md"))?;
    assert!(
        beta_content.contains("Agent Beta v2.0.0"),
        "agent-beta should be v2.0.0"
    );

    assert!(env
        .project_dir
        .join(".claude/agents/agent-gamma.md")
        .exists());
    let gamma_content = fs::read_to_string(env.project_dir.join(".claude/agents/agent-gamma.md"))?;
    assert!(
        gamma_content.contains("Agent Gamma v2.1.0"),
        "agent-gamma should be latest (v2.1.0)"
    );

    // Verify lockfile was created
    assert!(env.project_dir.join("ccpm.lock").exists());
    let lockfile = fs::read_to_string(env.project_dir.join("ccpm.lock"))?;

    // Check that lockfile contains all resources
    assert!(lockfile.contains("[[snippets]]"));
    assert!(lockfile.contains("name = \"snippet-one\""));
    assert!(lockfile.contains("name = \"snippet-two\""));
    assert!(lockfile.contains("[[commands]]"));
    assert!(lockfile.contains("name = \"deploy-cmd\""));
    assert!(lockfile.contains("[[agents]]"));
    assert!(lockfile.contains("name = \"agent-alpha\""));
    assert!(lockfile.contains("name = \"agent-beta\""));
    assert!(lockfile.contains("name = \"agent-gamma\""));

    // Verify correct versions are locked
    assert!(lockfile.contains("version = \"v1.0.0\""));
    assert!(lockfile.contains("version = \"v1.1.0\""));
    assert!(lockfile.contains("version = \"v2.0.0\""));
    assert!(lockfile.contains("version = \"v2.1.0\""));
    assert!(lockfile.contains("version = \"main\""));

    Ok(())
}

fn create_v1_resources(repo_dir: &Path) -> Result<()> {
    // Create snippets
    fs::create_dir_all(repo_dir.join("snippets"))?;
    fs::write(
        repo_dir.join("snippets/snippet1.md"),
        "# Snippet 1 v1.0.0\n\nInitial snippet one content",
    )?;
    fs::write(
        repo_dir.join("snippets/snippet2.md"),
        "# Snippet 2 v1.0.0\n\nInitial snippet two content",
    )?;

    // Create command
    fs::create_dir_all(repo_dir.join("commands"))?;
    fs::write(
        repo_dir.join("commands/deploy.md"),
        "# Deploy Command v1.0.0\n\nInitial deploy command",
    )?;

    // Create agents
    fs::create_dir_all(repo_dir.join("agents"))?;
    fs::write(
        repo_dir.join("agents/alpha.md"),
        "# Agent Alpha v1.0.0\n\nInitial alpha agent",
    )?;
    fs::write(
        repo_dir.join("agents/beta.md"),
        "# Agent Beta v1.0.0\n\nInitial beta agent",
    )?;
    fs::write(
        repo_dir.join("agents/gamma.md"),
        "# Agent Gamma v1.0.0\n\nInitial gamma agent",
    )?;

    Ok(())
}

fn update_snippets_v1_1(repo_dir: &Path) -> Result<()> {
    // Update snippet2 only
    fs::write(
        repo_dir.join("snippets/snippet2.md"),
        "# Snippet 2 v1.1.0\n\nUpdated snippet two with new features",
    )?;
    Ok(())
}

fn update_agents_v2(repo_dir: &Path) -> Result<()> {
    // Update beta and gamma agents
    fs::write(
        repo_dir.join("agents/beta.md"),
        "# Agent Beta v2.0.0\n\nMajor update to beta agent",
    )?;
    fs::write(
        repo_dir.join("agents/gamma.md"),
        "# Agent Gamma v2.0.0\n\nMajor update to gamma agent",
    )?;
    Ok(())
}

fn update_command_v2_1(repo_dir: &Path) -> Result<()> {
    // Update deploy command
    fs::write(
        repo_dir.join("commands/deploy.md"),
        "# Deploy Command v2.1.0\n\nEnhanced deploy command with new options",
    )?;

    // Also update gamma agent for main branch
    fs::write(
        repo_dir.join("agents/gamma.md"),
        "# Agent Gamma v2.1.0\n\nLatest gamma agent on main branch",
    )?;

    Ok(())
}

#[test]
fn test_install_with_version_conflicts() -> Result<()> {
    // Initialize test logging
    ccpm::test_utils::init_test_logging();

    let env = TestEnvironment::new()?;
    let repo_dir = env.sources_dir.join("conflict_repo");
    fs::create_dir_all(&repo_dir)?;

    // Initialize git repository
    let git = TestGit::new(&repo_dir);
    git.init()?;
    git.config_user()?;

    // Create resources with dependencies
    fs::create_dir_all(repo_dir.join("agents"))?;
    fs::write(
        repo_dir.join("agents/dependent.md"),
        r#"---
dependencies:
  - snippet-base@v1.0.0
---
# Dependent Agent

Requires snippet-base v1.0.0"#,
    )?;

    fs::create_dir_all(repo_dir.join("snippets"))?;
    fs::write(
        repo_dir.join("snippets/base.md"),
        "# Base Snippet v1.0.0\n\nBase functionality",
    )?;

    git.add_all()?;
    git.commit("Initial with v1.0.0")?;
    git.tag("v1.0.0")?;

    // Update base snippet to v2.0.0
    fs::write(
        repo_dir.join("snippets/base.md"),
        "# Base Snippet v2.0.0\n\nBreaking changes",
    )?;

    git.add_all()?;
    git.commit("Update to v2.0.0")?;
    git.tag("v2.0.0")?;

    // Create manifest requesting incompatible versions
    let repo_url = path_to_file_url(&repo_dir);
    let manifest_content = format!(
        r#"[sources]
conflict_repo = "{}"

[snippets]
snippet-base = {{ source = "conflict_repo", path = "snippets/base.md", version = "v2.0.0" }}

[agents]
agent-dependent = {{ source = "conflict_repo", path = "agents/dependent.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    fs::write(env.project_dir.join("ccpm.toml"), &manifest_content)?;

    // Log the manifest content and working directory for debugging
    debug!(
        "Generated manifest content for version conflict test:\n{}",
        manifest_content
    );
    debug!("Running ccpm from directory: {:?}", env.project_dir);

    // Install should succeed but we can check for warnings in future versions
    let mut cmd = env.ccpm_command();
    cmd.arg("install")
        .env("CCPM_CACHE_DIR", env.cache_path())
        .assert()
        .success();

    // Verify both are installed with their specified versions
    assert!(env
        .project_dir
        .join(".claude/ccpm/snippets/snippet-base.md")
        .exists());
    let snippet_content = fs::read_to_string(
        env.project_dir
            .join(".claude/ccpm/snippets/snippet-base.md"),
    )?;
    assert!(snippet_content.contains("v2.0.0"));

    assert!(env
        .project_dir
        .join(".claude/agents/agent-dependent.md")
        .exists());
    let agent_content =
        fs::read_to_string(env.project_dir.join(".claude/agents/agent-dependent.md"))?;
    assert!(agent_content.contains("Requires snippet-base v1.0.0"));

    Ok(())
}
