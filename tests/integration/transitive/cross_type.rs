// Integration tests for cross-type and cross-source transitive dependencies
//
// Tests scenarios involving resources with same names across different types
// or sources, and complex multi-tool dependency resolution.

use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test same-name resources from different sources with different tools
///
/// This test verifies that resources with the same name from different sources can coexist
/// when they use different tools (and thus install to different paths).
///
/// Scenario:
/// - community/snippets/helper.md (transitive from agent) → inherits claude-code → .claude/snippets/
/// - local/snippets/helper.md (direct snippet) → uses agpm default → .agpm/snippets/
/// - Both install successfully to different paths without collision
#[tokio::test]
async fn test_cross_source_same_name_disambiguation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create community source with helper snippet
    let community_repo = project.create_source_repo("community").await?;
    community_repo
        .add_resource("snippets", "helper", "# Community Helper\n\nFrom community source.")
        .await?;

    // Create main agent in community that depends on helper
    community_repo
        .add_resource(
            "agents",
            "main",
            r#"---
dependencies:
  snippets:
    - path: ../snippets/helper.md
      version: v1.0.0
---
# Main Agent
Depends on community helper.
"#,
        )
        .await?;
    community_repo.commit_all("Add resources")?;
    community_repo.tag_version("v1.0.0")?;

    // Create local source with helper snippet (same name, different content)
    let local_repo = project.create_source_repo("local").await?;
    local_repo.add_resource("snippets", "helper", "# Local Helper\n\nFrom local source.").await?;
    local_repo.commit_all("Add local helper")?;
    local_repo.tag_version("v1.0.0")?;

    // Create manifest that pulls main agent from community
    // Main agent has transitive dependency on community/snippets/helper
    // We'll also add a direct dependency on local/snippets/helper
    let community_url = community_repo.bare_file_url(project.sources_path())?;
    let local_url = local_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &community_url)
        .add_source("local", &local_url)
        .add_standard_agent("main", "community", "agents/main.md")
        .add_standard_snippet("local-helper", "local", "snippets/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should succeed because snippets use different tools and install to different paths
    // - community/helper (transitive from agent) uses claude-code → .claude/snippets/helper.md
    // - local/helper (direct snippet) uses agpm → .agpm/snippets/helper.md
    let output = project.run_agpm(&["install"])?;

    // Expected behavior: Should succeed - different tools mean different installation paths
    assert!(
        output.success,
        "Install should succeed - snippets use different tools and paths. Stderr: {}",
        output.stderr
    );

    // Verify both snippets are installed to their respective paths
    let community_helper = project.project_path().join(".claude/snippets/helper.md");
    let local_helper = project.project_path().join(".agpm/snippets/helper.md");

    assert!(
        tokio::fs::metadata(&community_helper).await.is_ok(),
        "Community helper should be installed to .claude/snippets (inherited claude-code from agent)"
    );
    assert!(
        tokio::fs::metadata(&local_helper).await.is_ok(),
        "Local helper should be installed to .agpm/snippets (using snippet's default tool)"
    );

    // Verify content to ensure they're the correct files
    let community_content = tokio::fs::read_to_string(&community_helper).await?;
    let local_content = tokio::fs::read_to_string(&local_helper).await?;

    assert!(
        community_content.contains("Community Helper"),
        "Community helper should have correct content"
    );
    assert!(local_content.contains("Local Helper"), "Local helper should have correct content");

    Ok(())
}
