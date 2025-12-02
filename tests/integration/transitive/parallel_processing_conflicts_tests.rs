// Conflict resolution tests for parallel processing in transitive dependency resolution
//
// Tests conflict handling and edge cases:
// - Transitive version conflicts under concurrent resolution
// - Concurrent progress tracking under load
// - Error handling in parallel scenarios

use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test transitive version conflicts under concurrent resolution
#[tokio::test]
async fn test_transitive_version_conflicts_concurrent() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create two source repos with conflicting versions of the same dependency
    let repo_a = project.create_source_repo("repo_a").await?;
    let repo_b = project.create_source_repo("repo_b").await?;

    // Add shared dependency in v1.0.0 to repo_a
    repo_a
        .add_resource(
            "agents",
            "shared-lib",
            r#"---
name: Shared Library v1
version: "1.0.0"
---
# Shared Library v1.0.0

This is version 1.0.0 of the shared library.
"#,
        )
        .await?;
    repo_a.commit_all("Add shared lib v1.0.0")?;
    repo_a.tag_version("v1.0.0")?;

    // Add same shared dependency in v2.0.0 to repo_b
    repo_b
        .add_resource(
            "agents",
            "shared-lib",
            r#"---
name: Shared Library v2
version: "2.0.0"
---
# Shared Library v2.0.0

This is version 2.0.0 of the shared library with breaking changes.
"#,
        )
        .await?;
    repo_b.commit_all("Add shared lib v2.0.0")?;
    repo_b.tag_version("v2.0.0")?;

    // Create top-level agents that will cause version conflicts
    for i in 0..5 {
        // Agent that depends on v1.0.0
        repo_a
            .add_resource(
                "agents",
                &format!("agent-v1-{:02}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: ./shared-lib.md
      version: v1.0.0
---
# Agent V1 {:02}

This agent depends on shared-lib v1.0.0.
"#,
                    i
                )
                .as_str(),
            )
            .await?;

        // Agent that depends on v2.0.0
        repo_b
            .add_resource(
                "agents",
                &format!("agent-v2-{:02}", i),
                format!(
                    r#"---
dependencies:
  agents:
    - path: ./shared-lib.md
      version: v2.0.0
---
# Agent V2 {:02}

This agent depends on shared-lib v2.0.0.
"#,
                    i
                )
                .as_str(),
            )
            .await?;
    }

    repo_a.commit_all("Add v1 agents")?;
    repo_a.tag_version("v1.1.0")?;
    repo_b.commit_all("Add v2 agents")?;
    repo_b.tag_version("v2.1.0")?;

    // Get source URLs
    let repo_a_url = repo_a.bare_file_url(project.sources_path()).await?;
    let repo_b_url = repo_b.bare_file_url(project.sources_path()).await?;

    // Create manifest that will cause version conflicts
    let manifest = ManifestBuilder::new()
        .add_source("repo_a", &repo_a_url)
        .add_source("repo_b", &repo_b_url)
        // Add agents that depend on different versions - this will create conflicts
        .add_agent("agent-v1-00", |d| {
            d.source("repo_a").path("agents/agent-v1-00.md").version("v1.1.0")
        })
        .add_agent("agent-v2-00", |d| {
            d.source("repo_b").path("agents/agent-v2-00.md").version("v2.1.0")
        })
        // Add both v1 and v2 versions of shared-lib directly to force conflict
        .add_agent("shared-lib-v1", |d| {
            d.source("repo_a").path("agents/shared-lib.md").version("v1.0.0")
        })
        .add_agent("shared-lib-v2", |d| {
            d.source("repo_b").path("agents/shared-lib.md").version("v2.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should fail with version conflict
    let output = project.run_agpm(&["install"])?;

    // Should fail due to version conflicts
    assert!(
        !output.success,
        "Install should fail due to version conflicts. Stderr: {}",
        output.stderr
    );

    // Check that error message mentions conflicts
    let stderr_lower = output.stderr.to_lowercase();
    assert!(
        stderr_lower.contains("conflict")
            || stderr_lower.contains("incompatible")
            || stderr_lower.contains("no compatible version")
            || stderr_lower.contains("target path conflicts"),
        "Error message should mention conflict. Got: {}",
        output.stderr
    );

    // Verify that conflict is detected (the exact message may vary)
    assert!(
        stderr_lower.contains("conflict")
            || stderr_lower.contains("overwrite")
            || stderr_lower.contains("same installation path"),
        "Error should mention conflict resolution. Got: {}",
        output.stderr
    );

    println!("✅ Concurrent version conflict test passed - conflicts correctly detected");

    Ok(())
}

/// Test atomic progress tracking under concurrent load
#[tokio::test]
async fn test_concurrent_progress_tracking() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with dependencies to stress test progress tracking
    let community_repo = project.create_source_repo("community").await?;

    // Create a dependency tree that will generate concurrent processing
    for i in 0..30 {
        community_repo
            .add_resource(
                "agents",
                &format!("progress-test-{:02}", i),
                r#"---
# Progress Test Agent
This agent is part of progress tracking tests.
---
"#,
            )
            .await?;
    }

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest
    let source_url = community_repo.bare_file_url(project.sources_path()).await?;
    let mut builder = ManifestBuilder::new().add_source("community", &source_url);

    for i in 0..30 {
        builder = builder.add_standard_agent(
            &format!("progress-test-{:02}", i),
            "community",
            &format!("agents/progress-test-{:02}.md", i),
        );
    }

    let manifest = builder.build();
    project.write_manifest(&manifest).await?;

    // Run install with verbose output to see progress tracking
    let output = project.run_agpm(&["install", "--verbose"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify all agents were installed
    let agents_dir = project.project_path().join(".claude/agents/agpm");
    let mut installed_count = 0;

    let mut entries = tokio::fs::read_dir(&agents_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_name().to_string_lossy().starts_with("progress-test-") {
            installed_count += 1;
        }
    }

    assert_eq!(
        installed_count, 30,
        "All 30 progress test agents should be installed (found {})",
        installed_count
    );

    // Check that progress indicators appeared in output
    assert!(
        output.stderr.contains("Processing")
            || output.stderr.contains("processed")
            || output.stderr.contains("resolved")
            || output.stderr.contains("dependencies"),
        "Should show progress information in verbose output"
    );

    Ok(())
}
