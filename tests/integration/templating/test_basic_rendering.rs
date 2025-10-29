//! Tests for basic template rendering functionality.
//!
//! Covers:
//! - Simple variable substitution
//! - Template syntax validation
//! - Basic resource information injection
//! - Literal block handling

use anyhow::Result;
use tokio::fs;

use crate::common::{ManifestBuilder, TestProject};

/// Test basic template variable substitution in markdown files.
#[tokio::test]
async fn test_basic_template_substitution() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with template variables
    test_repo
        .add_resource(
            "agents",
            "test-agent",
            r#"---
title: Test Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent is installed at: `{{ agpm.resource.install_path }}`
Version: {{ agpm.resource.version }}
"#,
        )
        .await?;

    test_repo.commit_all("Add test agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("test-agent", |d| {
            d.source("test-repo").path("agents/test-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install - templating enabled via frontmatter
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Read the installed file and verify template variables were replaced
    let installed_path = project.project_path().join(".claude/agents/test-agent.md");
    let content = fs::read_to_string(&installed_path).await?;

    // Verify variables were substituted - name includes resource type directory
    assert!(
        content.contains("# agents/test-agent"),
        "Resource name should be substituted with canonical format"
    );

    // Check for platform-native path separators
    #[cfg(windows)]
    let expected_path = "installed at: `.claude\\agents\\test-agent.md`";
    #[cfg(not(windows))]
    let expected_path = "installed at: `.claude/agents/test-agent.md`";

    assert!(
        content.contains(expected_path),
        "Install path should be substituted with platform-native separators. Content:\n{}",
        content
    );
    assert!(content.contains("Version: v1.0.0"), "Version should be substituted");

    // Verify original template syntax is gone
    assert!(!content.contains("{{ agpm"), "Template syntax should be replaced");

    Ok(())
}

/// Test that files without templating enabled can contain template-like syntax.
///
/// This is critical for snippets containing JSDoc or other documentation that uses
/// curly braces (e.g., `@param {{id: number, name: string}} user`).
#[tokio::test]
async fn test_non_templated_files_with_curly_braces() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a snippet with JSDoc-style syntax but NO templating enabled
    // This should install successfully without attempting to parse the {{ }} as templates
    test_repo
        .add_resource(
            "snippets",
            "javascript-snippet",
            r#"---
title: JavaScript Snippet
---
// JavaScript code with arrow functions
const calculateSum = (a, b) => {
    return a + b;
};

// Template literal syntax in JavaScript
const message = `Hello, ${name}!`;

// Object destructuring
const { firstName, lastName } = person;

// Array destructuring with rest
const [first, ...rest] = items;

console.log(calculateSum(5, 3));
console.log(message);
console.log(firstName, lastName);
console.log(first, rest);
"#,
        )
        .await?;

    test_repo.commit_all("Add JavaScript snippet")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_snippet("javascript-snippet", |d| {
            d.source("test-repo")
                .path("snippets/javascript-snippet.md")
                .version("v1.0.0")
                .tool("agpm")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install the snippet
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Read the installed file and verify it wasn't processed as a template
    let installed_path = project.project_path().join(".agpm/snippets/javascript-snippet.md");
    let content = fs::read_to_string(&installed_path).await?;

    // Verify JavaScript syntax is preserved exactly
    assert!(content.contains("const calculateSum = (a, b) => {"));
    assert!(content.contains("const message = `Hello, ${name}!`;"));
    assert!(content.contains("const { firstName, lastName } = person;"));
    assert!(content.contains("const [first, ...rest] = items;"));
    assert!(content.contains("console.log(calculateSum(5, 3));"));

    Ok(())
}

/// Test that resources can reference each other via templates.
#[tokio::test]
async fn test_dependency_references() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a helper snippet first
    test_repo
        .add_resource(
            "snippets",
            "helper",
            r#"---
title: Helper Functions
agpm:
  templating: true
---
# Helper Functions

This file contains helper functions.

## Function List
- sum
- multiply
- divide
"#,
        )
        .await?;

    // Create an agent that references the snippet via content filter
    test_repo
        .add_resource(
            "agents",
            "main-agent",
            r#"---
title: Main Agent
dependencies:
  snippets:
    - path: snippets/helper.md
      tool: agpm
      name: helper
agpm:
  templating: true
---
# Main Agent

This agent uses helper functions from snippets.

{{ agpm.deps.snippets.helper.content }}

## Usage

See helper functions above.
"#,
        )
        .await?;

    test_repo.commit_all("Add agent and snippet")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest with both dependencies
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_snippet("helper", |d| {
            d.source("test-repo").path("snippets/helper.md").version("v1.0.0").tool("agpm")
        })
        .add_agent("main-agent", |d| {
            d.source("test-repo").path("agents/main-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install both resources
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. stderr: {}", output.stderr);

    // Read the installed agent file
    let agent_path = project.project_path().join(".claude/agents/main-agent.md");
    let content = fs::read_to_string(&agent_path).await?;

    // Verify snippet content was embedded
    assert!(content.contains("# Helper Functions"));
    assert!(content.contains("## Function List"));
    assert!(content.contains("- sum"));
    assert!(content.contains("- multiply"));
    assert!(content.contains("- divide"));

    Ok(())
}

/// Test that templating can be disabled via frontmatter.
#[tokio::test]
async fn test_opt_out_via_frontmatter() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with templating disabled in frontmatter
    test_repo
        .add_resource(
            "agents",
            "no-template",
            r#"---
title: No Template Agent
agpm:
  templating: false
---
# Agent with Literal Syntax

This file contains literal template syntax: {{ agpm.resource.name }}

The syntax should not be processed.
"#,
        )
        .await?;

    test_repo.commit_all("Add agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("no-template", |d| {
            d.source("test-repo").path("agents/no-template.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/no-template.md");
    let content = fs::read_to_string(&installed_path).await?;

    // Verify template syntax was NOT processed
    assert!(
        content.contains("{{ agpm.resource.name }}"),
        "Template syntax should remain literal when templating is disabled"
    );

    Ok(())
}

/// Test that templating is disabled by default (template syntax preserved).
#[tokio::test]
async fn test_templating_disabled_by_default() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with template variables
    test_repo
        .add_resource(
            "agents",
            "test-agent",
            r#"---
title: Test Agent
---
# {{ agpm.resource.name }}

Install path: {{ agpm.resource.install_path }}
"#,
        )
        .await?;

    test_repo.commit_all("Add agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("test-agent", |d| {
            d.source("test-repo").path("agents/test-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install without --templating flag (templating disabled by default)
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/test-agent.md");
    let content = fs::read_to_string(&installed_path).await?;

    // Verify template syntax was NOT processed (default behavior)
    assert!(
        content.contains("# {{ agpm.resource.name }}"),
        "Template syntax should remain literal by default"
    );
    assert!(
        content.contains("{{ agpm.resource.install_path }}"),
        "All template syntax should be preserved by default"
    );

    Ok(())
}

/// Test that files without template syntax are unchanged.
#[tokio::test]
async fn test_no_template_syntax() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a file without any template syntax but with templating enabled
    test_repo
        .add_resource(
            "agents",
            "plain-agent",
            r#"---
title: Plain Agent
agpm:
  templating: true
---
# Plain Agent

This agent has no template syntax.

## Features

- Feature 1
- Feature 2
- Feature 3

## Usage

Just use it normally.
"#,
        )
        .await?;

    test_repo.commit_all("Add plain agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("plain-agent", |d| {
            d.source("test-repo").path("agents/plain-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/plain-agent.md");
    let content = fs::read_to_string(&installed_path).await?;

    // Verify content is unchanged
    assert!(content.contains("# Plain Agent"));
    assert!(content.contains("This agent has no template syntax."));
    assert!(content.contains("- Feature 1"));
    assert!(content.contains("Just use it normally."));

    Ok(())
}

/// Test conditional rendering with {% if %} blocks.
#[tokio::test]
async fn test_conditional_rendering() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    test_repo
        .add_resource(
            "agents",
            "conditional",
            r#"---
title: Conditional Agent
agpm:
  templating: true
---
# Conditional Content

{% if agpm.resource.source %}
This resource is from source: {{ agpm.resource.source }}
{% else %}
This is a local resource.
{% endif %}

{% if agpm.resource.version %}
Version: {{ agpm.resource.version }}
{% endif %}
"#,
        )
        .await?;

    test_repo.commit_all("Add agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("conditional", |d| {
            d.source("test-repo").path("agents/conditional.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    let installed_path = project.project_path().join(".claude/agents/conditional.md");
    let content = fs::read_to_string(&installed_path).await?;

    // Verify conditional blocks were processed
    assert!(
        content.contains("This resource is from source: test-repo"),
        "Conditional block should render when condition is true"
    );
    assert!(!content.contains("This is a local resource"), "Alternative block should not render");
    assert!(
        content.contains("Version: v1.0.0"),
        "Optional block should render when variable exists"
    );

    Ok(())
}

/// Test loop over dependencies with {% for %} blocks.
#[tokio::test]
async fn test_loop_over_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create multiple snippets
    test_repo
        .add_resource(
            "snippets",
            "helper1",
            r#"---
title: Helper 1
agpm:
  templating: true
---
# Helper 1

This is helper 1.
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "snippets",
            "helper2",
            r#"---
title: Helper 2
agpm:
  templating: true
---
# Helper 2

This is helper 2.
"#,
        )
        .await?;

    test_repo
        .add_resource(
            "snippets",
            "helper3",
            r#"---
title: Helper 3
agpm:
  templating: true
---
# Helper 3

This is helper 3.
"#,
        )
        .await?;

    // Create an agent that loops over snippets
    test_repo
        .add_resource(
            "agents",
            "looping-agent",
            r#"---
title: Looping Agent
dependencies:
  snippets:
    - path: snippets/helper1.md
      tool: agpm
      name: helper1
    - path: snippets/helper2.md
      tool: agpm
      name: helper2
    - path: snippets/helper3.md
      tool: agpm
      name: helper3
agpm:
  templating: true
---
# Looping Agent

## Available Helpers

{% for name, snippet in agpm.deps.snippets %}
### {{ name }}
{{ snippet.content }}
{% endfor %}

## Count

There are {{ agpm.deps.snippets | length }} helpers available.
"#,
        )
        .await?;

    test_repo.commit_all("Add resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_snippet("helper1", |d| {
            d.source("test-repo").path("snippets/helper1.md").version("v1.0.0").tool("agpm")
        })
        .add_snippet("helper2", |d| {
            d.source("test-repo").path("snippets/helper2.md").version("v1.0.0").tool("agpm")
        })
        .add_snippet("helper3", |d| {
            d.source("test-repo").path("snippets/helper3.md").version("v1.0.0").tool("agpm")
        })
        .add_agent("looping-agent", |d| {
            d.source("test-repo").path("agents/looping-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install all resources
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed");

    // Read the installed agent file
    let agent_path = project.project_path().join(".claude/agents/looping-agent.md");
    let content = fs::read_to_string(&agent_path).await?;

    assert!(content.contains("### helper1"));
    assert!(content.contains("# Helper 1"));
    assert!(content.contains("### helper2"));
    assert!(content.contains("# Helper 2"));
    assert!(content.contains("### helper3"));
    assert!(content.contains("# Helper 3"));
    assert!(content.contains("There are 3 helpers available."));

    Ok(())
}
