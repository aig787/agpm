/// Test to verify "version = main" vs "branch = main" behavior
///
/// This test investigates whether there's a difference between using
/// `version = "main"` and `branch = "main"` when tracking the latest
/// code from the main branch.
use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Test that `version = "main"` resolves to HEAD of main branch
#[tokio::test]
async fn test_version_main_gets_latest() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("test-repo").await?;

    // Create an agent with version in frontmatter
    repo.add_resource(
        "agents",
        "example",
        r#"---
version: v1.0.0
---
# Example Agent v1.0.0
This is the first version.
"#,
    )
    .await?;

    repo.commit_all("Initial version")?;
    repo.tag_version("v1.0.0")?;

    // Update the agent (new content, same frontmatter version declaration)
    repo.add_resource(
        "agents",
        "example",
        r#"---
version: v1.0.0
---
# Example Agent v1.0.0 - UPDATED
This is the updated content on main, but frontmatter still says v1.0.0.
"#,
    )
    .await?;

    repo.commit_all("Update agent on main")?;

    // Create manifest using `version = "main"`
    let repo_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("example", |d| d.source("test-repo").path("agents/example.md").version("main"))
        .build();

    project.write_manifest(&manifest).await?;

    // Install and check content
    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed. stderr: {}\nstdout: {}",
        output.stderr, output.stdout
    );

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/agpm/example.md");
    let installed = tokio::fs::read_to_string(&installed_path).await?;

    // Should have "UPDATED" content (from HEAD of main), not old v1.0.0 content
    assert!(
        installed.contains("UPDATED"),
        "Should get latest content from main branch, not tagged v1.0.0. Content:\n{}",
        installed
    );

    Ok(())
}

/// Test that `branch = "main"` explicitly tracks the main branch
#[tokio::test]
async fn test_branch_main_gets_latest() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("test-repo").await?;

    // Same setup as above
    repo.add_resource(
        "agents",
        "example",
        r#"---
version: v1.0.0
---
# Example Agent v1.0.0
This is the first version.
"#,
    )
    .await?;

    repo.commit_all("Initial version")?;
    repo.tag_version("v1.0.0")?;

    repo.add_resource(
        "agents",
        "example",
        r#"---
version: v1.0.0
---
# Example Agent v1.0.0 - UPDATED
This is the updated content on main, but frontmatter still says v1.0.0.
"#,
    )
    .await?;

    repo.commit_all("Update agent on main")?;

    // Create manifest using `branch = "main"` (explicit)
    let repo_url = repo.bare_file_url(project.sources_path())?;
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
example = {{ source = "test-repo", path = "agents/example.md", branch = "main" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Install and check content
    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed. stderr: {}\nstdout: {}",
        output.stderr, output.stdout
    );

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/agpm/example.md");
    let installed = tokio::fs::read_to_string(&installed_path).await?;

    // Should have "UPDATED" content (from HEAD of main)
    assert!(
        installed.contains("UPDATED"),
        "Should get latest content from main branch. Content:\n{}",
        installed
    );

    Ok(())
}

/// Test comparison: both should behave the same way (get latest from main)
#[tokio::test]
async fn test_version_main_vs_branch_main_equivalence() -> Result<()> {
    let project = TestProject::new().await?;
    let repo = project.create_source_repo("test-repo").await?;

    // Setup with version and update like before
    repo.add_resource(
        "agents",
        "example",
        r#"---
version: v1.0.0
---
# Initial"#,
    )
    .await?;
    repo.commit_all("Initial")?;
    repo.tag_version("v1.0.0")?;

    repo.add_resource(
        "agents",
        "example",
        r#"---
version: v1.0.0
---
# Updated on main"#,
    )
    .await?;
    repo.commit_all("Update on main")?;
    let main_sha = repo.git.get_head_sha()?;

    let repo_url = repo.bare_file_url(project.sources_path())?;

    // Test 1: version = "main"
    let manifest1 = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("example", |d| d.source("test-repo").path("agents/example.md").version("main"))
        .build();

    project.write_manifest(&manifest1).await?;
    let output1 = project.run_agpm(&["install"])?;
    assert!(output1.success);

    let lockfile1 = project.load_lockfile()?;
    let agent1_sha = lockfile1
        .agents
        .iter()
        .find(|a| a.name == "agents/example")
        .unwrap()
        .resolved_commit
        .as_ref()
        .unwrap();

    // Test 2: branch = "main"
    let manifest2 = format!(
        r#"[sources]
test-repo = "{}"

[agents]
example = {{ source = "test-repo", path = "agents/example.md", branch = "main" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest2).await?;
    let output2 = project.run_agpm(&["install", "--no-cache"])?;
    assert!(output2.success);

    let lockfile2 = project.load_lockfile()?;
    let agent2_sha = lockfile2
        .agents
        .iter()
        .find(|a| a.name == "agents/example")
        .unwrap()
        .resolved_commit
        .as_ref()
        .unwrap();

    // Both should resolve to the same SHA (HEAD of main)
    assert_eq!(
        agent1_sha, agent2_sha,
        "Both version='main' and branch='main' should resolve to same SHA"
    );
    assert_eq!(
        agent1_sha, &main_sha,
        "Should resolve to the latest commit on main, not v1.0.0 tag"
    );

    Ok(())
}
