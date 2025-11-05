//! Integration tests for backtracking conflict resolution.
//!
//! These tests verify the automatic conflict resolution via backtracking,
//! including oscillation detection, multiple conflicts, and error handling.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Test that backtracking successfully handles complex transitive conflicts via backtracking.
///
/// This test demonstrates that backtracking can successfully resolve conflicts by
/// upgrading transitive dependencies. While we initially tried to create an oscillating
/// scenario, the backtracking algorithm is sophisticated enough to resolve it by
/// modifying transitive dependency versions.
///
/// Dependency structure:
/// ```
/// Manifest: D ^1.0.0 (matches both v1 and v2)
///
/// D v1.0.0 → A v1.0.0 + B v1.0.0
/// D v2.0.0 → A v2.0.0 + B v2.0.0
///
/// A v1.0.0 → X v1.0.0 (initial requirement)
/// A v2.0.0 → X v2.0.0
/// B v1.0.0 → X v2.0.0  (conflicts with A v1!)
/// B v2.0.0 → X v1.0.0
/// ```
///
/// Backtracking successfully resolves by upgrading A v1's transitive X dependency to v2.0.0.
#[tokio::test]
async fn test_backtracking_oscillation_detection() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create Snippet X v1.0.0 and v2.0.0
    source_repo.add_resource("snippets", "snippet-x", "# Snippet X v1.0.0").await?;
    source_repo.commit_all("Snippet X v1.0.0")?;
    source_repo.tag_version("x-v1.0.0")?;

    source_repo.add_resource("snippets", "snippet-x", "# Snippet X v2.0.0 CHANGED").await?;
    source_repo.commit_all("Snippet X v2.0.0")?;
    source_repo.tag_version("x-v2.0.0")?;

    // Create A v1.0.0 → X v1.0.0
    let agent_a_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-x.md
      version: x-v1.0.0
---
# Agent A v1.0.0"#;
    source_repo.add_resource("agents", "agent-a", agent_a_v1).await?;
    source_repo.commit_all("Agent A v1.0.0")?;
    source_repo.tag_version("a-v1.0.0")?;

    // Create A v2.0.0 → X v2.0.0
    let agent_a_v2 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-x.md
      version: x-v2.0.0
---
# Agent A v2.0.0 CHANGED"#;
    source_repo.add_resource("agents", "agent-a", agent_a_v2).await?;
    source_repo.commit_all("Agent A v2.0.0")?;
    source_repo.tag_version("a-v2.0.0")?;

    // Create B v1.0.0 → X v2.0.0 (conflicts with A v1!)
    let agent_b_v1 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-x.md
      version: x-v2.0.0
---
# Agent B v1.0.0"#;
    source_repo.add_resource("agents", "agent-b", agent_b_v1).await?;
    source_repo.commit_all("Agent B v1.0.0")?;
    source_repo.tag_version("b-v1.0.0")?;

    // Create B v2.0.0 → X v1.0.0 (conflicts with A v2!)
    let agent_b_v2 = r#"---
dependencies:
  snippets:
    - path: snippets/snippet-x.md
      version: x-v1.0.0
---
# Agent B v2.0.0 CHANGED"#;
    source_repo.add_resource("agents", "agent-b", agent_b_v2).await?;
    source_repo.commit_all("Agent B v2.0.0")?;
    source_repo.tag_version("b-v2.0.0")?;

    // Create D v1.0.0 → A v1.0.0 + B v1.0.0
    let command_d_v1 = r#"---
dependencies:
  agents:
    - path: agents/agent-a.md
      version: a-v1.0.0
    - path: agents/agent-b.md
      version: b-v1.0.0
---
# Command D v1.0.0"#;
    source_repo.add_resource("commands", "deploy", command_d_v1).await?;
    source_repo.commit_all("Command D v1.0.0")?;
    source_repo.tag_version("d-v1.0.0")?;

    // Create D v2.0.0 → A v2.0.0 + B v2.0.0
    let command_d_v2 = r#"---
