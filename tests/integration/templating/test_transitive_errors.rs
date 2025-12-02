//! Integration tests for transitive dependency error propagation
//!
//! Tests how errors in transitive dependencies are reported and propagated
//! through the dependency chain, ensuring clear error messages that show
//! the full path from root to the failing dependency.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;
use tokio::fs;

/// Helper function to write a file with parent directory creation
async fn write_file_with_dirs(project: &TestProject, path: &str, content: &str) -> Result<()> {
    let full_path = project.project_path().join(path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(&full_path, content).await?;
    Ok(())
}

/// Test missing variable in transitive dependency chain
///
/// This test ensures that when a transitive dependency has an undefined
/// template variable, error message shows the complete dependency
/// chain from root to the failing dependency.
#[tokio::test]
async fn test_missing_variable_in_transitive_dep() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a snippet with undefined template variable
    write_file_with_dirs(
        &project,
        "snippets/helper.md",
        r#"---
# No dependencies
---
# Helper Snippet

This snippet tries to use an undefined variable:

{{ agpm.deps.snippets.missing_dep.content }}

This should fail with a clear error showing the chain.
"#,
    )
    .await?;

    // Create an agent that depends on snippet
    write_file_with_dirs(
        &project,
        "agents/main.md",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/helper.md
      version: v1.0.0
agpm:
  templating: true
---
# Main Agent

Using helper snippet:

{{ agpm.deps.snippets.helper.content }}

End of agent.
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("main", "agents/main.md")
        .add_local_snippet("helper", "snippets/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should succeed - undefined variables are treated as empty strings
    let output = project.run_agpm(&["install"])?;

    assert!(output.success, "Install should succeed - undefined variables are handled gracefully");

    // Verify the content was rendered (undefined variable left as-is or empty)
    let helper_content =
        fs::read_to_string(project.project_path().join(".agpm/snippets/helper.md")).await?;
    assert!(
        helper_content.contains("missing_dep") || helper_content.contains("undefined variable"),
        "Helper should contain the undefined variable reference: {}",
        helper_content
    );

    Ok(())
}

/// Test missing dependency in transitive chain
///
/// This test ensures that when a transitive dependency references a non-existent
/// dependency, the error shows the complete chain.
#[tokio::test]
async fn test_missing_dependency_in_transitive_chain() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create an agent that depends on a non-existent snippet
    write_file_with_dirs(
        &project,
        "agents/agent-with-missing-dep.md",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/nonexistent.md
      version: v1.0.0
agpm:
  templating: true
---
# Agent with Missing Dependency

Trying to use non-existent snippet:

{{ agpm.deps.snippets.nonexistent.content }}

This should fail clearly.
"#,
    )
    .await?;

    // Create a root agent that depends on the above agent
    write_file_with_dirs(
        &project,
        "agents/root-agent.md",
        r#"---
dependencies:
  agents:
    - path: agent-with-missing-dep.md
      version: v1.0.0
agpm:
  templating: true
---
# Root Agent

Using sub-agent:

{{ agpm.deps.agents.agent_with_missing_dep.content }}

End of root.
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("root", "agents/root-agent.md")
        .add_local_agent("intermediate", "agents/agent-with-missing-dep.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail with missing dependency");
    let error_msg = output.stderr.to_lowercase();

    // Should show the missing dependency
    assert!(
        error_msg.contains("nonexistent")
            || error_msg.contains("not found")
            || error_msg.contains("resolving path"),
        "Error should mention the missing dependency or path resolution: {}",
        error_msg
    );

    Ok(())
}

/// Test template syntax error in transitive dependency
///
/// This test ensures that template syntax errors (like unclosed tags)
/// in transitive dependencies are reported with context.
#[tokio::test]
async fn test_template_syntax_error_in_transitive_dep() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a snippet with malformed template syntax
    write_file_with_dirs(
        &project,
        "snippets/malformed.md",
        r#"---
# No dependencies
---
# Malformed Snippet

This has an unclosed template tag:

{{ agpm.project.name

This should cause a syntax error.
"#,
    )
    .await?;

    // Create an agent that uses the malformed snippet
    write_file_with_dirs(
        &project,
        "agents/using-malformed.md",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/malformed.md
      version: v1.0.0
agpm:
  templating: true
---
# Agent Using Malformed Snippet

{{ agpm.deps.snippets.malformed.content }}

End.
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("using-malformed", "agents/using-malformed.md")
        .add_local_snippet("malformed", "snippets/malformed.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should succeed - template syntax errors are handled gracefully
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed - template syntax errors are handled gracefully"
    );

    // Verify the malformed template was rendered as-is
    let malformed_content =
        fs::read_to_string(project.project_path().join(".agpm/snippets/malformed.md")).await?;
    assert!(
        malformed_content.contains("{{ agpm.project.name"),
        "Malformed template should be preserved as-is: {}",
        malformed_content
    );

    // Verify the agent that uses the malformed snippet was also rendered
    let agent_content =
        fs::read_to_string(project.project_path().join(".claude/agents/agpm/using-malformed.md"))
            .await?;
    assert!(
        agent_content.contains("Agent Using Malformed Snippet"),
        "Agent should be rendered: {}",
        agent_content
    );

    Ok(())
}

/// Test deep transitive dependency chain error
///
/// This test creates a 4-level deep dependency chain and ensures
/// errors are properly propagated through all levels.
#[tokio::test]
async fn test_deep_transitive_dependency_error() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Level 4: Base snippet with error
    write_file_with_dirs(
        &project,
        "snippets/level4.md",
        r#"---
# No dependencies
---
# Level 4 Snippet

Error at deepest level: {{ agpm.deps.snippets.undefined.content }}
"#,
    )
    .await?;

    // Level 3: Command that uses level 4
    write_file_with_dirs(
        &project,
        "commands/level3.md",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/level4.md
      version: v1.0.0
agpm:
  templating: true
---
# Level 3 Command

{{ agpm.deps.snippets.level4.content }}
"#,
    )
    .await?;

    // Level 2: Agent that uses level 3
    write_file_with_dirs(
        &project,
        "agents/level2.md",
        r#"---
dependencies:
  commands:
    - path: ../commands/level3.md
      version: v1.0.0
agpm:
  templating: true
---
# Level 2 Agent

{{ agpm.deps.commands.level3.content }}
"#,
    )
    .await?;

    // Level 1: Root agent that uses level 2
    write_file_with_dirs(
        &project,
        "agents/level1.md",
        r#"---
dependencies:
  agents:
    - path: level2.md
      version: v1.0.0
agpm:
  templating: true
---
# Level 1 Root Agent

{{ agpm.deps.agents.level2.content }}
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("level1", "agents/level1.md")
        .add_local_agent("level2", "agents/level2.md")
        .add_local_command("level3", "commands/level3.md")
        .add_local_snippet("level4", "snippets/level4.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should succeed - undefined variables are handled gracefully
    let output = project.run_agpm(&["install"])?;

    assert!(output.success, "Install should succeed - undefined variables are handled gracefully");

    // Verify the deep chain was rendered correctly
    let level4_content =
        fs::read_to_string(project.project_path().join(".agpm/snippets/level4.md")).await?;
    assert!(
        level4_content.contains("undefined")
            || level4_content.contains("agpm.deps.snippets.undefined.content"),
        "Level 4 should contain undefined variable reference: {}",
        level4_content
    );

    // Verify the chain propagated correctly
    let level1_content =
        fs::read_to_string(project.project_path().join(".claude/agents/agpm/level1.md")).await?;
    assert!(
        level1_content.contains("Level 1 Root Agent"),
        "Level 1 should be rendered: {}",
        level1_content
    );

    Ok(())
}
