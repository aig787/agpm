//! Integration tests for lockfile stability
//!
//! These tests ensure that running `agpm install` multiple times produces
//! deterministic, stable results. This is critical for:
//! - Team collaboration (same lockfile = same installation)
//! - CI/CD reproducibility
//! - Preventing template rendering bugs that cause content to change between runs

use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::common::{ManifestBuilder, TestProject};

/// Helper to compute SHA256 hash of a file
fn compute_file_hash(path: &std::path::Path) -> Result<String> {
    let content = std::fs::read(path)?;
    let hash = Sha256::digest(&content);
    Ok(format!("{:x}", hash))
}

/// Helper to collect hashes of all installed files in a directory
fn collect_installed_hashes(base_path: &std::path::Path) -> Result<Vec<(String, String)>> {
    let mut hashes = Vec::new();

    for entry in walkdir::WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let relative_path = path.strip_prefix(base_path)?.to_string_lossy().to_string();

        // Skip lockfile from comparison - focus on installed content stability
        if relative_path == "agpm.lock" {
            continue;
        }

        let hash = compute_file_hash(path)?;
        hashes.push((relative_path, hash));
    }

    // Sort for deterministic comparison
    hashes.sort();
    Ok(hashes)
}

/// Test basic stability: running install 10 times produces identical content
#[tokio::test]
async fn test_basic_stability_multiple_installs() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo with basic resources
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "my-agent", "# My Agent\nSimple agent content").await?;
    source_repo.add_resource("snippets", "my-snippet", "# My Snippet\nSnippet content").await?;
    source_repo.add_resource("commands", "my-command", "# My Command\nCommand content").await?;
    source_repo.commit_all("Add resources")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_agent("my-agent", "test-source", "agents/my-agent.md")
        .add_standard_snippet("my-snippet", "test-source", "snippets/my-snippet.md")
        .add_standard_command("my-command", "test-source", "commands/my-command.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Run install 10 times and collect hashes
    let mut all_hashes = Vec::new();
    for i in 0..10 {
        let output = project.run_agpm(&["install", "--quiet"])?;
        assert!(output.success, "Install #{} failed: {}", i + 1, output.stderr);

        // Collect hashes of all installed files
        let hashes = collect_installed_hashes(project.project_path())?;
        all_hashes.push(hashes);
    }

    // Verify all 10 runs produced identical hashes
    let first_hashes = &all_hashes[0];
    for (i, hashes) in all_hashes.iter().enumerate().skip(1) {
        assert_eq!(
            first_hashes,
            hashes,
            "Install #{} produced different content than install #1",
            i + 1
        );
    }

    Ok(())
}

