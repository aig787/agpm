// Integration tests for transitive dependency resolution
//
// Tests the resolver's ability to handle transitive dependencies declared
// within resource files via YAML frontmatter or JSON fields.

use anyhow::Result;

mod common;
use common::TestProject;

/// Test basic transitive dependency resolution with real Git repos
#[tokio::test]
async fn test_transitive_resolution_basic() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with a main agent that depends on a helper
    let community_repo = project.create_source_repo("community").await?;

    // Add helper agent first (no dependencies)
    community_repo
        .add_resource(
            "agents",
            "helper",
            r#"---
# Helper Agent
This is a helper agent with no dependencies.
---
"#,
        )
        .await?;

    // Add main agent that depends on helper
    community_repo
        .add_resource(
            "agents",
            "main-app",
            r#"---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
---

# Main App Agent
This agent depends on the helper agent.
"#,
        )
        .await?;

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest that only references the main agent (not the helper)
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let manifest_content = format!(
        r#"
[sources]
community = "{source_url}"

[agents]
main-app = {{ source = "community", path = "agents/main-app.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await?;

    // Run install
    project.run_ccpm(&["install"])?;

    // Verify both agents were installed (main + transitive helper)
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("main-app"),
        "Main agent should be in lockfile"
    );
    assert!(
        lockfile_content.contains("helper"),
        "Helper agent should be in lockfile (transitive)"
    );

    // Verify both were actually installed to .claude/agents
    let main_app_path = project.project_path().join(".claude/agents/main-app.md");
    let helper_path = project.project_path().join(".claude/agents/helper.md");
    assert!(
        tokio::fs::metadata(&main_app_path).await.is_ok(),
        "Main agent file should exist at {:?}",
        main_app_path
    );
    assert!(
        tokio::fs::metadata(&helper_path).await.is_ok(),
        "Helper agent file should exist at {:?}",
        helper_path
    );

    Ok(())
}

/// Test transitive dependencies with same-named resources from different sources
///
/// NOTE: This test is currently skipped because cross-source transitive dependencies
/// with the same name create path conflicts. The system correctly detects that
/// "utils" from source1 and "utils" from source2 would both install to the same path
/// but have different content (different commits).
///
/// TODO: Implement proper handling for cross-source transitive dependencies:
/// - Option 1: Qualify transitive dependency names by source (e.g., "source1__utils")
/// - Option 2: Use custom install paths for transitive deps from different sources
/// - Option 3: Detect and merge identical transitive deps, error on conflicts
#[tokio::test]
#[ignore = "Cross-source transitive dependencies with same names not yet supported"]
async fn test_transitive_cross_source_same_names() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create first source repo with a "utils" agent
    let source1_repo = project.create_source_repo("source1").await?;
    source1_repo
        .add_resource(
            "agents",
            "utils",
            "# Utils from Source 1\n\nSource 1 utilities",
        )
        .await?;
    source1_repo
        .add_resource(
            "agents",
            "app",
            r#"---
dependencies:
  agents:
    - path: agents/utils.md
      version: v1.0.0
---

# App from Source 1
Uses utils from same source
"#,
        )
        .await?;
    source1_repo.commit_all("Source 1 commit")?;
    source1_repo.tag_version("v1.0.0")?;

    // Create second source repo with different "utils" agent
    let source2_repo = project.create_source_repo("source2").await?;
    source2_repo
        .add_resource(
            "agents",
            "utils",
            "# Utils from Source 2\n\nSource 2 utilities (different)",
        )
        .await?;
    source2_repo
        .add_resource(
            "agents",
            "tool",
            r#"---
dependencies:
  agents:
    - path: agents/utils.md
      version: v1.0.0
---

# Tool from Source 2
Uses utils from same source
"#,
        )
        .await?;
    source2_repo.commit_all("Source 2 commit")?;
    source2_repo.tag_version("v1.0.0")?;

    // Create manifest referencing both top-level resources
    let source1_url = source1_repo.bare_file_url(project.sources_path())?;
    let source2_url = source2_repo.bare_file_url(project.sources_path())?;
    let manifest_content = format!(
        r#"
[sources]
source1 = "{source1_url}"
source2 = "{source2_url}"

[agents]
app = {{ source = "source1", path = "agents/app.md", version = "v1.0.0" }}
tool = {{ source = "source2", path = "agents/tool.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await?;

    // Run install - currently this fails with a path conflict error
    // because both "utils" transitive deps resolve to .claude/agents/utils.md
    // but have different commits (different sources)
    let output = project.run_ccpm(&["install"])?;

    // Expected behavior: Should detect path conflict
    assert!(
        !output.success,
        "Install should fail due to path conflict for cross-source same-named transitive deps"
    );
    assert!(
        output.stderr.contains("Target path conflicts"),
        "Should report path conflict, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that circular transitive dependencies are detected and rejected
#[tokio::test]
async fn test_transitive_cycle_detection() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with circular dependencies: A → B → C → A
    let repo = project.create_source_repo("community").await?;

    // Agent A depends on B
    repo.add_resource(
        "agents",
        "agent-a",
        r#"---
dependencies:
  agents:
    - path: agents/agent-b.md
      version: v1.0.0
---

# Agent A
Depends on Agent B
"#,
    )
    .await?;

    // Agent B depends on C
    repo.add_resource(
        "agents",
        "agent-b",
        r#"---
dependencies:
  agents:
    - path: agents/agent-c.md
      version: v1.0.0
---

# Agent B
Depends on Agent C
"#,
    )
    .await?;

    // Agent C depends on A (creates cycle)
    repo.add_resource(
        "agents",
        "agent-c",
        r#"---
dependencies:
  agents:
    - path: agents/agent-a.md
      version: v1.0.0
---

# Agent C
Depends on Agent A (creates cycle)
"#,
    )
    .await?;

    repo.commit_all("Add agents with circular dependencies")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest that references agent-a
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest_content = format!(
        r#"
[sources]
community = "{source_url}"

[agents]
agent-a = {{ source = "community", path = "agents/agent-a.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await?;

    // Run install - should fail with cycle detection
    let output = project.run_ccpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail due to circular dependency"
    );
    assert!(
        output.stderr.contains("Circular dependency") || output.stderr.contains("cycle"),
        "Error should mention circular dependency or cycle, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test diamond dependencies (same resource via multiple paths)
#[tokio::test]
async fn test_transitive_diamond_dependencies() -> Result<()> {
    ccpm::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with diamond pattern:
    //     A
    //    / \
    //   B   C
    //    \ /
    //     D
    let repo = project.create_source_repo("community").await?;

    // D - base dependency (no dependencies)
    repo.add_resource(
        "agents",
        "agent-d",
        r#"---
# Agent D
Base agent with no dependencies
---
"#,
    )
    .await?;

    // B depends on D
    repo.add_resource(
        "agents",
        "agent-b",
        r#"---
dependencies:
  agents:
    - path: agents/agent-d.md
      version: v1.0.0
---

# Agent B
Depends on Agent D
"#,
    )
    .await?;

    // C depends on D
    repo.add_resource(
        "agents",
        "agent-c",
        r#"---
dependencies:
  agents:
    - path: agents/agent-d.md
      version: v1.0.0
---

# Agent C
Depends on Agent D
"#,
    )
    .await?;

    // A depends on both B and C
    repo.add_resource(
        "agents",
        "agent-a",
        r#"---
dependencies:
  agents:
    - path: agents/agent-b.md
      version: v1.0.0
    - path: agents/agent-c.md
      version: v1.0.0
---

# Agent A
Depends on both Agent B and Agent C
"#,
    )
    .await?;

    repo.commit_all("Add agents with diamond dependency pattern")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest that references agent-a
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest_content = format!(
        r#"
[sources]
community = "{source_url}"

[agents]
agent-a = {{ source = "community", path = "agents/agent-a.md", version = "v1.0.0" }}
"#
    );

    project.write_manifest(&manifest_content).await?;

    // Run install - should succeed
    let output = project.run_ccpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed with diamond dependencies: {}",
        output.stderr
    );

    // Verify all agents are installed
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains("agent-a"),
        "Agent A should be in lockfile"
    );
    assert!(
        lockfile_content.contains("agent-b"),
        "Agent B should be in lockfile"
    );
    assert!(
        lockfile_content.contains("agent-c"),
        "Agent C should be in lockfile"
    );
    assert!(
        lockfile_content.contains("agent-d"),
        "Agent D should be in lockfile"
    );

    // Verify agent-d appears exactly once (no duplication despite two paths to it)
    let agent_d_count = lockfile_content.matches("name = \"agent-d\"").count();
    assert_eq!(
        agent_d_count, 1,
        "Agent D should appear exactly once in lockfile (deduplication), found {}",
        agent_d_count
    );

    Ok(())
}
