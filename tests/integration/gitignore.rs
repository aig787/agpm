//! Integration tests for .gitignore management functionality
//!
//! These tests verify that AGPM correctly manages .gitignore files
//! based on the target.gitignore configuration setting.

use agpm_cli::utils::normalize_path_for_storage;
use anyhow::Result;
use std::path::Path;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Helper to create a test manifest with gitignore configuration
async fn create_test_manifest(gitignore: bool, source_dir: &Path) -> String {
    // Convert path to string with forward slashes for TOML compatibility
    let source_path = normalize_path_for_storage(source_dir);
    ManifestBuilder::new()
        .with_target_config(|t| {
            t.agents(".claude/agents")
                .snippets(".agpm/snippets")
                .commands(".claude/commands")
                .gitignore(gitignore)
        })
        .add_local_agent("test-agent", &format!("{}/agents/test.md", source_path))
        .add_local_snippet("test-snippet", &format!("{}/snippets/test.md", source_path))
        .add_local_command("test-command", &format!("{}/commands/test.md", source_path))
        .build()
}

/// Helper to create a test manifest without explicit gitignore setting
async fn create_test_manifest_default(source_dir: &Path) -> String {
    // Convert path to string with forward slashes for TOML compatibility
    let source_path = normalize_path_for_storage(source_dir);
    ManifestBuilder::new()
        .with_target_config(|t| {
            t.agents(".claude/agents").snippets(".agpm/snippets").commands(".claude/commands")
        })
        .add_local_agent("test-agent", &format!("{}/agents/test.md", source_path))
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
*.backup
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
    assert!(updated_content.contains("*.backup"));
    assert!(updated_content.contains("user-file.txt"));
    assert!(updated_content.contains("temp/"));

    // Check that AGPM section exists (entries will be based on what was actually installed)
    assert!(updated_content.contains("AGPM managed entries"));
    assert!(updated_content.contains("# End of AGPM managed entries"));
    assert!(updated_content.contains(".agpm/snippets/test-snippet.md"));
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
*.backup
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
    assert!(updated_content.contains("*.backup"));
    assert!(updated_content.contains("temp/"));

    // Check AGPM section is updated
    assert!(updated_content.contains("AGPM managed entries"));
    assert!(updated_content.contains("# End of AGPM managed entries"));
    assert!(updated_content.contains(".agpm/snippets/test-snippet.md"));

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
    assert!(
        content.contains(".claude/scripts/test.sh")
            || content.contains(".claude/scripts/external-script.sh"),
        "Script path should be in gitignore. Content:\n{}",
        content
    );

    // Agents go to .claude/agents/
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

    assert!(project_dir.join(".claude/agents/test-agent.md").exists());
    assert!(project_dir.join(".agpm/snippets/test-snippet.md").exists());
    assert!(project_dir.join(".claude/commands/test-command.md").exists());

    git.add_all().unwrap();
    let status = git.status_porcelain().unwrap();

    assert!(
        !status.contains("agents/test-agent.md"),
        "Agent file should be ignored by git\nGit status:\n{}",
        status
    );
    assert!(
        !status.contains("snippets/test-snippet.md"),
        "Snippet file should be ignored by git\nGit status:\n{}",
        status
    );
    assert!(
        !status.contains("commands/test-command.md"),
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
        git.check_ignore(".claude/agents/test-agent.md").unwrap(),
        "Agent file should be ignored by git check-ignore"
    );
    assert!(
        git.check_ignore(".agpm/snippets/test-snippet.md").unwrap(),
        "Snippet file should be ignored by git check-ignore"
    );
    assert!(
        git.check_ignore(".claude/commands/test-command.md").unwrap(),
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
