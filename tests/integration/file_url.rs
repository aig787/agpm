use anyhow::Result;
use std::path::Path;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Convert a path to a file:// URL string, properly handling Windows paths
async fn path_to_file_url(path: &Path) -> String {
    let path_str = path.display().to_string().replace('\\', "/");
    format!("file://{path_str}")
}

#[tokio::test]
async fn test_file_url_source_repo_not_modified() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("file-url-source").await?;
    let git = &source_repo.git;

    fs::create_dir_all(source_repo.path.join("agents")).await?;

    fs::write(source_repo.path.join("agents/test.md"), "# Test Agent v1").await?;
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    fs::write(source_repo.path.join("agents/test.md"), "# Test Agent v2").await?;
    git.add_all()?;
    git.commit("Update to v2")?;
    git.tag("v2.0.0")?;

    git.checkout("v1.0.0")?;
    let source_head_before = git.get_commit_hash()?;

    let file_url = path_to_file_url(git.repo_path()).await;
    let manifest = ManifestBuilder::new()
        .add_source("local", &file_url)
        .add_agent("test-agent", |d| d.source("local").path("agents/test.md").version("v2.0.0"))
        .build();
    project.write_manifest(&manifest).await?;

    project.run_agpm(&["install"])?.assert_success();

    // Verify the installed file is from v2.0.0
    // Files use basename from path, not dependency name
    let installed_path = project.project_path().join(".claude/agents/test.md");
    let installed_content = fs::read_to_string(installed_path).await?;
    assert!(installed_content.contains("v2"), "Installed file should be from v2.0.0");

    let source_head_after = git.get_commit_hash()?;
    assert_eq!(
        source_head_before.trim(),
        source_head_after.trim(),
        "Source repository HEAD should not change"
    );

    let status = git.status_porcelain()?;
    assert!(status.trim().is_empty(), "Source repository should have no modifications");

    let source_content = fs::read_to_string(source_repo.path.join("agents/test.md")).await?;
    assert!(source_content.contains("v1"), "Source repo working directory should remain on v1.0.0");

    Ok(())
}

#[tokio::test]
async fn test_file_url_updates_work() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("file-url-updates").await?;
    let git = &source_repo.git;

    fs::create_dir_all(source_repo.path.join("agents")).await?;

    fs::write(source_repo.path.join("agents/test.md"), "# Test Agent v1").await?;
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    let file_url = path_to_file_url(git.repo_path()).await;
    let manifest_v1 = ManifestBuilder::new()
        .add_source("local", &file_url)
        .add_standard_agent("test-agent", "local", "agents/test.md")
        .build();
    project.write_manifest(&manifest_v1).await?;

    project.run_agpm(&["install"])?.assert_success();

    // Verify v1 is installed
    // Files use basename from path, not dependency name
    let installed_v1 =
        fs::read_to_string(project.project_path().join(".claude/agents/test.md")).await?;
    assert!(installed_v1.contains("v1"), "Should have v1 installed");

    // Now add a new version in the source repo
    fs::write(source_repo.path.join("agents/test.md"), "# Test Agent v2").await?;
    git.add_all()?;
    git.commit("Update to v2")?;
    git.tag("v2.0.0")?;

    let manifest_v2 = ManifestBuilder::new()
        .add_source("local", &file_url)
        .add_agent("test-agent", |d| d.source("local").path("agents/test.md").version("v2.0.0"))
        .build();
    project.write_manifest(&manifest_v2).await?;

    project.run_agpm(&["install"])?.assert_success();

    // Verify v2 is now installed
    // Files use basename from path, not dependency name
    let installed_v2 =
        fs::read_to_string(project.project_path().join(".claude/agents/test.md")).await?;
    assert!(installed_v2.contains("v2"), "Should have v2 installed after auto-update");

    Ok(())
}

#[tokio::test]
async fn test_file_url_with_uncommitted_changes() -> Result<()> {
    let project = TestProject::new().await?;
    let source_repo = project.create_source_repo("file-url-uncommitted").await?;
    let git = &source_repo.git;

    fs::create_dir_all(source_repo.path.join("agents")).await?;

    fs::write(source_repo.path.join("agents/test.md"), "# Test Agent v1").await?;
    git.add_all()?;
    git.commit("Initial commit")?;
    git.tag("v1.0.0")?;

    fs::write(source_repo.path.join("agents/test.md"), "# Test Agent - Work in Progress").await?;
    fs::write(source_repo.path.join("new_file.txt"), "Uncommitted work").await?;

    let status_before = git.status_porcelain()?;
    assert!(!status_before.trim().is_empty(), "Source repo should have uncommitted changes");

    let file_url = path_to_file_url(git.repo_path()).await;
    let manifest = ManifestBuilder::new()
        .add_source("local", &file_url)
        .add_standard_agent("test-agent", "local", "agents/test.md")
        .build();
    project.write_manifest(&manifest).await?;

    project.run_agpm(&["install"])?.assert_success();

    // Verify the installed file is from the committed v1.0.0, not the uncommitted changes
    // Files use basename from path, not dependency name
    let installed_content =
        fs::read_to_string(project.project_path().join(".claude/agents/test.md")).await?;
    assert!(installed_content.contains("v1"), "Install should use committed content");
    assert!(
        !installed_content.contains("Work in Progress"),
        "Uncommitted changes must not be installed"
    );

    let status_after = git.status_porcelain()?;
    assert_eq!(
        status_before.trim(),
        status_after.trim(),
        "Uncommitted changes should remain after install"
    );

    let source_content = fs::read_to_string(source_repo.path.join("agents/test.md")).await?;
    assert!(
        source_content.contains("Work in Progress"),
        "Source repo should still contain uncommitted changes"
    );

    Ok(())
}
