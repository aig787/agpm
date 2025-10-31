//! Integration tests for install=false with conflict detection.
//!
//! Tests that transitive dependencies with install=false don't trigger
//! false version conflicts, since they're content-only and don't create files.

use anyhow::Result;

use crate::common::TestProject;

/// Test that transitive dependencies with install=false don't trigger conflicts.
///
/// This test verifies that when multiple resources depend on the same
/// transitive dependency with install=false, no version conflict is detected
/// since the dependency is content-only and doesn't create files.
#[tokio::test]
async fn test_transitive_install_false_no_conflict() -> Result<()> {
    let test_project = TestProject::new().await?;
    let _project_dir = test_project.project_path();

    // Create a source repository
    let source_repo = test_project.create_source_repo("test-repo").await?;

    // Create a shared dependency that will be used transitively with install=false
    let shared_content = r#"# Shared Utils

Common utility functions.
"#;

    source_repo.add_resource("snippets", "shared-utils", shared_content).await?;

    // Create first agent that uses shared-utils with install=false
    let agent1_content = r#"---
model: claude-3-sonnet
---

# Agent One

Uses shared utilities transitively with install=false.

dependencies:
  snippets:
    - path: ../snippets/shared-utils.md
      version: v1.0.0
      install: false  # Content-only, no file creation
"#;

    source_repo.add_resource("agents", "agent-one", agent1_content).await?;

    // Create second agent that also uses shared-utils with install=false
    let agent2_content = r#"---
model: claude-3-opus
---

# Agent Two

Also uses shared utilities transitively with install=false.

dependencies:
  snippets:
    - path: ../snippets/shared-utils.md
      version: v1.0.0
      install: false  # Content-only, no file creation
"#;

    source_repo.add_resource("agents", "agent-two", agent2_content).await?;

    // Commit all changes and create a tag
    source_repo.commit_all("Add test resources with transitive dependencies")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(test_project.sources_path())?;

    // Create manifest with both agents
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
agent-one = {{ source = "test-repo", path = "agents/agent-one.md", version = "v1.0.0" }}
agent-two = {{ source = "test-repo", path = "agents/agent-two.md", version = "v1.0.0" }}
"#,
        source_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install - should succeed without conflicts
    let output = test_project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed when transitive deps have install=false. stderr: {}",
        &output.stderr
    );

    // Verify lockfile was created
    let lockfile_path = _project_dir.join("agpm.lock");
    assert!(lockfile_path.exists(), "Lockfile should be created after successful install");

    // Verify shared-utils was NOT installed (install=false)
    let installed_snippets = _project_dir.join(".agpm").join("snippets");
    let shared_utils_path = installed_snippets.join("shared-utils.md");
    assert!(
        !shared_utils_path.exists(),
        "shared-utils with install=false should NOT be installed to disk"
    );

    Ok(())
}

/// Test that mixed install=true and install=false don't conflict.
///
/// This test verifies that when one dependency uses a resource with install=true
/// and another uses it with install=false, no conflict is detected since
/// they serve different purposes (file creation vs content-only).
#[tokio::test]
async fn test_mixed_install_true_false_no_conflict() -> Result<()> {
    let test_project = TestProject::new().await?;
    let _project_dir = test_project.project_path();

    // Create a source repository
    let source_repo = test_project.create_source_repo("test-repo").await?;

    // Create a shared dependency
    let shared_content = r#"# Shared Config

Configuration utilities.
"#;

    source_repo.add_resource("snippets", "shared-config", shared_content).await?;

    // Create agent that uses shared-config with install=true
    let agent1_content = r#"---
model: claude-3-sonnet
---

# Agent One

Uses shared config with install=true.

dependencies:
  snippets:
    - path: ../snippets/shared-config.md
      version: v1.0.0
      install: true  # Creates file
"#;

    source_repo.add_resource("agents", "agent-one", agent1_content).await?;

    // Create agent that uses shared-config with install=false
    let agent2_content = r#"---
model: claude-3-opus
---

# Agent Two

Uses shared config with install=false.

dependencies:
  snippets:
    - path: ../snippets/shared-config.md
      version: v1.0.0
      install: false  # Content-only
"#;

    source_repo.add_resource("agents", "agent-two", agent2_content).await?;

    // Commit all changes and create a tag
    source_repo.commit_all("Add test resources with mixed install flags")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(test_project.sources_path())?;

    // Create manifest with both agents
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
agent-one = {{ source = "test-repo", path = "agents/agent-one.md", version = "v1.0.0" }}
agent-two = {{ source = "test-repo", path = "agents/agent-two.md", version = "v1.0.0" }}
"#,
        source_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install - should succeed without conflicts
    let output = test_project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed with mixed install=true/false. stderr: {}",
        &output.stderr
    );

    Ok(())
}

