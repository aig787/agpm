use anyhow::Result;
use std::fs as sync_fs;
use std::path::Path;
use tokio::fs;
use tracing::debug;

mod common;
mod fixtures;
use common::TestProject;

#[tokio::test]
async fn test_install_multiple_resources_with_versions() -> Result<()> {
    // Initialize test logging
    agpm::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a source repository using test utilities
    let source_repo = project.create_source_repo("test_repo").await?;

    // Create initial resources and commit (v1.0.0)
    create_v1_resources(&source_repo.path)?;
    source_repo.commit_all("Initial resources v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Create v1.1.0 with updated snippets
    update_snippets_v1_1(&source_repo.path)?;
    source_repo.commit_all("Update snippets v1.1.0")?;
    source_repo.tag_version("v1.1.0")?;

    // Create v1.2.0 with new scripts
    update_scripts_v1_2(&source_repo.path)?;
    source_repo.commit_all("Add scripts v1.2.0")?;
    source_repo.tag_version("v1.2.0")?;

    // Create v2.0.0 with updated agents
    update_agents_v2(&source_repo.path)?;
    source_repo.commit_all("Update agents v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

    // Create v2.1.0 with updated command and hooks
    update_command_v2_1(&source_repo.path)?;
    source_repo.commit_all("Update command and add hooks v2.1.0")?;
    source_repo.tag_version("v2.1.0")?;

    // Create v2.2.0 with MCP servers
    update_mcp_v2_2(&source_repo.path)?;
    source_repo.commit_all("Add MCP servers v2.2.0")?;
    source_repo.tag_version("v2.2.0")?;

    // Create v3.0.0 with major updates
    update_major_v3(&source_repo.path)?;
    source_repo.commit_all("Major update v3.0.0")?;
    source_repo.tag_version("v3.0.0")?;

    // Create v3.1.0 with additional resources
    update_additional_v3_1(&source_repo.path)?;
    source_repo.commit_all("Additional resources v3.1.0")?;
    source_repo.tag_version("v3.1.0")?;

    // Create v3.2.0 with more commands
    update_commands_v3_2(&source_repo.path)?;
    source_repo.commit_all("More commands v3.2.0")?;
    source_repo.tag_version("v3.2.0")?;

    // Create v4.0.0 with breaking changes
    update_breaking_v4(&source_repo.path)?;
    source_repo.commit_all("Breaking changes v4.0.0")?;
    source_repo.tag_version("v4.0.0")?;

    // Get the file URL for the bare repository (test utilities handle bare clone automatically)
    let repo_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest_content = format!(
        r#"[sources]
test_repo = "{}"

[agents]
agent-alpha = {{ source = "test_repo", path = "agents/alpha.md", version = "v1.0.0" }}
agent-beta = {{ source = "test_repo", path = "agents/beta.md", version = "v2.0.0" }}
agent-gamma = {{ source = "test_repo", path = "agents/gamma.md", version = "v4.0.0" }}
agent-delta = {{ source = "test_repo", path = "agents/delta.md", version = "v3.1.0" }}

[snippets]
snippet-one = {{ source = "test_repo", path = "snippets/snippet1.md", version = "v1.0.0" }}
snippet-two = {{ source = "test_repo", path = "snippets/snippet2.md", version = "v1.1.0" }}
snippet-three = {{ source = "test_repo", path = "snippets/snippet3.md", version = "v3.0.0" }}
snippet-four = {{ source = "test_repo", path = "snippets/snippet4.md", version = "v4.0.0" }}

[commands]
deploy-cmd = {{ source = "test_repo", path = "commands/deploy.md", version = "v2.1.0" }}
build-cmd = {{ source = "test_repo", path = "commands/build.md", version = "v3.2.0" }}
test-cmd = {{ source = "test_repo", path = "commands/test.md", version = "v3.2.0" }}
lint-cmd = {{ source = "test_repo", path = "commands/lint.md", version = "v4.0.0" }}

[scripts]
build-script = {{ source = "test_repo", path = "scripts/build.sh", version = "v1.2.0" }}
test-script = {{ source = "test_repo", path = "scripts/test.js", version = "v2.2.0" }}
deploy-script = {{ source = "test_repo", path = "scripts/deploy.py", version = "v3.0.0" }}

[hooks]
pre-commit = {{ source = "test_repo", path = "hooks/pre-commit.json", version = "v2.1.0" }}
post-commit = {{ source = "test_repo", path = "hooks/post-commit.json", version = "v3.1.0" }}

[mcp-servers]
filesystem = {{ source = "test_repo", path = "mcp-servers/filesystem.json", version = "v2.2.0" }}
postgres = {{ source = "test_repo", path = "mcp-servers/postgres.json", version = "v3.0.0" }}
redis = {{ source = "test_repo", path = "mcp-servers/redis.json", version = "v4.0.0" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest_content).await?;

    // Log the manifest content and working directory for debugging
    debug!("Generated manifest content:\n{}", manifest_content);
    debug!("Running agpm from directory: {:?}", project.project_path());

    // Run install
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify all 20 resources are installed with correct versions

    // Check agents (4 resources)
    // Files use basename from path, not dependency name
    verify_file_contains(
        &project.project_path().join(".claude/agents/alpha.md"),
        "Agent Alpha v1.0.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agents/beta.md"),
        "Agent Beta v2.0.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agents/gamma.md"),
        "Agent Gamma v4.0.0",
    )
    .await?; // v4.0.0
    verify_file_contains(
        &project.project_path().join(".claude/agents/delta.md"),
        "Agent Delta v3.1.0",
    )
    .await?;

    // Check snippets (4 resources)
    // Files use basename from path, not dependency name
    verify_file_contains(
        &project.project_path().join(".claude/agpm/snippets/snippet1.md"),
        "Snippet 1 v1.0.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/snippets/snippet2.md"),
        "Snippet 2 v1.1.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/snippets/snippet3.md"),
        "Snippet 3 v3.0.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/snippets/snippet4.md"),
        "Snippet 4 v4.0.0",
    )
    .await?;

    // Check commands (4 resources)
    // Files use basename from path, not dependency name
    verify_file_contains(
        &project.project_path().join(".claude/commands/deploy.md"),
        "Deploy Command v2.1.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/commands/build.md"),
        "Build Command v3.2.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/commands/test.md"),
        "Test Command v3.2.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/commands/lint.md"),
        "Lint Command v4.0.0",
    )
    .await?;

    // Check scripts (3 resources)
    // Files use basename from path, not dependency name
    verify_file_contains(
        &project.project_path().join(".claude/agpm/scripts/build.sh"),
        "Build Script v1.2.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/scripts/test.js"),
        "Test Script v2.2.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/scripts/deploy.py"),
        "Deploy Script v3.0.0",
    )
    .await?;

    // Check hooks (2 resources)
    verify_file_contains(
        &project.project_path().join(".claude/agpm/hooks/pre-commit.json"),
        "Pre-commit hook v2.1.0",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/hooks/post-commit.json"),
        "Post-commit hook v3.1.0",
    )
    .await?;

    // Check MCP servers (3 resources)
    verify_file_contains(
        &project.project_path().join(".claude/agpm/mcp-servers/filesystem.json"),
        "\"version\": \"v2.2.0\"",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/mcp-servers/postgres.json"),
        "\"version\": \"v3.0.0\"",
    )
    .await?;
    verify_file_contains(
        &project.project_path().join(".claude/agpm/mcp-servers/redis.json"),
        "\"version\": \"v4.0.0\"",
    )
    .await?;

    // Verify lockfile was created
    assert!(project.project_path().join("agpm.lock").exists());
    let lockfile = fs::read_to_string(project.project_path().join("agpm.lock")).await?;

    // Check that lockfile contains all 20 resources
    assert!(lockfile.contains("[[agents]]"));
    assert!(lockfile.contains("[[snippets]]"));
    assert!(lockfile.contains("[[commands]]"));
    assert!(lockfile.contains("[[scripts]]"));
    assert!(lockfile.contains("[[hooks]]"));
    assert!(lockfile.contains("[[mcp-servers]]"));

    // Verify all resource names are present
    let resource_names = [
        "agent-alpha",
        "agent-beta",
        "agent-gamma",
        "agent-delta",
        "snippet-one",
        "snippet-two",
        "snippet-three",
        "snippet-four",
        "deploy-cmd",
        "build-cmd",
        "test-cmd",
        "lint-cmd",
        "build-script",
        "test-script",
        "deploy-script",
        "pre-commit",
        "post-commit",
        "filesystem",
        "postgres",
        "redis",
    ];

    for name in &resource_names {
        assert!(
            lockfile.contains(&format!("name = \"{}\"", name)),
            "Lockfile should contain resource: {}",
            name
        );
    }

    // Verify all 10+ versions are locked
    let versions = [
        "v1.0.0", "v1.1.0", "v1.2.0", "v2.0.0", "v2.1.0", "v2.2.0", "v3.0.0", "v3.1.0", "v3.2.0",
        "v4.0.0",
    ];

    for version in &versions {
        assert!(
            lockfile.contains(&format!("version = \"{}\"", version)),
            "Lockfile should contain version: {}",
            version
        );
    }

    // All versions are now tags, no branch references needed

    Ok(())
}

