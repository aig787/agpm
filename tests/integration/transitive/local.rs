// Integration tests for local file transitive dependencies
//
// Tests transitive dependency resolution for local file dependencies
// including relative path handling, cross-directory dependencies,
// and error cases.

use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test local file dependency with transitive metadata should emit warning
///
/// This test verifies that local file dependencies (no source) that contain transitive
/// metadata in their frontmatter will NOT trigger transitive resolution. Instead, the
/// resolver should skip them and emit a warning.
///
/// This applies to both:
/// - Simple dependencies: `agent = "local.md"`
/// - Detailed dependencies without source: `agent = { path = "local.md" }`
///
/// Scenario:
/// - Local file has frontmatter with dependencies section
/// - Manifest references it as a local file dependency (no source)
/// - Should warn about skipping transitive deps
/// - Should NOT install the transitive dependencies
#[tokio::test]
async fn test_local_file_dependency_skips_transitive_with_warning() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a local file with transitive dependencies in frontmatter
    let local_agent_path = project.project_path().join("local-agent.md");
    let local_agent_content = r#"---
dependencies:
  snippets:
    - path: ../snippets/helper.md
      version: v1.0.0
---
# Local Agent
This is a local agent with transitive dependencies.
"#;
    tokio::fs::write(&local_agent_path, local_agent_content).await?;

    // Create manifest with local file dependency (no source)
    let manifest = ManifestBuilder::new().add_local_agent("local-agent", "local-agent.md").build();

    project.write_manifest(&manifest).await?;

    // Run install - now FAILS because transitive dependency resolution errors are hard failures
    // The missing file "../snippets/helper.md" causes fetch_resource_content to fail
    let output = project.run_agpm(&["install"])?;
    assert!(
        !output.success,
        "Install should fail when transitive dependency path resolution fails"
    );

    // Verify the error message indicates transitive dependency failure
    assert!(
        output.stderr.contains("Failed to resolve transitive dependency")
            || output.stderr.contains("Failed to fetch resource")
            || output.stderr.contains("file access")
            || output.stderr.contains("File system error: resolving path"),
        "Error should indicate transitive dependency failure, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test mixed local and remote transitive dependency tree
///
/// This test verifies that we can handle a resolution run with both local file installs
/// and remote Git source metadata extraction in the same transitive dependency tree.
///
/// Scenario:
/// - Manifest has a local file dependency (Simple path)
/// - Manifest also has a Git source dependency with transitive deps
/// - Both should install correctly in the same run
/// - Ensures local installs and remote metadata fetching coexist
#[tokio::test]
async fn test_mixed_local_remote_transitive_tree() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create remote snippet that will be a transitive dependency
    repo.add_resource("snippets", "remote-helper", "# Remote Helper\n\nFrom Git source.").await?;

    // Create remote agent that depends on remote-helper (transitive)
    repo.add_resource(
        "agents",
        "remote-parent",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/remote-helper.md
      version: v1.0.0
---
# Remote Parent Agent
Depends on remote-helper from same Git source.
"#,
    )
    .await?;

    repo.commit_all("Add remote resources")?;
    repo.tag_version("v1.0.0")?;

    // Create a local file (no transitive deps, just a Simple dependency)
    let local_snippet_path = project.project_path().join("local-snippet.md");
    let local_snippet_content = "# Local Snippet\n\nLocal file without transitive dependencies.";
    tokio::fs::write(&local_snippet_path, local_snippet_content).await?;

    // Create manifest with both local file and remote Git dependency (use relative path)
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("remote-parent", "community", "agents/remote-parent.md")
        .add_local_snippet("local-snippet", "local-snippet.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should handle both local and remote in same run
    project.run_agpm(&["install"])?.assert_success();

    // Verify local snippet is installed
    // Note: Snippets default to tool="agpm", so they install to .agpm/snippets/
    let installed_local = project.project_path().join(".agpm/snippets/local-snippet.md");
    assert!(
        tokio::fs::metadata(&installed_local).await.is_ok(),
        "Local snippet should be installed"
    );

    // Verify remote parent agent is installed
    let installed_remote_parent =
        project.project_path().join(".claude/agents/agpm/remote-parent.md");
    assert!(
        tokio::fs::metadata(&installed_remote_parent).await.is_ok(),
        "Remote parent agent should be installed"
    );

    // Verify transitive remote helper is installed
    // Note: Transitive snippets inherit parent agent's tool (claude-code)
    let installed_remote_helper =
        project.project_path().join(".claude/snippets/agpm/snippets/remote-helper.md");
    assert!(
        tokio::fs::metadata(&installed_remote_helper).await.is_ok(),
        "Remote helper (transitive) should be installed"
    );

    // Verify lockfile has all three resources
    let lockfile_content = project.read_lockfile().await?;
    // All dependencies use canonical names
    // Local snippet path is "local-snippet.md" (no resource type prefix), so name is just "local-snippet"
    assert!(
        lockfile_content.contains(r#"name = "local-snippet""#),
        "Lockfile should contain local-snippet"
    );
    assert!(
        lockfile_content.contains(r#"name = "agents/remote-parent""#),
        "Lockfile should contain remote-parent with canonical name"
    );
    // Transitive dependency
    assert!(
        lockfile_content.contains(r#"name = "snippets/remote-helper""#),
        "Lockfile should contain remote-helper (transitive)"
    );

    // Verify the remote resources have source = "community"
    assert!(
        lockfile_content.contains(r#"source = "community""#),
        "Lockfile should show community source for remote resources"
    );

    Ok(())
}

/// Test local file dependency with same-directory transitive dependency (./relative)
///
/// This test verifies that local file dependencies (path-only, no Git source) can now
/// declare transitive dependencies using file-relative paths starting with `./`.
///
/// Scenario:
/// - Local agent at `agents/local-agent.md` depends on `./helper.md`
/// - Helper is in the same directory: `agents/helper.md`
/// - Both should be installed correctly
#[tokio::test]
async fn test_local_with_current_dir_transitive() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    let agents_dir = project.project_path().join("agents");
    tokio::fs::create_dir_all(&agents_dir).await?;

    // Create helper (the transitive dependency)
    let helper_path = agents_dir.join("helper.md");
    tokio::fs::write(&helper_path, "# Helper Agent\n\nA helper agent without dependencies.")
        .await?;

    // Create main agent that depends on helper via ./relative path
    let local_agent_path = agents_dir.join("local-agent.md");
    tokio::fs::write(
        &local_agent_path,
        r#"---
dependencies:
  agents:
    - path: ./helper.md
---
# Local Agent
This is a local agent with a transitive dependency on ./helper.md.
"#,
    )
    .await?;

    // Create manifest with local file dependency (relative path)
    let manifest =
        ManifestBuilder::new().add_local_agent("local-agent", "agents/local-agent.md").build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Verify both agents were installed
    let installed_local = project.project_path().join(".claude/agents/agpm/local-agent.md");
    let installed_helper = project.project_path().join(".claude/agents/agpm/helper.md");

    assert!(
        tokio::fs::metadata(&installed_local).await.is_ok(),
        "Local agent should be installed at {:?}",
        installed_local
    );
    assert!(
        tokio::fs::metadata(&installed_helper).await.is_ok(),
        "Helper agent (transitive) should be installed at {:?}",
        installed_helper
    );

    // Verify lockfile contains both resources
    let lockfile_content = project.read_lockfile().await?;
    // Both direct and transitive dependencies use canonical names with resource type directory
    // Direct manifest dependency also has manifest_alias field
    assert!(
        lockfile_content.contains(r#"name = "agents/local-agent""#),
        "Lockfile should contain local-agent with canonical name. Lockfile:\n{}",
        lockfile_content
    );
    // Transitive dependency has canonical name but no manifest_alias
    assert!(
        lockfile_content.contains(r#"name = "agents/helper""#),
        "Lockfile should contain helper (transitive). Lockfile:\n{}",
        lockfile_content
    );
    // Verify path uses forward slashes
    assert!(lockfile_content.contains(r#"path = "agents/helper.md""#));

    Ok(())
}

/// Test local file dependency with parent-directory transitive dependency (../relative)
///
/// This test verifies that local file dependencies can declare transitive dependencies
/// using file-relative paths with `..` to navigate up directories.
///
/// Scenario:
/// - Local agent at `agents/subfolder/local-agent.md` depends on `../helper.md`
/// - Helper resolves to `agents/helper.md`
/// - Both should be installed correctly
#[tokio::test]
async fn test_local_with_parent_dir_transitive() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    let agents_dir = project.project_path().join("agents");
    let subfolder = agents_dir.join("subfolder");
    tokio::fs::create_dir_all(&subfolder).await?;

    // Create helper in parent directory
    let helper_path = agents_dir.join("helper.md");
    tokio::fs::write(&helper_path, "# Helper Agent\n\nA helper agent without dependencies.")
        .await?;

    // Create main agent in subfolder that depends on ../helper.md
    let local_agent_path = subfolder.join("local-agent.md");
    tokio::fs::write(
        &local_agent_path,
        r#"---
dependencies:
  agents:
    - path: ../helper.md
---
# Local Agent
This agent depends on ../helper.md (parent directory).
"#,
    )
    .await?;

    // Create manifest (path is agents/subfolder/local-agent.md, preserving subdirectory)
    let manifest = ManifestBuilder::new()
        .add_agent("local-agent", |d| d.path("agents/subfolder/local-agent.md").flatten(false))
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Verify both agents were installed
    // Local agent preserves full path: agents/subfolder/local-agent.md -> agpm/agents/subfolder/local-agent.md
    let installed_local =
        project.project_path().join(".claude/agents/agpm/agents/subfolder/local-agent.md");
    // Helper after resolving ../: agents/helper.md -> agpm/helper.md (agents/ prefix stripped)
    let installed_helper = project.project_path().join(".claude/agents/agpm/helper.md");

    assert!(tokio::fs::metadata(&installed_local).await.is_ok(), "Local agent should be installed");
    assert!(
        tokio::fs::metadata(&installed_helper).await.is_ok(),
        "Helper agent (transitive from parent dir) should be installed"
    );

    // Verify lockfile
    let lockfile_content = project.read_lockfile().await?;
    // Both direct and transitive use canonical names
    // Direct dependency preserves subdirectory: agents/subfolder/local-agent.md -> agents/subfolder/local-agent
    assert!(lockfile_content.contains(r#"name = "agents/subfolder/local-agent""#));
    assert!(lockfile_content.contains(r#"name = "agents/helper""#));
    // Verify path uses forward slashes
    assert!(lockfile_content.contains(r#"path = "agents/helper.md""#));

    Ok(())
}

/// Test local file dependency with cross-directory transitive dependency
///
/// This test verifies that transitive dependencies can navigate across different
/// resource type directories (e.g., agents -> snippets).
///
/// Scenario:
/// - Local agent at `agents/local-agent.md` depends on `../snippets/utils.md`
/// - Utils snippet is at `snippets/utils.md`
/// - Both should be installed to their correct directories
#[tokio::test]
async fn test_local_with_cross_directory_transitive() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    let agents_dir = project.project_path().join("agents");
    let snippets_dir = project.project_path().join("snippets");
    tokio::fs::create_dir_all(&agents_dir).await?;
    tokio::fs::create_dir_all(&snippets_dir).await?;

    // Create snippet (the transitive dependency)
    let utils_path = snippets_dir.join("utils.md");
    tokio::fs::write(&utils_path, "# Utils Snippet\n\nUtility functions.").await?;

    // Create agent that depends on snippet in different directory
    let local_agent_path = agents_dir.join("local-agent.md");
    tokio::fs::write(
        &local_agent_path,
        r#"---
dependencies:
  snippets:
    - path: ../snippets/utils.md
---
# Local Agent
This agent depends on a snippet in a different directory.
"#,
    )
    .await?;

    // Create manifest (use relative path)
    let manifest =
        ManifestBuilder::new().add_local_agent("local-agent", "agents/local-agent.md").build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Verify agent installed to .claude/agents
    let installed_agent = project.project_path().join(".claude/agents/agpm/local-agent.md");
    assert!(tokio::fs::metadata(&installed_agent).await.is_ok(), "Local agent should be installed");

    // Verify snippet installed to .claude/snippets (inherits parent agent's default tool)
    // Note: Transitive snippets inherit parent's tool (claude-code from agent)
    let installed_snippet = project.project_path().join(".claude/snippets/agpm/snippets/utils.md");
    assert!(
        tokio::fs::metadata(&installed_snippet).await.is_ok(),
        "Utils snippet (transitive) should be installed to .claude/snippets (inheriting parent's tool)"
    );

    // Verify lockfile
    let lockfile_content = project.read_lockfile().await?;
    // Both direct and transitive use canonical names
    assert!(lockfile_content.contains(r#"name = "agents/local-agent""#));
    assert!(lockfile_content.contains(r#"name = "snippets/utils""#));
    // Verify path uses forward slashes
    assert!(lockfile_content.contains(r#"path = "snippets/utils.md""#));

    Ok(())
}

/// Test local file transitive dependency with non-existent file
///
/// This test verifies proper error handling when a transitive dependency
/// path resolves to a file that doesn't exist.
///
/// Scenario:
/// - Local agent declares transitive dependency on ./missing.md
/// - File doesn't exist
/// - Should emit warning and skip the transitive dep (not fail install)
#[tokio::test]
async fn test_local_transitive_missing_file() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    let agents_dir = project.project_path().join("agents");
    tokio::fs::create_dir_all(&agents_dir).await?;

    // Create agent with transitive dep pointing to non-existent file
    let local_agent_path = agents_dir.join("local-agent.md");
    tokio::fs::write(
        &local_agent_path,
        r#"---
dependencies:
  agents:
    - path: ./missing.md
---
# Local Agent
This agent has a transitive dependency that doesn't exist.
"#,
    )
    .await?;

    // Create manifest
    let manifest =
        ManifestBuilder::new().add_local_agent("local-agent", "agents/local-agent.md").build();

    project.write_manifest(&manifest).await?;

    // Run install - now FAILS because missing transitive dep is a hard failure
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail when transitive dependency is missing");

    // Verify the error message indicates the failure
    assert!(
        output.stderr.contains("Failed to resolve transitive dependency")
            || output.stderr.contains("Failed to fetch resource")
            || output.stderr.contains("file access")
            || output.stderr.contains("File system error: resolving path"),
        "Error should indicate transitive dependency failure, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that bare filenames in transitive dependencies are auto-normalized
///
/// This test verifies that transitive dependency paths without `./` or `../` prefixes
/// are automatically normalized to file-relative paths (e.g., `helper.md` → `./helper.md`).
///
/// Scenario:
/// - Local agent declares transitive dependency with bare filename (no ./ or ../)
/// - Should auto-normalize to `./helper.md` and resolve successfully
/// - Both agents should be installed
#[tokio::test]
async fn test_local_transitive_invalid_path_format() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create directory structure
    let agents_dir = project.project_path().join("agents");
    tokio::fs::create_dir_all(&agents_dir).await?;

    // Create helper file
    let helper_path = agents_dir.join("helper.md");
    tokio::fs::write(&helper_path, "# Helper\n").await?;

    // Create agent with bare filename transitive dep (auto-normalized to ./helper.md)
    let local_agent_path = agents_dir.join("local-agent.md");
    tokio::fs::write(
        &local_agent_path,
        r#"---
dependencies:
  agents:
    - path: helper.md
---
# Local Agent
This agent has a bare filename transitive dependency that gets auto-normalized.
"#,
    )
    .await?;

    // Create manifest
    let manifest =
        ManifestBuilder::new().add_local_agent("local-agent", "agents/local-agent.md").build();

    project.write_manifest(&manifest).await?;

    // Run install - should SUCCEED with auto-normalization
    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed with auto-normalized bare filename: {}",
        output.stderr
    );

    // Verify both agents were installed
    let installed_agent = project.project_path().join(".claude/agents/agpm/local-agent.md");
    let installed_helper = project.project_path().join(".claude/agents/agpm/helper.md");
    assert!(installed_agent.exists(), "Local agent should be installed");
    assert!(
        installed_helper.exists(),
        "Helper agent should be installed via transitive dependency"
    );

    Ok(())
}

/// Test local file transitive dependency outside the manifest directory
///
/// This is a regression test for the bug where transitive dependencies
/// resolved to paths outside the manifest directory would fail with
/// "not under manifest directory" error.
///
/// Scenario:
/// - Create a shared directory outside the project
/// - Local agent in project references snippet in shared directory
/// - Should use absolute paths in lockfile for cross-directory references
/// - Should install successfully to tool-specific directory
#[tokio::test]
async fn test_local_transitive_outside_manifest_directory() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    // Create the test project first
    let project = TestProject::new().await?;

    // Create a shared directory as a sibling to the project directory
    // Structure: <temp_parent>/project/ and <temp_parent>/shared/
    let project_parent = project.project_path().parent().unwrap();
    let shared_dir = project_parent.join("shared");
    tokio::fs::create_dir_all(&shared_dir).await?;

    // Create a shared snippet
    let shared_snippet = shared_dir.join("utils.md");
    tokio::fs::write(
        &shared_snippet,
        r#"# Shared Utils
Common utilities shared across projects.
"#,
    )
    .await?;

    // Create agent that references the shared snippet (outside manifest directory)
    let agents_dir = project.project_path().join("agents");
    tokio::fs::create_dir_all(&agents_dir).await?;

    // Calculate relative path from agent to shared directory
    // <project>/agents/my-agent.md -> ../../shared/utils.md
    let relative_to_shared = "../../shared/utils.md";

    let agent_path = agents_dir.join("my-agent.md");
    tokio::fs::write(
        &agent_path,
        format!(
            r#"---
dependencies:
  snippets:
    - path: {}
      tool: agpm
---
# My Agent
Uses a shared snippet outside the project directory.
"#,
            relative_to_shared
        ),
    )
    .await?;

    // Create manifest
    let manifest = ManifestBuilder::new().add_local_agent("my-agent", "agents/my-agent.md").build();

    project.write_manifest(&manifest).await?;

    // Run install - should succeed with manifest-relative path handling
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed with cross-directory transitive dependency");

    // Verify agent installed
    let installed_agent = project.project_path().join(".claude/agents/agpm/my-agent.md");
    assert!(tokio::fs::metadata(&installed_agent).await.is_ok(), "Agent should be installed");

    // Verify shared snippet installed to agpm snippets directory (since tool=agpm was specified)
    // For manifest-relative paths with ../, like "../shared/utils.md",
    // the installed path becomes ".agpm/snippets/shared/utils.md" (strips ../, installs to snippets dir)
    let expected_snippet_path = project.project_path().join(".agpm/snippets/shared/utils.md");
    assert!(
        tokio::fs::metadata(&expected_snippet_path).await.is_ok(),
        "Shared snippet should be installed at .agpm/snippets/shared/utils.md"
    );

    // Verify content matches
    let installed_content = tokio::fs::read(&expected_snippet_path).await?;
    let expected_content = b"# Shared Utils\nCommon utilities shared across projects.\n";
    assert_eq!(
        installed_content, expected_content,
        "Installed snippet should have correct content"
    );

    // Verify lockfile contains manifest-relative path (even with ../) for portability
    let lockfile_content = project.read_lockfile().await?;
    // Direct manifest dependency uses canonical name
    assert!(
        lockfile_content.contains(r#"name = "agents/my-agent""#),
        "Lockfile should contain agent with canonical name"
    );

    // The path should be manifest-relative with ../ since it's outside the project
    // Structure: <parent>/project/ and <parent>/shared/utils.md
    // Relative path from project to shared: ../shared/utils.md
    assert!(
        lockfile_content.contains(r#"path = "../shared/utils.md""#),
        "Lockfile should contain manifest-relative path (with ../) for cross-directory dependency.\nLockfile:\n{}",
        lockfile_content
    );

    // Verify tool field is set correctly
    assert!(
        lockfile_content.contains(r#"tool = "agpm""#),
        "Lockfile should specify agpm tool for snippet"
    );

    Ok(())
}