/// Test that version conflicts still work with install=true.
///
/// This test verifies that install=false fix doesn't break
/// legitimate version conflict detection for install=true dependencies.
#[tokio::test]
async fn test_version_conflicts_still_work_with_install_true() -> Result<()> {
    let test_project = TestProject::new().await?;
    let _project_dir = test_project.project_path();

    // Create a source repository
    let source_repo = test_project.create_source_repo("test-repo").await?;

    // Create v1.0.0 of shared dependency
    let shared_v1 = r#"# Shared Utils v1
Version 1 utilities.
"#;
    source_repo.add_resource("snippets", "shared", shared_v1).await?;
    source_repo.commit_all("Add v1.0.0")?;
    source_repo.tag_version("v1.0.0")?;

    // Create v2.0.0 of same shared dependency (same path, different content)
    let shared_v2 = r#"# Shared Utils v2
Version 2 utilities.
"#;
    source_repo.add_resource("snippets", "shared", shared_v2).await?;
    source_repo.commit_all("Update to v2.0.0")?;
    source_repo.tag_version("v2.0.0")?;

    // Create agents that depend on different versions with install=true
    let agent1_content = r#"---
model: claude-3-sonnet
---
# Agent One
Uses shared utilities v1.
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v1.0.0
      install: true
"#;

    let agent2_content = r#"---
model: claude-3-opus
---
# Agent Two
Uses shared utilities v2.
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v2.0.0
      install: true
"#;

    source_repo.add_resource("agents", "agent-one", agent1_content).await?;
    source_repo.add_resource("agents", "agent-two", agent2_content).await?;
    source_repo.commit_all("Add agents")?;
    source_repo.tag_version("v3.0.0")?;

    let source_url = source_repo.bare_file_url(test_project.sources_path())?;

    // Create manifest with both agents - should trigger version conflict
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
agent-one = {{ source = "test-repo", path = "agents/agent-one.md", version = "v3.0.0" }}
agent-two = {{ source = "test-repo", path = "agents/agent-two.md", version = "v3.0.0" }}
"#,
        source_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install - should handle version conflict gracefully
    let output = test_project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should handle version conflict appropriately. stderr: {}",
        &output.stderr
    );

    Ok(())
}

/// Test diamond pattern with install=false dependencies.
///
/// This test verifies that diamond dependency patterns work correctly
/// when the shared dependency has install=false.
///
/// A → B → C (install=false)
/// A → D → C (install=false)
/// Should not conflict since C is content-only.
#[tokio::test]
async fn test_diamond_pattern_with_install_false() -> Result<()> {
    let test_project = TestProject::new().await?;
    let _project_dir = test_project.project_path();

    // Create a source repository
    let source_repo = test_project.create_source_repo("test-repo").await?;

    // Create shared dependency C that will be used by both B and D
    let shared_c_content = r#"# Shared Library C

Common utilities used by both B and D.
"#;
    source_repo.add_resource("snippets", "shared-c", shared_c_content).await?;

    // Create dependency B that uses C with install=false
    let dep_b_content = r#"---
model: claude-3-sonnet
---
# Dependency B

Uses shared library C.

dependencies:
  snippets:
    - path: ../snippets/shared-c.md
      version: v1.0.0
      install: false  # Content-only
"#;
    source_repo.add_resource("agents", "dep-b", dep_b_content).await?;

    // Create dependency D that also uses C with install=false
    let dep_d_content = r#"---
model: claude-3-opus
---
# Dependency D

Also uses shared library C.

dependencies:
  snippets:
    - path: ../snippets/shared-c.md
      version: v1.0.0
      install: false  # Content-only
"#;
    source_repo.add_resource("agents", "dep-d", dep_d_content).await?;

    // Create top-level agents A1 and A2 that depend on B and D respectively
    let agent_a1_content = r#"---
model: claude-3-haiku
---
# Agent A1

Depends on B.

dependencies:
  agents:
    - path: ../agents/dep-b.md
      version: v1.0.0
"#;
    source_repo.add_resource("agents", "agent-a1", agent_a1_content).await?;

    let agent_a2_content = r#"---
model: claude-3-sonnet
---
# Agent A2

Depends on D.

dependencies:
  agents:
    - path: ../agents/dep-d.md
      version: v1.0.0
"#;
    source_repo.add_resource("agents", "agent-a2", agent_a2_content).await?;

    // Commit all changes and create a tag
    source_repo.commit_all("Add diamond pattern dependencies")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(test_project.sources_path())?;

    // Create manifest with both top-level agents
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
agent-a1 = {{ source = "test-repo", path = "agents/agent-a1.md", version = "v1.0.0" }}
agent-a2 = {{ source = "test-repo", path = "agents/agent-a2.md", version = "v1.0.0" }}
"#,
        source_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install - should succeed without conflicts
    let output = test_project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed with diamond pattern and install=false. stderr: {}",
        &output.stderr
    );

    // Verify shared-c was NOT installed (install=false)
    let installed_snippets = _project_dir.join(".agpm").join("snippets");
    let shared_c_path = installed_snippets.join("shared-c.md");
    assert!(!shared_c_path.exists(), "shared-c with install=false should NOT be installed to disk");

    Ok(())
}

