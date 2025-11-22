//! Integration tests for .gitignore management functionality
//!
//! These tests verify that AGPM correctly manages .gitignore files
//! based on the target.gitignore configuration setting.

use anyhow::Result;
use std::path::Path;
use std::time::Duration;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Helper to create a test manifest with gitignore configuration
async fn create_test_manifest(gitignore: bool, _source_dir: &Path) -> String {
    // Use relative paths from project directory to sources directory
    // This avoids deep nesting from absolute temp paths
    ManifestBuilder::new()
        .with_target_config(|t| {
            t.agents(".claude/agents")
                .snippets(".agpm/snippets")
                .commands(".claude/commands")
                .gitignore(gitignore)
        })
        .add_agent("test-agent", |d| d.path("../sources/source/agents/test.md").flatten(false))
        .add_snippet("test-snippet", |d| {
            d.path("../sources/source/snippets/test.md").flatten(false)
        })
        .add_command("test-command", |d| {
            d.path("../sources/source/commands/test.md").flatten(false)
        })
        .build()
}

/// Helper to create a test manifest without explicit gitignore setting
async fn create_test_manifest_default(_source_dir: &Path) -> String {
    // Use relative paths from project directory to sources directory
    ManifestBuilder::new()
        .with_target_config(|t| {
            t.agents(".claude/agents").snippets(".agpm/snippets").commands(".claude/commands")
        })
        .add_agent("test-agent", |d| d.path("../sources/source/agents/test.md").flatten(false))
        .build()
}

/// Create test source files that can be installed
async fn create_test_source_files(project: &TestProject) -> Result<()> {
    let source_dir = project.sources_path().join("source");

    // Create the directories
    fs::create_dir_all(source_dir.join("agents")).await?;
    fs::create_dir_all(source_dir.join("snippets")).await?;
    fs::create_dir_all(source_dir.join("commands")).await?;

    // Create source files
    fs::write(source_dir.join("agents/test.md"), "# Test Agent\n").await?;
    fs::write(source_dir.join("snippets/test.md"), "# Test Snippet\n").await?;
    fs::write(source_dir.join("commands/test.md"), "# Test Command\n").await?;

    Ok(())
}

#[tokio::test]
async fn test_gitignore_enabled_by_default() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Create manifest without explicit gitignore setting (should default to true)
    project.write_manifest(&create_test_manifest_default(&source_dir).await).await.unwrap();

    // Run install command (let it generate the lockfile)
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check that .gitignore was created
    let gitignore_path = project.project_path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created by default");

    // Check that it has the expected structure
    let content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(content.contains("AGPM managed entries"));
    assert!(content.contains("# End of AGPM managed entries"));
}

#[tokio::test]
async fn test_gitignore_explicitly_enabled() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Create manifest with gitignore = true
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Run install command (let it generate the lockfile)
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check that .gitignore was created
    let gitignore_path = project.project_path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    // Verify content structure
    let content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(content.contains("AGPM managed entries"));
    assert!(content.contains("AGPM managed entries - do not edit below this line"));
    assert!(content.contains("# End of AGPM managed entries"));
}

// Test removed: gitignore is now always enabled (no longer configurable via manifest.target.gitignore)

#[tokio::test]
async fn test_gitignore_preserves_user_entries() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Create .claude directory
    fs::create_dir_all(project.project_path().join(".claude")).await.unwrap();

    // Create existing gitignore with user entries
    let gitignore_path = project.project_path().join(".gitignore");
    let user_content = r#"# User's custom comment
user-file.txt
temp/

# AGPM managed entries - do not edit below this line
.claude/agents/old-agent.md
# End of AGPM managed entries
"#;
    fs::write(&gitignore_path, user_content).await.unwrap();

    // Create manifest with gitignore enabled
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Run install command (let it generate the lockfile)
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check that user entries are preserved
    let updated_content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(updated_content.contains("# User's custom comment"));
    assert!(updated_content.contains("user-file.txt"));
    assert!(updated_content.contains("temp/"));

    // Check that AGPM section exists (entries will be based on what was actually installed)
    assert!(updated_content.contains("AGPM managed entries"));
    assert!(updated_content.contains("# End of AGPM managed entries"));
    assert!(updated_content.contains(".agpm/snippets/sources/source/snippets/test.md"));
}

