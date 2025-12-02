//! Integration tests for resource-specific template variables with transitive dependencies
//!
//! This tests the critical use case where a single generic resource (e.g., backend-engineer.md)
//! is specialized for different languages using template_vars, and its transitive dependencies
//! should be resolved using those resource-specific vars, not the global project config.

use crate::common::{ManifestBuilder, TestProject};
use anyhow::Result;

/// Regression test: ensure snippets referenced both directly (via manifest key)
/// and transitively (via path-prefixed dependency refs) still resolve to the
/// same template alias (`agpm.deps.snippets.best_practices`) after installation.
#[tokio::test]
async fn test_transitive_snippet_alias_resolves_with_direct_manifest_entry() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let community_repo = project.create_source_repo("community").await?;

    community_repo
        .add_resource(
            "snippets",
            "rust-best-practices",
            "# Rust Best Practices\n- Use Result\n- Embrace ownership\n",
        )
        .await?;

    community_repo
        .add_resource(
            "agents",
            "rust-dev",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: best_practices
      path: ../snippets/rust-best-practices.md
      install: false
---
# Rust Developer

{{ agpm.deps.snippets.best_practices.content }}
"#,
        )
        .await?;

    community_repo.commit_all("Add rust resources with shared snippet")?;
    community_repo.tag_version("v1.0.0")?;

    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_snippet("rust-best-practices", |d| {
            d.source("community").path("snippets/rust-best-practices.md").version("v1.0.0")
        })
        .add_agent("rust-dev", |d| {
            d.source("community").path("agents/rust-dev.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed. Stderr:\n{}\nStdout:\n{}",
        output.stderr, output.stdout
    );
    assert!(
        !output.stderr.contains("agpm.deps.snippets.best_practices"),
        "Install stderr should not contain missing best_practices alias error:\n{}",
        output.stderr
    );

    let agent_path = project.project_path().join(".claude/agents/agpm/rust-dev.md");
    let agent_content = tokio::fs::read_to_string(&agent_path).await?;
    assert!(
        agent_content.contains("Use Result"),
        "Rendered agent should include snippet content. File:\n{}",
        agent_content
    );

    Ok(())
}

/// Test that resource-specific template_vars override global config for transitive dependencies
#[tokio::test]
async fn test_resource_template_vars_override_for_transitive_deps() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo
    let community_repo = project.create_source_repo("community").await?;

    // Add language-specific helper snippets
    community_repo
        .add_resource(
            "snippets",
            "best-practices/golang-best-practices",
            "# Golang Best Practices\n- Use error wrapping\n- Prefer composition over inheritance",
        )
        .await?;

    community_repo
        .add_resource(
            "snippets",
            "best-practices/python-best-practices",
            "# Python Best Practices\n- Follow PEP 8\n- Use type hints",
        )
        .await?;

    community_repo
        .add_resource(
            "snippets",
            "best-practices/rust-best-practices",
            "# Rust Best Practices\n- Embrace ownership\n- Use Result for errors",
        )
        .await?;

    // Add generic backend engineer agent with templated transitive dependencies
    community_repo
        .add_resource(
            "agents",
            "backend-engineer",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - name: best-practices
      path: ../snippets/best-practices/{{ agpm.project.language }}-best-practices.md
      install: false
---

# Backend Engineer

You are a backend engineer specializing in {{ agpm.project.language }}.

## Best Practices

{{ agpm.deps.snippets.best_practices.content }}
"#,
        )
        .await?;

    community_repo.commit_all("Add resources with templated deps")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest with:
    // - Global project config: language = "python"
    // - Specific resource overrides for golang and rust
    let source_url = community_repo.bare_file_url(project.sources_path())?;

    // Build manifest manually to include template_vars
    // Note: In format! strings, {{ becomes { in the output, so {{{{ becomes {{
    let manifest = format!(
        r#"[project]
language = "python"

[sources]
community = "{}"

[agents.backend-engineer-default]
source = "community"
path = "agents/backend-engineer.md"
version = "v1.0.0"

[agents.backend-engineer-golang]
source = "community"
path = "agents/backend-engineer.md"
version = "v1.0.0"
filename = "backend-engineer-golang.md"

[agents.backend-engineer-golang.template_vars.project]
language = "golang"

[agents.backend-engineer-rust]
source = "community"
path = "agents/backend-engineer.md"
version = "v1.0.0"
filename = "backend-engineer-rust.md"

[agents.backend-engineer-rust.template_vars.project]
language = "rust"
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed. Stderr:\n{}\n\nStdout:\n{}",
        output.stderr, output.stdout
    );

    // Verify lockfile contains all three variants with correct transitive deps
    let lockfile_content = project.read_lockfile().await?;

    // FIXED: With the corrected deduplication logic that always checks variant_inputs,
    // multiple manifest entries with the same path but different template_vars now
    // correctly create separate lockfile entries.
    //
    // Each manifest key with different template_vars creates a separate lockfile entry.

    // Verify all three variants are present
    assert!(
        lockfile_content.contains("manifest_alias = \"backend-engineer-default\""),
        "backend-engineer-default variant should be in lockfile. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("manifest_alias = \"backend-engineer-golang\""),
        "backend-engineer-golang variant should be in lockfile. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("manifest_alias = \"backend-engineer-rust\""),
        "backend-engineer-rust variant should be in lockfile. Lockfile:\n{}",
        lockfile_content
    );

    // Verify ALL transitive dependencies are still resolved (they don't get deduplicated)
    assert!(
        lockfile_content.contains("snippets/best-practices/python-best-practices"),
        "Should have python best practices. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("snippets/best-practices/golang-best-practices"),
        "Should have golang best practices. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("snippets/best-practices/rust-best-practices"),
        "Should have rust best practices. Lockfile:\n{}",
        lockfile_content
    );

    // Verify ALL THREE installed files contain the correct content
    // With the bug fix, all three variants should exist with their own template_vars

    // Check default variant (Python)
    let default_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/agpm/backend-engineer.md"),
    )
    .await?;
    assert!(
        default_content.contains("Follow PEP 8"),
        "Default variant should contain Python best practices"
    );

    // Check Golang variant
    let golang_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/agpm/backend-engineer-golang.md"),
    )
    .await?;
    assert!(
        golang_content.contains("Use error wrapping"),
        "Golang variant should contain Golang best practices"
    );

    // Check Rust variant
    let rust_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/agpm/backend-engineer-rust.md"),
    )
    .await?;
    assert!(
        rust_content.contains("Embrace ownership"),
        "Rust variant should contain Rust best practices"
    );

    Ok(())
}

