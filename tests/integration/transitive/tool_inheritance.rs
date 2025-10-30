//! Integration tests for tool inheritance in transitive dependencies
//!
//! Tests that transitive dependencies properly inherit the tool from their parent
//! resource when not explicitly specified.

use anyhow::Result;
use tokio::fs;

use crate::common::TestProject;

/// Test that transitive dependencies inherit tool from parent resource
///
/// This test verifies that when a resource with an explicit tool (e.g., opencode)
/// has transitive dependencies declared in its frontmatter, those dependencies
/// inherit the parent's tool if not explicitly specified.
///
/// Scenario:
/// - OpenCode command declares a snippet dependency in frontmatter
/// - Snippet does not specify a tool (should inherit opencode)
/// - Command template uses {{ agpm.deps.snippets.helper.content }}
/// - Should resolve to the opencode-specific snippet, not agpm default
///
/// This test verifies the fix for a bug where transitive dependencies
/// would get the default tool for their type instead of inheriting from parent.
///
/// The fix (commit 7e22f77) merged user tool configs with built-in defaults,
/// ensuring transitive dependencies can inherit the parent's tool correctly.
#[tokio::test]
async fn test_transitive_dependency_inherits_parent_tool() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("commands")).await?;
    fs::create_dir_all(project.project_path().join("snippets")).await?;

    // Create a snippet file that will be referenced as a transitive dependency
    let snippet_content = r#"# Helper Snippet
This is a helper snippet for commands.

## Usage
Use this snippet to help with command tasks.
"#;
    fs::write(project.project_path().join("snippets/helper.md"), snippet_content).await?;

    // Create an OpenCode command that depends on the snippet
    // The snippet dependency does NOT specify tool - should inherit opencode from parent
    let command_content = r#"---
description: Test command with transitive snippet dependency
agpm:
  templating: true
dependencies:
  snippets:
    - name: helper
      path: ../snippets/helper.md
      install: false
---

# Test Command

This command uses a helper snippet:

{{ agpm.deps.snippets.helper.content }}
"#;
    fs::write(project.project_path().join("commands/test-command.md"), command_content).await?;

    // Create manifest with opencode tool enabled and command referencing the file
    // The command has tool=opencode, so its transitive snippet dep should inherit opencode
    let manifest_content = r#"
[tools.claude-code]
path = ".claude"
resources = { commands = { path = "commands", flatten = true } }

[tools.opencode]
enabled = true
path = ".opencode"
resources = { commands = { path = "command", flatten = true } }

[tools.agpm]
path = ".agpm"
resources = { snippets = { path = "snippets" } }

[commands]
test-command = { path = "commands/test-command.md", tool = "opencode" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should succeed with snippet inheriting opencode tool
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        eprintln!("STDOUT:\n{}", output.stdout);
        eprintln!("STDERR:\n{}", output.stderr);
    }

    assert!(
        output.success,
        "Install should succeed when transitive dependency inherits parent tool. stderr: {}",
        output.stderr
    );

    // Verify the command was installed to opencode directory
    let command_path =
        project.project_path().join(".opencode").join("command").join("test-command.md");

    assert!(command_path.exists(), "OpenCode command should be installed at: {:?}", command_path);

    // Read the installed command and verify template was rendered with snippet content
    let installed_content = fs::read_to_string(&command_path).await?;

    // The template should have been rendered with the snippet content
    assert!(
        installed_content.contains("Helper Snippet"),
        "Template should be rendered with snippet content. Got:\n{}",
        installed_content
    );

    assert!(
        !installed_content.contains("{{ agpm.deps.snippets.helper.content }}"),
        "Template variable should be replaced, not left as-is. Got:\n{}",
        installed_content
    );

    // Verify lockfile records correct tool for transitive dependency
    let lockfile = project.load_lockfile()?;
    let snippet_entry = lockfile
        .snippets
        .iter()
        .find(|s| s.name == "snippets/helper")
        .ok_or_else(|| anyhow::anyhow!("Snippet 'snippets/helper' not found in lockfile"))?;

    assert_eq!(
        snippet_entry.tool,
        Some("opencode".to_string()),
        "Transitive dependency should inherit parent tool in lockfile. Got: {:?}",
        snippet_entry.tool
    );

    Ok(())
}

