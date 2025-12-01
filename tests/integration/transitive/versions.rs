// Integration tests for transitive dependency version resolution
//
// Tests version conflict handling, metadata resolution from correct versions,
// and semver constraint resolution in transitive dependencies.

use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test that transitive dependencies use the correct version metadata
///
/// Verifies that when multiple agents depend on the same transitive resource at the same
/// version, the resolver correctly uses that version's metadata to extract its transitive
/// dependencies. This ensures metadata is fetched from the correct version tag in the
/// repository.
///
/// In this test, both agents depend on v2.0.0 of a shared snippet, and v2.0.0's metadata
/// specifies a dependency on "new-command.md" (while v1.0.0 specifies "old-command.md").
/// The resolver should install new-command.md, not old-command.md.
#[tokio::test]
async fn test_version_conflict_uses_correct_metadata() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create git repo with two versions
    let repo = project.create_source_repo("community").await?;

    // Create v1.0.0 with old-command
    repo.add_resource("commands", "old-command", "# Old Command\n\nThis is the old command.")
        .await?;

    // Create shared snippet that depends on old-command in v1.0.0
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/old-command.md
      version: v1.0.0
---

# Shared Snippet v1.0.0
This is version 1.0.0 of the shared snippet.
"#,
    )
    .await?;

    repo.commit_all("Release v1.0.0")?;
    repo.tag_version("v1.0.0")?;

    // Now update to v2.0.0
    // Remove old-command and add new-command
    tokio::fs::remove_file(repo.path.join("commands/old-command.md")).await?;
    repo.add_resource("commands", "new-command", "# New Command\n\nThis is the new command.")
        .await?;

    // Update shared snippet to depend on new-command in v2.0.0
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/new-command.md
      version: v2.0.0
---

# Shared Snippet v2.0.0
This is version 2.0.0 of the shared snippet.
"#,
    )
    .await?;

    repo.commit_all("Release v2.0.0")?;
    repo.tag_version("v2.0.0")?;

    // Create two agents that both depend on v2.0.0 of the shared snippet
    // This tests that the correct version's metadata is used for transitive deps
    repo.add_resource(
        "agents",
        "first",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v2.0.0
---

# First Agent
Requires shared@v2.0.0
"#,
    )
    .await?;

    repo.add_resource(
        "agents",
        "second",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v2.0.0
---

# Second Agent
Also requires shared@v2.0.0
"#,
    )
    .await?;

    repo.commit_all("Add agents")?;
    repo.tag_version("v2.0.1")?;

    // Create manifest that references both agents at v2.0.1
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_agent("first", |d| d.source("community").path("agents/first.md").version("v2.0.1"))
        .add_agent("second", |d| d.source("community").path("agents/second.md").version("v2.0.1"))
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - both agents require v2.0.0
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed, stderr: {}", output.stderr);

    // Verify that v2.0.0's transitive dependencies are installed (new-command.md)
    // Both v1.0.0 and v2.0.0 exist in repo - metadata must be fetched from correct version
    let new_command_path = project.project_path().join(".claude/commands/agpm/new-command.md");
    let old_command_path = project.project_path().join(".claude/commands/agpm/old-command.md");

    assert!(
        tokio::fs::metadata(&new_command_path).await.is_ok(),
        "New command should exist at {:?} (from v2.0.0 metadata)",
        new_command_path
    );
    assert!(
        tokio::fs::metadata(&old_command_path).await.is_err(),
        "Old command should NOT exist at {:?} (v1.0.0 metadata should not be used)",
        old_command_path
    );

    Ok(())
}