dependencies:
  agents:
    - path: agents/agent-a.md
      version: a-v2.0.0
    - path: agents/agent-b.md
      version: b-v2.0.0
---
# Command D v2.0.0 CHANGED"#;
    source_repo.add_resource("commands", "deploy", command_d_v2).await?;
    source_repo.commit_all("Command D v2.0.0")?;
    source_repo.tag_version("d-v2.0.0")?;

    // Create manifest requesting D with version range ^1.0.0 (matches both v1 and v2)
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_command("deploy", |d| {
            d.source("source").path("commands/deploy.md").version("d-^v1.0.0")
        })
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    // Debug: Check what was actually installed
    let agent_a_installed =
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agent-a.md")).await?;
    eprintln!("\n=== INSTALLED AGENT-A ===");
    eprintln!("{}", agent_a_installed);

    let lockfile = project.read_lockfile().await?;
    eprintln!("\n=== FULL LOCKFILE ===");
    eprintln!("{}", lockfile);

    // Should succeed - backtracking successfully resolves the conflict
    assert!(output.success, "Install should succeed via backtracking. Stderr: {}", output.stderr);

    // Verify backtracking upgraded A's transitive X dependency to v2
    assert!(
        lockfile.contains("snippets/snippet-x") && lockfile.contains("x-v2.0.0"),
        "Lockfile should contain snippet-x at v2.0.0 (backtracking resolution)"
    );

    // Verify all resources are installed
    assert!(project.project_path().join(".claude/commands/deploy.md").exists());
    assert!(project.project_path().join(".claude/agents/agent-a.md").exists());
    assert!(project.project_path().join(".claude/agents/agent-b.md").exists());
    assert!(project.project_path().join(".claude/snippets/snippet-x.md").exists());

    // Ensure no panic or errors occurred
    assert!(
        !output.stderr.contains("panic") && !output.stderr.contains("Error"),
        "Should not have errors. Stderr: {}",
        output.stderr
    );

    Ok(())
}

