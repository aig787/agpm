//! Integration tests for circular dependency detection in templating context
//!
//! Tests various circular dependency scenarios specifically in the context of
//! template rendering, including cross-resource type cycles, conditional
//! dependencies, and mixed templating/non-templating cycles.

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

/// Test basic circular dependency detection in templating context
///
/// This is similar to the existing transitive test but specifically
/// focuses on templating context and error message clarity.
#[tokio::test]
async fn test_circular_dependency_detection_templating() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create A → B → C → A circular dependency
    write_file_with_dirs(
        &project,
        "agents/a.md",
        r#"---
dependencies:
  agents:
    - path: b.md
      version: v1.0.0
agpm:
  templating: true
---
# Agent A
Content from B: {{ agpm.deps.agents.b.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/b.md",
        r#"---
dependencies:
  agents:
    - path: c.md
      version: v1.0.0
agpm:
  templating: true
---
# Agent B
Content from C: {{ agpm.deps.agents.c.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/c.md",
        r#"---
dependencies:
  agents:
    - path: a.md
      version: v1.0.0
agpm:
  templating: true
---
# Agent C
Content from A: {{ agpm.deps.agents.a.content }}
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("a", "agents/a.md")
        .add_local_agent("b", "agents/b.md")
        .add_local_agent("c", "agents/c.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail with circular dependency");
    let err_lower = output.stderr.to_lowercase();

    assert!(
        err_lower.contains("circular") || err_lower.contains("cycle"),
        "Error should mention circular dependency: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agents/a") || output.stderr.contains("agent:agents/a"),
        "Error should mention agent A: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agents/b") || output.stderr.contains("agent:agents/b"),
        "Error should mention agent B: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agents/c") || output.stderr.contains("agent:agents/c"),
        "Error should mention agent C: {}",
        output.stderr
    );

    Ok(())
}

/// Test cross-resource type circular dependency
///
/// Tests cycles that span different resource types (agent → snippet → agent).
#[tokio::test]
async fn test_cross_resource_type_circular_dependency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Agent → Snippet → Agent cycle
    write_file_with_dirs(
        &project,
        "agents/agent1.md",
        r#"---
agpm:
  templating: true
  dependencies:
    snippets:
      - path: ../snippets/snippet1.md
        install: false
---
# Agent 1
Using snippet: {{ agpm.deps.snippets.snippet1.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "snippets/snippet1.md",
        r#"---
agpm:
  templating: true
  dependencies:
    agents:
      - path: ../agents/agent2.md
        install: false
---
# Snippet 1
Using agent: {{ agpm.deps.agents.agent2.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/agent2.md",
        r#"---
agpm:
  templating: true
  dependencies:
    agents:
      - path: agent1.md
        install: false
---
# Agent 2
Back to agent 1: {{ agpm.deps.agents.agent1.content }}
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("agent1", "agents/agent1.md")
        .add_local_agent("agent2", "agents/agent2.md")
        .add_local_snippet("snippet1", "snippets/snippet1.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail with circular dependency");
    let err_lower = output.stderr.to_lowercase();

    assert!(
        err_lower.contains("circular") || err_lower.contains("cycle"),
        "Error should mention circular dependency: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("agent1")
            && output.stderr.contains("snippet1")
            && output.stderr.contains("agent2"),
        "Error should mention all resources in cycle: {}",
        output.stderr
    );

    Ok(())
}

/// Test circular dependency with conditional dependencies
///
/// Tests cycles that are created through conditional logic in templates.
#[tokio::test]
async fn test_conditional_circular_dependency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create conditional circular dependency
    write_file_with_dirs(
        &project,
        "agents/conditional-a.md",
        r#"---
dependencies:
  agents:
    - path: conditional-b.md
      install: false
{% if agpm.project.feature_enabled %}
    - path: conditional-c.md
      install: false
  {% endif %}
agpm:
  templating: true
---
# Conditional Agent A
From B: {{ agpm.deps.agents.conditional_b.content }}
{% if agpm.project.feature_enabled %}
From C: {{ agpm.deps.agents.conditional_c.content }}
{% endif %}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/conditional-b.md",
        r#"---
dependencies:
  agents:
    - path: conditional-c.md
      install: false
agpm:
  templating: true
---
# Conditional Agent B
From C: {{ agpm.deps.agents.conditional_c.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/conditional-c.md",
        r#"---
dependencies:
  agents:
    - path: conditional-a.md
      install: false
agpm:
  templating: true
---
# Conditional Agent C
Back to A: {{ agpm.deps.agents.conditional_a.content }}
"#,
    )
    .await?;

    // Test with feature enabled (creates cycle) - build manifest manually to include template_vars
    let manifest = r#"[agents.conditional-a]
path = "agents/conditional-a.md"

[agents.conditional-b]
path = "agents/conditional-b.md"

[agents.conditional-c]
path = "agents/conditional-c.md"

[agents.conditional-a.template_vars.project]
feature_enabled = "true"
"#;

    project.write_manifest(manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail with conditional circular dependency");

    let err_lower = output.stderr.to_lowercase();
    assert!(
        err_lower.contains("circular") || err_lower.contains("cycle"),
        "Error should mention circular dependency: {}",
        output.stderr
    );

    Ok(())
}

/// Test mixed templating/non-templating circular dependency
///
/// Tests cycles where some resources have templating enabled and others don't.
#[tokio::test]
async fn test_mixed_templating_circular_dependency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Agent with templating → Agent without templating → Agent with templating
    write_file_with_dirs(
        &project,
        "agents/templated-a.md",
        r#"---
dependencies:
  agents:
    - path: literal-b.md
      install: false
agpm:
  templating: true
---
# Templated Agent A
From B: {{ agpm.deps.agents.literal_b.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/literal-b.md",
        r#"---
dependencies:
  agents:
    - path: templated-c.md
      install: false
agpm:
  templating: false
---
# Literal Agent B
This agent has templating: false
It depends on templated-c but won't render templates.
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/templated-c.md",
        r#"---
dependencies:
  agents:
    - path: templated-a.md
      install: false
agpm:
  templating: true
---
# Templated Agent C
Back to A: {{ agpm.deps.agents.templated_a.content }}
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("templated-a", "agents/templated-a.md")
        .add_local_agent("literal-b", "agents/literal-b.md")
        .add_local_agent("templated-c", "agents/templated-c.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail with circular dependency");
    let err_lower = output.stderr.to_lowercase();

    assert!(
        err_lower.contains("circular") || err_lower.contains("cycle"),
        "Error should mention circular dependency: {}",
        output.stderr
    );

    Ok(())
}

/// Test self-dependency (resource depends on itself)
///
/// Tests the edge case where a resource directly depends on itself.
#[tokio::test]
async fn test_self_dependency() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Agent that depends on itself
    write_file_with_dirs(
        &project,
        "agents/self-dep.md",
        r#"---
dependencies:
  agents:
    - path: self-dep.md
      install: false
agpm:
  templating: true
---
# Self-Dependent Agent
This should fail: {{ agpm.deps.agents.self_dep.content }}
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new().add_local_agent("self-dep", "agents/self-dep.md").build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail with self-dependency");
    let err_lower = output.stderr.to_lowercase();

    assert!(
        err_lower.contains("circular") || err_lower.contains("cycle") || err_lower.contains("self"),
        "Error should mention circular dependency or self-dependency: {}",
        output.stderr
    );

    Ok(())
}

/// Test complex diamond pattern with cycle
///
/// Tests a diamond dependency pattern that results in a cycle.
///     A
///    / \
///   B   C
///    \ /
///     D
///     |
///     A (creates cycle)
#[tokio::test]
async fn test_diamond_with_cycle() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create diamond with cycle
    write_file_with_dirs(
        &project,
        "agents/diamond-a.md",
        r#"---
dependencies:
  agents:
    - path: diamond-b.md
      install: false
    - path: diamond-c.md
      install: false
agpm:
  templating: true
---
# Diamond A
From B: {{ agpm.deps.agents.diamond_b.content }}
From C: {{ agpm.deps.agents.diamond_c.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/diamond-b.md",
        r#"---
dependencies:
  agents:
    - path: diamond-d.md
      install: false
agpm:
  templating: true
---
# Diamond B
From D: {{ agpm.deps.agents.diamond_d.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/diamond-c.md",
        r#"---
dependencies:
  agents:
    - path: diamond-d.md
      install: false
agpm:
  templating: true
---
# Diamond C
From D: {{ agpm.deps.agents.diamond_d.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/diamond-d.md",
        r#"---
dependencies:
  agents:
    - path: diamond-a.md
      install: false
agpm:
  templating: true
---
# Diamond D
Back to A (creates cycle): {{ agpm.deps.agents.diamond_a.content }}
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("diamond-a", "agents/diamond-a.md")
        .add_local_agent("diamond-b", "agents/diamond-b.md")
        .add_local_agent("diamond-c", "agents/diamond-c.md")
        .add_local_agent("diamond-d", "agents/diamond-d.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail with circular dependency");
    let err_lower = output.stderr.to_lowercase();

    assert!(
        err_lower.contains("circular") || err_lower.contains("cycle"),
        "Error should mention circular dependency: {}",
        output.stderr
    );

    Ok(())
}

/// Test circular dependency with custom names
///
/// Tests cycles where dependencies have custom names in the manifest.
#[tokio::test]
async fn test_circular_dependency_with_custom_names() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create cycle with custom names
    write_file_with_dirs(
        &project,
        "agents/custom-a.md",
        r#"---
dependencies:
  agents:
    - path: custom-b.md
      name: custom_b_alias
      install: false
agpm:
  templating: true
---
# Custom A
From B: {{ agpm.deps.agents.custom_b_alias.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/custom-b.md",
        r#"---
dependencies:
  agents:
    - path: custom-c.md
      name: custom_c_alias
      install: false
agpm:
  templating: true
---
# Custom B
From C: {{ agpm.deps.agents.custom_c_alias.content }}
"#,
    )
    .await?;

    write_file_with_dirs(
        &project,
        "agents/custom-c.md",
        r#"---
dependencies:
  agents:
    - path: custom-a.md
      name: custom_a_alias
      install: false
agpm:
  templating: true
---
# Custom C
Back to A: {{ agpm.deps.agents.custom_a_alias.content }}
"#,
    )
    .await?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_local_agent("custom-a", "agents/custom-a.md")
        .add_local_agent("custom-b", "agents/custom-b.md")
        .add_local_agent("custom-c", "agents/custom-c.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Install should fail
    let output = project.run_agpm(&["install"])?;

    assert!(!output.success, "Install should fail with circular dependency");
    let err_lower = output.stderr.to_lowercase();

    assert!(
        err_lower.contains("circular") || err_lower.contains("cycle"),
        "Error should mention circular dependency: {}",
        output.stderr
    );

    Ok(())
}