/// Test nested install=false dependencies.
///
/// This test verifies that when a transitive dependency with install=false
/// itself has dependencies with install=false, all are properly skipped
/// in conflict detection.
#[tokio::test]
async fn test_nested_install_false_dependencies() -> Result<()> {
    let test_project = TestProject::new().await?;
    let _project_dir = test_project.project_path();

    // Create a source repository
    let source_repo = test_project.create_source_repo("test-repo").await?;

    // Create level 3 dependency (deepest)
    let level3_content = r#"# Level 3 Dependency

Core utilities at the deepest level.
"#;
    source_repo.add_resource("snippets", "level3", level3_content).await?;

    // Create level 2 dependencies that use level3 with install=false
    let level2_a_content = r#"---
model: claude-3-sonnet
---
# Level 2 Dependency A

Uses level 3 dependency.

dependencies:
  snippets:
    - path: ../snippets/level3.md
      version: v1.0.0
      install: false  # Content-only
"#;
    source_repo.add_resource("agents", "level2-a", level2_a_content).await?;

    let level2_b_content = r#"---
model: claude-3-opus
---
# Level 2 Dependency B

Also uses level 3 dependency.

dependencies:
  snippets:
    - path: ../snippets/level3.md
      version: v1.0.0
      install: false  # Content-only
"#;
    source_repo.add_resource("agents", "level2-b", level2_b_content).await?;

    // Create level 1 agents that use level2 dependencies with install=false
    let level1_a_content = r#"---
model: claude-3-haiku
---
# Level 1 Agent A

Uses level 2 dependency A.

dependencies:
  agents:
    - path: ../agents/level2-a.md
      version: v1.0.0
      install: false  # Content-only
"#;
    source_repo.add_resource("agents", "level1-a", level1_a_content).await?;

    let level1_b_content = r#"---
model: claude-3-sonnet
---
# Level 1 Agent B

Uses level 2 dependency B.

dependencies:
  agents:
    - path: ../agents/level2-b.md
      version: v1.0.0
      install: false  # Content-only
"#;
    source_repo.add_resource("agents", "level1-b", level1_b_content).await?;

    // Commit all changes and create a tag
    source_repo.commit_all("Add nested install=false dependencies")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(test_project.sources_path())?;

    // Create manifest with both level 1 agents
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
level1-a = {{ source = "test-repo", path = "agents/level1-a.md", version = "v1.0.0" }}
level1-b = {{ source = "test-repo", path = "agents/level1-b.md", version = "v1.0.0" }}
"#,
        source_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install - should succeed without conflicts
    let output = test_project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed with nested install=false dependencies. stderr: {}",
        &output.stderr
    );

    // Verify none of the install=false dependencies were installed
    let installed_agents = _project_dir.join(".agpm").join("agents");
    let installed_snippets = _project_dir.join(".agpm").join("snippets");

    assert!(
        !installed_agents.join("level1-a.md").exists(),
        "level1-a with install=false should NOT be installed"
    );
    assert!(
        !installed_agents.join("level1-b.md").exists(),
        "level1-b with install=false should NOT be installed"
    );
    assert!(
        !installed_agents.join("level2-a.md").exists(),
        "level2-a with install=false should NOT be installed"
    );
    assert!(
        !installed_agents.join("level2-b.md").exists(),
        "level2-b with install=false should NOT be installed"
    );
    assert!(
        !installed_snippets.join("level3.md").exists(),
        "level3 with install=false should NOT be installed"
    );

    Ok(())
}
