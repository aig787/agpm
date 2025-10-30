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
/// Bug: Currently transitive dependencies get the default tool for their type
/// (snippets default to agpm) instead of inheriting from parent.
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
    let command_path = project
        .project_path()
        .join(".opencode")
        .join("command")
        .join("test-command.md");

    assert!(
        command_path.exists(),
        "OpenCode command should be installed at: {:?}",
        command_path
    );

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
    let command_path = project
        .project_path()
        .join(".claude")
        .join("commands")
        .join("config-cmd.md");

    assert!(command_path.exists(), "Command should be installed");

    let installed_content = fs::read_to_string(&command_path).await?;
    assert!(
        installed_content.contains("Config Snippet"),
        "Template should be rendered with snippet content"
    );

    Ok(())
}
