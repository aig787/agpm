//! Stress test for pattern-based installation performance.
//!
//! This test exercises pattern expansion and install performance with a
//! moderate number of resources. It lives in the stress suite to avoid
//! contention with integration tests when the full suite runs.

use anyhow::Result;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Validate pattern matching/install performance with a larger file set.
#[tokio::test]
async fn test_pattern_performance() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create 100 agent files
    for i in 0..100 {
        let content = format!("# Agent {}\n\nAgent {} description", i, i);
        test_repo.add_resource("agents", &format!("agent{:03}", i), &content).await?;
    }

    test_repo.commit_all("Add 100 agents")?;
    test_repo.tag_version("v1.0.0")?;

    // Get repo URL as file://
    let repo_url = test_repo.bare_file_url(project.sources_path()).await?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent_pattern("all-agents", "test-repo", "agents/*.md", "v1.0.0")
        .build();

    project.write_manifest(&manifest).await?;

    // Measure installation time
    let start = std::time::Instant::now();

    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    let duration = start.elapsed();

    // Log performance (no assertion - rely on nextest timeout for hangs)
    println!("Pattern installation of 100 files completed in {:?}", duration);

    // Verify all files were installed
    let lockfile_content = fs::read_to_string(project.project_path().join("agpm.lock")).await?;
    let agent_count = lockfile_content.matches("agent").count();
    assert!(agent_count >= 100, "Not all agents were installed");

    Ok(())
}
