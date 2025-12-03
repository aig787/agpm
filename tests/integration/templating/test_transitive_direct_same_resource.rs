//! Integration tests for template lookups when the same resource appears as both
//! a direct dependency and a transitive dependency from a different relative path.
//!
//! This tests the critical case where:
//! 1. A resource is directly declared in the manifest (e.g., `commands/checkpoint.md`)
//! 2. Another resource declares the same file as a transitive dependency using a relative
//!    path (e.g., `./checkpoint.md` from `commands/squash.md`)
//! 3. Templates in the second resource try to access the first via `agpm.deps`
//!
//! The bug was that `LockfileDependencyRef` in the transitive resolver always used
//! local format, but `extract_dependency_specs` uses Git format when the parent has
//! a source, causing template lookup mismatches.

use crate::common::TestProject;
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;

/// Test that template lookups work when the same resource is both a direct dependency
/// and a transitive dependency referenced via a relative path.
///
/// Setup:
/// - `commands/checkpoint.md`: Simple command, direct dependency
/// - `commands/squash.md`: Command with templating that depends on `./checkpoint.md`
/// - Both are direct dependencies in the manifest
/// - `squash.md` tries to embed `checkpoint.md` content via template
#[tokio::test]
async fn test_direct_and_transitive_same_resource_template_lookup() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("test").await?;

    // Create checkpoint command - simple content
    repo.add_resource(
        "commands",
        "checkpoint",
        r#"---
description: Create a checkpoint
---
# Checkpoint Command

This creates a git checkpoint for safe editing.

CHECKPOINT_MARKER_CONTENT
"#,
    )
    .await?;

    // Create squash command - depends on checkpoint via relative path
    // Uses templating to embed checkpoint content
    repo.add_resource(
        "commands",
        "squash",
        r#"---
description: Squash commits
agpm:
  templating: true
  dependencies:
    commands:
      - path: ./checkpoint.md
        install: false
---
# Squash Command

This command squashes commits. It uses the checkpoint command internally.

## Embedded Checkpoint:

{{ agpm.deps.commands.checkpoint.content }}

END_OF_SQUASH
"#,
    )
    .await?;

    repo.commit_all("Add commands")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest with BOTH as direct dependencies
    project
        .write_manifest(&format!(
            r#"
[sources]
test = "{}"

[commands]
checkpoint = {{ source = "test", path = "commands/checkpoint.md", version = "v1.0.0" }}
squash = {{ source = "test", path = "commands/squash.md", version = "v1.0.0" }}
"#,
            repo.path.display()
        ))
        .await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. stderr: {}", output.stderr);

    // Verify squash.md was installed with checkpoint content embedded
    let squash_path = project.project_path().join(".claude/commands/agpm/squash.md");
    let squash_content = fs::read_to_string(&squash_path).await?;

    // The checkpoint content should be embedded (with frontmatter stripped)
    assert!(
        squash_content.contains("CHECKPOINT_MARKER_CONTENT"),
        "Squash should contain checkpoint content. Actual content:\n{}",
        squash_content
    );

    // Verify the template was fully rendered
    assert!(squash_content.contains("END_OF_SQUASH"), "Squash should be fully rendered");

    // Should NOT contain the raw template syntax
    assert!(
        !squash_content.contains("{{ agpm.deps"),
        "Template syntax should be rendered, not raw. Content:\n{}",
        squash_content
    );

    Ok(())
}

/// Test with parent directory relative path (../sibling/file.md)
#[tokio::test]
async fn test_parent_relative_path_transitive_lookup() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("test").await?;

    // Create helper snippet in snippets/
    repo.add_resource(
        "snippets",
        "helper",
        r#"---
description: Helper utilities
---
# Helper Snippet

HELPER_UNIQUE_CONTENT
"#,
    )
    .await?;

    // Create agent in agents/ that depends on ../snippets/helper.md
    repo.add_resource(
        "agents",
        "parent",
        r#"---
description: Parent agent
agpm:
  templating: true
  dependencies:
    snippets:
      - path: ../snippets/helper.md
        install: false
---
# Parent Agent

Uses helper snippet:

{{ agpm.deps.snippets.helper.content }}

END_OF_PARENT
"#,
    )
    .await?;

    repo.commit_all("Add resources")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest with both as direct dependencies
    project
        .write_manifest(&format!(
            r#"
[sources]
test = "{}"

[snippets]
helper = {{ source = "test", path = "snippets/helper.md", version = "v1.0.0" }}

[agents]
parent = {{ source = "test", path = "agents/parent.md", version = "v1.0.0" }}
"#,
            repo.path.display()
        ))
        .await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. stderr: {}", output.stderr);

    // Verify parent.md was installed with helper content embedded
    let parent_path = project.project_path().join(".claude/agents/agpm/parent.md");
    let parent_content = fs::read_to_string(&parent_path).await?;

    assert!(
        parent_content.contains("HELPER_UNIQUE_CONTENT"),
        "Parent should contain helper content. Actual content:\n{}",
        parent_content
    );

    assert!(!parent_content.contains("{{ agpm.deps"), "Template syntax should be rendered");

    Ok(())
}

