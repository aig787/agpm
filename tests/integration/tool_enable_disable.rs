//! Integration tests for tool enable/disable functionality
//!
//! These tests verify that dependencies are properly included or excluded
//! when tools are enabled or disabled in the manifest.

use crate::common::TestProject;
use anyhow::Result;
use tokio::fs;

/// Test that disabled tools are excluded during installation
#[tokio::test]
async fn test_install_disabled_tools_excluded() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repository with both claude-code and opencode agents
    let source_repo = project.create_source_repo("test").await?;

    // Create directory structure
    fs::create_dir_all(source_repo.path.join("agents")).await?;
    fs::create_dir_all(source_repo.path.join("agent")).await?;

    // Create claude-code agent
    fs::write(
        source_repo.path.join("agents/claude-agent.md"),
        "# Claude Code Agent\nThis is a claude-code agent.",
    )
    .await?;

    // Create opencode agent
    fs::write(
        source_repo.path.join("agent/opencode-agent.md"),
        "# OpenCode Agent\nThis is an opencode agent.",
    )
    .await?;

    // Commit the files
    source_repo.git.add_all()?;
    source_repo.git.commit("Initial commit")?;
    source_repo.git.tag("v1.0.0")?;

    // Create manifest with opencode disabled
    let manifest_content = format!(
        r#"
[sources]
test = "{}"

[tools.claude-code]
path = ".claude"
resources = {{ agents = {{ path = "agents" }} }}

[tools.opencode]
enabled = false
path = ".opencode"
resources = {{ agents = {{ path = "agent" }} }}

[agents]
claude-agent = {{ source = "test", path = "agents/claude-agent.md", version = "v1.0.0" }}
opencode-agent = {{ source = "test", path = "agent/opencode-agent.md", version = "v1.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Check that only claude-code agent was installed
    let claude_agent_path =
        project.project_path().join(".claude").join("agents").join("claude-agent.md");
    assert!(claude_agent_path.exists(), "Claude-code agent should be installed");

    let opencode_agent_path =
        project.project_path().join(".opencode").join("agent").join("opencode-agent.md");
    assert!(!opencode_agent_path.exists(), "OpenCode agent should NOT be installed");

    // Check lockfile contains only enabled dependencies
    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock")).await?;
    assert!(lockfile_content.contains("claude-agent"));
    assert!(!lockfile_content.contains("opencode-agent"));

    Ok(())
}

/// Test that enabling a tool includes its dependencies during installation
#[tokio::test]
async fn test_install_enabled_tools_included() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repository with both claude-code and opencode agents
    let source_repo = project.create_source_repo("test").await?;

    // Create directory structure
    fs::create_dir_all(source_repo.path.join("agents")).await?;
    fs::create_dir_all(source_repo.path.join("agent")).await?;

    // Create claude-code agent
    fs::write(
        source_repo.path.join("agents/claude-agent.md"),
        "# Claude Code Agent\nThis is a claude-code agent.",
    )
    .await?;

    // Create opencode agent
    fs::write(
        source_repo.path.join("agent/opencode-agent.md"),
        "# OpenCode Agent\nThis is an opencode agent.",
    )
    .await?;

    // Commit the files
    source_repo.git.add_all()?;
    source_repo.git.commit("Initial commit")?;
    source_repo.git.tag("v1.0.0")?;

    // Create manifest with both tools enabled (opencode explicitly enabled)
    let manifest_content = format!(
        r#"
[sources]
test = "{}"

[tools.claude-code]
path = ".claude"
resources = {{ agents = {{ path = "agents" }} }}

[tools.opencode]
enabled = true
path = ".opencode"
resources = {{ agents = {{ path = "agent" }} }}

[agents]
claude-agent = {{ source = "test", path = "agents/claude-agent.md", version = "v1.0.0" }}
opencode-agent = {{ source = "test", path = "agent/opencode-agent.md", version = "v1.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Check that both agents were installed
    let claude_agent_path =
        project.project_path().join(".claude").join("agents").join("claude-agent.md");
    assert!(claude_agent_path.exists(), "Claude-code agent should be installed");

    let opencode_agent_path =
        project.project_path().join(".opencode").join("agent").join("opencode-agent.md");
    assert!(opencode_agent_path.exists(), "OpenCode agent should be installed");

    // Check lockfile contains both dependencies
    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock")).await?;
    assert!(lockfile_content.contains("claude-agent"));
    assert!(lockfile_content.contains("opencode-agent"));

    Ok(())
}

/// Test that default tool behavior (enabled=true) works correctly
#[tokio::test]
async fn test_install_default_tool_enabled() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repository with opencode agent
    let source_repo = project.create_source_repo("test").await?;

    // Create directory structure
    fs::create_dir_all(source_repo.path.join("agent")).await?;

    // Create opencode agent
    fs::write(
        source_repo.path.join("agent/opencode-agent.md"),
        "# OpenCode Agent\nThis is an opencode agent.",
    )
    .await?;

    // Commit the files
    source_repo.git.add_all()?;
    source_repo.git.commit("Initial commit")?;
    source_repo.git.tag("v1.0.0")?;

    // Create manifest with opencode tool defined but no explicit enabled field (should default to true)
    let manifest_content = format!(
        r#"
[sources]
test = "{}"

[tools.opencode]
path = ".opencode"
resources = {{ agents = {{ path = "agent" }} }}

[agents]
opencode-agent = {{ source = "test", path = "agent/opencode-agent.md", version = "v1.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Check that opencode agent was installed (should be enabled by default)
    let opencode_agent_path =
        project.project_path().join(".opencode").join("agent").join("opencode-agent.md");
    assert!(opencode_agent_path.exists(), "OpenCode agent should be installed by default");

    // Check lockfile contains the dependency
    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock")).await?;
    assert!(lockfile_content.contains("opencode-agent"));

    Ok(())
}

/// Test that update command respects tool enable/disable and install handles newly enabled tools
#[tokio::test]
async fn test_update_and_install_respect_tool_enable_disable() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repository
    let source_repo = project.create_source_repo("test").await?;

    // Create directory structure
    fs::create_dir_all(source_repo.path.join("agents")).await?;
    fs::create_dir_all(source_repo.path.join("agent")).await?;

    // Create initial versions
    fs::write(
        source_repo.path.join("agents/claude-agent.md"),
        "# Claude Code Agent v1\nThis is version 1.",
    )
    .await?;

    fs::write(
        source_repo.path.join("agent/opencode-agent.md"),
        "# OpenCode Agent v1\nThis is version 1.",
    )
    .await?;

    // Commit initial versions
    source_repo.git.add_all()?;
    source_repo.git.commit("Initial commit")?;
    source_repo.git.tag("v1.0.0")?;

    // Create manifest with opencode disabled
    let manifest_content = format!(
        r#"
[sources]
test = "{}"

[tools.claude-code]
path = ".claude"
resources = {{ agents = {{ path = "agents" }} }}

[tools.opencode]
enabled = false
path = ".opencode"
resources = {{ agents = {{ path = "agent" }} }}

[agents]
claude-agent = {{ source = "test", path = "agents/claude-agent.md", version = "v1.0.0" }}
opencode-agent = {{ source = "test", path = "agent/opencode-agent.md", version = "v1.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Debug: Check the manifest that was parsed
    let manifest_content_debug =
        fs::read_to_string(project.project_path().join("agpm.toml")).await?;
    println!("Manifest content:\n{}", manifest_content_debug);

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Verify only claude-agent was installed
    assert!(project.project_path().join(".claude/agents/claude-agent.md").exists());
    assert!(!project.project_path().join(".opencode/agent/opencode-agent.md").exists());

    // Create new versions
    fs::write(
        source_repo.path.join("agents/claude-agent.md"),
        "# Claude Code Agent v2\nThis is version 2.",
    )
    .await?;

    fs::write(
        source_repo.path.join("agent/opencode-agent.md"),
        "# OpenCode Agent v2\nThis is version 2.",
    )
    .await?;

    // Commit new versions
    source_repo.git.add_all()?;
    source_repo.git.commit("Update to v2")?;
    source_repo.git.tag("v2.0.0")?;

    // Update manifest to use v2.0.0
    let updated_manifest_content = format!(
        r#"
[sources]
test = "{}"

[tools.claude-code]
path = ".claude"
resources = {{ agents = {{ path = "agents" }} }}

[tools.opencode]
enabled = false
path = ".opencode"
resources = {{ agents = {{ path = "agent" }} }}

[agents]
claude-agent = {{ source = "test", path = "agents/claude-agent.md", version = "v2.0.0" }}
opencode-agent = {{ source = "test", path = "agent/opencode-agent.md", version = "v2.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), updated_manifest_content).await?;

    // Run update
    let output = project.run_agpm(&["update"])?;
    assert!(output.success);

    // Verify opencode agent is still not installed (should remain ignored)
    assert!(
        !project.project_path().join(".opencode/agent/opencode-agent.md").exists(),
        "OpenCode agent should still not be installed when tool is disabled"
    );

    // Now enable opencode and update again
    let enabled_manifest_content = format!(
        r#"
[sources]
test = "{}"

[tools.claude-code]
path = ".claude"
resources = {{ agents = {{ path = "agents" }} }}

[tools.opencode]
enabled = true
path = ".opencode"
resources = {{ agents = {{ path = "agent" }} }}

[agents]
claude-agent = {{ source = "test", path = "agents/claude-agent.md", version = "v2.0.0" }}
opencode-agent = {{ source = "test", path = "agent/opencode-agent.md", version = "v2.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), enabled_manifest_content).await?;

    // Run install again (not update) to install newly enabled dependencies
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Now opencode agent should be installed
    assert!(
        project.project_path().join(".opencode/agent/opencode-agent.md").exists(),
        "OpenCode agent should be installed when tool is enabled"
    );

    let opencode_agent_content =
        fs::read_to_string(project.project_path().join(".opencode/agent/opencode-agent.md"))
            .await?;
    assert!(opencode_agent_content.contains("version 2"));

    Ok(())
}

/// Test that opencode tool has correct defaults (enabled=true, agents/commands flattened)
#[tokio::test]
async fn test_opencode_defaults() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repository with opencode resources in subdirectories
    let source_repo = project.create_source_repo("test").await?;

    // Create directory structure with subdirectories to test flattening
    fs::create_dir_all(source_repo.path.join("agent/subdir")).await?;
    fs::create_dir_all(source_repo.path.join("command/commands")).await?;

    // Create opencode agent in subdirectory
    fs::write(
        source_repo.path.join("agent/subdir/my-agent.md"),
        "# My OpenCode Agent\nThis is in a subdirectory.",
    )
    .await?;

    // Create opencode command in subdirectory
    fs::write(
        source_repo.path.join("command/commands/my-command.md"),
        "# My OpenCode Command\nThis is in a subdirectory.",
    )
    .await?;

    // Commit the files
    source_repo.git.add_all()?;
    source_repo.git.commit("Initial commit")?;
    source_repo.git.tag("v1.0.0")?;

    // Create manifest with NO tool configuration at all
    // This should use the complete defaults: enabled=true, agents/commands flattened
    let manifest_content = format!(
        r#"
[sources]
test = "{}"

[agents]
my-agent = {{ source = "test", path = "agent/subdir/my-agent.md", version = "v1.0.0", tool = "opencode" }}

[commands]
my-command = {{ source = "test", path = "command/commands/my-command.md", version = "v1.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Debug: print the output to see what happened
    println!("Install stdout: {}", output.stdout);
    println!("Install stderr: {}", output.stderr);

    // Check what files were actually created
    let opencode_dir = project.project_path().join(".opencode");
    if opencode_dir.exists() {
        println!("OpenCode directory exists");
        let mut entries = fs::read_dir(&opencode_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            println!("Found: {}", entry.path().display());
            if entry.path().is_dir() {
                let mut sub_entries = fs::read_dir(&entry.path()).await?;
                while let Some(sub_entry) = sub_entries.next_entry().await? {
                    println!("  Sub-entry: {}", sub_entry.path().display());
                }
            }
        }
    } else {
        println!("OpenCode directory does not exist");
    }

    // Check if the files are in the nested directories (flattening not working)
    let nested_agent_path =
        project.project_path().join(".opencode").join("agent").join("subdir").join("my-agent.md");
    let nested_command_path = project
        .project_path()
        .join(".opencode")
        .join("command")
        .join("commands")
        .join("my-command.md");

    if nested_agent_path.exists() {
        println!("Agent found in nested directory - flattening is not working");
        let content = fs::read_to_string(&nested_agent_path).await?;
        println!("Agent content: {}", content);
    }

    if nested_command_path.exists() {
        println!("Command found in nested directory - flattening is not working");
        let content = fs::read_to_string(&nested_command_path).await?;
        println!("Command content: {}", content);
    }

    // Check that opencode agent was installed (should be enabled by default)
    let opencode_agent_path =
        project.project_path().join(".opencode").join("agent").join("my-agent.md");
    assert!(opencode_agent_path.exists(), "OpenCode agent should be installed by default");

    // Check that opencode command was installed (should be enabled by default)
    let opencode_command_path =
        project.project_path().join(".opencode").join("command").join("my-command.md");
    assert!(opencode_command_path.exists(), "OpenCode command should be installed by default");

    // Verify flattening: files should be directly in agent/ and command/ directories
    // NOT in agent/subdir/ or command/commands/ subdirectories
    let nested_agent_path =
        project.project_path().join(".opencode").join("agent").join("subdir").join("my-agent.md");
    let nested_command_path = project
        .project_path()
        .join(".opencode")
        .join("command")
        .join("commands")
        .join("my-command.md");

    assert!(
        !nested_agent_path.exists(),
        "Agent should be flattened, not preserve subdirectory structure"
    );
    assert!(
        !nested_command_path.exists(),
        "Command should be flattened, not preserve subdirectory structure"
    );

    // Verify the content is correct
    let agent_content = fs::read_to_string(&opencode_agent_path).await?;
    assert!(agent_content.contains("My OpenCode Agent"));

    let command_content = fs::read_to_string(&opencode_command_path).await?;
    assert!(command_content.contains("My OpenCode Command"));

    // Check lockfile contains both dependencies
    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock")).await?;
    assert!(lockfile_content.contains("my-agent"));
    assert!(lockfile_content.contains("my-command"));

    Ok(())
}

/// Test that explicit opencode configuration requires explicit flatten settings
#[tokio::test]
async fn test_opencode_explicit_config_requires_explicit_flatten() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repository with opencode resources in subdirectories
    let source_repo = project.create_source_repo("test").await?;

    // Create directory structure with subdirectories to test flattening
    fs::create_dir_all(source_repo.path.join("agent/subdir")).await?;
    fs::create_dir_all(source_repo.path.join("command/commands")).await?;

    // Create opencode agent in subdirectory
    fs::write(
        source_repo.path.join("agent/subdir/my-agent.md"),
        "# My OpenCode Agent\nThis is in a subdirectory.",
    )
    .await?;

    // Create opencode command in subdirectory
    fs::write(
        source_repo.path.join("command/commands/my-command.md"),
        "# My OpenCode Command\nThis is in a subdirectory.",
    )
    .await?;

    // Commit the files
    source_repo.git.add_all()?;
    source_repo.git.commit("Initial commit")?;
    source_repo.git.tag("v1.0.0")?;

    // Create manifest with EXPLICIT opencode tool configuration
    // When explicitly configured, we must specify flatten=true to get flattening
    let manifest_content = format!(
        r#"
[sources]
test = "{}"

[tools.opencode]
path = ".opencode"
resources = {{ agents = {{ path = "agent", flatten = true }}, commands = {{ path = "command", flatten = true }}, mcp-servers = {{ merge-target = ".opencode/opencode.json" }} }}

[agents]
my-agent = {{ source = "test", path = "agent/subdir/my-agent.md", version = "v1.0.0", tool = "opencode" }}

[commands]
my-command = {{ source = "test", path = "command/commands/my-command.md", version = "v1.0.0", tool = "opencode" }}
"#,
        source_repo.path.display()
    );

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Check that opencode agent was installed and flattened
    let opencode_agent_path =
        project.project_path().join(".opencode").join("agent").join("my-agent.md");
    assert!(opencode_agent_path.exists(), "OpenCode agent should be installed and flattened");

    // Check that opencode command was installed and flattened
    let opencode_command_path =
        project.project_path().join(".opencode").join("command").join("my-command.md");
    assert!(opencode_command_path.exists(), "OpenCode command should be installed and flattened");

    // Verify flattening: files should be directly in agent/ and command/ directories
    let nested_agent_path =
        project.project_path().join(".opencode").join("agent").join("subdir").join("my-agent.md");
    let nested_command_path = project
        .project_path()
        .join(".opencode")
        .join("command")
        .join("commands")
        .join("my-command.md");

    assert!(!nested_agent_path.exists(), "Agent should be flattened when explicitly configured");
    assert!(
        !nested_command_path.exists(),
        "Command should be flattened when explicitly configured"
    );

    // Verify the content is correct
    let agent_content = fs::read_to_string(&opencode_agent_path).await?;
    assert!(agent_content.contains("My OpenCode Agent"));

    let command_content = fs::read_to_string(&opencode_command_path).await?;
    assert!(command_content.contains("My OpenCode Command"));

    Ok(())
}