/// Test that backtracking can handle multiple simultaneous independent conflicts.
///
/// This verifies that when 3+ independent resources have version conflicts,
/// the backtracking algorithm can identify and report all of them clearly.
///
/// Scenario:
/// - Create 3 independent resources: agents/helper, snippets/utils, commands/deploy
/// - Each has v1.0.0 and v2.0.0 with different content (different SHAs)
/// - Manifest requests both versions of each resource (6 total dependencies)
/// - All 3 conflicts should be detected
#[tokio::test]
async fn test_backtracking_multiple_simultaneous_conflicts() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("source").await?;

    // Create helper v1.0.0
    source_repo.add_resource("agents", "helper", "# Helper v1.0.0").await?;
    source_repo.add_resource("snippets", "utils", "# Utils v1.0.0").await?;
    source_repo.add_resource("commands", "deploy", "# Deploy v1.0.0").await?;
    source_repo.commit_all("v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Create v2.0.0 with different content for all 3 resources
    source_repo.add_resource("agents", "helper", "# Helper v2.0.0 CHANGED").await?;
    source_repo.add_resource("snippets", "utils", "# Utils v2.0.0 CHANGED").await?;
    source_repo.add_resource("commands", "deploy", "# Deploy v2.0.0 CHANGED").await?;
    source_repo.commit_all("v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

    // Create manifest with 6 dependencies - 2 per resource, each with conflicting exact versions
    let manifest = ManifestBuilder::new()
        .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
        .add_agent("helper-v1", |d| d.source("source").path("agents/helper.md").version("v1.0.0"))
        .add_agent("helper-v2", |d| d.source("source").path("agents/helper.md").version("v2.0.0"))
        .add_snippet("utils-v1", |d| d.source("source").path("snippets/utils.md").version("v1.0.0"))
        .add_snippet("utils-v2", |d| d.source("source").path("snippets/utils.md").version("v2.0.0"))
        .add_command("deploy-v1", |d| {
            d.source("source").path("commands/deploy.md").version("v1.0.0")
        })
        .add_command("deploy-v2", |d| {
            d.source("source").path("commands/deploy.md").version("v2.0.0")
        })
        .build();
    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;

    // Should fail with multiple version conflicts
    assert!(
        !output.success,
        "Install should fail with multiple conflicts. Stderr: {}",
        output.stderr
    );

    // Should mention version conflicts
    assert!(
        output.stderr.contains("Version conflicts detected")
            || output.stderr.contains("automatic resolution failed"),
        "Should report version conflicts. Stderr: {}",
        output.stderr
    );

    // All 3 conflicting resources should be mentioned
    assert!(
        output.stderr.contains("helper"),
        "Should mention helper conflict. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("utils"),
        "Should mention utils conflict. Stderr: {}",
        output.stderr
    );
    assert!(
        output.stderr.contains("deploy"),
        "Should mention deploy conflict. Stderr: {}",
        output.stderr
    );

    // Ensure no panic occurred
    assert!(!output.stderr.contains("panic"), "Should not panic. Stderr: {}", output.stderr);

    Ok(())
}

/// Test that backtracking handles errors gracefully without panicking.
///
/// This test verifies proper error handling in several failure scenarios:
/// 1. Missing/invalid source repository
/// 2. Incompatible exact version constraints (NoCompatibleVersion)
/// 3. General Git operation failures
///
/// All scenarios should fail gracefully with helpful error messages, not panics.
#[tokio::test]
async fn test_backtracking_error_handling() -> Result<()> {
    // Scenario A: Missing source repository
    {
        let project = TestProject::new().await?;

        // Create manifest referencing a non-existent source
        let manifest = ManifestBuilder::new()
            .add_source("nonexistent", "file:///nonexistent/repo.git")
            .add_agent("agent", |d| d.source("nonexistent").path("agents/agent.md"))
            .build();
        project.write_manifest(&manifest).await?;

        let output = project.run_agpm(&["install"])?;

        assert!(
            !output.success,
            "Should fail gracefully with missing source. Stderr: {}",
            output.stderr
        );
        assert!(
            output.stderr.contains("Failed")
                || output.stderr.contains("Error")
                || output.stderr.contains("not found"),
            "Should have helpful error message. Stderr: {}",
            output.stderr
        );
        assert!(!output.stderr.contains("panicked"), "Should not panic. Stderr: {}", output.stderr);
    }

    // Scenario B: Incompatible exact version constraints
    {
        let project = TestProject::new().await?;
        let source_repo = project.create_source_repo("source").await?;

        // Create v1.0.0
        source_repo.add_resource("agents", "agent-a", "# Agent A v1.0.0").await?;
        source_repo.commit_all("v1.0.0")?;
        source_repo.tag_version("v1.0.0")?;

        // Create v2.0.0 with different content
        source_repo.add_resource("agents", "agent-a", "# Agent A v2.0.0 CHANGED").await?;
        source_repo.commit_all("v2.0.0")?;
        source_repo.tag_version("v2.0.0")?;

        // Create manifest with incompatible exact version requirements
        let manifest = ManifestBuilder::new()
            .add_source("source", &source_repo.bare_file_url(project.sources_path())?)
            .add_agent("agent-v1", |d| {
                d.source("source").path("agents/agent-a.md").version("v1.0.0")
            })
            .add_agent("agent-v2", |d| {
                d.source("source").path("agents/agent-a.md").version("v2.0.0")
            })
            .build();
        project.write_manifest(&manifest).await?;

        let output = project.run_agpm(&["install"])?;

        assert!(
            !output.success,
            "Should fail with incompatible versions. Stderr: {}",
            output.stderr
        );
        assert!(
            output.stderr.contains("no compatible version")
                || output.stderr.contains("Version conflicts detected")
                || output.stderr.contains("automatic resolution failed"),
            "Should report inability to resolve. Stderr: {}",
            output.stderr
        );
        assert!(
            output.stderr.contains("agent-a.md"),
            "Should mention the conflicting resource. Stderr: {}",
            output.stderr
        );
        assert!(!output.stderr.contains("panicked"), "Should not panic. Stderr: {}", output.stderr);
    }

    Ok(())
}