// Helper function to verify file contains expected content
async fn verify_file_contains(path: &Path, expected: &str) -> Result<()> {
    assert!(path.exists(), "File should exist: {:?}", path);
    let content = fs::read_to_string(path).await?;
    assert!(
        content.contains(expected),
        "File {:?} should contain '{}', but got: {}",
        path.file_name().unwrap_or_default(),
        expected,
        content
    );
    Ok(())
}

fn create_v1_resources(repo_dir: &Path) -> Result<()> {
    // Create agents
    sync_fs::create_dir_all(repo_dir.join("agents"))?;
    sync_fs::write(
        repo_dir.join("agents/alpha.md"),
        "# Agent Alpha v1.0.0\n\nInitial alpha agent",
    )?;
    sync_fs::write(repo_dir.join("agents/beta.md"), "# Agent Beta v1.0.0\n\nInitial beta agent")?;
    sync_fs::write(
        repo_dir.join("agents/gamma.md"),
        "# Agent Gamma v1.0.0\n\nInitial gamma agent",
    )?;
    sync_fs::write(
        repo_dir.join("agents/delta.md"),
        "# Agent Delta v1.0.0\n\nInitial delta agent",
    )?;

    // Create snippets
    sync_fs::create_dir_all(repo_dir.join("snippets"))?;
    sync_fs::write(
        repo_dir.join("snippets/snippet1.md"),
        "# Snippet 1 v1.0.0\n\nInitial snippet one",
    )?;
    sync_fs::write(
        repo_dir.join("snippets/snippet2.md"),
        "# Snippet 2 v1.0.0\n\nInitial snippet two",
    )?;
    sync_fs::write(
        repo_dir.join("snippets/snippet3.md"),
        "# Snippet 3 v1.0.0\n\nInitial snippet three",
    )?;
    sync_fs::write(
        repo_dir.join("snippets/snippet4.md"),
        "# Snippet 4 v1.0.0\n\nInitial snippet four",
    )?;

    // Create commands
    sync_fs::create_dir_all(repo_dir.join("commands"))?;
    sync_fs::write(
        repo_dir.join("commands/deploy.md"),
        "# Deploy Command v1.0.0\n\nInitial deploy",
    )?;
    sync_fs::write(repo_dir.join("commands/build.md"), "# Build Command v1.0.0\n\nInitial build")?;
    sync_fs::write(repo_dir.join("commands/test.md"), "# Test Command v1.0.0\n\nInitial test")?;
    sync_fs::write(repo_dir.join("commands/lint.md"), "# Lint Command v1.0.0\n\nInitial lint")?;

    Ok(())
}

