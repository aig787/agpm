use crate::common::{ManifestBuilder, TestProject};
use agpm_cli::utils::platform::normalize_path_for_storage;
use anyhow::Result;

/// Tests that local dependencies with same path but different checksums conflict
#[tokio::test]
async fn test_local_dependencies_same_path_different_checksums_conflict() -> Result<()> {
    let project = TestProject::new().await?;

    // Create two separate local repositories with different content
    let repo1 = project.create_source_repo("local1").await?;
    let repo2 = project.create_source_repo("local2").await?;

    // Add resource with same path but different content in repo1
    repo1
        .add_resource(
            "agents",
            "shared-resource",
            r#"---
name: Shared Resource
---
# Shared Resource from repo1
This is version 1 of the shared resource.
"#,
        )
        .await?;

    repo1.commit_all("Add shared resource v1")?;

    // Add resource with same path but different content in repo2
    repo2
        .add_resource(
            "agents",
            "shared-resource",
            r#"---
name: Shared Resource
---
# Shared Resource from repo2
This is version 2 of the shared resource with different content.
"#,
        )
        .await?;

    repo2.commit_all("Add shared resource v2")?;

    // Create manifest that depends on both resources
    let repo1_path = format!("file://{}", normalize_path_for_storage(&repo1.path));
    let repo2_path = format!("file://{}", normalize_path_for_storage(&repo2.path));

    let manifest = ManifestBuilder::new()
        .add_source("local1", &repo1_path)
        .add_source("local2", &repo2_path)
        .add_agent("resource1", |d| d.source("local1").path("agents/shared-resource.md"))
        .add_agent("resource2", |d| d.source("local2").path("agents/shared-resource.md"))
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should fail due to checksum conflict
    let output = project.run_agpm(&["install"])?;

    assert!(
        !output.success,
        "Install should fail due to checksum conflict. stdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );

    // Verify the error message mentions conflict
    let error_output = format!("{} {}", output.stdout, output.stderr);
    assert!(
        error_output.contains("conflict") || error_output.contains("Conflict"),
        "Error should mention conflict. Got: {}",
        error_output
    );

    Ok(())
}

/// Tests that local dependencies with same path and same checksums do NOT conflict
#[tokio::test]
async fn test_local_dependencies_same_path_same_checksums_no_conflict() -> Result<()> {
    let project = TestProject::new().await?;

    // Create two separate local repositories with identical content
    let repo1 = project.create_source_repo("local").await?;

    // Add identical resource with same path
    let resource_content = r#"---
name: Shared Resource
---
# Shared Resource
This is identical content.
"#;

    repo1.add_resource("agents", "shared-resource", resource_content).await?;

    repo1.commit_all("Add shared resource")?;

    // Create manifest with a dependency on the resource
    let repo_path = format!("file://{}", normalize_path_for_storage(&repo1.path));

    let manifest = ManifestBuilder::new()
        .add_source("local", &repo_path)
        .add_agent("resource", |d| d.source("local").path("agents/shared-resource.md"))
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should succeed with no conflicts
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed with no conflicts. stderr: {}\nstdout: {}",
        output.stderr, output.stdout
    );

    // Verify the lockfile - should have one entry
    let lockfile = project.load_lockfile()?;

    // Should have exactly 1 agent entry
    assert_eq!(lockfile.agents.len(), 1, "Should have exactly 1 agent");

    let agent = lockfile
        .agents
        .first()
        .ok_or_else(|| anyhow::anyhow!("Expected at least one agent in lockfile"))?;
    assert_eq!(agent.name, "agents/shared-resource");
    assert_eq!(agent.path, "agents/shared-resource.md");
    // Verify checksum is stored (starts with "sha256:")
    assert!(agent.checksum.starts_with("sha256:"));

    Ok(())
}

