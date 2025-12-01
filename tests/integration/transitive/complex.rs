// Integration tests for complex transitive dependency resolution scenarios
//
// These tests cover edge cases and complex scenarios in transitive dependency resolution.

use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test version conflict resolution with correct metadata extraction
///
/// When a shared transitive dependency is re-resolved after resolve_version_conflict
/// picks a different version, verify that the final lockfile only contains dependencies
/// from the winning version, not stale dependencies from earlier versions.
///
/// Scenario:
/// - shared@v1.0.0 has old-dep as transitive dependency
/// - shared@v2.0.0 has new-dep as transitive dependency (different from v1.0.0)
/// - parent-a depends on shared@v1.0.0
/// - parent-b depends on shared@v2.0.0
/// - Resolver should pick v2.0.0 (highest version) and use ITS dependencies
/// - Lockfile should show shared with ONLY new-dep (not old-dep)
#[tokio::test]
async fn test_version_conflict_uses_winning_version_metadata() -> Result<()> {
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
      version: v1.0.0
---
# Shared v1.0.0
Version 1 with old-dep.
"#,
    )
    .await?;

    repo.commit_all("Add v1.0.0 resources")?;
    repo.tag_version("v1.0.0")?;

    // Create new-dep that will be in v2.0.0's transitive tree
    // Remove old-dep to make the difference clear
    tokio::fs::remove_file(repo.path.join("commands/old-dep.md")).await?;
    repo.add_resource("commands", "new-dep", "# New Dep\n\nNew command.").await?;

    // Update shared to v2.0.0 with new-dep as transitive dependency (NOT old-dep)
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/new-dep.md
      version: v2.0.0
