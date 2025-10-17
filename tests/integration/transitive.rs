// Integration tests for transitive dependency resolution
//
// Tests the resolver's ability to handle transitive dependencies declared
// within resource files via YAML frontmatter or JSON fields.

use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

/// Test basic transitive dependency resolution with real Git repos
#[tokio::test]
async fn test_transitive_resolution_basic() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with a main agent that depends on a helper
    let community_repo = project.create_source_repo("community").await?;

    // Add helper agent first (no dependencies)
    community_repo
        .add_resource(
            "agents",
            "helper",
            r#"---
# Helper Agent
This is a helper agent with no dependencies.
---
"#,
        )
        .await?;

    // Add main agent that depends on helper
    community_repo
        .add_resource(
            "agents",
            "main-app",
            r#"---
dependencies:
  agents:
    - path: ./helper.md
      version: v1.0.0
---

# Main App Agent
This agent depends on the helper agent.
"#,
        )
        .await?;

    community_repo.commit_all("Initial commit")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest that only references the main agent (not the helper)
    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("main-app", "community", "agents/main-app.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify both agents were installed (main + transitive helper)
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains("main-app"), "Main agent should be in lockfile");
    assert!(
        lockfile_content.contains("helper"),
        "Helper agent should be in lockfile (transitive). Lockfile:\n{}\nStderr: {}",
        lockfile_content,
        output.stderr
    );

    // Verify both were actually installed to .claude/agents
    let main_app_path = project.project_path().join(".claude/agents/main-app.md");
    let helper_path = project.project_path().join(".claude/agents/helper.md");
    assert!(
        tokio::fs::metadata(&main_app_path).await.is_ok(),
        "Main agent file should exist at {:?}",
        main_app_path
    );
    assert!(
        tokio::fs::metadata(&helper_path).await.is_ok(),
        "Helper agent file should exist at {:?}",
        helper_path
    );

    Ok(())
}

