// Integration tests for transitive dependency pattern resolution
//
// Tests pattern expansion in transitive dependencies and manifest patterns
// with transitive dependency resolution.

use anyhow::Result;

use crate::common::{ManifestBuilder, TestProject};

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
    let source_url = repo.bare_file_url(project.sources_path()).await?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_standard_agent("parent", "community", "agents/parent.md")
        .build();

    project.write_manifest(&manifest).await?;

    // Run install - pattern should expand and all transitive deps should be resolved
    project.run_agpm(&["install"])?.assert_success();

    // Verify parent agent is installed
    let parent_path = project.project_path().join(".claude/agents/agpm/parent.md");
    assert!(tokio::fs::metadata(&parent_path).await.is_ok(), "Parent agent should be installed");

    // Verify that the pattern-matched snippets ARE installed
    // (pattern expansion should discover them as transitive deps, inheriting claude-code from parent agent)
    let helper_one_path = project.project_path().join(".claude/snippets/agpm/helper-one.md");
    let helper_two_path = project.project_path().join(".claude/snippets/agpm/helper-two.md");

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
    let cmd_one_path = project.project_path().join(".claude/commands/agpm/cmd-one.md");
    let cmd_two_path = project.project_path().join(".claude/commands/agpm/cmd-two.md");

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
    // All dependencies use canonical names with resource type directory
    assert!(
        lockfile_content.contains(r#"name = "agents/parent""#),
        "Lockfile should contain parent with canonical name"
    );
    // Transitive dependencies also have canonical names
    assert!(
        lockfile_content.contains(r#"name = "snippets/helper-one""#),
        "Lockfile should contain helper-one"
    );
    assert!(
        lockfile_content.contains(r#"name = "snippets/helper-two""#),
        "Lockfile should contain helper-two"
    );
    assert!(
        lockfile_content.contains(r#"name = "commands/cmd-one""#),
        "Lockfile should contain cmd-one"
    );
    assert!(
        lockfile_content.contains(r#"name = "commands/cmd-two""#),
        "Lockfile should contain cmd-two"
    );

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
    let source_url = repo.bare_file_url(project.sources_path()).await?;
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
    let cmd_a_path = project.project_path().join(".claude/commands/agpm/cmd-a.md");
    let cmd_b_path = project.project_path().join(".claude/commands/agpm/cmd-b.md");

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
    // Pattern-expanded manifest dependencies have canonical names with resource type directory
    assert!(
        lockfile_content.contains(r#"name = "snippets/util-one""#),
        "Lockfile should contain util-one"
    );
    assert!(
        lockfile_content.contains(r#"name = "snippets/util-two""#),
        "Lockfile should contain util-two"
    );
    // Transitive command dependencies also have canonical names
    assert!(
        lockfile_content.contains(r#"name = "commands/cmd-a""#),
        "Lockfile should contain cmd-a"
    );
    assert!(
        lockfile_content.contains(r#"name = "commands/cmd-b""#),
        "Lockfile should contain cmd-b"
    );

    Ok(())
}
