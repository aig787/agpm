//! Tests for edge cases and platform-specific behavior.
//!
//! Covers:
//! - Cross-platform path handling
//! - Project template variables
//! - Empty template scenarios
//! - Performance edge cases

use anyhow::{Context, Result};
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test template paths are rendered in platform-native format.
#[tokio::test]
async fn test_template_paths_platform_native() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent that references its install path
    let template_content = r#"---
title: Path Test Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent is installed at: `{{ agpm.resource.install_path }}`

You can find it at: following location:
{{ agpm.resource.install_path }}
"#;

    test_repo.add_resource("agents", "path-test", template_content).await?;

    // Also create a snippet with a nested path
    let snippet_content = r#"---
title: Nested Snippet
agpm:
  templating: true
---
# Utility Snippet

Install path: {{ agpm.resource.install_path }}
"#;

    test_repo.add_resource("snippets", "utils", snippet_content).await?;

    test_repo.commit_all("Add test resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path()).await?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("path-test", |d| {
            d.source("test-repo").path("agents/path-test.md").version("v1.0.0")
        })
        .add_snippet("nested-helper", |d| {
            d.source("test-repo").path("snippets/utils.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install resources
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. stderr: {}", output.stderr);

    // Read installed agent file
    let agent_path = project.project_path().join(".claude/agents/agpm/path-test.md");
    let agent_content = fs::read_to_string(&agent_path).await?;

    // Read installed snippet file
    let snippet_path = project.project_path().join(".agpm/snippets/utils.md");
    let snippet_content = fs::read_to_string(&snippet_path).await?;

    // Verify paths use platform-native separators
    #[cfg(windows)]
    {
        // On Windows, rendered paths should use backslashes
        // Note: agents install to .claude/agents/agpm/ subdirectory
        assert!(
            agent_content.contains(".claude\\agents\\agpm\\path-test.md"),
            "Agent content should contain Windows-style path with backslashes. Content:\n{}",
            agent_content
        );
        assert!(
            snippet_content.contains(".agpm\\snippets\\utils.md"),
            "Snippet content should contain Windows-style path with backslashes. Content:\n{}",
            snippet_content
        );
    }

    #[cfg(not(windows))]
    {
        // On Unix/macOS, rendered paths should use forward slashes
        assert!(
            agent_content.contains(".claude/agents/agpm/path-test.md"),
            "Agent content should contain Unix-style path with forward slashes"
        );
        assert!(
            snippet_content.contains(".agpm/snippets/utils.md"),
            "Snippet content should contain Unix-style path with forward slashes"
        );
    }

    Ok(())
}

/// Test project-specific template variables from manifest.
#[tokio::test]
async fn test_project_template_variables() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent that uses project variables for AI coding guidance
    test_repo
        .add_resource(
            "agents",
            "project-agent",
            r#"---
title: Project Code Reviewer
agpm:
  templating: true
---
# {{ agpm.project.name }} Code Reviewer

I review code for {{ agpm.project.name }} (version {{ agpm.project.version }}).

## Guidelines to Follow

Please refer to our documentation:
- Style Guide: {{ agpm.project.paths.style_guide }}
- Architecture: {{ agpm.project.paths.architecture }}
- Conventions: {{ agpm.project.paths.conventions }}

## Code Standards

When reviewing or generating code, enforce:
- Max line length: {{ agpm.project.standards.max_line_length }} characters
- Indentation: {{ agpm.project.standards.indent_size }} {{ agpm.project.standards.indent_style }}
- Naming: {{ agpm.project.standards.naming_convention }}

## Testing Requirements

{% if agpm.project.custom.require_tests %}
All code changes MUST include tests using {{ agpm.project.custom.test_framework }}.
{% endif %}

{% if agpm.project.custom.require_docstrings %}
All functions require docstrings in {{ agpm.project.custom.docstring_style }} format.
{% endif %}
"#,
        )
        .await?;

    test_repo.commit_all("Add project agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path()).await?;

    // Create complete manifest content directly
    // Note: [project] can have any structure - it's just a map of arbitrary variables
    let manifest_content = format!(
        r#"[sources]
test-repo = "{}"

[agents]
project-agent = {{ source = "test-repo", path = "agents/project-agent.md", version = "v1.0.0" }}

[project]
# Arbitrary variables - structure is completely flexible
name = "TestProject"
version = "2.1.0"

# Nested sections for organization (optional, just convention)
[project.paths]
style_guide = "docs/STYLE_GUIDE.md"
architecture = "docs/ARCHITECTURE.md"
conventions = "docs/CONVENTIONS.md"

[project.standards]
max_line_length = 100
indent_style = "spaces"
indent_size = 4
naming_convention = "snake_case"

[project.custom]
require_tests = true
test_framework = "pytest"
require_docstrings = true
docstring_style = "google"
"#,
        repo_url
    );

    // Write manifest
    let manifest_path = project.project_path().join("agpm.toml");
    fs::write(&manifest_path, &manifest_content).await?;

    // Install with templating enabled
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Installation should succeed");

    // Read installed file
    let installed_path = project.project_path().join(".claude/agents/agpm/project-agent.md");
    let content =
        fs::read_to_string(&installed_path).await.context("Failed to read installed agent file")?;

    // Verify project variables were substituted
    assert!(
        content.contains("# TestProject Code Reviewer"),
        "Project name should be substituted in title. Content:\n{}",
        content
    );
    assert!(
        content.contains("I review code for TestProject (version 2.1.0)"),
        "Project name and version should be substituted. Content:\n{}",
        content
    );
    assert!(
        content.contains("Style Guide: docs/STYLE_GUIDE.md"),
        "Style guide path should be substituted. Content:\n{}",
        content
    );
    assert!(
        content.contains("Architecture: docs/ARCHITECTURE.md"),
        "Architecture path should be substituted. Content:\n{}",
        content
    );
    assert!(
        content.contains("Conventions: docs/CONVENTIONS.md"),
        "Conventions path should be substituted. Content:\n{}",
        content
    );
    assert!(
        content.contains("Max line length: 100 characters"),
        "Max line length standard should be substituted. Content:\n{}",
        content
    );
    assert!(
        content.contains("Indentation: 4 spaces"),
        "Indentation standard should be substituted. Content:\n{}",
        content
    );
    assert!(
        content.contains("Naming: snake_case"),
        "Naming convention should be substituted. Content:\n{}",
        content
    );
    assert!(
        content.contains("All code changes MUST include tests using pytest"),
        "Testing requirement should be rendered. Content:\n{}",
        content
    );
    assert!(
        content.contains("All functions require docstrings in google format"),
        "Docstring requirement should be rendered. Content:\n{}",
        content
    );

    // Verify original template syntax is gone
    assert!(
        !content.contains("{{ agpm.project"),
        "Template syntax should be replaced. Content:\n{}",
        content
    );
    assert!(!content.contains("{% for"), "Loop syntax should be replaced. Content:\n{}", content);

    Ok(())
}