/// Test that template_vars are stored in lockfile for reproducibility
#[tokio::test]
async fn test_template_vars_stored_in_lockfile() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo
    let community_repo = project.create_source_repo("community").await?;

    community_repo.add_resource("agents", "simple-agent", "# Simple Agent\nDoes things").await?;

    community_repo.commit_all("Add agent")?;
    community_repo.tag_version("v1.0.0")?;

    let source_url = community_repo.bare_file_url(project.sources_path())?;

    let manifest = format!(
        r#"[sources]
community = "{}"

[agents.agent-with-vars]
source = "community"
path = "agents/simple-agent.md"
version = "v1.0.0"

[agents.agent-with-vars.template_vars.project]
language = "golang"
framework = "gin"
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Read lockfile and verify template_vars are stored
    let lockfile_content = project.read_lockfile().await?;

    // The lockfile should contain the variant_inputs
    assert!(
        lockfile_content.contains("variant_inputs"),
        "Lockfile should store variant_inputs field. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("golang"),
        "Lockfile should store language=golang. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("gin"),
        "Lockfile should store framework=gin. Lockfile:\n{}",
        lockfile_content
    );

    // Parse lockfile to verify structure
    let lockfile: toml::Value = toml::from_str(&lockfile_content)?;
    let agents = lockfile.get("agents").and_then(|a| a.as_array()).unwrap();
    let agent = &agents[0];

    assert!(agent.get("variant_inputs").is_some(), "Agent entry should have variant_inputs field");

    let variant_inputs = agent.get("variant_inputs").unwrap();
    let variant_inputs_table = variant_inputs.as_table().unwrap();
    let project_vars = variant_inputs_table.get("project").and_then(|p| p.as_table()).unwrap();

    assert_eq!(
        project_vars.get("language").and_then(|l| l.as_str()),
        Some("golang"),
        "Should have language=golang in template_vars"
    );
    assert_eq!(
        project_vars.get("framework").and_then(|f| f.as_str()),
        Some("gin"),
        "Should have framework=gin in template_vars"
    );

    Ok(())
}