---
# Shared v2.0.0
Version 2 with new-dep (NOT old-dep).
"#,
    )
    .await?;

    repo.commit_all("Update to v2.0.0")?;
    repo.tag_version("v2.0.0")?;

    // Create parent-a that depends on shared@>=v1.0.0 (compatible range)
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
Depends on shared@>=v1.0.0.
"#,
    )
    .await?;

    // Create parent-b that depends on shared@>=v2.0.0 (creates resolvable conflict - picks v2.0.0)
    repo.add_resource(
        "agents",
        "parent-b",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: ">=v2.0.0"
---
# Parent B
Depends on shared@>=v2.0.0 (intersection with parent-a picks v2.0.0).
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

    // Verify that v2.0.0 won (highest version)
    let lockfile_content = project.read_lockfile().await?;

    // Check that shared is at v2.0.0
    // Transitive dependency has canonical name with resource type directory
    assert!(
        lockfile_content.contains(r#"name = "snippets/shared""#)
            && lockfile_content.contains("v2.0.0"),
        "Lockfile should show shared at v2.0.0"
    );

    // Find the shared snippet section in lockfile
    // Transitive dependencies use canonical names: "snippets/shared"
    let shared_section_start = lockfile_content
        .find("[[snippets]]")
        .and_then(|pos| {
            if lockfile_content[pos..].contains(r#"name = "snippets/shared""#) {
                Some(pos)
            } else {
                None
            }
        })
        .expect("Should find shared snippet section");

    let shared_section_end = lockfile_content[shared_section_start + 1..]
        .find("[[")
        .map(|offset| shared_section_start + 1 + offset)
        .unwrap_or(lockfile_content.len());

    let shared_section = &lockfile_content[shared_section_start..shared_section_end];

    // EXPECTED: Only new-dep should be in dependencies
    let has_new_dep = shared_section.contains("new-dep");
    let has_old_dep = shared_section.contains("old-dep");

    assert!(has_new_dep, "Shared should have new-dep in dependencies (from v2.0.0)");
    assert!(
        !has_old_dep,
        "Shared should NOT have old-dep in dependencies (stale from v1.0.0).\nShared section:\n{}",
        shared_section
    );

    // Also verify that new-dep file exists and old-dep does NOT
    let new_dep_path = project.project_path().join(".claude/commands/agpm/new-dep.md");
    let old_dep_path = project.project_path().join(".claude/commands/agpm/old-dep.md");

    assert!(tokio::fs::metadata(&new_dep_path).await.is_ok(), "new-dep should be installed");
    assert!(
        tokio::fs::metadata(&old_dep_path).await.is_err(),
        "old-dep should NOT be installed (doesn't exist at v2.0.0)"
    );

    Ok(())
}

/// Test version metadata propagation for nested resources
///
/// Verify that when dependencies are in subdirectories, the version metadata
/// is correctly added to dependency references in the lockfile. This ensures
/// proper version tracking even for resources with complex path structures.
///
/// Scenario:
/// - Agent depends on snippet in subdirectory: snippets/helpers/foo.md
/// - Lockfile should contain version information in dependency reference
/// - Version metadata should not be lost during path normalization
#[tokio::test]
async fn test_version_metadata_for_nested_resources() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create snippet in subdirectory
    let helpers_dir = repo.path.join("snippets/helpers");
    tokio::fs::create_dir_all(&helpers_dir).await?;
    tokio::fs::write(
        helpers_dir.join("foo.md"),
        "# Foo Helper\n\nA helper snippet in a subdirectory.",
    )
    .await?;

    // Create agent that depends on the nested snippet
    repo.add_resource(
        "agents",
        "main",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/helpers/foo.md
      version: v1.0.0
---
# Main Agent
Depends on a snippet in a subdirectory.
"#,
    )
    .await?;

    repo.commit_all("Initial commit")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("main", "community", "agents/main.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Read lockfile
    let lockfile_content = project.read_lockfile().await?;

    // Find the main agent section
    // Direct manifest dependencies use canonical names with resource type directory
    let main_section_start = lockfile_content
        .find("[[agents]]")
        .and_then(|pos| {
            if lockfile_content[pos..].contains(r#"name = "agents/main""#) {
                Some(pos)
            } else {
                None
            }
        })
        .expect("Should find main agent section");

    let main_section_end = lockfile_content[main_section_start + 1..]
        .find("[[")
        .map(|offset| main_section_start + 1 + offset)
        .unwrap_or(lockfile_content.len());

    let main_section = &lockfile_content[main_section_start..main_section_end];

    // Check for version metadata in dependency reference
    // The name after type-stripping should be "helpers/foo" or "helpers-foo"
    let has_version_metadata = main_section.contains("helpers")
        && (main_section.contains("@v1.0.0") || main_section.contains(":v1.0.0"));

    assert!(
        has_version_metadata,
        "Version metadata should be preserved for nested resource.\nMain agent section:\n{}",
        main_section
    );

    Ok(())
}

/// Test custom dependency aliases with cross-source dependencies
///
/// Verify that custom dependency names (aliases) work correctly even when
/// dependencies are from different sources. The templating system should
/// properly resolve custom names regardless of source annotations.
///
/// Scenario:
/// - Agent depends on snippet with custom name "custom_helper"
/// - Template references the dependency using custom alias
/// - Template should render correctly with the custom name
#[tokio::test]
async fn test_custom_aliases_for_cross_source_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source with an agent
    let source_repo = project.create_source_repo("community").await?;
    source_repo
        .add_resource(
            "agents",
            "main",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: ../snippets/helper.md
      name: custom_helper
---
# Main Agent
Uses a snippet with a custom name.

Template test: {{ agpm.deps.snippets.custom_helper.name }}
"#,
        )
        .await?;

    // Add snippet
    source_repo.add_resource("snippets", "helper", "# Helper\n\nA helper snippet.").await?;

    source_repo.commit_all("Initial commit")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest
    let source_url = source_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("main", "community", "agents/main.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Read the installed agent to check template rendering
    let installed_agent = project.project_path().join(".claude/agents/agpm/main.md");
    let agent_content = tokio::fs::read_to_string(&installed_agent).await?;

    // The template should have been rendered with the custom alias
    // Expected: "Template test: snippets/helper" (canonical name in lockfile)
    // The .name field uses canonical naming, not just basename
    let template_rendered = agent_content.contains("Template test: snippets/helper");

    assert!(
        template_rendered,
        "Template with custom dependency alias should render correctly.\nAgent content:\n{}",
        agent_content
    );

    // Also verify that the helper snippet was installed
    let helper_path = project.project_path().join(".claude/snippets/agpm/snippets/helper.md");
    assert!(tokio::fs::metadata(&helper_path).await.is_ok(), "Helper snippet should be installed");

    Ok(())
}