/// Test with LOCAL dependencies (no source) where the transitive dependency
/// uses a custom name and relative path.
///
/// This matches the user's scenario:
/// - Manifest has local deps: `checkpoint = { path = "../artifacts/commands/checkpoint.md" }`
/// - Manifest has local deps: `squash = { path = "../artifacts/commands/squash.md" }`
/// - `squash.md` has transitive dep: `{ name: checkpoint, path: ./checkpoint.md }`
/// - Template uses: `{{ agpm.deps.commands.checkpoint.install_path }}`
#[tokio::test]
async fn test_local_deps_with_custom_name_transitive_lookup() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a sibling directory to simulate "../artifacts/"
    let artifacts_dir = project.project_path().parent().unwrap().join("artifacts");
    let commands_dir = artifacts_dir.join("commands");
    fs::create_dir_all(&commands_dir).await?;

    // Create checkpoint command
    let checkpoint_path = commands_dir.join("checkpoint.md");
    fs::write(
        &checkpoint_path,
        r#"---
description: Create a checkpoint
---
# Checkpoint Command

CHECKPOINT_INSTALL_PATH_MARKER
"#,
    )
    .await?;

    // Create squash command with transitive dep using custom name
    let squash_path = commands_dir.join("squash.md");
    fs::write(
        &squash_path,
        r#"---
description: Squash commits
agpm:
  templating: true
dependencies:
  commands:
    - name: checkpoint
      path: ./checkpoint.md
---
# Squash Command

Uses checkpoint at: {{ agpm.deps.commands.checkpoint.install_path }}

END_OF_SQUASH
"#,
    )
    .await?;

    // Create manifest with LOCAL dependencies (no source)
    let relative_path = PathBuf::from("../artifacts/commands");
    project
        .write_manifest(&format!(
            r#"
[commands]
checkpoint = {{ path = "{}/checkpoint.md" }}
squash = {{ path = "{}/squash.md" }}
"#,
            relative_path.display(),
            relative_path.display()
        ))
        .await?;

    // Install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. stderr: {}", output.stderr);

    // Verify squash.md was installed with checkpoint install_path embedded
    let squash_installed_path = project.project_path().join(".claude/commands/agpm/squash.md");
    let squash_content = fs::read_to_string(&squash_installed_path).await?;

    // The checkpoint install_path should be embedded
    assert!(
        squash_content.contains(".claude/commands/agpm/checkpoint.md")
            || squash_content.contains("commands/agpm/checkpoint"),
        "Squash should contain checkpoint install_path. Actual content:\n{}",
        squash_content
    );

    // Should NOT contain the raw template syntax
    assert!(
        !squash_content.contains("{{ agpm.deps"),
        "Template syntax should be rendered, not raw. Content:\n{}",
        squash_content
    );

    Ok(())
}

/// Test with LOCAL dependencies where transitive dep accesses .content
#[tokio::test]
async fn test_local_deps_transitive_content_access() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create sibling directory
    let artifacts_dir = project.project_path().parent().unwrap().join("artifacts2");
    let commands_dir = artifacts_dir.join("commands");
    fs::create_dir_all(&commands_dir).await?;

    // Create helper command
    let helper_path = commands_dir.join("helper.md");
    fs::write(
        &helper_path,
        r#"---
description: Helper utility
---
# Helper Command

HELPER_CONTENT_MARKER
"#,
    )
    .await?;

    // Create main command with transitive dep
    let main_path = commands_dir.join("main.md");
    fs::write(
        &main_path,
        r#"---
description: Main command
agpm:
  templating: true
dependencies:
  commands:
    - name: helper
      path: ./helper.md
      install: false
---
# Main Command

Embedded helper content:

{{ agpm.deps.commands.helper.content }}

END_OF_MAIN
"#,
    )
    .await?;

    // Create manifest with LOCAL dependencies
    project
        .write_manifest(
            r#"
[commands]
helper = { path = "../artifacts2/commands/helper.md" }
main = { path = "../artifacts2/commands/main.md" }
"#,
        )
        .await?;

    // Install
    let output = project.run_agpm(&["install"])?;

    // Debug: Print the lockfile
    let lockfile_content = project.read_lockfile().await.unwrap_or_default();
    eprintln!("=== LOCKFILE ===\n{}\n=== END LOCKFILE ===", lockfile_content);

    assert!(output.success, "Install should succeed. stderr: {}", output.stderr);

    // Verify main.md was installed with helper content embedded
    let main_installed_path = project.project_path().join(".claude/commands/agpm/main.md");
    let main_content = fs::read_to_string(&main_installed_path).await?;

    // The helper content should be embedded (with frontmatter stripped)
    assert!(
        main_content.contains("HELPER_CONTENT_MARKER"),
        "Main should contain helper content. Actual content:\n{}",
        main_content
    );

    // Should NOT contain the raw template syntax
    assert!(
        !main_content.contains("{{ agpm.deps"),
        "Template syntax should be rendered, not raw. Content:\n{}",
        main_content
    );

    Ok(())
}