/// Test frozen stability: install once, then frozen install 10 times produces same content
#[tokio::test]
async fn test_frozen_stability_multiple_installs() -> Result<()> {
    let project = TestProject::new().await?;

    // Create a source repo
    let source_repo = project.create_source_repo("test-source").await?;
    source_repo.add_resource("agents", "my-agent", "# My Agent\nAgent content").await?;
    source_repo.commit_all("Add agent")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_agent("my-agent", "test-source", "agents/my-agent.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Initial install to create lockfile
    let output = project.run_agpm(&["install", "--quiet"])?;
    assert!(output.success, "Initial install failed: {}", output.stderr);

    // Save the initial hashes
    let initial_hashes = collect_installed_hashes(project.project_path())?;

    // Run frozen install 10 times
    let mut all_frozen_hashes = Vec::new();
    for i in 0..10 {
        let output = project.run_agpm(&["install", "--frozen", "--quiet"])?;
        assert!(output.success, "Frozen install #{} failed: {}", i + 1, output.stderr);

        let hashes = collect_installed_hashes(project.project_path())?;
        all_frozen_hashes.push(hashes);
    }

    // Verify all frozen installs match the initial install
    for (i, hashes) in all_frozen_hashes.iter().enumerate() {
        assert_eq!(
            &initial_hashes,
            hashes,
            "Frozen install #{} produced different content than initial install",
            i + 1
        );
    }

    Ok(())
}

/// Test stability with templated resources (regression test for template rendering bug)
#[tokio::test]
async fn test_templating_stability() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with templated resources
    let source_repo = project.create_source_repo("test-source").await?;

    // Create a base snippet that will be embedded
    source_repo
        .add_resource(
            "snippets",
            "base",
            "---\nagpm:\n  templating: false\n---\n# Base Content\nThis is base content",
        )
        .await?;

    // Create a templated command that embeds the snippet
    source_repo
        .add_resource(
            "commands",
            "my-command",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: base
      install: false
      path: ../snippets/base.md
---
# My Command

{{ agpm.deps.snippets.base.content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add templated resources")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_command("my-command", "test-source", "commands/my-command.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Run install 10 times and verify content stability
    let mut all_hashes = Vec::new();
    for i in 0..10 {
        let output = project.run_agpm(&["install", "--quiet"])?;
        assert!(output.success, "Install #{} with templating failed: {}", i + 1, output.stderr);

        // Check that the command file contains the base content (template was rendered)
        let command_path = project.project_path().join(".claude/commands/my-command.md");
        let command_content = tokio::fs::read_to_string(&command_path).await?;
        assert!(
            command_content.contains("Base Content"),
            "Template was not rendered correctly in install #{}",
            i + 1
        );

        let hashes = collect_installed_hashes(project.project_path())?;
        all_hashes.push(hashes);
    }

    // Verify all runs produced identical content
    let first_hashes = &all_hashes[0];
    for (i, hashes) in all_hashes.iter().enumerate().skip(1) {
        assert_eq!(
            first_hashes,
            hashes,
            "Templated install #{} produced different content than install #1.\n\
             This indicates a template rendering stability issue.",
            i + 1
        );
    }

    Ok(())
}

/// Test stability with transitive dependencies (regression test)
#[tokio::test]
async fn test_transitive_dependency_stability() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with multi-level dependencies
    let source_repo = project.create_source_repo("test-source").await?;

    // Level 1: Base snippet (no dependencies)
    source_repo
        .add_resource(
            "snippets",
            "level1",
            "---\nagpm:\n  templating: false\n---\n# Level 1\nBase level",
        )
        .await?;

    // Level 2: Snippet that depends on level1
    source_repo
        .add_resource(
            "snippets",
            "level2",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: level1
      install: false
      path: snippets/level1.md
---
# Level 2

{{ agpm.deps.snippets.level1.content }}
"#,
        )
        .await?;

    // Level 3: Command that depends on level2 (which transitively depends on level1)
    source_repo
        .add_resource(
            "commands",
            "my-command",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: level2
      install: false
      path: ../snippets/level2.md
---
# My Command

{{ agpm.deps.snippets.level2.content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add multi-level dependencies")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_command("my-command", "test-source", "commands/my-command.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Run install 10 times
    let mut all_hashes = Vec::new();
    for i in 0..10 {
        let output = project.run_agpm(&["install", "--quiet"])?;
        assert!(
            output.success,
            "Install #{} with transitive deps failed: {}",
            i + 1,
            output.stderr
        );

        // Verify transitive content is present
        let command_path = project.project_path().join(".claude/commands/my-command.md");
        let command_content = tokio::fs::read_to_string(&command_path).await?;
        assert!(
            command_content.contains("Level 1"),
            "Transitive dependency content missing in install #{}",
            i + 1
        );

        let hashes = collect_installed_hashes(project.project_path())?;
        all_hashes.push(hashes);
    }

    // Verify stability
    let first_hashes = &all_hashes[0];
    for (i, hashes) in all_hashes.iter().enumerate().skip(1) {
        assert_eq!(
            first_hashes,
            hashes,
            "Install #{} with transitive deps produced different content.\n\
             This indicates a transitive dependency resolution issue.",
            i + 1
        );
    }

    Ok(())
}

/// Test stability with multiple resources having same custom dependency name (regression test)
#[tokio::test]
async fn test_custom_name_collision_stability() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo
    let source_repo = project.create_source_repo("test-source").await?;

    // Create multiple different snippets
    for i in 1..=3 {
        source_repo
            .add_resource(
                "snippets",
                &format!("content-{}", i),
                &format!("# Content {}\nUnique content {}", i, i),
            )
            .await?;
    }

    // Create multiple commands, each using different snippet but same custom name "base"
    for i in 1..=3 {
        source_repo
            .add_resource(
                "commands",
                &format!("cmd-{}", i),
                &format!(
                    r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: base
      install: false
      path: ../snippets/content-{}.md
---
# Command {}

{{{{ agpm.deps.snippets.base.content }}}}
"#,
                    i, i
                ),
            )
            .await?;
    }

    source_repo.commit_all("Add commands with name collisions")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest with all commands AND snippets (so they're in lockfile)
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_command("cmd-1", "test-source", "commands/cmd-1.md")
        .add_standard_command("cmd-2", "test-source", "commands/cmd-2.md")
        .add_standard_command("cmd-3", "test-source", "commands/cmd-3.md")
        .add_standard_snippet("content-1", "test-source", "snippets/content-1.md")
        .add_standard_snippet("content-2", "test-source", "snippets/content-2.md")
        .add_standard_snippet("content-3", "test-source", "snippets/content-3.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Run install 10 times
    let mut all_hashes = Vec::new();
    let mut all_contents = Vec::new();
    for i in 0..10 {
        let output = project.run_agpm(&["install", "--quiet"])?;
        assert!(
            output.success,
            "Install #{} with name collisions failed: {}",
            i + 1,
            output.stderr
        );

        // Verify each command got the correct content (not mixed up)
        for j in 1..=3 {
            let cmd_path = project.project_path().join(format!(".claude/commands/cmd-{}.md", j));
            let cmd_content = tokio::fs::read_to_string(&cmd_path).await?;
            assert!(
                cmd_content.contains(&format!("Unique content {}", j)),
                "Command {} got wrong content in install #{}. \
                 Expected 'Unique content {}', but content was:\n{}",
                j,
                i + 1,
                j,
                cmd_content
            );
        }

        let hashes = collect_installed_hashes(project.project_path())?;
        all_hashes.push(hashes.clone());

        // Also store the actual content for detailed comparison
        let mut contents = Vec::new();
        for j in 1..=3 {
            let cmd_path = project.project_path().join(format!(".claude/commands/cmd-{}.md", j));
            let content = tokio::fs::read_to_string(&cmd_path).await?;
            contents.push((format!("cmd-{}.md", j), content));
        }
        all_contents.push(contents);
    }

    // Verify stability
    let first_hashes = &all_hashes[0];
    for (i, hashes) in all_hashes.iter().enumerate().skip(1) {
        if first_hashes != hashes {
            // Provide detailed diff if they don't match
            let first_contents = &all_contents[0];
            let current_contents = &all_contents[i];

            for ((name1, content1), (_name2, content2)) in
                first_contents.iter().zip(current_contents.iter())
            {
                if content1 != content2 {
                    panic!(
                        "Install #{} produced different content for {}.\n\
                         First install:\n{}\n\n\
                         Install #{}:\n{}",
                        i + 1,
                        name1,
                        content1,
                        i + 1,
                        content2
                    );
                }
            }

            panic!(
                "Install #{} produced different hashes but content appears same. Hash issue?",
                i + 1
            );
        }
    }

    Ok(())
}

/// Test stability with nested transitive custom names
///
/// This tests the case where:
/// - Command depends on Snippet A (with custom name "base")
/// - Snippet A depends on Snippet B (with custom name "helper")
/// - Command can reference both via `agpm.deps.snippets.base` and `agpm.deps.snippets.helper`
///
/// This is a regression test for non-deterministic HashMap ordering when extracting
/// custom names from frontmatter across the dependency tree.
#[tokio::test]
async fn test_nested_transitive_custom_names_stability() -> Result<()> {
    let project = TestProject::new().await?;

    // Create source repo with nested dependencies
    let source_repo = project.create_source_repo("test-source").await?;

    // Create the deepest snippet (no dependencies)
    source_repo
        .add_resource(
            "snippets",
            "helper",
            "---\nagpm:\n  templating: false\n---\n# Helper Content\nThis is helper content",
        )
        .await?;

    // Create an intermediate snippet that depends on helper
    source_repo
        .add_resource(
            "snippets",
            "base",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: helper
      install: false
      path: ../snippets/helper.md
---
# Base Content

{{ agpm.deps.snippets.helper.content }}
"#,
        )
        .await?;

    // Create a command that depends on base (which transitively depends on helper)
    // The command must declare helper explicitly if it wants to access it directly
    source_repo
        .add_resource(
            "commands",
            "my-command",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: base
      install: false
      path: ../snippets/base.md
    - name: helper
      install: false
      path: ../snippets/helper.md
---
# My Command

Base: {{ agpm.deps.snippets.base.content }}

Helper: {{ agpm.deps.snippets.helper.content }}
"#,
        )
        .await?;

    source_repo.commit_all("Add nested dependencies")?;
    source_repo.tag_version("v1.0.0")?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-source", &source_repo.file_url())
        .add_standard_command("my-command", "test-source", "commands/my-command.md")
        .build();
    project.write_manifest(&manifest).await?;

    // Run install 10 times and verify content stability
    let mut all_hashes = Vec::new();
    for i in 0..10 {
        let output = project.run_agpm(&["install", "--quiet"])?;
        assert!(
            output.success,
            "Install #{} with nested transitive custom names failed: {}",
            i + 1,
            output.stderr
        );

        // Verify both custom names are accessible in the template
        let command_path = project.project_path().join(".claude/commands/my-command.md");
        let command_content = tokio::fs::read_to_string(&command_path).await?;
        assert!(
            command_content.contains("Helper Content"),
            "Nested transitive custom name 'helper' was not accessible in install #{}",
            i + 1
        );

        let hashes = collect_installed_hashes(project.project_path())?;
        all_hashes.push(hashes);
    }

    // Verify all runs produced identical content
    let first_hashes = &all_hashes[0];
    for (i, hashes) in all_hashes.iter().enumerate().skip(1) {
        assert_eq!(
            first_hashes,
            hashes,
            "Install #{} with nested transitive custom names produced different content than install #1.\n\
             This indicates a template rendering stability issue with custom name extraction.",
            i + 1
        );
    }

    Ok(())
}