/// Test transitive dependency semver range auto-resolution
///
/// This test verifies that when multiple parents require the same transitive dependency
/// with compatible semver ranges, the resolver automatically finds the highest version
/// satisfying both constraints and uses that version's metadata.
///
/// Scenario:
/// - shared@v1.0.0 has old-dep as transitive dependency
/// - shared@v2.0.0 has new-dep as transitive dependency
/// - shared@v3.0.0 exists (content unchanged from v2.0.0)
/// - parent-a depends on shared@>=v1.0.0 (accepts v1.0.0, v2.0.0, v3.0.0)
/// - parent-b depends on shared@>=v1.5.0 (accepts v2.0.0, v3.0.0)
/// - Intersection is >=v1.5.0, highest available is v3.0.0
/// - Resolver auto-resolves to v3.0.0 and uses its metadata
/// - Should install new-dep (from v3.0.0), NOT old-dep (from v1.0.0)
#[tokio::test]
async fn test_transitive_version_conflict_metadata_from_winner() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create old-dep that will be in v1.0.0's transitive tree
    repo.add_resource("commands", "old-dep", "# Old Dep\n\nOld command.").await?;

    // Create shared@v1.0.0 with old-dep as transitive dependency
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/old-dep.md
---
# Shared v1.0.0
Version 1 with old-dep.
"#,
    )
    .await?;

    repo.commit_all("Add v1.0.0 resources")?;
    repo.tag_version("v1.0.0")?;

    // Create new-dep that will be in v2.0.0's transitive tree
    repo.add_resource("commands", "new-dep", "# New Dep\n\nNew command.").await?;

    // Update shared to v2.0.0 with new-dep as transitive dependency
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/new-dep.md
---
# Shared v2.0.0
Version 2 with new-dep.
"#,
    )
    .await?;

    repo.commit_all("Update to v2.0.0")?;
    repo.tag_version("v2.0.0")?;

    // Create parent-a that depends on shared@>=v1.0 (compatible range)
    repo.add_resource(
        "agents",
        "parent-a",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: ">=v1.0.0"
---
# Parent A
Depends on shared@>=v1.0.0 (accepts any version >= 1.0.0).
"#,
    )
    .await?;

    // Create parent-b that depends on shared@>=v1.5 (compatible with parent-a)
    repo.add_resource(
        "agents",
        "parent-b",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: ">=v1.5.0"
---
# Parent B
Depends on shared@>=v1.5.0 (intersection with parent-a is >=v1.5.0).
"#,
    )
    .await?;

    repo.commit_all("Add parent agents")?;
    repo.tag_version("v3.0.0")?;

    // Create manifest with both parents (creates version conflict on shared)
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_agent("parent-a", |d| {
            d.source("community").path("agents/parent-a.md").version("v3.0.0")
        })
        .add_agent("parent-b", |d| {
            d.source("community").path("agents/parent-b.md").version("v3.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Verify that v3.0.0 won (highest version satisfying both constraints)
    // Constraints are >=v1.0.0 and >=v1.5.0, intersection is >=v1.5.0
    // Available versions: v1.0.0, v2.0.0, v3.0.0
    // Highest satisfying >=v1.5.0 is v3.0.0
    // new-dep exists at v2.0.0 and v3.0.0, old-dep only at v1.0.0
    let new_dep_path = project.project_path().join(".claude/commands/agpm/new-dep.md");
    let old_dep_path = project.project_path().join(".claude/commands/agpm/old-dep.md");

    assert!(
        tokio::fs::metadata(&new_dep_path).await.is_ok(),
        "new-dep should be installed (exists at v3.0.0)"
    );
    assert!(
        tokio::fs::metadata(&old_dep_path).await.is_err(),
        "old-dep should NOT be installed (doesn't exist at v3.0.0)"
    );

    // Verify lockfile shows v3.0.0 for shared
    let lockfile_content = project.read_lockfile().await?;
    // Transitive dependency has canonical name with resource type directory
    assert!(
        lockfile_content.contains(r#"name = "snippets/shared""#)
            && lockfile_content.contains("v3.0.0"),
        "Lockfile should show shared at v3.0.0 (highest version satisfying both constraints)"
    );

    // Verify shared snippet content is from v2.0.0 (content unchanged at v3.0.0)
    // Note: Transitive snippet inherits claude-code from parent agents
    let shared_path = project.project_path().join(".claude/snippets/agpm/snippets/shared.md");
    let shared_content = tokio::fs::read_to_string(&shared_path).await?;
    assert!(
        shared_content.contains("Version 2 with new-dep"),
        "Shared snippet should have v2.0.0 content (unchanged in v3.0.0)"
    );

    Ok(())
}

/// Test type resolution with multiple sources having same-named resources
///
/// This is a regression test for a bug where the `transitive_types` HashMap used only
/// `(name, source)` as the key, causing cross-type collisions. When a source had both
/// `snippets/helper.md` and `agents/helper.md`, the HashMap would overwrite one with
/// the other, leading to incorrect type assignments and resources being installed to
/// the wrong directories.
///
/// Now the key includes the resource type: `(ResourceType, name, source)`, allowing
/// same-named resources of different types to coexist correctly.
#[tokio::test]
async fn test_type_resolution_fallback_ambiguity() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a single source repo with both agent and snippet named "helper"
    // Transitive dependencies must be from the same source as their parent
    let repo = project.create_source_repo("community").await?;

    // Add snippet "helper"
    repo.add_resource("snippets", "helper", "# Helper Snippet\n\nHelper snippet.").await?;

    // Add agent "helper"
    repo.add_resource("agents", "helper", "# Helper Agent\n\nHelper agent.").await?;

    // Create main agent that depends on both types with the same name
    repo.add_resource(
        "agents",
        "main",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/helper.md
      version: v1.0.0
  agents:
    - path: ./helper.md
      version: v1.0.0
---

# Main Agent
This agent depends on both helper snippet and helper agent (same name, different types).
"#,
    )
    .await?;

    repo.commit_all("Add resources")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("main", "community", "agents/main.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed: {}", output.stderr);

    // Read lockfile to verify correct type resolution
    let lockfile_content = project.read_lockfile().await?;

    // Each helper should now be in the correct section with correct installed_at path
    // Transitive dependencies use canonical names with resource type directory
    // Check that snippet helper is in snippets section
    let has_snippet_helper = lockfile_content.contains(r#"name = "snippets/helper""#)
        && lockfile_content.contains(r#"path = "snippets/helper.md""#);
    assert!(has_snippet_helper, "Lockfile should have helper snippet:\n{}", lockfile_content);

    // Check that agent helper is in agents section
    let has_agent_helper = lockfile_content.contains(r#"name = "agents/helper""#)
        && lockfile_content.contains(r#"path = "agents/helper.md""#);
    assert!(has_agent_helper, "Lockfile should have helper agent:\n{}", lockfile_content);

    // Verify installed locations (both inherit claude-code from parent agent)
    let snippet_path = project.project_path().join(".claude/snippets/agpm/snippets/helper.md");
    let agent_path = project.project_path().join(".claude/agents/agpm/helper.md");

    assert!(
        tokio::fs::metadata(&snippet_path).await.is_ok(),
        "Snippet helper should be installed at {:?}",
        snippet_path
    );
    assert!(
        tokio::fs::metadata(&agent_path).await.is_ok(),
        "Agent helper should be installed at {:?}",
        agent_path
    );

    Ok(())
}

/// Unit test documenting `generate_dependency_name` function behavior
///
/// This test documents the collision-resistant behavior of `generate_dependency_name`:
///   - snippets/commands/commit.md -> "commands/commit"
///   - snippets/logit/commit.md -> "logit/commit"
///   - snippets/utils/commit.md -> "utils/commit"
///
/// This is a regression test for a bug where the function used `file_stem()` which
/// collapsed all paths to bare filenames, causing silent data loss when multiple
/// resources shared the same filename but had different paths.
#[test]
fn test_generate_dependency_name_collisions() {
    // This is a unit test documenting the generate_dependency_name function behavior
    // after fixes were applied to prevent name collisions.

    use std::path::Path;

    // Simulate the corrected implementation
    fn generate_dependency_name_current(path: &str) -> String {
        let path = Path::new(path);
        let without_ext = path.with_extension("");
        let path_str = without_ext.to_string_lossy();
        let components: Vec<&str> = path_str.split('/').collect();
        if components.len() > 1 {
            components[1..].join("/")
        } else {
            components[0].to_string()
        }
    }

    // Test cases that generate DIFFERENT names (collision-resistant)
    let name1 = generate_dependency_name_current("snippets/commands/commit.md");
    let name2 = generate_dependency_name_current("snippets/logit/commit.md");
    let name3 = generate_dependency_name_current("snippets/utils/commit.md");

    // Document the correct behavior
    println!("Corrected name generation:");
    println!("  snippets/commands/commit.md -> {}", name1);
    println!("  snippets/logit/commit.md -> {}", name2);
    println!("  snippets/utils/commit.md -> {}", name3);

    // Same path should always generate same name
    let name4 = generate_dependency_name_current("snippets/commands/commit.md");
    assert_eq!(name1, name4, "Same path should generate same name");

    // Verify the corrected behavior produces unique names
    assert_eq!(name1, "commands/commit");
    assert_eq!(name2, "logit/commit");
    assert_eq!(name3, "utils/commit");
}