/// Test that explicit tool specification overrides parent inheritance
///
/// This test verifies that when a transitive dependency explicitly specifies
/// a tool, it uses that tool instead of inheriting from the parent.
///
/// Scenario:
/// - Claude Code command declares a snippet dependency
/// - Snippet explicitly specifies tool=agpm
/// - Should use agpm tool, not inherit claude-code from parent
#[tokio::test]
async fn test_transitive_dependency_explicit_tool_overrides_parent() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("commands")).await?;
    fs::create_dir_all(project.project_path().join("snippets")).await?;

    // Create a snippet file
    let snippet_content = r#"# Config Snippet
Configuration helper snippet.
"#;
    fs::write(project.project_path().join("snippets/config.md"), snippet_content).await?;

    // Create a Claude Code command that depends on the snippet with explicit tool=agpm
    let command_content = r#"---
description: Test command with explicit tool in transitive dependency
agpm:
  templating: true
dependencies:
  snippets:
    - name: config
      path: ../snippets/config.md
      tool: agpm
      install: false
---

# Test Command

Using config: {{ agpm.deps.snippets.config.content }}
"#;
    fs::write(project.project_path().join("commands/config-cmd.md"), command_content).await?;

    // Create manifest
    let manifest_content = r#"
[tools.claude-code]
path = ".claude"
resources = { commands = { path = "commands", flatten = true } }

[tools.agpm]
path = ".agpm"
resources = { snippets = { path = "snippets" } }

[commands]
config-cmd = { path = "commands/config-cmd.md", tool = "claude-code" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should succeed with explicit tool override
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        eprintln!("STDOUT:\n{}", output.stdout);
        eprintln!("STDERR:\n{}", output.stderr);
    }

    assert!(
        output.success,
        "Install should succeed when transitive dependency has explicit tool. stderr: {}",
        output.stderr
    );

    // Verify the command was installed and rendered
    let command_path =
        project.project_path().join(".claude").join("commands").join("config-cmd.md");

    assert!(command_path.exists(), "Command should be installed");

    let installed_content = fs::read_to_string(&command_path).await?;
    assert!(
        installed_content.contains("Config Snippet"),
        "Template should be rendered with snippet content"
    );

    // Verify lockfile records correct tool for transitive dependency
    let lockfile = project.load_lockfile()?;
    let snippet_entry = lockfile
        .snippets
        .iter()
        .find(|s| s.name == "snippets/config")
        .ok_or_else(|| anyhow::anyhow!("Snippet 'snippets/config' not found in lockfile"))?;

    assert_eq!(
        snippet_entry.tool,
        Some("agpm".to_string()),
        "Transitive dependency with explicit tool should record it in lockfile. Got: {:?}",
        snippet_entry.tool
    );

    Ok(())
}

/// Test that partial tool configs are merged with defaults
///
/// This test verifies the fix for the bug where old manifests (pre-v0.4)
/// that didn't specify all resource types would fail when transitive
/// dependencies needed those missing resource types.
///
/// Scenario:
/// - OpenCode config only specifies agents and commands (old manifest style)
/// - OpenCode command has a transitive snippet dependency
/// - Snippet resource type is NOT in user config
/// - Should succeed: snippets come from merged defaults
/// - Should inherit opencode tool, not fall back to agpm
#[tokio::test]
async fn test_partial_tool_config_merges_with_defaults() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("commands")).await?;
    fs::create_dir_all(project.project_path().join("snippets")).await?;

    // Create a snippet file
    let snippet_content = r#"# Config Helper
This is a configuration helper snippet.
"#;
    fs::write(project.project_path().join("snippets/config.md"), snippet_content).await?;

    // Create an OpenCode command with transitive snippet dependency
    let command_content = r#"---
description: Command with transitive snippet dependency
agpm:
  templating: true
dependencies:
  snippets:
    - name: config
      path: ../snippets/config.md
      install: false
---

# Setup Command

Configuration: {{ agpm.deps.snippets.config.content }}
"#;
    fs::write(project.project_path().join("commands/setup.md"), command_content).await?;

    // CRITICAL: Manifest with partial opencode config (old style)
    // Only agents and commands specified - snippets missing!
    let manifest_content = r#"
