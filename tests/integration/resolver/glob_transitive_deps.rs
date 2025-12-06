//! Integration test for glob-expanded resources with transitive dependencies.
//!
//! This test reproduces the bug where transitive dependencies declared by
//! glob-expanded resources fail to resolve due to path normalization mismatch.

use crate::common::TestProject;
use anyhow::Result;

/// Test that glob-expanded resources can have transitive dependencies
/// that are correctly resolved and rendered in templates.
///
/// Regression test for: transitive deps not resolved for glob-expanded resources
/// due to path normalization mismatch in compute_canonical_name() for Git context.
#[tokio::test]
async fn test_glob_expanded_resource_with_transitive_deps() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create a snippet that will be a transitive dependency
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
agpm:
  templating: false
---
# Shared Snippet Content
This is reusable content from transitive dep.
"#,
    )
    .await?;

    // Create multiple agents that match a glob pattern and declare transitive deps
    // The relative path ../../snippets/shared.md triggers the bug
    let agent_content = r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: shared_snippet
      path: ../../snippets/shared.md
      install: false
---
# Agent Content
{{ agpm.deps.snippets.shared_snippet.content }}
"#;
    repo.add_resource("agents/specialists", "agent-one", agent_content).await?;
    repo.add_resource("agents/specialists", "agent-two", agent_content).await?;

    repo.commit_all("Add agents with transitive deps")?;
    repo.tag_version("v1.0.0")?;
    let url = repo.bare_file_url(project.sources_path()).await?;

    // Create manifest using glob pattern (the trigger for this bug)
    project
        .write_manifest(&format!(
            r#"
[sources]
community = "{}"

[agents]
specialists = {{ source = "community", path = "agents/specialists/*.md", version = "v1.0.0" }}
"#,
            url
        ))
        .await?;

    // Run install - this should succeed without template errors
    let output = project.run_agpm(&["install"])?;
    output.assert_success();

    // Verify both agents were installed with their transitive deps resolved
    let agent_one_path = project.project_path().join(".claude/agents/agpm/agent-one.md");
    let agent_one = tokio::fs::read_to_string(&agent_one_path).await?;
    assert!(
        agent_one.contains("# Shared Snippet Content"),
        "Transitive dep content should be embedded in agent-one. Content:\n{}",
        agent_one
    );

    let agent_two_path = project.project_path().join(".claude/agents/agpm/agent-two.md");
    let agent_two = tokio::fs::read_to_string(&agent_two_path).await?;
    assert!(
        agent_two.contains("# Shared Snippet Content"),
        "Transitive dep content should be embedded in agent-two. Content:\n{}",
        agent_two
    );

    Ok(())
}
