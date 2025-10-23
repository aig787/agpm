//! Integration tests for resource-specific template variables with transitive dependencies
//!
//! This tests the critical use case where a single generic resource (e.g., backend-engineer.md)
//! is specialized for different languages using template_vars, and its transitive dependencies
//! should be resolved using those resource-specific vars, not the global project config.

use crate::common::TestProject;
use anyhow::Result;

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

    // All three agent variants should be present
    assert!(
        lockfile_content.contains("backend-engineer-default"),
        "Default variant should be in lockfile"
    );
    assert!(
        lockfile_content.contains("backend-engineer-golang"),
        "Golang variant should be in lockfile"
    );
    assert!(
        lockfile_content.contains("backend-engineer-rust"),
        "Rust variant should be in lockfile"
    );

    // Each should have resolved its transitive dependencies correctly
    assert!(
        lockfile_content.contains("python-best-practices"),
        "Should have python best practices for default variant. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("golang-best-practices"),
        "Should have golang best practices for golang variant. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("rust-best-practices"),
        "Should have rust best practices for rust variant. Lockfile:\n{}",
        lockfile_content
    );

    // Verify the installed files contain the correct content
    let default_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/backend-engineer.md"),
    )
    .await?;
    assert!(
        default_content.contains("Follow PEP 8"),
        "Default variant should contain Python best practices"
    );

    let golang_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/backend-engineer-golang.md"),
    )
    .await?;
    assert!(
        golang_content.contains("Use error wrapping"),
        "Golang variant should contain Golang best practices"
    );

    let rust_content = tokio::fs::read_to_string(
        project.project_path().join(".claude/agents/backend-engineer-rust.md"),
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

    // The lockfile should contain the template_vars
    assert!(
        lockfile_content.contains("template_vars"),
        "Lockfile should store template_vars field. Lockfile:\n{}",
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

    assert!(agent.get("template_vars").is_some(), "Agent entry should have template_vars field");

    let template_vars = agent.get("template_vars").unwrap();
    let template_vars_str = template_vars.as_str().unwrap();
    let template_vars_json: serde_json::Value = serde_json::from_str(template_vars_str).unwrap();
    let project_vars = template_vars_json.get("project").and_then(|p| p.as_object()).unwrap();

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
        tokio::fs::read_to_string(project.project_path().join(".claude/agents/agent-java.md"))
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