fn update_snippets_v1_1(repo_dir: &Path) -> Result<()> {
    // Update snippet2 only
    sync_fs::write(
        repo_dir.join("snippets/snippet2.md"),
        "# Snippet 2 v1.1.0\n\nUpdated snippet two",
    )?;
    Ok(())
}

fn update_scripts_v1_2(repo_dir: &Path) -> Result<()> {
    // Add scripts
    sync_fs::create_dir_all(repo_dir.join("scripts"))?;
    sync_fs::write(
        repo_dir.join("scripts/build.sh"),
        "#!/bin/bash\n# Build Script v1.2.0\necho 'Building...'",
    )?;
    sync_fs::write(
        repo_dir.join("scripts/test.js"),
        "// Test Script v1.2.0\nconsole.log('Testing...');",
    )?;
    sync_fs::write(
        repo_dir.join("scripts/deploy.py"),
        "#!/usr/bin/env python\n# Deploy Script v1.2.0\nprint('Deploying...')",
    )?;
    Ok(())
}

fn update_agents_v2(repo_dir: &Path) -> Result<()> {
    // Update beta and gamma agents
    sync_fs::write(repo_dir.join("agents/beta.md"), "# Agent Beta v2.0.0\n\nMajor update to beta")?;
    sync_fs::write(
        repo_dir.join("agents/gamma.md"),
        "# Agent Gamma v2.0.0\n\nMajor update to gamma",
    )?;
    Ok(())
}

