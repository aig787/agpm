// Integration tests for basic transitive dependency resolution
//
// Tests basic transitive resolution, diamond dependencies, cycle detection,
// deduplication, and cross-type collision scenarios.

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
    let main_app_path = project.project_path().join(".claude/agents/agpm/main-app.md");
    let helper_path = project.project_path().join(".claude/agents/agpm/helper.md");
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
    // because both "utils" transitive deps resolve to .claude/agents/agpm/utils.md
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
    // Transitive dependencies have canonical names like "agents/agent-d"
    let agent_d_count = lockfile_content.matches("name = \"agents/agent-d\"").count();
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
/// "snippets/logit/commit.md"), both should be installed correctly to .claude/snippets/
/// (inheriting claude-code tool from the command parent):
///   - .claude/snippets/agpm/snippets/commands/commit.md (content: "commands version")
///   - .claude/snippets/agpm/snippets/logit/commit.md (content: "logit version")
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

    // Verify both snippets are installed at their respective paths (inheriting claude-code from command parent)
    let commands_snippet_path =
        project.project_path().join(".claude/snippets/agpm/snippets/commands/commit.md");
    let logit_snippet_path =
        project.project_path().join(".claude/snippets/agpm/snippets/logit/commit.md");

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
///   - .claude/agents/agpm/helper.md
///   - .claude/commands/agpm/helper.md
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
    // Transitive dependencies use canonical names with resource type directory
    let has_agent_helper = lockfile_content.contains("[[agents]]")
        && lockfile_content.contains(r#"name = "agents/helper""#)
        && lockfile_content.contains(r#"path = "agents/helper.md""#);

    let has_command_helper = lockfile_content.contains("[[commands]]")
        && lockfile_content.contains(r#"name = "commands/helper""#)
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
    let agent_path = project.project_path().join(".claude/agents/agpm/helper.md");
    let command_path = project.project_path().join(".claude/commands/agpm/helper.md");

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
    // All dependencies use canonical names with resource type directory
    let deploy_in_install = install_lockfile.contains(r#"name = "commands/deploy""#);
    let deploy_in_update = update_lockfile.contains(r#"name = "commands/deploy""#);
    assert!(deploy_in_install && deploy_in_update, "Deploy command should exist in both lockfiles");

    // Transitive dependency also has canonical name
    let helper_in_install = install_lockfile.contains(r#"name = "snippets/helper""#);
    let helper_in_update = update_lockfile.contains(r#"name = "snippets/helper""#);
    assert!(helper_in_install && helper_in_update, "Helper snippet should exist in both lockfiles");

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

    // Verify shared snippet is installed (direct dependency uses agpm)
    let shared_agpm_path = project.project_path().join(".agpm/snippets/shared.md");
    assert!(
        tokio::fs::metadata(&shared_agpm_path).await.is_ok(),
        "Shared snippet (agpm) should be installed"
    );

    // Also verify the transitive version (inherits claude-code from agent parent)
    // Snippets have flatten=false, so source path is preserved: snippets/shared.md
    let shared_claude_path =
        project.project_path().join(".claude/snippets/agpm/snippets/shared.md");
    assert!(
        tokio::fs::metadata(&shared_claude_path).await.is_ok(),
        "Shared snippet (claude-code) should be installed"
    );

    // Verify lockfile has TWO entries for shared (one per tool)
    // This is expected behavior: same resource with different tools are treated as separate entries
    let lockfile_content = project.read_lockfile().await?;
    // Both entries have canonical name "snippets/shared"
    let shared_count = lockfile_content.matches(r#"name = "snippets/shared""#).count();

    assert_eq!(
        shared_count, 2,
        "Should have two lockfile entries for shared (one per tool), found {}",
        shared_count
    );

    // Verify parent agent is also installed
    let parent_path = project.project_path().join(".claude/agents/agpm/parent.md");
    assert!(tokio::fs::metadata(&parent_path).await.is_ok(), "Parent agent should be installed");

    // The lockfile entry for shared should exist and have v1.0.0
    assert!(
        lockfile_content.contains(r#"name = "snippets/shared""#)
            && lockfile_content.contains("v1.0.0"),
        "Lockfile should show shared at v1.0.0"
    );

    Ok(())
}