[tools.claude-code]
path = ".claude"

[tools.opencode]
enabled = true
path = ".opencode"
resources = { agents = { path = "agent", flatten = true }, commands = { path = "command", flatten = true } }

[tools.agpm]
path = ".agpm"
resources = { snippets = { path = "snippets" } }

[commands]
setup = { path = "commands/setup.md", tool = "opencode" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should succeed with snippet using merged default config
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        eprintln!("STDOUT:\n{}", output.stdout);
        eprintln!("STDERR:\n{}", output.stderr);
    }

    assert!(
        output.success,
        "Install should succeed when tool config is partial and merges with defaults. stderr: {}",
        output.stderr
    );

    // Verify command was installed to opencode directory
    let command_path = project.project_path().join(".opencode").join("command").join("setup.md");

    assert!(command_path.exists(), "OpenCode command should be installed");

    // Verify template was rendered with snippet content
    let installed_content = fs::read_to_string(&command_path).await?;

    assert!(
        installed_content.contains("Config Helper"),
        "Template should be rendered with snippet content (tool inherited). Got:\n{}",
        installed_content
    );

    // Verify snippet was NOT installed to agpm directory (wrong tool)
    let wrong_path = project.project_path().join(".agpm").join("snippets").join("config.md");

    assert!(
        !wrong_path.exists(),
        "Snippet should NOT be installed to agpm directory (should inherit opencode)"
    );

    // Verify lockfile records correct tool for transitive dependency
    let lockfile = project.load_lockfile()?;
    let snippet_entry = lockfile
        .snippets
        .iter()
        .find(|s| s.name == "snippets/config")
        .ok_or_else(|| anyhow::anyhow!("Snippet 'snippets/config' not found in lockfile"))?;

    assert_eq!(
        snippet_entry.tool,
        Some("opencode".to_string()),
        "Transitive dependency should inherit opencode tool in lockfile from partial config. Got: {:?}",
        snippet_entry.tool
    );

    Ok(())
}

/// Test that custom tools work correctly with transitive dependencies
///
/// Custom tools don't have built-in defaults, so they should use
/// their explicit configuration without merging.
#[tokio::test]
async fn test_custom_tool_transitive_dependency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("commands")).await?;
    fs::create_dir_all(project.project_path().join("snippets")).await?;

    // Create snippet
    let snippet_content = r#"# Custom Snippet
Helper for custom tool.
"#;
    fs::write(project.project_path().join("snippets/helper.md"), snippet_content).await?;

    // Create command with transitive dependency
    let command_content = r#"---
description: Custom tool command
agpm:
  templating: true
dependencies:
  snippets:
    - name: helper
      path: ../snippets/helper.md
      install: false
---

# Custom Command

Using: {{ agpm.deps.snippets.helper.content }}
"#;
    fs::write(project.project_path().join("commands/cmd.md"), command_content).await?;

    // Manifest with custom tool (not well-known)
    let manifest_content = r#"
[tools.my_custom_tool]
enabled = true
path = ".my-custom"
resources = { commands = { path = "commands", flatten = true }, snippets = { path = "snippets", flatten = false } }

[commands]
cmd = { path = "commands/cmd.md", tool = "my_custom_tool" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should succeed with custom tool
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed for custom tool with transitive deps. stderr: {}",
        output.stderr
    );

    // Verify command installed to custom tool directory
    let command_path = project.project_path().join(".my-custom").join("commands").join("cmd.md");

    assert!(command_path.exists(), "Custom tool command should be installed");

    let content = fs::read_to_string(&command_path).await?;
    assert!(content.contains("Custom Snippet"), "Template should be rendered with snippet content");

    Ok(())
}

/// Test that transitive dependency chains inherit tool correctly
///
/// Scenario: Agent → Command → Snippet (3-level chain)
/// All without explicit tool should inherit from root agent
#[tokio::test]
async fn test_transitive_chain_inherits_tool() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("agents")).await?;
    fs::create_dir_all(project.project_path().join("commands")).await?;
    fs::create_dir_all(project.project_path().join("snippets")).await?;

    // Level 3: Snippet (leaf node)
    let snippet_content = r#"# Base Config