/// Test that template_vars from lockfile are used during template rendering
#[tokio::test]
async fn test_lockfile_template_vars_used_in_rendering() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;

    // Create source repo with a templated agent
    let community_repo = project.create_source_repo("community").await?;

    community_repo
        .add_resource(
            "agents",
            "templated-agent",
            r#"---
agpm:
  templating: true
---

# Agent for {{ agpm.project.language }}

Specialized for {{ agpm.project.language }}"#,
        )
        .await?;

    community_repo.commit_all("Add templated agent")?;
    community_repo.tag_version("v1.0.0")?;

    let source_url = community_repo.bare_file_url(project.sources_path())?;

    let manifest = format!(
        r#"[sources]
community = "{}"

[agents.agent-java]
source = "community"
path = "agents/templated-agent.md"
version = "v1.0.0"
filename = "agent-java.md"

[agents.agent-java.template_vars.project]
language = "java"
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. Stderr: {}", output.stderr);

    // Verify the installed file was rendered with the template_vars
    let installed_content =
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agpm/agent-java.md"))
            .await?;

    assert!(
        installed_content.contains("Agent for java"),
        "Should render with language=java from template_vars. Content:\n{}",
        installed_content
    );
    assert!(
        installed_content.contains("Specialized for java"),
        "Should render all occurrences with template_vars. Content:\n{}",
        installed_content
    );
    assert!(
        !installed_content.contains("{{"),
        "Should not contain unrendered template syntax. Content:\n{}",
        installed_content
    );

    Ok(())
}

/// Regression test: tool overrides should work with relative dependency paths
///
/// This tests that when a resource declares a dependency with:
/// - A relative path (../snippets/...)
/// - An explicit tool override (tool = "agpm")
///
/// The tool override is preserved during template rendering, and the snippet
/// installs to the correct tool directory (.agpm/ not .claude/).
///
/// Bug: Path normalization mismatch between extract_dependency_specs (normalizes)
/// and build_dependencies_data (uses raw lockfile paths) causes cache lookup to fail,
/// losing the tool override and causing the dependency to be dropped.
#[tokio::test]
async fn test_tool_override_preserved_with_relative_paths() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let community_repo = project.create_source_repo("community").await?;

    // Create a snippet in a subdirectory
    community_repo
        .add_resource(
            "snippets/agents",
            "github-actions-expert",
            "# GitHub Actions Expert\n- Use workflow best practices\n- Cache dependencies\n",
        )
        .await?;

    // Create an agent that declares the snippet as a dependency with:
    // 1. Relative path (../snippets/...)
    // 2. Explicit tool override (tool: agpm)
    // 3. Template reference to snippet content
    community_repo
        .add_resource(
            "agents",
            "devops-agent",
            r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: ../snippets/agents/github-actions-expert.md
      tool: agpm
      name: github_actions_expert
      install: false
---
# DevOps Agent

## GitHub Actions Best Practices

{{ agpm.deps.snippets.github_actions_expert.content }}
"#,
        )
        .await?;

    community_repo.commit_all("Add agent with relative dependency and tool override")?;
    community_repo.tag_version("v1.0.0")?;

    let source_url = community_repo.bare_file_url(project.sources_path())?;
    let manifest = ManifestBuilder::new()
        .add_source("community", &source_url)
        .add_agent("devops-agent", |d| {
            d.source("community").path("agents/devops-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;
    assert!(
        output.success,
        "Install should succeed. Stderr:\n{}\nStdout:\n{}",
        output.stderr, output.stdout
    );

    // Verify the snippet was NOT dropped during template rendering
    assert!(
        !output.stderr.contains("agpm.deps.snippets.github_actions_expert"),
        "Should not have template error about missing snippet. Stderr:\n{}",
        output.stderr
    );

    // Verify the agent was rendered with the snippet content
    let agent_path = project.project_path().join(".claude/agents/agpm/devops-agent.md");
    let agent_content = tokio::fs::read_to_string(&agent_path).await?;

    assert!(
        agent_content.contains("Use workflow best practices"),
        "Agent should contain snippet content. File:\n{}",
        agent_content
    );
    assert!(
        agent_content.contains("Cache dependencies"),
        "Agent should contain full snippet content. File:\n{}",
        agent_content
    );
    assert!(
        !agent_content.contains("{{"),
        "Agent should not have unrendered template syntax. File:\n{}",
        agent_content
    );

    // Verify the snippet itself was NOT installed (install: false)
    let snippet_claude_path =
        project.project_path().join(".claude/snippets/agpm/agents/github-actions-expert.md");
    let snippet_agpm_path =
        project.project_path().join(".agpm/snippets/agents/github-actions-expert.md");

    assert!(
        !snippet_claude_path.exists(),
        "Snippet should not be installed to .claude/ (wrong tool)"
    );
    assert!(
        !snippet_agpm_path.exists(),
        "Snippet should not be installed to .agpm/ (install: false)"
    );

    Ok(())
}