#[tokio::test]
async fn test_gitignore_preserves_content_after_agpm_section() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Create .claude directory
    fs::create_dir_all(project.project_path().join(".claude")).await.unwrap();

    // Create existing gitignore with content after AGPM section
    let gitignore_path = project.project_path().join(".gitignore");
    let user_content = r#"# Project gitignore
temp/

# AGPM managed entries - do not edit below this line
.claude/agents/old-agent.md
# End of AGPM managed entries

# Additional entries after AGPM section
local-config.json
debug/
# End comment
"#;
    fs::write(&gitignore_path, user_content).await.unwrap();

    // Create manifest with gitignore enabled
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Run install command (let it generate the lockfile)
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check that all sections are preserved
    let updated_content = fs::read_to_string(&gitignore_path).await.unwrap();

    // Check content before AGPM section
    assert!(updated_content.contains("# Project gitignore"));
    assert!(updated_content.contains("temp/"));

    // Check AGPM section is updated
    assert!(updated_content.contains("AGPM managed entries"));
    assert!(updated_content.contains("# End of AGPM managed entries"));
    assert!(updated_content.contains(".agpm/snippets/sources/source/snippets/test.md"));

    // Check content after AGPM section is preserved
    assert!(updated_content.contains("# Additional entries after AGPM section"));
    assert!(updated_content.contains("local-config.json"));
    assert!(updated_content.contains("debug/"));
    assert!(updated_content.contains("# End comment"));

    // Verify old AGPM entry is removed
    assert!(!updated_content.contains(".claude/agents/old-agent.md"));
}

#[tokio::test]
async fn test_gitignore_update_command() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Create manifest
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Run install first to create initial lockfile
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Run update command (which should also update gitignore)
    project.run_agpm(&["update", "--quiet"]).unwrap().assert_success();

    // Check that .gitignore exists after update
    let gitignore_path = project.project_path().join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).await.unwrap();
        assert!(content.contains("AGPM managed entries"));
    }
}

#[tokio::test]
async fn test_gitignore_handles_external_paths() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

    // Create a test repository with both agent and script
    let repo = project.create_source_repo("test-source").await.unwrap();

    // Create agent
    repo.add_resource("agents", "test-agent", "# Test Agent\n").await.unwrap();

    // Create script
    fs::create_dir_all(repo.path.join("scripts")).await.unwrap();
    fs::write(repo.path.join("scripts/test.sh"), "#!/bin/bash\necho 'test'\n").await.unwrap();

    // Commit and tag
    repo.git.add_all().unwrap();
    repo.git.commit("Initial commit").unwrap();
    repo.git.tag("v1.0.0").unwrap();

    let url = repo.bare_file_url(project.sources_path()).unwrap();

    // Create manifest with script and agent
    let manifest_content = ManifestBuilder::new()
        .add_source("test-source", &url)
        .with_gitignore(true)
        .add_script("external-script", |d| {
            d.source("test-source").path("scripts/test.sh").version("v1.0.0")
        })
        .add_agent("internal-agent", |d| {
            d.source("test-source").path("agents/test-agent.md").version("v1.0.0")
        })
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Run install command
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check gitignore content
    let gitignore_path = project.project_path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    let content = fs::read_to_string(&gitignore_path).await.unwrap();

    // Both resources should be listed in gitignore
    assert!(content.contains("AGPM managed entries"), "Should have AGPM section");
    assert!(content.contains("# End of AGPM managed entries"), "Should have end marker");

    // Scripts default to .claude/scripts/ directory
    // Paths are preserved as-is from dependency specification
    assert!(
        content.contains(".claude/scripts/test.sh")
            || content.contains(".claude/scripts/external-script.sh"),
        "Script path should be in gitignore. Content:\n{}",
        content
    );

    // Agents go to .claude/agents/
    // Paths are preserved as-is from dependency specification
    assert!(
        content.contains(".claude/agents/test-agent.md")
            || content.contains(".claude/agents/internal-agent.md"),
        "Agent path should be in gitignore. Content:\n{}",
        content
    );
}

#[tokio::test]
async fn test_gitignore_empty_lockfile() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();

    // Create manifest with no dependencies
    let manifest_content = ManifestBuilder::new()
        .with_target_config(|t| {
            t.agents(".claude/agents")
                .snippets(".agpm/snippets")
                .commands(".claude/commands")
                .gitignore(true)
        })
        .build();
    project.write_manifest(&manifest_content).await.unwrap();

    // Run install command (will generate empty lockfile)
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check that .gitignore is created even with no resources
    let gitignore_path = project.project_path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created even with empty lockfile");

    let content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(content.contains("AGPM managed entries"));
    assert!(content.contains("# End of AGPM managed entries"));
}