/// Tests that mixed Git and local dependencies at same path conflict
#[tokio::test]
async fn test_mixed_git_local_dependencies_same_path_conflict() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a Git repository
    let git_repo = project.create_source_repo("community").await?;

    git_repo
        .add_resource(
            "agents",
            "shared-resource",
            r#"---
name: Shared Resource
---
# Shared Resource from Git repo
This is the Git version of the shared resource.
"#,
        )
        .await?;

    git_repo.commit_all("Add shared resource")?;
    git_repo.tag_version("v1.0.0")?;

    // Create a local repository
    let local_repo = project.create_source_repo("local").await?;

    local_repo
        .add_resource(
            "agents",
            "shared-resource",
            r#"---
name: Shared Resource
---
# Shared Resource from local repo
This is the local version of the shared resource with different content.
"#,
        )
        .await?;

    local_repo.commit_all("Add shared resource")?;

    // Create manifest that depends on both resources
    let git_repo_url = git_repo.bare_file_url(project.sources_path()).await?;
    let local_repo_path = format!("file://{}", normalize_path_for_storage(&local_repo.path));

    let manifest = ManifestBuilder::new()
        .add_source("community", &git_repo_url)
        .add_source("local", &local_repo_path)
        .add_agent("git-resource", |d| {
            d.source("community").path("agents/shared-resource.md").version("v1.0.0")
        })
        .add_agent("local-resource", |d| d.source("local").path("agents/shared-resource.md"))
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should fail due to conflict
    let output = project.run_agpm(&["install"])?;

    assert!(
        !output.success,
        "Install should fail due to Git/local conflict. stdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );

    // Verify the error message mentions conflict
    let error_output = format!("{} {}", output.stdout, output.stderr);
    assert!(
        error_output.contains("conflict") || error_output.contains("Conflict"),
        "Error should mention conflict. Got: {}",
        error_output
    );

    Ok(())
}

/// Tests that transitive local dependencies with same path but different checksums conflict
#[tokio::test]
async fn test_transitive_local_dependencies_checksum_conflict() -> Result<()> {
    let project = TestProject::new().await?;

    // Create two separate local repositories
    let repo1 = project.create_source_repo("local1").await?;
    let repo2 = project.create_source_repo("local2").await?;

    // Add different shared resources in each repo
    repo1
        .add_resource(
            "agents",
            "shared-resource",
            r#"---
name: Shared Resource
---
# Shared Resource from repo1
This is version 1 from repository 1.
dependencies:
  agents:
    - path: ./transitive-resource.md
"#,
        )
        .await?;

    repo1
        .add_resource(
            "agents",
            "transitive-resource",
            r#"---
name: Transitive Resource
---
# Transitive Resource from repo1
This is a transitive dependency.
"#,
        )
        .await?;

    repo1.commit_all("Add resources")?;

    repo2
        .add_resource(
            "agents",
            "shared-resource",
            r#"---
name: Shared Resource
---
# Shared Resource from repo2
This is version 2 from repository 2.
dependencies:
  agents:
    - path: ./transitive-resource.md
"#,
        )
        .await?;

    repo2
        .add_resource(
            "agents",
            "transitive-resource",
            r#"---
name: Transitive Resource
---
# Transitive Resource from repo2
This is a different transitive dependency.
"#,
        )
        .await?;

    repo2.commit_all("Add resources")?;

    // Create manifest with direct dependencies that transitively conflict
    let repo1_path = format!("file://{}", normalize_path_for_storage(&repo1.path));
    let repo2_path = format!("file://{}", normalize_path_for_storage(&repo2.path));

    let manifest = ManifestBuilder::new()
        .add_source("local1", &repo1_path)
        .add_source("local2", &repo2_path)
        .add_agent("dep1", |d| d.source("local1").path("agents/shared-resource.md"))
        .add_agent("dep2", |d| d.source("local2").path("agents/shared-resource.md"))
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should fail due to checksum conflict
    let output = project.run_agpm(&["install"])?;

    assert!(
        !output.success,
        "Install should fail due to transitive checksum conflict. stdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );

    // Verify the error message mentions conflict
    let error_output = format!("{} {}", output.stdout, output.stderr);
    assert!(
        error_output.contains("conflict") || error_output.contains("Conflict"),
        "Error should mention conflict. Got: {}",
        error_output
    );

    Ok(())
}