Configuration values.
"#;
    fs::write(project.project_path().join("snippets/config.md"), snippet_content).await?;

    // Level 2: Command that depends on snippet
    let command_content = r#"---
description: Middle command
agpm:
  templating: true
dependencies:
  snippets:
    - name: config
      path: ../snippets/config.md
      install: false
---

# Command

Config: {{ agpm.deps.snippets.config.content }}
"#;
    fs::write(project.project_path().join("commands/helper.md"), command_content).await?;

    // Level 1: Agent that depends on command
    let agent_content = r#"---
description: Root agent
agpm:
  templating: true
dependencies:
  commands:
    - name: helper
      path: ../commands/helper.md
      install: false
---

# Agent

Helper: {{ agpm.deps.commands.helper.content }}
"#;
    fs::write(project.project_path().join("agents/root.md"), agent_content).await?;

    // Manifest with opencode tool at root
    let manifest_content = r#"
[tools.opencode]
enabled = true
path = ".opencode"
resources = { agents = { path = "agent", flatten = true }, commands = { path = "command", flatten = true } }
# Note: snippets not specified - should merge from defaults

[agents]
root = { path = "agents/root.md", tool = "opencode" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - entire chain should inherit opencode
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed with transitive chain inheritance. stderr: {}",
        output.stderr
    );

    // Verify agent installed with fully rendered chain
    let agent_path = project.project_path().join(".opencode").join("agent").join("root.md");

    assert!(agent_path.exists(), "Agent should be installed");

    let content = fs::read_to_string(&agent_path).await?;

    // Should contain content from entire chain
    assert!(
        content.contains("Base Config"),
        "Agent should have rendered content from entire dependency chain"
    );

    Ok(())
}

/// Test that transitive dependency fails when tool doesn't support resource type
///
/// This test verifies that when a tool doesn't support a resource type
/// needed by a transitive dependency, installation fails with a clear error.
#[tokio::test]
async fn test_transitive_dependency_unsupported_resource_type() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("agents")).await?;
    fs::create_dir_all(project.project_path().join("snippets")).await?;

    // Create a snippet file
    let snippet_content = r#"# Helper Snippet
This is a helper snippet.
"#;
    fs::write(project.project_path().join("snippets/helper.md"), snippet_content).await?;

    // Create an agent that depends on the snippet
    let agent_content = r#"---
description: Agent with transitive snippet dependency
agpm:
  templating: true
dependencies:
  snippets:
    - name: helper
      path: ../snippets/helper.md
      install: false
---

# Test Agent

This agent uses a snippet: {{ agpm.deps.snippets.helper.content }}
"#;
    fs::write(project.project_path().join("agents/test-agent.md"), agent_content).await?;

    // Create manifest with custom tool that only supports agents (not snippets)
    let manifest_content = r#"