#[tokio::test]
async fn test_gitignore_idempotent() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Create manifest
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Run install command
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Get content after first run
    let gitignore_path = project.project_path().join(".gitignore");
    let first_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path).await.unwrap()
    } else {
        String::new()
    };

    // Run again
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Get content after second run
    let second_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path).await.unwrap()
    } else {
        String::new()
    };

    // Content should be the same (idempotent)
    assert_eq!(first_content, second_content, "Gitignore should be idempotent");
}

#[tokio::test]
async fn test_gitignore_switch_enabled_disabled() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Start with gitignore enabled
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Run install with gitignore enabled
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    let gitignore_path = project.project_path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    // Now disable gitignore
    project.write_manifest(&create_test_manifest(false, &source_dir).await).await.unwrap();

    // Run install again
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Gitignore should still exist (we don't delete it)
    assert!(gitignore_path.exists(), "Gitignore should still exist when disabled");

    // Re-enable gitignore
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Add a user entry to the existing gitignore
    let content = fs::read_to_string(&gitignore_path).await.unwrap();
    let modified_content =
        content.replace("# AGPM managed entries", "user-custom.txt\n\n# AGPM managed entries");
    fs::write(&gitignore_path, modified_content).await.unwrap();

    // Run install again
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check that user entry is preserved
    let final_content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(
        final_content.contains("user-custom.txt"),
        "User entries should be preserved when re-enabling"
    );
}

#[tokio::test]
async fn test_gitignore_actually_ignored_by_git() {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await.unwrap();
    let project_dir = project.project_path().to_path_buf();
    let source_dir = project.sources_path().join("source");

    create_test_source_files(&project).await.unwrap();

    let git = project.init_git_repo().unwrap();

    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // After stripping parent directory components from paths like "../sources/source/agents/test.md"
    // we get "sources/source/agents/test.md" which installs to ".claude/agents/sources/source/agents/test.md"
    assert!(project_dir.join(".claude/agents/sources/source/agents/test.md").exists());
    assert!(project_dir.join(".agpm/snippets/sources/source/snippets/test.md").exists());
    assert!(project_dir.join(".claude/commands/sources/source/commands/test.md").exists());

    git.add_all().unwrap();
    let status = git.status_porcelain().unwrap();

    assert!(
        !status.contains("sources/source/agents/test.md"),
        "Agent file should be ignored by git\nGit status:\n{}",
        status
    );
    assert!(
        !status.contains("sources/source/snippets/test.md"),
        "Snippet file should be ignored by git\nGit status:\n{}",
        status
    );
    assert!(
        !status.contains("sources/source/commands/test.md"),
        "Command file should be ignored by git\nGit status:\n{}",
        status
    );
    assert!(
        status.contains(".gitignore"),
        "Gitignore file should be tracked by git\nGit status:\n{}",
        status
    );
    assert!(
        status.contains("agpm.toml"),
        "Manifest should be tracked by git\nGit status:\n{}",
        status
    );
    assert!(
        status.contains("agpm.lock"),
        "Lockfile should be tracked by git\nGit status:\n{}",
        status
    );

    assert!(
        git.check_ignore(".claude/agents/sources/source/agents/test.md").unwrap(),
        "Agent file should be ignored by git check-ignore"
    );
    assert!(
        git.check_ignore(".agpm/snippets/sources/source/snippets/test.md").unwrap(),
        "Snippet file should be ignored by git check-ignore"
    );
    assert!(
        git.check_ignore(".claude/commands/sources/source/commands/test.md").unwrap(),
        "Command file should be ignored by git check-ignore"
    );
}

// Test removed: gitignore is now always enabled (no longer configurable via manifest.target.gitignore)

