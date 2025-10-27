//! Tests for lockfile determinism and dual checksum consistency

use crate::common::TestProject;
use anyhow::Result;
use std::collections::HashSet;
use tokio::fs;

/// Test that lockfile generation is deterministic across multiple runs
/// This is the key test to verify the dual checksum system produces consistent results
#[tokio::test]
async fn test_lockfile_determinism_5_runs() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create multiple resources with different complexity levels
    test_repo
        .add_resource(
            "agents",
            "simple-agent",
            r#"---
title: Simple Agent
model: claude-3-sonnet
temperature: 0.7
---
# Simple Agent

I am a simple agent.
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "agents",
            "templated-agent",
            r#"---
title: "{{ project.name }} Agent"
model: "{{ config.model }}"
temperature: {{ config.temperature }}
dependencies:
  agents:
    - path: agents/helper.md
      version: "v1.0.0"
agpm:
  templating: true
---
# {{ project.name }} Agent

I am a templated agent with variables.
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "agents",
            "helper",
            r#"---
title: Helper Agent
model: claude-3-haiku
---
# Helper Agent

I help other agents.
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "snippets",
            "code-snippet",
            r#"---
title: Code Snippet
language: rust
---
// Rust code snippet
fn main() {
    println!("Hello, world!");
}
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "commands",
            "deploy-command",
            r#"---
title: Deploy Command
description: Deploy application to production
---
# Deploy Command

```bash
#!/bin/bash
echo "Deploying to production..."
```
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Complex manifest with template variables and patches
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
simple = {{ source = "test-repo", path = "agents/simple-agent.md", version = "v1.0.0" }}
templated = {{ source = "test-repo", path = "agents/templated-agent.md", version = "v1.0.0", template_vars = {{ project = {{ name = "Production" }}, config = {{ model = "claude-3-opus", temperature = 0.5 }} }} }}
helper = {{ source = "test-repo", path = "agents/helper.md", version = "v1.0.0" }}

[snippets]
code = {{ source = "test-repo", path = "snippets/code-snippet.md", version = "v1.0.0" }}

[commands]
deploy = {{ source = "test-repo", path = "commands/deploy-command.md", version = "v1.0.0" }}

[patch.agents.simple]
model = "claude-3-sonnet-20240229"
temperature = "0.8"

[patch.agents.templated]
temperature = "0.3"
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Run install 5 times and collect lockfile content
    let mut lockfile_contents = Vec::new();
    let mut checksums = HashSet::new();

    for run in 1..=5 {
        // Clean any existing lockfile
        let lockfile_path = project.project_path().join("agpm.lock");
        if lockfile_path.exists() {
            fs::remove_file(&lockfile_path).await?;
        }

        // Run install
        let output = project.run_agpm(&["install"])?;
        assert!(output.success, "Run {} should succeed. Stderr: {}", run, output.stderr);

        // Read lockfile
        let lockfile = project.load_lockfile()?;

        // Serialize lockfile to string for hashing
        let lockfile_content = toml::to_string(&lockfile)?;

        // Normalize lockfile by removing timestamps before hashing
        let normalize = |s: &str| {
            s.lines()
                .filter(|line| !line.trim().starts_with("fetched_at"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let normalized_content = normalize(&lockfile_content);

        // Calculate checksum of normalized content
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        normalized_content.hash(&mut hasher);
        let content_hash = hasher.finish();

        lockfile_contents.push(lockfile_content.clone());
        checksums.insert(content_hash);

        // Verify key elements are present using struct
        let has_context_checksum = lockfile.agents.iter().any(|a| a.context_checksum.is_some());
        assert!(has_context_checksum, "Run {} should contain context_checksum", run);

        let has_variant_inputs = lockfile.agents.iter().any(|a| {
            // Check if variant_inputs is not an empty object
            a.variant_inputs.json().as_object().map_or(false, |obj| !obj.is_empty())
        });
        assert!(has_variant_inputs, "Run {} should contain variant_inputs", run);

        assert!(
            lockfile.agents.iter().any(|a| a.name.contains("simple")),
            "Run {} should contain simple agent",
            run
        );
        assert!(
            lockfile.agents.iter().any(|a| a.name.contains("templated")),
            "Run {} should contain templated agent",
            run
        );

        // Verify consistent ordering and formatting (excluding timestamps)
        if run > 1 {
            // Re-use the same normalize function from above
            let normalized_first = normalize(&lockfile_contents[0]);
            let normalized_current = normalized_content.clone();

            assert_eq!(
                normalized_first,
                normalized_current,
                "Lockfile content must be identical across runs (excluding timestamps). Run 1 vs Run {}:\n\nDiff:\n{}",
                run,
                unified_diff(&normalized_first, &normalized_current)
            );
        }
    }

    // All 5 runs should produce identical checksums (deterministic)
    assert_eq!(
        checksums.len(),
        1,
        "All 5 runs should produce identical lockfile content, but got {} different versions",
        checksums.len()
    );

    // Additional verification: check that context checksums are consistent
    // Load first lockfile to extract checksums
    let first_lockfile = toml::from_str::<agpm_cli::lockfile::LockFile>(&lockfile_contents[0])?;

    // Extract context checksums from agents that have them
    let context_checksums: Vec<&str> = first_lockfile
        .agents
        .iter()
        .filter_map(|agent| agent.context_checksum.as_deref())
        .collect();

    // Should have context checksums for templated resources
    assert!(
        !context_checksums.is_empty(),
        "Should have at least one context checksum for templated resources"
    );

    // All context checksums should be valid SHA-256 format
    for checksum in &context_checksums {
        assert!(
            checksum.starts_with("sha256:"),
            "Context checksum should have sha256: prefix: {}",
            checksum
        );
        let hash_part = &checksum[7..]; // Remove "sha256:" prefix
        assert_eq!(hash_part.len(), 64, "SHA-256 hash should be 64 characters: {}", hash_part);
        assert!(
            hash_part.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA-256 hash should be hex digits: {}",
            hash_part
        );
    }

    Ok(())
}

/// Test that variant_inputs serialization is deterministic
#[tokio::test]
async fn test_variant_inputs_determinism() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a complex template with nested objects and arrays
    test_repo
        .add_resource(
            "agents",
            "complex-variant",
            r#"---
title: "{{ project.name }}"
config:
  database:
    host: "{{ db.host }}"
    port: {{ db.port }}
    ssl: {{ db.ssl }}
  features:
    {% for feature in features %}
    - {{ feature }}
    {% endfor %}
agpm:
  templating: true
---
# {{ project.name }} Agent

Database: {{ db.host }}:{{ db.port }}
Features: {{ features | join(sep=", ") }}
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Manifest with complex variant_inputs structure
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
complex = {{ source = "test-repo", path = "agents/complex-variant.md", version = "v1.0.0", template_vars = {{ project = {{ name = "DatabaseAgent" }}, db = {{ host = "localhost", port = 5432, ssl = true }}, features = ["auth", "logging", "monitoring"] }} }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Run multiple times to test serialization consistency
    let mut serialized_vars = Vec::new();

    for run in 1..=3 {
        // Clean lockfile
        let lockfile_path = project.project_path().join("agpm.lock");
        if lockfile_path.exists() {
            fs::remove_file(&lockfile_path).await?;
        }

        // Run install
        let output = project.run_agpm(&["install"])?;
        assert!(output.success, "Run {} should succeed", run);

        // Load lockfile and extract variant_inputs for complex agent
        let lockfile = project.load_lockfile()?;

        // Find the complex agent
        let complex_agent =
            lockfile.agents.iter().find(|agent| agent.name == "agents/complex-variant").expect(
                &format!(
                    "Should find complex agent in run {}. Available agents: {:?}",
                    run,
                    lockfile.agents.iter().map(|a| &a.name).collect::<Vec<_>>()
                ),
            );

        // Serialize variant_inputs for comparison
        let variant_inputs_serialized = toml::to_string(&complex_agent.variant_inputs)?;

        serialized_vars.push(variant_inputs_serialized);
    }

    // All serialized variant_inputs should be identical
    for i in 1..serialized_vars.len() {
        assert_eq!(
            serialized_vars[0],
            serialized_vars[i],
            "Variant inputs serialization should be deterministic. Run 1 vs Run {}:\n\nRun 1: {}\n\nRun {}: {}",
            i + 1,
            serialized_vars[0],
            i + 1,
            serialized_vars[i]
        );
    }

    Ok(())
}

/// Test that toml_edit formatting produces deterministic output
#[tokio::test]
async fn test_toml_edit_determinism() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create multiple resources to test array ordering
    test_repo
        .add_resource(
            "agents",
            "zebra-agent",
            r#"---
title: Zebra Agent
---
# Zebra Agent
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "agents",
            "alpha-agent",
            r#"---
title: Alpha Agent
---
# Alpha Agent
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "agents",
            "beta-agent",
            r#"---
title: Beta Agent
---
# Beta Agent
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Manifest with resources in non-alphabetical order
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[agents]
zebra = {{ source = "test-repo", path = "agents/zebra-agent.md", version = "v1.0.0" }}
alpha = {{ source = "test-repo", path = "agents/alpha-agent.md", version = "v1.0.0" }}
beta = {{ source = "test-repo", path = "agents/beta-agent.md", version = "v1.0.0" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Run multiple times to test ordering consistency
    let mut agent_sections = Vec::new();

    for run in 1..=3 {
        // Clean lockfile
        let lockfile_path = project.project_path().join("agpm.lock");
        if lockfile_path.exists() {
            fs::remove_file(&lockfile_path).await?;
        }

        // Run install
        let output = project.run_agpm(&["install"])?;
        assert!(output.success, "Run {} should succeed", run);

        // Load lockfile and extract agent names in order
        let lockfile = project.load_lockfile()?;
        let current_agents: Vec<String> =
            lockfile.agents.iter().map(|agent| agent.name.clone()).collect();

        agent_sections.push(current_agents);
    }

    // All agent orders should be identical
    for i in 1..agent_sections.len() {
        assert_eq!(
            agent_sections[0],
            agent_sections[i],
            "Agent ordering should be deterministic. Run 1 vs Run {}:\n\nRun 1: {:?}\n\nRun {}: {:?}",
            i + 1,
            agent_sections[0],
            i + 1,
            agent_sections[i]
        );
    }

    // Verify consistent ordering (should be sorted alphabetically for determinism)
    assert_eq!(
        agent_sections[0],
        vec![
            "agents/alpha-agent".to_string(),
            "agents/beta-agent".to_string(),
            "agents/zebra-agent".to_string()
        ],
        "Agents should be sorted alphabetically for deterministic lockfiles"
    );

    Ok(())
}

/// Test that multi-tool dependency lookups are deterministic
///
/// This is a regression test for the lockfile non-determinism issue where
/// dependencies with the same name but different tools would be resolved
/// non-deterministically, causing context_checksum to vary between runs.
///
/// The test creates:
/// - A snippet that exists for both `agpm` and `claude-code` tools
/// - A command for `claude-code` that depends on the snippet
/// - Verifies that the command always resolves to the claude-code variant
/// - Verifies that context checksums stay stable across multiple runs
#[tokio::test]
async fn test_multi_tool_dependency_determinism() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a snippet that will be used by both tools
    test_repo
        .add_resource(
            "snippets/commands",
            "commit",
            r#"---
title: Commit Guidelines
---
# Commit Guidelines

Always write clear commit messages.
"#,
        )
        .await?;

    // Create a command that depends on the snippet
    test_repo
        .add_resource(
            "commands",
            "update-examples",
            r#"---
title: Update Examples Command
dependencies:
  snippets:
    - path: snippets/commands/commit.md
      version: v1.0.0
agpm:
  templating: true
---
# Update Examples

{{ agpm.deps.snippets.commit.content }}

Run the update script.
"#,
        )
        .await?;

    test_repo.commit_all("Initial version")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Manifest with the snippet for both tools and a command for claude-code
    let manifest = format!(
        r#"[sources]
test-repo = "{}"

[snippets]
# This snippet gets installed for both agpm and claude-code tools
commit-agpm = {{ source = "test-repo", path = "snippets/commands/commit.md", version = "v1.0.0", tool = "agpm" }}
commit-claude = {{ source = "test-repo", path = "snippets/commands/commit.md", version = "v1.0.0", tool = "claude-code" }}

[commands]
# This command uses claude-code, so it should resolve to the claude-code variant of the snippet
update-examples = {{ source = "test-repo", path = "commands/update-examples.md", version = "v1.0.0", tool = "claude-code" }}
"#,
        repo_url
    );

    project.write_manifest(&manifest).await?;

    // Run install 5 times and verify:
    // 1. Lockfile is identical across runs
    // 2. Context checksums are stable
    // 3. Command resolves to correct tool-specific snippet
    let mut lockfile_contents = Vec::new();
    let mut context_checksums = Vec::new();

    for run in 1..=5 {
        // Clean any existing lockfile
        let lockfile_path = project.project_path().join("agpm.lock");
        if lockfile_path.exists() {
            fs::remove_file(&lockfile_path).await?;
        }

        // Run install
        let output = project.run_agpm(&["install"])?;
        assert!(output.success, "Run {} should succeed. Stderr: {}", run, output.stderr);

        // Load lockfile
        let lockfile = project.load_lockfile()?;
        let lockfile_content = toml::to_string(&lockfile)?;
        lockfile_contents.push(lockfile_content.clone());

        // Extract context checksum for the command using struct
        let update_examples_cmd =
            lockfile.commands.iter().find(|cmd| cmd.name == "commands/update-examples").expect(
                &format!(
                    "Run {} should have update-examples command. Available commands: {:?}",
                    run,
                    lockfile.commands.iter().map(|c| &c.name).collect::<Vec<_>>()
                ),
            );

        let command_context_checksum = update_examples_cmd
            .context_checksum
            .as_ref()
            .expect(&format!("Run {} should have context_checksum for templated command", run));
        context_checksums.push(command_context_checksum.clone());

        // Verify lockfile contains both snippet variants using struct
        let has_agpm_tool = lockfile.snippets.iter().any(|s| s.tool.as_deref() == Some("agpm"));
        assert!(has_agpm_tool, "Run {} should contain agpm tool variant", run);

        let has_claude_tool =
            lockfile.snippets.iter().any(|s| s.tool.as_deref() == Some("claude-code"));
        assert!(has_claude_tool, "Run {} should contain claude-code tool variant", run);

        let has_update_examples =
            lockfile.commands.iter().any(|c| c.name == "commands/update-examples");
        assert!(has_update_examples, "Run {} should contain commands/update-examples command", run);

        // Verify consistent content across runs (ignoring fetched_at timestamps)
        if run > 1 {
            let normalize = |s: &str| {
                s.lines()
                    .filter(|line| !line.trim().starts_with("fetched_at"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            let normalized_first = normalize(&lockfile_contents[0]);
            let normalized_current = normalize(&lockfile_content);

            assert_eq!(
                normalized_first,
                normalized_current,
                "Lockfile must be identical across runs (excluding timestamps). Run 1 vs Run {}:\n\nDiff:\n{}",
                run,
                unified_diff(&normalized_first, &normalized_current)
            );
        }
    }

    // All 5 runs should produce identical context checksums (key determinism test)
    for i in 1..context_checksums.len() {
        assert_eq!(
            context_checksums[0],
            context_checksums[i],
            "Context checksum should be stable. Run 1 vs Run {}:\n\nRun 1: {}\n\nRun {}: {}",
            i + 1,
            context_checksums[0],
            i + 1,
            context_checksums[i]
        );
    }

    // Verify the rendered command file includes the snippet content
    let command_path = project.project_path().join(".claude/commands/update-examples.md");
    let command_content = fs::read_to_string(&command_path).await?;

    assert!(
        command_content.contains("Commit Guidelines"),
        "Command should include snippet content from dependency"
    );
    assert!(
        command_content.contains("Always write clear commit messages"),
        "Command should include snippet body"
    );

    Ok(())
}

/// Helper function to produce a simple unified diff for debugging
fn unified_diff(a: &str, b: &str) -> String {
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();

    if a_lines == b_lines {
        return "No differences".to_string();
    }

    let mut diff = String::new();
    for (i, (line_a, line_b)) in a_lines.iter().zip(b_lines.iter()).enumerate() {
        if line_a != line_b {
            diff.push_str(&format!("Line {}:\n", i + 1));
            diff.push_str(&format!("- {}\n", line_a));
            diff.push_str(&format!("+ {}\n", line_b));
        }
    }

    if a_lines.len() != b_lines.len() {
        diff.push_str(&format!(
            "\nLength difference: {} lines vs {} lines\n",
            a_lines.len(),
            b_lines.len()
        ));
    }

    diff
}