fn update_command_v2_1(repo_dir: &Path) -> Result<()> {
    // Update deploy command and add hooks
    sync_fs::write(
        repo_dir.join("commands/deploy.md"),
        "# Deploy Command v2.1.0\n\nEnhanced deploy",
    )?;
    sync_fs::write(repo_dir.join("agents/gamma.md"), "# Agent Gamma v2.1.0\n\nGamma v2.1.0")?;

    // Add hooks
    sync_fs::create_dir_all(repo_dir.join("hooks"))?;
    sync_fs::write(
        repo_dir.join("hooks/pre-commit.json"),
        r#"{"events": ["PreToolUse"], "matcher": ".*", "type": "command", "command": "echo 'Pre-commit hook'", "description": "Pre-commit hook v2.1.0"}"#,
    )?;
    sync_fs::write(
        repo_dir.join("hooks/post-commit.json"),
        r#"{"events": ["PostToolUse"], "matcher": ".*", "type": "command", "command": "echo 'Post-commit hook'", "description": "Post-commit hook v2.1.0"}"#,
    )?;
    Ok(())
}

fn update_mcp_v2_2(repo_dir: &Path) -> Result<()> {
    // Update test script and add MCP servers
    sync_fs::write(
        repo_dir.join("scripts/test.js"),
        "// Test Script v2.2.0\nconsole.log('Testing v2.2...');",
    )?;

    sync_fs::create_dir_all(repo_dir.join("mcp-servers"))?;
    sync_fs::write(
        repo_dir.join("mcp-servers/filesystem.json"),
        r#"{"name": "filesystem", "version": "v2.2.0", "type": "filesystem"}"#,
    )?;
    sync_fs::write(
        repo_dir.join("mcp-servers/postgres.json"),
        r#"{"name": "postgres", "version": "v2.2.0", "type": "database"}"#,
    )?;
    sync_fs::write(
        repo_dir.join("mcp-servers/redis.json"),
        r#"{"name": "redis", "version": "v2.2.0", "type": "cache"}"#,
    )?;
    Ok(())
}

fn update_major_v3(repo_dir: &Path) -> Result<()> {
    // Major updates to multiple resources
    sync_fs::write(
        repo_dir.join("snippets/snippet3.md"),
        "# Snippet 3 v3.0.0\n\nMajor snippet three",
    )?;
    sync_fs::write(
        repo_dir.join("scripts/deploy.py"),
        "#!/usr/bin/env python\n# Deploy Script v3.0.0\nprint('Deploying v3...')",
    )?;
    sync_fs::write(
        repo_dir.join("mcp-servers/postgres.json"),
        r#"{"name": "postgres", "version": "v3.0.0", "type": "database", "features": ["ssl"]}"#,
    )?;
    Ok(())
}