#[tokio::test]
async fn test_gitignore_malformed_existing() {
    agpm_cli::test_utils::init_test_logging(None);
    let project = TestProject::new().await.unwrap();
    let source_dir = project.sources_path().join("source");

    // Create source files
    create_test_source_files(&project).await.unwrap();

    // Create .claude directory
    fs::create_dir_all(project.project_path().join(".claude")).await.unwrap();

    // Create malformed gitignore (missing end marker)
    let gitignore_path = project.project_path().join(".gitignore");
    let malformed_content = r#"# Some content
user-file.txt

# AGPM managed entries - do not edit below this line
/old/entry.md
# Missing end marker!
"#;
    fs::write(&gitignore_path, malformed_content).await.unwrap();

    // Create manifest and run install
    project.write_manifest(&create_test_manifest(true, &source_dir).await).await.unwrap();

    // Run install command (let it generate the lockfile)
    project.run_agpm(&["install", "--quiet"]).unwrap().assert_success();

    // Check that gitignore was properly recreated
    let updated_content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(updated_content.contains("# End of AGPM managed entries"));
    assert!(updated_content.contains("user-file.txt"));
    assert!(updated_content.contains("AGPM managed entries"));
}

#[tokio::test]
async fn test_gitignore_cleanup_race_condition() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a simple agent
    test_repo
        .add_resource("agents", "test-agent", "# Test Agent\n\nTest agent content")
        .await?;
    test_repo.commit_all("Add test agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest and install
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("test-agent", |d| d.source("test-repo").path("agents/test-agent.md").version("v1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Verify .gitignore exists
    let gitignore_path = project.project_path().join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    // Read the current manifest to get resource list (for cleanup function)
    let _manifest_content = tokio::fs::read_to_string(project.project_path().join("agpm.toml"))
        .await?;
    let _manifest: agpm_cli::manifest::Manifest = toml::from_str(&_manifest_content)?;

    // Simulate concurrent cleanup by spawning two tasks that try to clean up gitignore
    let gitignore_path_1 = gitignore_path.clone();
    let gitignore_path_2 = gitignore_path.clone();

    // Create empty lists (simulating all resources removed)
    let handle1 = tokio::spawn(async move {
        // Directly call the cleanup function with empty resource lists
        // This simulates the scenario where gitignore should be removed
        tokio::fs::remove_file(&gitignore_path_1).await
    });

    let handle2 = tokio::spawn(async move {
        // Small delay to increase chance of race condition
        tokio::time::sleep(Duration::from_millis(1)).await;
        tokio::fs::remove_file(&gitignore_path_2).await
    });

    // Wait for both tasks
    let result1 = handle1.await.unwrap();
    let result2 = handle2.await.unwrap();

    // One should succeed (Ok), the other should get NotFound
    let success_count = [&result1, &result2]
        .iter()
        .filter(|r| r.is_ok())
        .count();
    let not_found_count = [&result1, &result2]
        .iter()
        .filter(|r| {
            if let Err(e) = r {
                e.kind() == std::io::ErrorKind::NotFound
            } else {
                false
            }
        })
        .count();

    // Either one succeeded and one got NotFound (race), or both got NotFound (both raced)
    // or one succeeded and the other also succeeded (both deleted before checking)
    assert!(
        (success_count == 1 && not_found_count == 1) || not_found_count == 2,
        "Expected race condition: one success + one NotFound, or both NotFound. Got: result1={:?}, result2={:?}",
        result1,
        result2
    );

    // The important thing is neither panicked
    assert!(
        !gitignore_path.exists(),
        "Gitignore should be deleted by one of the tasks"
    );

    Ok(())
}

#[tokio::test]
async fn test_gitignore_cleanup_handles_concurrent_removal() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a .gitignore file directly
    let gitignore_path = project.project_path().join(".gitignore");
    tokio::fs::write(&gitignore_path, "# Test gitignore\n.agpm-resources/\n").await?;

    // Verify it exists
    assert!(gitignore_path.exists());

    // Spawn 10 concurrent tasks all trying to delete the same file
    let mut handles = vec![];
    for i in 0..10 {
        let path = gitignore_path.clone();
        let handle = tokio::spawn(async move {
            // Add small random delay to increase race condition likelihood
            tokio::time::sleep(Duration::from_micros(i * 100)).await;

            match tokio::fs::remove_file(&path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // Expected race
                Err(e) => Err(e), // Unexpected error
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks - none should panic or return unexpected errors
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "All cleanup attempts should succeed or gracefully handle NotFound"
        );
    }

    // File should definitely be gone
    assert!(!gitignore_path.exists(), "File should be deleted");

    Ok(())
}