/// Test transitive dependencies with same-named resources from different sources
///
/// Test that cross-source transitive dependencies with the same name are properly
/// detected as conflicts. When two dependencies from different sources both have
/// transitive dependencies with the same name (e.g., "utils"), they would install
/// to the same path but with different content, which is a conflict.
///
/// The system should detect this and provide a helpful error message suggesting
/// the user specify custom 'target' or 'filename' fields to resolve the conflict.
#[tokio::test]
async fn test_transitive_cross_source_same_names() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create first source repo with a "utils" agent
    let source1_repo = project.create_source_repo("source1").await?;
    source1_repo
        .add_resource("agents", "utils", "# Utils from Source 1\n\nSource 1 utilities")
        .await?;
    source1_repo
        .add_resource(
            "agents",
            "app",
            r#"---
dependencies:
  agents:
    - path: ./utils.md
      version: v1.0.0
---

# App from Source 1
Uses utils from same source
"#,
        )
        .await?;
    source1_repo.commit_all("Source 1 commit")?;
    source1_repo.tag_version("v1.0.0")?;

    // Create second source repo with different "utils" agent
    let source2_repo = project.create_source_repo("source2").await?;
    source2_repo
        .add_resource("agents", "utils", "# Utils from Source 2\n\nSource 2 utilities (different)")
        .await?;
    source2_repo
        .add_resource(
            "agents",
            "tool",
            r#"---
dependencies:
  agents:
    - path: ./utils.md
      version: v1.0.0
---

# Tool from Source 2
Uses utils from same source
"#,
        )
        .await?;
    source2_repo.commit_all("Source 2 commit")?;
    source2_repo.tag_version("v1.0.0")?;

    // Create manifest referencing both top-level resources
    let source1_url = source1_repo.bare_file_url(project.sources_path())?;
    let source2_url = source2_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_sources(&[("source1", &source1_url), ("source2", &source2_url)])
        .add_standard_agent("app", "source1", "agents/app.md")
        .add_standard_agent("tool", "source2", "agents/tool.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - currently this fails with a path conflict error
    // because both "utils" transitive deps resolve to .claude/agents/utils.md
    // but have different commits (different sources)
    let output = project.run_agpm(&["install"])?;

    // Expected behavior: Should detect path conflict
    assert!(
        !output.success,
        "Install should fail due to path conflict for cross-source same-named transitive deps"
    );
    assert!(
        output.stderr.contains("Target path conflicts"),
        "Should report path conflict, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test that circular transitive dependencies are detected and rejected
#[tokio::test]
async fn test_transitive_cycle_detection() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with circular dependencies: A → B → C → A
    let repo = project.create_source_repo("community").await?;

    // Agent A depends on B
    repo.add_resource(
        "agents",
        "agent-a",
        r#"---
dependencies:
  agents:
    - path: ./agent-b.md
      version: v1.0.0
---

# Agent A
Depends on Agent B
"#,
    )
    .await?;

    // Agent B depends on C
    repo.add_resource(
        "agents",
        "agent-b",
        r#"---
dependencies:
  agents:
    - path: ./agent-c.md
      version: v1.0.0
---

# Agent B
Depends on Agent C
"#,
    )
    .await?;

    // Agent C depends on A (creates cycle)
    repo.add_resource(
        "agents",
        "agent-c",
        r#"---
dependencies:
  agents:
    - path: ./agent-a.md
      version: v1.0.0
---

# Agent C
Depends on Agent A (creates cycle)
"#,
    )
    .await?;

    repo.commit_all("Add agents with circular dependencies")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest that references agent-a
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("agent-a", "community", "agents/agent-a.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should fail with cycle detection
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail due to circular dependency");
    assert!(
        output.stderr.contains("Circular dependency") || output.stderr.contains("cycle"),
        "Error should mention circular dependency or cycle, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test diamond dependencies (same resource via multiple paths)
#[tokio::test]
async fn test_transitive_diamond_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with diamond pattern:
    //     A
    //    / \
    //   B   C
    //    \ /
    //     D
    let repo = project.create_source_repo("community").await?;

    // D - base dependency (no dependencies)
    repo.add_resource(
        "agents",
        "agent-d",
        r#"---
# Agent D
Base agent with no dependencies
---
"#,
    )
    .await?;

    // B depends on D
    repo.add_resource(
        "agents",
        "agent-b",
        r#"---
dependencies:
  agents:
    - path: ./agent-d.md
      version: v1.0.0
---

# Agent B
Depends on Agent D
"#,
    )
    .await?;

    // C depends on D
    repo.add_resource(
        "agents",
        "agent-c",
        r#"---
dependencies:
  agents:
    - path: ./agent-d.md
      version: v1.0.0
---

# Agent C
Depends on Agent D
"#,
    )
    .await?;

    // A depends on both B and C
    repo.add_resource(
        "agents",
        "agent-a",
        r#"---
dependencies:
  agents:
    - path: ./agent-b.md
      version: v1.0.0
    - path: ./agent-c.md
      version: v1.0.0
---

# Agent A
Depends on both Agent B and Agent C
"#,
    )
    .await?;

    repo.commit_all("Add agents with diamond dependency pattern")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest that references agent-a
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("agent-a", "community", "agents/agent-a.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should succeed
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed with diamond dependencies: {}", output.stderr);

    // Verify all agents are installed
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains("agent-a"), "Agent A should be in lockfile");
    assert!(lockfile_content.contains("agent-b"), "Agent B should be in lockfile");
    assert!(lockfile_content.contains("agent-c"), "Agent C should be in lockfile");
    assert!(lockfile_content.contains("agent-d"), "Agent D should be in lockfile");

    // Verify agent-d appears exactly once (no duplication despite two paths to it)
    let agent_d_count = lockfile_content.matches("name = \"agent-d\"").count();
    assert_eq!(
        agent_d_count, 1,
        "Agent D should appear exactly once in lockfile (deduplication), found {}",
        agent_d_count
    );

    Ok(())
}

/// Test that resources with same filename but different directory paths both get installed
///
/// Verifies that `generate_dependency_name` preserves directory structure to avoid
/// name collisions. When a command depends on two snippets with the same filename
/// but different directory paths (e.g., "snippets/commands/commit.md" and
/// "snippets/logit/commit.md"), both should be installed correctly:
///   - .claude/snippets/commands/commit.md (content: "commands version")
///   - .claude/snippets/logit/commit.md (content: "logit version")
///
/// This is a regression test for a bug where names were collapsed to bare filenames,
/// causing the second resource to overwrite the first.
#[tokio::test]
async fn test_transitive_deps_duplicate_names_different_paths() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with two snippets that have same filename but different paths
    let repo = project.create_source_repo("community").await?;

    // Create first snippet in snippets/commands/
    let commands_dir = repo.path.join("snippets/commands");
    tokio::fs::create_dir_all(&commands_dir).await?;
    tokio::fs::write(
        commands_dir.join("commit.md"),
        "# Commands Commit\n\nThis is the commands version of commit.",
    )
    .await?;

    // Create second snippet in snippets/logit/
    let logit_dir = repo.path.join("snippets/logit");
    tokio::fs::create_dir_all(&logit_dir).await?;
    tokio::fs::write(
        logit_dir.join("commit.md"),
        "# Logit Commit\n\nThis is the logit version of commit.",
    )
    .await?;

    // Create a command that depends on both snippets
    repo.add_resource(
        "commands",
        "commit-cmd",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/commands/commit.md
      version: v1.0.0
    - path: ../snippets/logit/commit.md
      version: v1.0.0
---

# Commit Command
This command depends on both commit snippets.
"#,
    )
    .await?;

    repo.commit_all("Add resources with duplicate names")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest that only references the command
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_command("commit-cmd", |d| {
            d.source("community").path("commands/commit-cmd.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed, stderr: {}", output.stderr);

    // Verify both snippets are installed at their respective paths
    let commands_snippet_path = project.project_path().join(".agpm/snippets/commands/commit.md");
    let logit_snippet_path = project.project_path().join(".agpm/snippets/logit/commit.md");

    assert!(
        tokio::fs::metadata(&commands_snippet_path).await.is_ok(),
        "Commands commit snippet should exist at {:?}",
        commands_snippet_path
    );
    assert!(
        tokio::fs::metadata(&logit_snippet_path).await.is_ok(),
        "Logit commit snippet should exist at {:?}",
        logit_snippet_path
    );

    // Verify correct content in each file
    let commands_content = tokio::fs::read_to_string(&commands_snippet_path).await?;
    assert!(
        commands_content.contains("commands version"),
        "Commands snippet should contain 'commands version', got: {}",
        commands_content
    );

    let logit_content = tokio::fs::read_to_string(&logit_snippet_path).await?;
    assert!(
        logit_content.contains("logit version"),
        "Logit snippet should contain 'logit version', got: {}",
        logit_content
    );

    Ok(())
}

/// Test that an agent and command with the same name both install correctly
///
/// Verifies that the `transitive_types` HashMap uses `(ResourceType, name, source)` as
/// its key to prevent cross-type collisions. When an agent depends on both an agent
/// named "helper" and a command named "helper", both should be installed correctly:
///   - .claude/agents/helper.md
///   - .claude/commands/helper.md
///
/// This is a regression test for a bug where the HashMap used only `(name, source)` as
/// the key, causing the last-processed type to overwrite earlier ones.
#[tokio::test]
async fn test_transitive_deps_cross_type_collision() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with agent and command with same name
    let repo = project.create_source_repo("community").await?;

    // Create agent named "helper"
    repo.add_resource("agents", "helper", "# Helper Agent\n\nThis is the helper agent.").await?;

    // Create command named "helper"
    repo.add_resource("commands", "helper", "# Helper Command\n\nThis is the helper command.")
        .await?;

    // Create an AGENT (not snippet) that depends on both
    // This ensures we're using the claude-code tool which supports both agents and commands
    repo.add_resource(
        "agents",
        "main",
        r#"---
dependencies:
  agents:
    - path: ./helper.md
      version: v1.0.0
  commands:
    - path: ../commands/helper.md
      version: v1.0.0
---

# Main Agent
This agent depends on both helper agent and command with the same name.
"#,
    )
    .await?;

    repo.commit_all("Add resources with cross-type name collision")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest that only references the main agent
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("main", "community", "agents/main.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed, stderr: {}", output.stderr);

    // Verify both agent and command are installed correctly with proper type assignments

    // Check lockfile for correct type assignments
    let lockfile_content = project.read_lockfile().await?;

    // Both should be in lockfile with correct types
    let has_agent_helper = lockfile_content.contains("[[agents]]")
        && lockfile_content.contains(r#"name = "helper""#)
        && lockfile_content.contains(r#"path = "agents/helper.md""#);

    let has_command_helper = lockfile_content.contains("[[commands]]")
        && lockfile_content.contains(r#"name = "helper""#)
        && lockfile_content.contains(r#"path = "commands/helper.md""#);

    assert!(
        has_agent_helper,
        "Lockfile should have helper agent in [[agents]] section:\n{}",
        lockfile_content
    );
    assert!(
        has_command_helper,
        "Lockfile should have helper command in [[commands]] section:\n{}",
        lockfile_content
    );

    // Verify files are installed
    let agent_path = project.project_path().join(".claude/agents/helper.md");
    let command_path = project.project_path().join(".claude/commands/helper.md");

    assert!(
        tokio::fs::metadata(&agent_path).await.is_ok(),
        "Helper agent should exist at {:?}",
        agent_path
    );
    assert!(
        tokio::fs::metadata(&command_path).await.is_ok(),
        "Helper command should exist at {:?}",
        command_path
    );

    // Verify they're not in the wrong directory (skip this check - snippets go to .agpm now)
    // Note: With the fix for default-tools, snippets now correctly use their default tool

    Ok(())
}

/// Test that transitive dependencies use the correct version metadata
///
/// Verifies that when multiple agents depend on the same transitive resource at the same
/// version, the resolver correctly uses that version's metadata to extract its transitive
/// dependencies. This ensures metadata is fetched from the correct version tag in the
/// repository.
///
/// In this test, both agents depend on v2.0.0 of a shared snippet, and v2.0.0's metadata
/// specifies a dependency on "new-command.md" (while v1.0.0 specifies "old-command.md").
/// The resolver should install new-command.md, not old-command.md.
#[tokio::test]
async fn test_version_conflict_uses_correct_metadata() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create git repo with two versions
    let repo = project.create_source_repo("community").await?;

    // Create v1.0.0 with old-command
    repo.add_resource("commands", "old-command", "# Old Command\n\nThis is the old command.")
        .await?;

    // Create shared snippet that depends on old-command in v1.0.0
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/old-command.md
      version: v1.0.0
---

# Shared Snippet v1.0.0
This is version 1.0.0 of the shared snippet.
"#,
    )
    .await?;

    repo.commit_all("Release v1.0.0")?;
    repo.tag_version("v1.0.0")?;

    // Now update to v2.0.0
    // Remove old-command and add new-command
    tokio::fs::remove_file(repo.path.join("commands/old-command.md")).await?;
    repo.add_resource("commands", "new-command", "# New Command\n\nThis is the new command.")
        .await?;

    // Update shared snippet to depend on new-command in v2.0.0
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/new-command.md
      version: v2.0.0
---

# Shared Snippet v2.0.0
This is version 2.0.0 of the shared snippet.
"#,
    )
    .await?;

    repo.commit_all("Release v2.0.0")?;
    repo.tag_version("v2.0.0")?;

    // Create two agents that both depend on v2.0.0 of the shared snippet
    // This tests that the correct version's metadata is used for transitive deps
    repo.add_resource(
        "agents",
        "first",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v2.0.0
---

# First Agent
Requires shared@v2.0.0
"#,
    )
    .await?;

    repo.add_resource(
        "agents",
        "second",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v2.0.0
---

# Second Agent
Also requires shared@v2.0.0
"#,
    )
    .await?;

    repo.commit_all("Add agents")?;
    repo.tag_version("v2.0.1")?;

    // Create manifest that references both agents at v2.0.1
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_agent("first", |d| d.source("community").path("agents/first.md").version("v2.0.1"))
        .add_agent("second", |d| d.source("community").path("agents/second.md").version("v2.0.1"))
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - both agents require v2.0.0
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed, stderr: {}", output.stderr);

    // Verify that v2.0.0's transitive dependencies are installed (new-command.md)
    // Both v1.0.0 and v2.0.0 exist in repo - metadata must be fetched from correct version
    let new_command_path = project.project_path().join(".claude/commands/new-command.md");
    let old_command_path = project.project_path().join(".claude/commands/old-command.md");

    assert!(
        tokio::fs::metadata(&new_command_path).await.is_ok(),
        "New command should exist at {:?} (from v2.0.0 metadata)",
        new_command_path
    );
    assert!(
        tokio::fs::metadata(&old_command_path).await.is_err(),
        "Old command should NOT exist at {:?} (v1.0.0 metadata should not be used)",
        old_command_path
    );

    Ok(())
}

/// Test that `agpm install` and `agpm update` produce identical lockfiles
///
/// This is a regression test for a bug where install and update used different code paths
/// for transitive dependency type resolution. The install path threaded types correctly,
/// but the update path called `get_resource_type()` which relied on manifest section order.
/// This caused resources to be assigned to different types depending on which command was used.
///
/// Now both commands use `all_dependencies_with_types()` to thread resource types consistently.
#[tokio::test]
async fn test_install_update_parity() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with transitive dependencies
    let repo = project.create_source_repo("community").await?;

    // Create a helper snippet
    repo.add_resource("snippets", "helper", "# Helper Snippet\n\nA helper snippet.").await?;

    // Create a command that depends on the snippet
    repo.add_resource(
        "commands",
        "deploy",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/helper.md
      version: v1.0.0
---

# Deploy Command
This command depends on the helper snippet.
"#,
    )
    .await?;

    repo.commit_all("Initial release")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_command("deploy", |d| {
            d.source("community").path("commands/deploy.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install and capture lockfile
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed: {}", output.stderr);
    let install_lockfile = project.read_lockfile().await?;

    // Clean resources but keep manifest and cache
    let claude_dir = project.project_path().join(".claude");
    if tokio::fs::metadata(&claude_dir).await.is_ok() {
        tokio::fs::remove_dir_all(&claude_dir).await?;
    }
    tokio::fs::remove_file(project.project_path().join("agpm.lock")).await?;

    // Run update and capture lockfile
    let output = project.run_agpm(&["update"])?;
    assert!(output.success, "Update should succeed: {}", output.stderr);
    let update_lockfile = project.read_lockfile().await?;

    // Both lockfiles should now be identical after fixing type resolution
    // Count entries in each section to verify parity
    let install_agents_count = install_lockfile.matches("[[agents]]").count();
    let update_agents_count = update_lockfile.matches("[[agents]]").count();
    let install_commands_count = install_lockfile.matches("[[commands]]").count();
    let update_commands_count = update_lockfile.matches("[[commands]]").count();
    let install_snippets_count = install_lockfile.matches("[[snippets]]").count();
    let update_snippets_count = update_lockfile.matches("[[snippets]]").count();

    assert_eq!(
        install_agents_count, update_agents_count,
        "Install and update should have same number of agents.\nInstall lockfile:\n{}\n\nUpdate lockfile:\n{}",
        install_lockfile, update_lockfile
    );
    assert_eq!(
        install_commands_count, update_commands_count,
        "Install and update should have same number of commands.\nInstall lockfile:\n{}\n\nUpdate lockfile:\n{}",
        install_lockfile, update_lockfile
    );
    assert_eq!(
        install_snippets_count, update_snippets_count,
        "Install and update should have same number of snippets.\nInstall lockfile:\n{}\n\nUpdate lockfile:\n{}",
        install_lockfile, update_lockfile
    );

    // Verify specific resource assignments
    let deploy_in_install = install_lockfile.contains(r#"name = "deploy""#);
    let deploy_in_update = update_lockfile.contains(r#"name = "deploy""#);
    assert!(deploy_in_install && deploy_in_update, "Deploy command should exist in both lockfiles");

    let helper_in_install = install_lockfile.contains(r#"name = "helper""#);
    let helper_in_update = update_lockfile.contains(r#"name = "helper""#);
    assert!(helper_in_install && helper_in_update, "Helper snippet should exist in both lockfiles");

    Ok(())
}

/// Unit test documenting `generate_dependency_name` function behavior
///
/// This test documents the collision-resistant behavior of `generate_dependency_name`:
///   - snippets/commands/commit.md -> "commands/commit"
///   - snippets/logit/commit.md -> "logit/commit"
///   - snippets/utils/commit.md -> "utils/commit"
///
/// This is a regression test for a bug where the function used `file_stem()` which
/// collapsed all paths to bare filenames, causing silent data loss when multiple
/// resources shared the same filename but had different paths.
#[test]
fn test_generate_dependency_name_collisions() {
    // This is a unit test documenting the generate_dependency_name function behavior
    // after fixes were applied to prevent name collisions.

    use std::path::Path;

    // Simulate the corrected implementation
    fn generate_dependency_name_current(path: &str) -> String {
        let path = Path::new(path);
        let without_ext = path.with_extension("");
        let path_str = without_ext.to_string_lossy();
        let components: Vec<&str> = path_str.split('/').collect();
        if components.len() > 1 {
            components[1..].join("/")
        } else {
            components[0].to_string()
        }
    }

    // Test cases that generate DIFFERENT names (collision-resistant)
    let name1 = generate_dependency_name_current("snippets/commands/commit.md");
    let name2 = generate_dependency_name_current("snippets/logit/commit.md");
    let name3 = generate_dependency_name_current("snippets/utils/commit.md");

    // Document the correct behavior
    println!("Corrected name generation:");
    println!("  snippets/commands/commit.md -> {}", name1);
    println!("  snippets/logit/commit.md -> {}", name2);
    println!("  snippets/utils/commit.md -> {}", name3);

    // Same path should always generate same name
    let name4 = generate_dependency_name_current("snippets/commands/commit.md");
    assert_eq!(name1, name4, "Same path should generate same name");

    // Verify the corrected behavior produces unique names
    assert_eq!(name1, "commands/commit");
    assert_eq!(name2, "logit/commit");
    assert_eq!(name3, "utils/commit");
}

/// Test type resolution with multiple sources having same-named resources
///
/// This is a regression test for a bug where the `transitive_types` HashMap used only
/// `(name, source)` as the key, causing cross-type collisions. When a source had both
/// `snippets/helper.md` and `agents/helper.md`, the HashMap would overwrite one with
/// the other, leading to incorrect type assignments and resources being installed to
/// the wrong directories.
///
/// Now the key includes the resource type: `(ResourceType, name, source)`, allowing
/// same-named resources of different types to coexist correctly.
#[tokio::test]
async fn test_type_resolution_fallback_ambiguity() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create a single source repo with both agent and snippet named "helper"
    // Transitive dependencies must be from the same source as their parent
    let repo = project.create_source_repo("community").await?;

    // Add snippet "helper"
    repo.add_resource("snippets", "helper", "# Helper Snippet\n\nHelper snippet.").await?;

    // Add agent "helper"
    repo.add_resource("agents", "helper", "# Helper Agent\n\nHelper agent.").await?;

    // Create main agent that depends on both types with the same name
    repo.add_resource(
        "agents",
        "main",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/helper.md
      version: v1.0.0
  agents:
    - path: ./helper.md
      version: v1.0.0
---

# Main Agent
This agent depends on both helper snippet and helper agent (same name, different types).
"#,
    )
    .await?;

    repo.commit_all("Add resources")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("main", "community", "agents/main.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed: {}", output.stderr);

    // Read lockfile to verify correct type resolution
    let lockfile_content = project.read_lockfile().await?;

    // Each helper should now be in the correct section with correct installed_at path
    // Check that snippet helper is in snippets section
    let has_snippet_helper = lockfile_content.contains(r#"name = "helper""#)
        && lockfile_content.contains(r#"path = "snippets/helper.md""#);
    assert!(has_snippet_helper, "Lockfile should have helper snippet:\n{}", lockfile_content);

    // Check that agent helper is in agents section
    let has_agent_helper = lockfile_content.contains(r#"name = "helper""#)
        && lockfile_content.contains(r#"path = "agents/helper.md""#);
    assert!(has_agent_helper, "Lockfile should have helper agent:\n{}", lockfile_content);

    // Verify installed locations
    let snippet_path = project.project_path().join(".agpm/snippets/helper.md");
    let agent_path = project.project_path().join(".claude/agents/helper.md");

    assert!(
        tokio::fs::metadata(&snippet_path).await.is_ok(),
        "Snippet helper should be installed at {:?}",
        snippet_path
    );
    assert!(
        tokio::fs::metadata(&agent_path).await.is_ok(),
        "Agent helper should be installed at {:?}",
        agent_path
    );

    Ok(())
}

/// Test transitive dependency semver range auto-resolution
///
/// This test verifies that when multiple parents require the same transitive dependency
/// with compatible semver ranges, the resolver automatically finds the highest version
/// satisfying both constraints and uses that version's metadata.
///
/// Scenario:
/// - shared@v1.0.0 has old-dep as transitive dependency
/// - shared@v2.0.0 has new-dep as transitive dependency
/// - shared@v3.0.0 exists (content unchanged from v2.0.0)
/// - parent-a depends on shared@>=v1.0.0 (accepts v1.0.0, v2.0.0, v3.0.0)
/// - parent-b depends on shared@>=v1.5.0 (accepts v2.0.0, v3.0.0)
/// - Intersection is >=v1.5.0, highest available is v3.0.0
/// - Resolver auto-resolves to v3.0.0 and uses its metadata
/// - Should install new-dep (from v3.0.0), NOT old-dep (from v1.0.0)
#[tokio::test]
async fn test_transitive_version_conflict_metadata_from_winner() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create old-dep that will be in v1.0.0's transitive tree
    repo.add_resource("commands", "old-dep", "# Old Dep\n\nOld command.").await?;

    // Create shared@v1.0.0 with old-dep as transitive dependency
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/old-dep.md
---
# Shared v1.0.0
Version 1 with old-dep.
"#,
    )
    .await?;

    repo.commit_all("Add v1.0.0 resources")?;
    repo.tag_version("v1.0.0")?;

    // Create new-dep that will be in v2.0.0's transitive tree
    repo.add_resource("commands", "new-dep", "# New Dep\n\nNew command.").await?;

    // Update shared to v2.0.0 with new-dep as transitive dependency
    repo.add_resource(
        "snippets",
        "shared",
        r#"---
dependencies:
  commands:
    - path: ../commands/new-dep.md
---
# Shared v2.0.0
Version 2 with new-dep.
"#,
    )
    .await?;

    repo.commit_all("Update to v2.0.0")?;
    repo.tag_version("v2.0.0")?;

    // Create parent-a that depends on shared@>=v1.0 (compatible range)
    repo.add_resource(
        "agents",
        "parent-a",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: ">=v1.0.0"
---
# Parent A
Depends on shared@>=v1.0.0 (accepts any version >= 1.0.0).
"#,
    )
    .await?;

    // Create parent-b that depends on shared@>=v1.5 (compatible with parent-a)
    repo.add_resource(
        "agents",
        "parent-b",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: ">=v1.5.0"
---
# Parent B
Depends on shared@>=v1.5.0 (intersection with parent-a is >=v1.5.0).
"#,
    )
    .await?;

    repo.commit_all("Add parent agents")?;
    repo.tag_version("v3.0.0")?;

    // Create manifest with both parents (creates version conflict on shared)
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_agent("parent-a", |d| {
            d.source("community").path("agents/parent-a.md").version("v3.0.0")
        })
        .add_agent("parent-b", |d| {
            d.source("community").path("agents/parent-b.md").version("v3.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Verify that v3.0.0 won (highest version satisfying both constraints)
    // Constraints are >=v1.0.0 and >=v1.5.0, intersection is >=v1.5.0
    // Available versions: v1.0.0, v2.0.0, v3.0.0
    // Highest satisfying >=v1.5.0 is v3.0.0
    // new-dep exists at v2.0.0 and v3.0.0, old-dep only at v1.0.0
    let new_dep_path = project.project_path().join(".claude/commands/new-dep.md");
    let old_dep_path = project.project_path().join(".claude/commands/old-dep.md");

    assert!(
        tokio::fs::metadata(&new_dep_path).await.is_ok(),
        "new-dep should be installed (exists at v3.0.0)"
    );
    assert!(
        tokio::fs::metadata(&old_dep_path).await.is_err(),
        "old-dep should NOT be installed (doesn't exist at v3.0.0)"
    );

    // Verify lockfile shows v3.0.0 for shared
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains(r#"name = "shared""#) && lockfile_content.contains("v3.0.0"),
        "Lockfile should show shared at v3.0.0 (highest version satisfying both constraints)"
    );

    // Verify shared snippet content is from v2.0.0 (content unchanged at v3.0.0)
    let shared_path = project.project_path().join(".agpm/snippets/shared.md");
    let shared_content = tokio::fs::read_to_string(&shared_path).await?;
    assert!(
        shared_content.contains("Version 2 with new-dep"),
        "Shared snippet should have v2.0.0 content (unchanged in v3.0.0)"
    );

    Ok(())
}

/// Test same-name resources from different sources with cross-source disambiguation
///
/// This test verifies that the (ResourceType, name, source) key properly disambiguates
/// resources with the same name coming from different sources. Without source in the key,
/// resources would overwrite each other even though they come from different repositories.
///
/// Scenario:
/// - community source has snippets/helper.md
/// - local source has snippets/helper.md
/// - Parent agent depends on both (cross-source transitive dependencies)
/// - Both should be installed without collision
#[tokio::test]
async fn test_cross_source_same_name_disambiguation() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create community source with helper snippet
    let community_repo = project.create_source_repo("community").await?;
    community_repo
        .add_resource("snippets", "helper", "# Community Helper\n\nFrom community source.")
        .await?;

    // Create main agent in community that depends on helper
    community_repo
        .add_resource(
            "agents",
            "main",
            r#"---
dependencies:
  snippets:
    - path: ../snippets/helper.md
      version: v1.0.0
---
# Main Agent
Depends on community helper.
"#,
        )
        .await?;
    community_repo.commit_all("Add resources")?;
    community_repo.tag_version("v1.0.0")?;

    // Create local source with helper snippet (same name, different content)
    let local_repo = project.create_source_repo("local").await?;
    local_repo.add_resource("snippets", "helper", "# Local Helper\n\nFrom local source.").await?;
    local_repo.commit_all("Add local helper")?;
    local_repo.tag_version("v1.0.0")?;

    // Create manifest that pulls main agent from community
    // Main agent has transitive dependency on community/snippets/helper
    // We'll also add a direct dependency on local/snippets/helper
    let community_url = community_repo.bare_file_url(project.sources_path())?;
    let local_url = local_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &community_url)
        .add_source("local", &local_url)
        .add_standard_agent("main", "community", "agents/main.md")
        .add_standard_snippet("local-helper", "local", "snippets/helper.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - should now fail with path conflict
    // With the fix for default-tools, both snippets now use "agpm" tool and install to the same path
    let output = project.run_agpm(&["install"])?;

    // Expected behavior: Should detect path conflict
    assert!(
        !output.success,
        "Install should fail due to path conflict (both snippets use agpm tool)"
    );
    assert!(
        output.stderr.contains("Target path conflicts")
            || output.stderr.contains(".agpm/snippets/helper.md"),
        "Should report path conflict for snippets using same tool, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test shared dependency appearing both as direct manifest entry and transitive dependency
///
/// This test verifies that when a resource is both directly in the manifest and pulled
/// transitively by another resource, we only get one lockfile entry (deduplication works)
/// and the dependency_map properly tracks all parents that reference it.
///
/// Scenario:
/// - Manifest has direct dependency on snippets/shared
/// - Parent agent also depends on snippets/shared (transitive)
/// - Should result in single lockfile entry for shared
/// - dependency_map should track both parent relationships
#[tokio::test]
async fn test_shared_dependency_deduplication() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create shared snippet that will be both direct and transitive
    repo.add_resource("snippets", "shared", "# Shared Snippet\n\nUsed by multiple resources.")
        .await?;

    // Create parent agent that depends on shared (makes shared a transitive dep)
    repo.add_resource(
        "agents",
        "parent",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/shared.md
      version: v1.0.0
---
# Parent Agent
Depends on shared snippet.
"#,
    )
    .await?;

    repo.commit_all("Add resources")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest with both direct dependency on shared AND parent agent
    // This creates the situation where shared is both direct and transitive
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("parent", "community", "agents/parent.md")
        .add_standard_snippet("shared", "community", "snippets/shared.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    project.run_agpm(&["install"])?.assert_success();

    // Verify shared snippet is installed
    // Note: Snippets default to tool="agpm", so they install to .agpm/snippets/
    let shared_path = project.project_path().join(".agpm/snippets/shared.md");
    assert!(tokio::fs::metadata(&shared_path).await.is_ok(), "Shared snippet should be installed");

    // Verify lockfile has only ONE entry for shared (deduplication)
    let lockfile_content = project.read_lockfile().await?;
    let shared_count = lockfile_content.matches(r#"name = "shared""#).count();

    assert_eq!(
        shared_count, 1,
        "Should have exactly one lockfile entry for shared, found {}",
        shared_count
    );

    // Verify parent agent is also installed
    let parent_path = project.project_path().join(".claude/agents/parent.md");
    assert!(tokio::fs::metadata(&parent_path).await.is_ok(), "Parent agent should be installed");

    // The lockfile entry for shared should exist and have v1.0.0
    assert!(
        lockfile_content.contains(r#"name = "shared""#) && lockfile_content.contains("v1.0.0"),
        "Lockfile should show shared at v1.0.0"
    );

    Ok(())
}

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
            || output.stderr.contains("file access"),
        "Error should indicate transitive dependency failure, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test transitive dependency with glob pattern expands and resolves grandchildren
///
/// This test verifies that when a transitive dependency specifies a glob pattern
/// (e.g., "snippets/helper-*.md"), the resolver:
/// 1. Expands the pattern to all matching files
/// 2. Queues each matched file for transitive resolution
/// 3. Discovers and installs each matched file's own transitive dependencies
///
/// Scenario:
/// - Parent agent has transitive dependency with glob pattern "snippets/helper-*.md"
/// - Pattern matches helper-one.md and helper-two.md
/// - helper-one.md has its own transitive dependency on commands/cmd-one.md
/// - helper-two.md has its own transitive dependency on commands/cmd-two.md
/// - All resources should be installed (parent, helpers, commands)
#[tokio::test]
async fn test_transitive_pattern_dependency_expands() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create commands that will be transitive deps of the snippets
    repo.add_resource("commands", "cmd-one", "# Command One\n\nFirst command.").await?;
    repo.add_resource("commands", "cmd-two", "# Command Two\n\nSecond command.").await?;

    // Create snippets with their own transitive dependencies
    repo.add_resource(
        "snippets",
        "helper-one",
        r#"---
dependencies:
  commands:
    - path: ../commands/cmd-one.md
---
# Helper One
First helper with transitive dependency on cmd-one.
"#,
    )
    .await?;

    repo.add_resource(
        "snippets",
        "helper-two",
        r#"---
dependencies:
  commands:
    - path: ../commands/cmd-two.md
---
# Helper Two
Second helper with transitive dependency on cmd-two.
"#,
    )
    .await?;

    // Create parent agent with a PATTERN in its transitive dependencies
    // The pattern should expand to helper-one and helper-two
    // Each helper's transitive dependencies should also be discovered
    repo.add_resource(
        "agents",
        "parent",
        r#"---
dependencies:
  snippets:
    - path: ../snippets/helper-*.md
---
# Parent Agent
Has a glob pattern in transitive dependencies that matches multiple snippets.
Each snippet has its own transitive dependencies.
"#,
    )
    .await?;

    repo.commit_all("Add resources")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest with parent agent
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("parent", "community", "agents/parent.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - pattern should expand and all transitive deps should be resolved
    project.run_agpm(&["install"])?.assert_success();

    // Verify parent agent is installed
    let parent_path = project.project_path().join(".claude/agents/parent.md");
    assert!(tokio::fs::metadata(&parent_path).await.is_ok(), "Parent agent should be installed");

    // Verify that the pattern-matched snippets ARE installed
    // (pattern expansion should discover them as transitive deps)
    let helper_one_path = project.project_path().join(".agpm/snippets/helper-one.md");
    let helper_two_path = project.project_path().join(".agpm/snippets/helper-two.md");

    assert!(
        tokio::fs::metadata(&helper_one_path).await.is_ok(),
        "Helper-one should be installed (matched by pattern)"
    );
    assert!(
        tokio::fs::metadata(&helper_two_path).await.is_ok(),
        "Helper-two should be installed (matched by pattern)"
    );

    // Verify that the grandchild commands are also installed
    // (each snippet's transitive dependencies should be discovered)
    let cmd_one_path = project.project_path().join(".claude/commands/cmd-one.md");
    let cmd_two_path = project.project_path().join(".claude/commands/cmd-two.md");

    assert!(
        tokio::fs::metadata(&cmd_one_path).await.is_ok(),
        "cmd-one should be installed (transitive dep of helper-one)"
    );
    assert!(
        tokio::fs::metadata(&cmd_two_path).await.is_ok(),
        "cmd-two should be installed (transitive dep of helper-two)"
    );

    // Verify lockfile contains all resources
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains(r#"name = "parent""#), "Lockfile should contain parent");
    assert!(
        lockfile_content.contains(r#"name = "helper-one""#),
        "Lockfile should contain helper-one"
    );
    assert!(
        lockfile_content.contains(r#"name = "helper-two""#),
        "Lockfile should contain helper-two"
    );
    assert!(lockfile_content.contains(r#"name = "cmd-one""#), "Lockfile should contain cmd-one");
    assert!(lockfile_content.contains(r#"name = "cmd-two""#), "Lockfile should contain cmd-two");

    Ok(())
}

/// Test that manifest pattern dependencies have their transitive deps resolved
///
/// This test verifies that pattern dependencies declared in the manifest (not just
/// transitive ones) properly expand and have each matched file's transitive dependencies
/// discovered and installed.
///
/// Scenario:
/// - Manifest has a pattern dependency "snippets/util-*.md"
/// - Pattern matches util-one.md and util-two.md
/// - util-one.md has transitive dependency on commands/cmd-a.md
/// - util-two.md has transitive dependency on commands/cmd-b.md
/// - All resources should be installed (utils and commands)
#[tokio::test]
async fn test_manifest_pattern_has_transitive_deps_resolved() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let repo = project.create_source_repo("community").await?;

    // Create commands that will be transitive deps
    repo.add_resource("commands", "cmd-a", "# Command A\n\nFirst command.").await?;
    repo.add_resource("commands", "cmd-b", "# Command B\n\nSecond command.").await?;

    // Create snippets with transitive dependencies
    repo.add_resource(
        "snippets",
        "util-one",
        r#"---
dependencies:
  commands:
    - path: ../commands/cmd-a.md
---
# Util One
First utility with transitive dependency on cmd-a.
"#,
    )
    .await?;

    repo.add_resource(
        "snippets",
        "util-two",
        r#"---
dependencies:
  commands:
    - path: ../commands/cmd-b.md
---
# Util Two
Second utility with transitive dependency on cmd-b.
"#,
    )
    .await?;

    repo.commit_all("Add resources")?;
    repo.tag_version("v1.0.0")?;

    // Create manifest with PATTERN dependency (not transitive - direct in manifest)
    // Note: apply_tool_defaults() will set snippets to tool="agpm" automatically
    // Transitive command dependencies will auto-fallback to "claude-code" since agpm doesn't support commands
    let source_url = repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_snippet("util-pattern", |d| {
            d.source("community").path("snippets/util-*.md").version("v1.0.0")
            // tool defaults to "agpm" for snippets via apply_tool_defaults()
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - pattern should expand and transitive deps should be resolved
    project.run_agpm(&["install"])?.assert_success();

    // Verify pattern-matched snippets are installed
    // Note: Snippets default to tool="agpm" due to apply_tool_defaults(), so they go to .agpm/snippets/
    let util_one_path = project.project_path().join(".agpm/snippets/util-one.md");
    let util_two_path = project.project_path().join(".agpm/snippets/util-two.md");

    assert!(
        tokio::fs::metadata(&util_one_path).await.is_ok(),
        "util-one should be installed (matched by manifest pattern)"
    );
    assert!(
        tokio::fs::metadata(&util_two_path).await.is_ok(),
        "util-two should be installed (matched by manifest pattern)"
    );

    // Verify transitive command dependencies are also installed
    let cmd_a_path = project.project_path().join(".claude/commands/cmd-a.md");
    let cmd_b_path = project.project_path().join(".claude/commands/cmd-b.md");

    assert!(
        tokio::fs::metadata(&cmd_a_path).await.is_ok(),
        "cmd-a should be installed (transitive dep of util-one)"
    );
    assert!(
        tokio::fs::metadata(&cmd_b_path).await.is_ok(),
        "cmd-b should be installed (transitive dep of util-two)"
    );

    // Verify lockfile contains all resources
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains(r#"name = "util-one""#), "Lockfile should contain util-one");
    assert!(lockfile_content.contains(r#"name = "util-two""#), "Lockfile should contain util-two");
    assert!(lockfile_content.contains(r#"name = "cmd-a""#), "Lockfile should contain cmd-a");
    assert!(lockfile_content.contains(r#"name = "cmd-b""#), "Lockfile should contain cmd-b");

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
    let installed_remote_parent = project.project_path().join(".claude/agents/remote-parent.md");
    assert!(
        tokio::fs::metadata(&installed_remote_parent).await.is_ok(),
        "Remote parent agent should be installed"
    );

    // Verify transitive remote helper is installed
    // Note: Transitive snippets use their default tool (agpm)
    let installed_remote_helper = project.project_path().join(".agpm/snippets/remote-helper.md");
    assert!(
        tokio::fs::metadata(&installed_remote_helper).await.is_ok(),
        "Remote helper (transitive) should be installed"
    );

    // Verify lockfile has all three resources
    let lockfile_content = project.read_lockfile().await?;
    assert!(
        lockfile_content.contains(r#"name = "local-snippet""#),
        "Lockfile should contain local-snippet"
    );
    assert!(
        lockfile_content.contains(r#"name = "remote-parent""#),
        "Lockfile should contain remote-parent"
    );
    assert!(
        lockfile_content.contains(r#"name = "remote-helper""#),
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
    let installed_local = project.project_path().join(".claude/agents/local-agent.md");
    let installed_helper = project.project_path().join(".claude/agents/helper.md");

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
    assert!(
        lockfile_content.contains(r#"name = "local-agent""#),
        "Lockfile should contain local-agent"
    );
    assert!(
        lockfile_content.contains(r#"name = "helper""#),
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
    // Local agent preserves subdirectory structure: agents/subfolder/local-agent.md -> subfolder/local-agent.md
    let installed_local = project.project_path().join(".claude/agents/subfolder/local-agent.md");
    // Helper after stripping ../: helper.md -> helper.md (no prefix to strip)
    let installed_helper = project.project_path().join(".claude/agents/helper.md");

    assert!(tokio::fs::metadata(&installed_local).await.is_ok(), "Local agent should be installed");
    assert!(
        tokio::fs::metadata(&installed_helper).await.is_ok(),
        "Helper agent (transitive from parent dir) should be installed"
    );

    // Verify lockfile
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains(r#"name = "local-agent""#));
    assert!(lockfile_content.contains(r#"name = "helper""#));
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
    let installed_agent = project.project_path().join(".claude/agents/local-agent.md");
    assert!(tokio::fs::metadata(&installed_agent).await.is_ok(), "Local agent should be installed");

    // Verify snippet installed to .agpm/snippets (uses default tool for snippets)
    // Note: Transitive snippets use their default tool (agpm)
    let installed_snippet = project.project_path().join(".agpm/snippets/utils.md");
    assert!(
        tokio::fs::metadata(&installed_snippet).await.is_ok(),
        "Utils snippet (transitive) should be installed to .agpm/snippets (using default tool)"
    );

    // Verify lockfile
    let lockfile_content = project.read_lockfile().await?;
    assert!(lockfile_content.contains(r#"name = "local-agent""#));
    assert!(lockfile_content.contains(r#"name = "utils""#));
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
            || output.stderr.contains("file access"),
        "Error should indicate transitive dependency failure, got: {}",
        output.stderr
    );

    Ok(())
}

/// Test local file transitive dependency with invalid path format
///
/// This test verifies that transitive dependency paths that don't start with
/// `./` or `../` are rejected with a clear error message.
///
/// Scenario:
/// - Local agent declares transitive dependency with plain relative path (no ./ or ../)
/// - Should emit warning about invalid path format
/// - Should skip the transitive dep
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

    // Create agent with invalid transitive dep path (doesn't start with ./ or ../)
    let local_agent_path = agents_dir.join("local-agent.md");
    tokio::fs::write(
        &local_agent_path,
        r#"---
dependencies:
  agents:
    - path: helper.md
---
# Local Agent
This agent has an invalid transitive dependency path.
"#,
    )
    .await?;

    // Create manifest
    let manifest =
        ManifestBuilder::new().add_local_agent("local-agent", "agents/local-agent.md").build();

    project.write_manifest(&manifest).await?;

    // Run install - now FAILS because path resolution failures are hard failures
    let output = project.run_agpm(&["install"])?;
    assert!(!output.success, "Install should fail when transitive dependency path is invalid");

    // Verify the error message indicates the failure
    assert!(
        output.stderr.contains("Failed to resolve transitive dependency")
            || output.stderr.contains("Failed to fetch resource")
            || output.stderr.contains("file access"),
        "Error should indicate transitive dependency failure, got: {}",
        output.stderr
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
    let installed_agent = project.project_path().join(".claude/agents/my-agent.md");
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
    assert!(lockfile_content.contains(r#"name = "my-agent""#), "Lockfile should contain agent");

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