fn update_additional_v3_1(repo_dir: &Path) -> Result<()> {
    // Update delta agent and post-commit hook
    sync_fs::write(
        repo_dir.join("agents/delta.md"),
        "# Agent Delta v3.1.0\n\nDelta enhanced v3.1",
    )?;
    sync_fs::write(
        repo_dir.join("hooks/post-commit.json"),
        r#"{"events": ["PostToolUse"], "matcher": ".*", "type": "command", "command": "echo 'Post-commit v3.1'", "description": "Post-commit hook v3.1.0"}"#,
    )?;
    Ok(())
}

fn update_commands_v3_2(repo_dir: &Path) -> Result<()> {
    // Add new commands
    sync_fs::write(
        repo_dir.join("commands/build.md"),
        "# Build Command v3.2.0\n\nBuild automation v3.2",
    )?;
    sync_fs::write(repo_dir.join("commands/test.md"), "# Test Command v3.2.0\n\nTest runner v3.2")?;
    Ok(())
}

fn update_breaking_v4(repo_dir: &Path) -> Result<()> {
    // Breaking changes v4.0.0
    sync_fs::write(
        repo_dir.join("snippets/snippet4.md"),
        "# Snippet 4 v4.0.0\n\nBreaking snippet four",
    )?;
    sync_fs::write(repo_dir.join("commands/lint.md"), "# Lint Command v4.0.0\n\nLinter v4.0")?;
    sync_fs::write(
        repo_dir.join("mcp-servers/redis.json"),
        r#"{"name": "redis", "version": "v4.0.0", "type": "cache", "breaking": true}"#,
    )?;
    sync_fs::write(
        repo_dir.join("agents/gamma.md"),
        "# Agent Gamma v4.0.0\n\nGamma breaking v4.0",
    )?;
    Ok(())
}

#[tokio::test]
async fn test_install_with_version_conflicts() -> Result<()> {
    // Initialize test logging
    agpm::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a source repository using test utilities
    let source_repo = project.create_source_repo("conflict_repo").await?;

    // Create resources with dependencies
    fs::create_dir_all(source_repo.path.join("agents")).await?;
    fs::write(
        source_repo.path.join("agents/dependent.md"),
        r#"---
dependencies:
  - snippet-base@v1.0.0
---
# Dependent Agent

Requires snippet-base v1.0.0"#,
    )
    .await?;

    fs::create_dir_all(source_repo.path.join("snippets")).await?;
    fs::write(
        source_repo.path.join("snippets/base.md"),
        "# Base Snippet v1.0.0\n\nBase functionality",
    )
    .await?;

    source_repo.commit_all("Initial with v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Update base snippet to v2.0.0
    fs::write(
        source_repo.path.join("snippets/base.md"),
        "# Base Snippet v2.0.0\n\nBreaking changes",
    )
    .await?;

    source_repo.commit_all("Update to v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

    // Get the file URL for the bare repository (test utilities handle bare clone automatically)
    let repo_url = source_repo.bare_file_url(project.sources_path())?;
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

    fs::write(project.project_path().join("agpm.toml"), &manifest_content).await?;

    // Log the manifest content and working directory for debugging
    debug!("Generated manifest content for version conflict test:\n{}", manifest_content);
    debug!("Running agpm from directory: {:?}", project.project_path());

    // Install should succeed but we can check for warnings in future versions
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify both are installed with their specified versions
    // Files use basename from path, not dependency name
    assert!(project.project_path().join(".claude/agpm/snippets/base.md").exists());
    let snippet_content =
        fs::read_to_string(project.project_path().join(".claude/agpm/snippets/base.md")).await?;
    assert!(snippet_content.contains("v2.0.0"));

    assert!(project.project_path().join(".claude/agents/dependent.md").exists());
    let agent_content =
        fs::read_to_string(project.project_path().join(".claude/agents/dependent.md")).await?;
    assert!(agent_content.contains("Requires snippet-base v1.0.0"));

    Ok(())
}
