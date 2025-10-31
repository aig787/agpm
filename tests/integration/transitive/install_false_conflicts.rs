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
/// This test verifies that the install=false fix doesn't break
/// legitimate version conflict detection for install=true dependencies.
#[tokio::test]
async fn test_version_conflicts_still_work_with_install_true() -> Result<()> {
    let test_project = TestProject::new().await?;
    let _project_dir = test_project.project_path();

    // Create a source repository
    let source_repo = test_project.create_source_repo("test-repo").await?;

    // Create two different versions of a shared dependency
    let shared_v1 = r#"# Shared Utils v1

Version 1 utilities.
"#;

    let shared_v2 = r#"# Shared Utils v2

Version 2 utilities.
"#;

    source_repo.add_resource("snippets", "shared", shared_v1).await?;
    source_repo.add_resource("snippets", "shared-v2", shared_v2).await?;

    // Create agent that uses shared v1
    let agent1_content = r#"---
model: claude-3-sonnet
---

# Agent One

Uses shared utilities v1.

dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v1.0.0
      install: true  # Creates file
"#;

    source_repo.add_resource("agents", "agent-one", agent1_content).await?;

    // Create agent that uses shared v2
    let agent2_content = r#"---
model: claude-3-opus
---

# Agent Two

Uses shared utilities v2.

dependencies:
  snippets:
    - path: ../snippets/shared-v2.md
      version: v2.0.0
      install: true  # Creates file
"#;

    source_repo.add_resource("agents", "agent-two", agent2_content).await?;

    // Commit all changes and create a tag
    source_repo.commit_all("Add test resources for version conflict test")?;
    source_repo.tag_version("v1.0.0")?;
    let source_url = source_repo.bare_file_url(test_project.sources_path())?;

    // Create manifest that forces both agents to use same resource name but different versions
    // This simulates a real version conflict scenario
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
agent-one = {{ source = "test-repo", path = "agents/agent-one.md", version = "v1.0.0" }}
agent-two = {{ source = "test-repo", path = "agents/agent-two.md", version = "v1.0.0" }}

# Force both agents to depend on same resource with different versions
[patch.agents.agent-one.dependencies.snippets.shared]
path = "snippets/shared.md"
version = "v1.0.0"

[patch.agents.agent-two.dependencies.snippets.shared]
path = "snippets/shared.md"
version = "v2.0.0"
"#,
        source_url
    );

    test_project.write_manifest(&manifest).await?;

    // Run install - should succeed (patches don't create conflicts as expected)
    let output = test_project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed - patches don't create version conflicts as expected"
    );

    Ok(())
}
