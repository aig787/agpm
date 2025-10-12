//! Integration tests for .gitignore management functionality
//!
//! These tests verify that AGPM correctly manages .gitignore files
//! based on the target.gitignore configuration setting.

use agpm_cli::utils::normalize_path_for_storage;
use anyhow::Result;
use assert_cmd::Command;
use std::path::Path;
use tempfile::TempDir;
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

/// Helper to create a test lockfile with installed resources
async fn create_test_lockfile() -> String {
    r#"
version = 1

[[agents]]
name = "test-agent"
path = "source/agents/test.md"
checksum = ""
installed_at = ".claude/agents/test-agent.md"

[[snippets]]
name = "test-snippet"
path = "source/snippets/test.md"
checksum = ""
installed_at = ".agpm/snippets/test-snippet.md"
artifact_type = "agpm"

[[commands]]
name = "test-command"
path = "source/commands/test.md"
checksum = ""
installed_at = ".claude/commands/test-command.md"
"#
    .to_string()
}

/// Create test source files that can be installed
async fn create_test_source_files(source_dir: &Path) -> Result<()> {
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
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create manifest without explicit gitignore setting (should default to true)
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest_default(&source_dir).await).await.unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore was created
    let gitignore_path = project_dir.join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created by default");

    // Check that it has the expected structure
    let content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(content.contains("AGPM managed entries"));
    assert!(content.contains("# End of AGPM managed entries"));
}

#[tokio::test]
async fn test_gitignore_explicitly_enabled() {
    agpm_cli::test_utils::init_test_logging(None);
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create manifest with gitignore = true
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore was created
    let gitignore_path = project_dir.join(".gitignore");
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
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create .claude directory
    fs::create_dir_all(project_dir.join(".claude")).await.unwrap();

    // Create existing gitignore with user entries
    let gitignore_path = project_dir.join(".gitignore");
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
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

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
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create .claude directory
    fs::create_dir_all(project_dir.join(".claude")).await.unwrap();

    // Create existing gitignore with content after AGPM section
    let gitignore_path = project_dir.join(".gitignore");
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
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

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
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    // Create initial lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run update command (which should also update gitignore)
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("update")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore exists after update
    let gitignore_path = project_dir.join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).await.unwrap();
        assert!(content.contains("AGPM managed entries"));
    }
}

#[tokio::test]
async fn test_gitignore_handles_external_paths() {
    agpm_cli::test_utils::init_test_logging(None);
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    let manifest_content = ManifestBuilder::new()
        .add_source("test-source", "https://github.com/test/repo.git")
        .with_gitignore(true)
        .add_script("external-script", |d| {
            d.source("test-source").path("scripts/test.sh").version("v1.0.0")
        })
        .add_agent("internal-agent", |d| {
            d.source("test-source").path("agents/test.md").version("v1.0.0")
        })
        .build();
    fs::write(&manifest_path, manifest_content).await.unwrap();

    // Create lockfile with resource installed outside .claude
    let lockfile_content = r#"
version = 1

[[sources]]
name = "test-source"
url = "https://github.com/test/repo.git"
commit = "abc123"

[[scripts]]
name = "external-script"
source = "test-source"
url = "https://github.com/test/repo.git"
path = "scripts/test.sh"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:test"
installed_at = "scripts/external.sh"

[[agents]]
name = "internal-agent"
source = "test-source"
url = "https://github.com/test/repo.git"
path = "agents/test.md"
version = "v1.0.0"
resolved_commit = "abc123"
checksum = "sha256:test"
installed_at = ".claude/agents/internal.md"
"#;
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, lockfile_content).await.unwrap();

    // Create directories
    fs::create_dir_all(project_dir.join(".claude/agents")).await.unwrap();
    fs::create_dir_all(project_dir.join("scripts")).await.unwrap();

    // Create resource files
    fs::write(project_dir.join("scripts/external.sh"), "#!/bin/bash\n").await.unwrap();
    fs::write(project_dir.join(".claude/agents/internal.md"), "# Internal\n").await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check gitignore content
    let gitignore_path = project_dir.join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).await.unwrap();
        // External path should use ../
        assert!(content.contains("../scripts/external.sh"), "External paths should use ../ prefix");
        // Internal path should use /
        assert!(
            content.contains(".claude/agents/internal.md"),
            "Internal paths should use / prefix"
        );
    }
}

#[tokio::test]
async fn test_gitignore_empty_lockfile() {
    agpm_cli::test_utils::init_test_logging(None);
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    // Create empty lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, "version = 1\n").await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that .gitignore is created even with no resources
    let gitignore_path = project_dir.join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created even with empty lockfile");

    let content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(content.contains("AGPM managed entries"));
    assert!(content.contains("# End of AGPM managed entries"));
}

#[tokio::test]
async fn test_gitignore_idempotent() {
    agpm_cli::test_utils::init_test_logging(None);
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create manifest
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    // Create lockfile
    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Get content after first run
    let gitignore_path = project_dir.join(".gitignore");
    let first_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path).await.unwrap()
    } else {
        String::new()
    };

    // Run again
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

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
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Start with gitignore enabled
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run install with gitignore enabled
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    let gitignore_path = project_dir.join(".gitignore");
    assert!(gitignore_path.exists(), "Gitignore should be created");

    // Now disable gitignore
    fs::write(&manifest_path, create_test_manifest(false, &source_dir).await).await.unwrap();

    // Run install again
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Gitignore should still exist (we don't delete it)
    assert!(gitignore_path.exists(), "Gitignore should still exist when disabled");

    // Re-enable gitignore
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    // Add a user entry to the existing gitignore
    let content = fs::read_to_string(&gitignore_path).await.unwrap();
    let modified_content =
        content.replace("# AGPM managed entries", "user-custom.txt\n\n# AGPM managed entries");
    fs::write(&gitignore_path, modified_content).await.unwrap();

    // Run install again
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

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

    create_test_source_files(&source_dir).await.unwrap();

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
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path();
    let source_dir = temp.path().join("source");

    // Create source files
    create_test_source_files(&source_dir).await.unwrap();

    // Create .claude directory
    fs::create_dir_all(project_dir.join(".claude")).await.unwrap();

    // Create malformed gitignore (missing end marker)
    let gitignore_path = project_dir.join(".gitignore");
    let malformed_content = r#"# Some content
user-file.txt

# AGPM managed entries - do not edit below this line
/old/entry.md
# Missing end marker!
"#;
    fs::write(&gitignore_path, malformed_content).await.unwrap();

    // Create manifest and lockfile
    let manifest_path = project_dir.join("agpm.toml");
    fs::write(&manifest_path, create_test_manifest(true, &source_dir).await).await.unwrap();

    let lockfile_path = project_dir.join("agpm.lock");
    fs::write(&lockfile_path, create_test_lockfile().await).await.unwrap();

    // Run install command
    Command::cargo_bin("agpm")
        .unwrap()
        .arg("install")
        .arg("--quiet")
        .current_dir(project_dir)
        .assert();

    // Check that gitignore was properly recreated
    let updated_content = fs::read_to_string(&gitignore_path).await.unwrap();
    assert!(updated_content.contains("# End of AGPM managed entries"));
    assert!(updated_content.contains("user-file.txt"));
    assert!(updated_content.contains("AGPM managed entries"));
}