[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents", flatten = true } }

[tools.my_custom_tool]
enabled = true
path = ".my-custom"
resources = { agents = { path = "agents", flatten = true } }
# IMPORTANT: No snippets configured for my_custom_tool

[agents]
test-agent = { path = "agents/test-agent.md", tool = "my_custom_tool" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should fail because my_custom_tool doesn't support snippets
    let output = project.run_agpm(&["install"])?;

    assert!(
        !output.success,
        "Install should fail when tool doesn't support required resource type"
    );

    assert!(
        output.stderr.contains("does not support resource type")
            || output.stderr.contains("unsupported")
            || output.stderr.contains("snippets"),
        "Error should mention unsupported resource type. Got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that circular dependencies in transitive chain are detected
///
/// This test verifies that circular dependencies are properly detected
/// even when they span multiple levels of transitive dependencies.
#[tokio::test]
async fn test_transitive_circular_dependency_detection() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("agents")).await?;
    fs::create_dir_all(project.project_path().join("commands")).await?;
    fs::create_dir_all(project.project_path().join("snippets")).await?;

    // Level 3: Snippet that depends on Agent A (creates cycle)
    let snippet_content = r#"---
description: Snippet that depends on agent (creates cycle)
agpm:
  templating: true
dependencies:
  agents:
    - name: root-agent
      path: ../agents/root-agent.md
      install: false
---

# Cycle Snippet

This snippet creates a cycle back to agent.
"#;
    fs::write(project.project_path().join("snippets/cycle.md"), snippet_content).await?;

    // Level 2: Command that depends on snippet
    let command_content = r#"---
description: Command that depends on snippet
agpm:
  templating: true
dependencies:
  snippets:
    - name: cycle
      path: ../snippets/cycle.md
      install: false
---

# Middle Command

This command uses cycle snippet: {{ agpm.deps.snippets.cycle.content }}
"#;
    fs::write(project.project_path().join("commands/middle.md"), command_content).await?;

    // Level 1: Agent that depends on command
    let agent_content = r#"---
description: Root agent in cycle
agpm:
  templating: true
dependencies:
  commands:
    - name: middle
      path: ../commands/middle.md
      install: false
---

# Root Agent

This agent uses middle command: {{ agpm.deps.commands.middle.content }}
"#;
    fs::write(project.project_path().join("agents/root-agent.md"), agent_content).await?;

    // Manifest that creates the cycle: Agent -> Command -> Snippet -> Agent
    let manifest_content = r#"
[tools.claude-code]
path = ".claude"
resources = { agents = { path = "agents", flatten = true } }

[agents]
root-agent = { path = "agents/root-agent.md", tool = "claude-code" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should detect circular dependency
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail when circular dependency is detected");

    assert!(
        output.stderr.contains("Circular dependency") || output.stderr.contains("circular"),
        "Error should mention circular dependency. Got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that missing transitive dependency files are handled gracefully
///
/// This test verifies that when a transitive dependency references
/// a non-existent file, installation fails with a clear error.
#[tokio::test]
async fn test_transitive_dependency_file_not_found() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("commands")).await?;
    // Note: NOT creating snippets directory - file should be missing

    // Create a command that depends on a non-existent snippet
    let command_content = r#"---
description: Command with missing transitive dependency
agpm:
  templating: true
dependencies:
  snippets:
    - name: missing
      path: ../snippets/missing.md
      install: false
---

# Test Command

This command tries to use a missing snippet: {{ agpm.deps.snippets.missing.content }}
"#;
    fs::write(project.project_path().join("commands/test.md"), command_content).await?;

    // Simple manifest
    let manifest_content = r#"
[tools.claude-code]
path = ".claude"
resources = { commands = { path = "commands", flatten = true } }

[commands]
test = { path = "commands/test.md", tool = "claude-code" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should fail because snippet file doesn't exist
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail when transitive dependency file is not found");

    assert!(
        output.stderr.contains("not found")
            || output.stderr.contains("No such file")
            || output.stderr.contains("does not exist")
            || output.stderr.contains("missing.md")
            || output.stderr.contains("file access")
            || output.stderr.contains("Failed to file access"),
        "Error should mention missing file. Got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that malformed tool configurations are detected
///
/// This test verifies that malformed tool configurations in manifest
/// result in clear error messages during installation.
#[tokio::test]
async fn test_malformed_tool_configuration() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    fs::create_dir_all(project.project_path().join("agents")).await?;

    // Create a simple agent
    let agent_content = r#"# Simple Agent
This is a test agent.
"#;
    fs::write(project.project_path().join("agents/simple.md"), agent_content).await?;

    // Manifest with malformed tool configuration (missing required 'path' field)
    let manifest_content = r#"
[tools.claude-code]
# Missing 'path' field - this is malformed
resources = { agents = { path = "agents" }

[tools.malformed_tool]
enabled = true
# Missing both 'path' and 'resources' fields - completely invalid

[agents]
simple = { path = "agents/simple.md", tool = "claude-code" }
"#;

    fs::write(project.project_path().join("agpm.toml"), manifest_content).await?;

    // Run install - should fail due to malformed tool configuration
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail when tool configuration is malformed");

    assert!(
        output.stderr.contains("missing field")
            || output.stderr.contains("required")
            || output.stderr.contains("path")
            || output.stderr.contains("invalid")
            || output.stderr.contains("malformed"),
        "Error should mention configuration issue. Got: {}",
        output.stderr
    );

    Ok(())
}
