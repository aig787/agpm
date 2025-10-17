//! Integration tests for markdown templating functionality.
//!
//! These tests verify that:
//! - Template syntax is correctly rendered in markdown files
//! - Resources can reference each other via templates
//! - Templating can be disabled globally or per-resource
//! - Invalid templates are handled gracefully
//! - Template validation works with `validate --render`

use anyhow::{Context, Result};
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

    // Verify variables were substituted
    assert!(content.contains("# test-agent"), "Resource name should be substituted");

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

/// Test that resources can reference dependencies via templates.
#[tokio::test]
async fn test_dependency_references() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create a helper snippet
    test_repo
        .add_resource(
            "snippets",
            "helper",
            r#"---
title: Helper Snippet
---
# Helper Functions
This is a helper snippet.
"#,
        )
        .await?;

    // Create an agent that references the snippet
    test_repo
        .add_resource(
            "agents",
            "main-agent",
            r#"---
title: Main Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent uses the helper snippet located at:
`{{ agpm.deps.snippets.helper_snippet.install_path }}`

{% if agpm.deps.snippets.helper_snippet %}
Helper is available with version: {{ agpm.deps.snippets.helper_snippet.version }}
{% endif %}
"#,
        )
        .await?;

    test_repo.commit_all("Add resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest with both resources using correct tools
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_snippet("helper-snippet", |d| {
            d.source("test-repo").path("snippets/helper.md").version("v1.0.0").tool("agpm") // snippets use agpm tool
        })
        .add_agent("main-agent", |d| {
            d.source("test-repo").path("agents/main-agent.md").version("v1.0.0")
            // Remove explicit tool to see if that's the issue
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install - templating enabled via frontmatter
    let output = project.run_agpm(&["install"])?;
    println!("=== INSTALL OUTPUT ===");
    println!("stdout: {}", output.stdout);
    println!("stderr: {}", output.stderr);
    println!("success: {}", output.success);
    println!("=== END INSTALL OUTPUT ===");
    if !output.success {
        println!("Install failed!");
    }
    assert!(output.success);

    // Read the installed agent
    let agent_path = project.project_path().join(".claude/agents/main-agent.md");
    let content = fs::read_to_string(&agent_path).await?;

    // Debug: Print actual content and paths
    println!("=== INSTALLED AGENT CONTENT ===");
    println!("{}", content);
    println!("=== END CONTENT ===");

    // Debug: Check if file exists and print paths
    println!("=== DEBUG INFO ===");
    println!("Agent path: {:?}", agent_path);
    println!("Agent path exists: {}", agent_path.exists());
    println!("Project path: {:?}", project.project_path());

    // List files in .claude directory
    let claude_dir = project.project_path().join(".claude");
    if claude_dir.exists() {
        println!("Files in .claude:");
        for entry in std::fs::read_dir(&claude_dir).unwrap() {
            let entry = entry.unwrap();
            println!("  {:?}", entry.path());
        }

        // List files in .claude/agents
        let agents_dir = claude_dir.join("agents");
        if agents_dir.exists() {
            println!("Files in .claude/agents:");
            for entry in std::fs::read_dir(&agents_dir).unwrap() {
                let entry = entry.unwrap();
                println!("  {:?}", entry.path());
            }
        }

        // List files in .claude/snippets
        let snippets_dir = claude_dir.join("snippets");
        if snippets_dir.exists() {
            println!("Files in .claude/snippets:");
            for entry in std::fs::read_dir(&snippets_dir).unwrap() {
                let entry = entry.unwrap();
                println!("  {:?}", entry.path());
            }
        }

        // Check for .agpm directory (snippets with agpm tool)
        let agpm_dir = project.project_path().join(".agpm");
        if agpm_dir.exists() {
            println!("Files in .agpm:");
            for entry in std::fs::read_dir(&agpm_dir).unwrap() {
                let entry = entry.unwrap();
                println!("  {:?}", entry.path());
            }

            let agpm_snippets_dir = agpm_dir.join("snippets");
            if agpm_snippets_dir.exists() {
                println!("Files in .agpm/snippets:");
                for entry in std::fs::read_dir(&agpm_snippets_dir).unwrap() {
                    let entry = entry.unwrap();
                    println!("  {:?}", entry.path());
                }
            }
        }
    }
    println!("=== END DEBUG INFO ===");

    // Verify dependency reference was substituted
    assert!(content.contains("# main-agent"), "Resource name should be substituted");

    // Check for platform-native path separators
    #[cfg(windows)]
    let expected_snippet_path = ".agpm\\snippets\\helper.md";
    #[cfg(not(windows))]
    let expected_snippet_path = ".agpm/snippets/helper.md";

    assert!(
        content.contains(expected_snippet_path),
        "Dependency install path should be substituted with platform-native separators. Content:\n{}",
        content
    );
    assert!(
        content.contains("Helper is available with version: v1.0.0"),
        "Dependency version should be accessible"
    );

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

    let original_content = r#"---
title: Plain Agent
---
# Plain Agent

This is a plain markdown file without any template syntax.
It should be installed exactly as-is.
"#;

    test_repo.add_resource("agents", "plain-agent", original_content).await?;

    test_repo.commit_all("Add agent")?;
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
    assert!(output.success);

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/plain-agent.md");
    let content = fs::read_to_string(&installed_path).await?;

    // Verify content is identical (normalize line endings for cross-platform compatibility)
    let normalized_content = content.replace("\r\n", "\n");
    let normalized_original = original_content.replace("\r\n", "\n");
    assert_eq!(
        normalized_content, normalized_original,
        "Plain files should be unchanged (modulo line endings)"
    );

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

    // Verify template syntax is gone
    assert!(!content.contains("{% if"), "Control flow syntax should be removed");

    Ok(())
}

/// Test template with loops over dependencies.
#[tokio::test]
async fn test_loop_over_dependencies() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create multiple snippets
    test_repo.add_resource("snippets", "helper1", "# Helper 1").await?;
    test_repo.add_resource("snippets", "helper2", "# Helper 2").await?;

    // Create an agent that loops over snippets
    test_repo
        .add_resource(
            "agents",
            "main",
            r#"---
title: Main Agent
agpm:
  templating: true
---
# Available Snippets

{% for name, snippet in agpm.deps.snippets %}
- {{ name }}: {{ snippet.install_path }}
{% endfor %}
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
        .add_agent("main", |d| d.source("test-repo").path("agents/main.md").version("v1.0.0"))
        .build();

    project.write_manifest(&manifest).await?;

    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    let agent_path = project.project_path().join(".claude/agents/main.md");
    let content = fs::read_to_string(&agent_path).await?;

    // Verify loop was processed and snippets are listed
    // Check for platform-native path separators
    #[cfg(windows)]
    {
        assert!(
            content.contains("- helper1: .agpm\\snippets\\helper1.md"),
            "First snippet should be listed with Windows-style path. Content:\n{}",
            content
        );
        assert!(
            content.contains("- helper2: .agpm\\snippets\\helper2.md"),
            "Second snippet should be listed with Windows-style path. Content:\n{}",
            content
        );
    }
    #[cfg(not(windows))]
    {
        assert!(
            content.contains("- helper1: .agpm/snippets/helper1.md"),
            "First snippet should be listed with Unix-style path. Content:\n{}",
            content
        );
        assert!(
            content.contains("- helper2: .agpm/snippets/helper2.md"),
            "Second snippet should be listed with Unix-style path. Content:\n{}",
            content
        );
    }
    assert!(!content.contains("{% for"), "Loop syntax should be removed");

    Ok(())
}

/// Test validate --render with valid templates.
#[tokio::test]
async fn test_validate_render_valid_templates() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with valid template syntax
    test_repo
        .add_resource(
            "agents",
            "test-agent",
            r#"---
title: Test Agent
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

    // Install first to create lockfile
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Now validate with --render flag
    let output = project.run_agpm(&["validate", "--render"])?;
    if !output.success {
        eprintln!("STDERR: {}", output.stderr);
        eprintln!("STDOUT: {}", output.stdout);
    }
    assert!(output.success, "Validation should succeed for valid templates");

    let stdout = &output.stdout;
    assert!(
        stdout.contains("templates rendered successfully"),
        "Should report successful rendering"
    );

    Ok(())
}

/// Test validate --render with invalid template syntax.
#[tokio::test]
async fn test_validate_render_invalid_syntax() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with invalid template syntax (unclosed variable)
    test_repo
        .add_resource(
            "agents",
            "broken-agent",
            r#"---
title: Broken Agent
---
# {{ agpm.resource.name

This template has a syntax error.
"#,
        )
        .await?;

    test_repo.commit_all("Add broken agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("broken-agent", |d| {
            d.source("test-repo").path("agents/broken-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install without --templating to create lockfile despite template errors
    let install_output = project.run_agpm(&["install"])?;
    assert!(install_output.success, "Install without templating should succeed");

    // Now validate with --render flag - should fail due to template syntax errors
    let output = project.run_agpm(&["validate", "--render"])?;
    assert!(!output.success, "Validation should fail for invalid templates");

    let stderr = &output.stderr;
    assert!(
        stderr.contains("Template rendering failed") || stderr.contains("rendering failed"),
        "Should report template rendering failure. Actual stderr: {}",
        stderr
    );

    Ok(())
}

/// Test validate --render with missing variable.
#[tokio::test]
async fn test_validate_render_missing_variable() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent that references a non-existent variable
    test_repo
        .add_resource(
            "agents",
            "missing-var-agent",
            r#"---
title: Missing Variable Agent
---
# {{ agpm.resource.name }}

This uses a non-existent variable: {{ agpm.nonexistent.field }}
"#,
        )
        .await?;

    test_repo.commit_all("Add agent with missing variable")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("missing-var-agent", |d| {
            d.source("test-repo").path("agents/missing-var-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install without --templating to create lockfile
    let install_output = project.run_agpm(&["install"])?;
    assert!(install_output.success, "Install without templating should succeed");

    // Validate with --render should fail due to missing variable
    let output = project.run_agpm(&["validate", "--render"])?;
    assert!(!output.success, "Validation should fail for missing variables");

    Ok(())
}

/// Test validate --render without lockfile.
#[tokio::test]
async fn test_validate_render_no_lockfile() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    test_repo.add_resource("agents", "test-agent", "# Test Agent\n").await?;

    test_repo.commit_all("Add test agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest but don't install
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("test-agent", |d| {
            d.source("test-repo").path("agents/test-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Try to validate with --render without lockfile
    let output = project.run_agpm(&["validate", "--render"])?;
    assert!(!output.success, "Should fail without lockfile");

    let stderr = &output.stderr;
    assert!(
        stderr.contains("Lockfile required") || stderr.contains("lockfile not found"),
        "Should report missing lockfile. Actual stderr: {}",
        stderr
    );

    Ok(())
}

/// Test validate --render with JSON output format.
#[tokio::test]
async fn test_validate_render_json_output() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with template syntax
    test_repo
        .add_resource(
            "agents",
            "test-agent",
            r#"# {{ agpm.resource.name }}
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

    // Install first
    let output = project.run_agpm(&["install"])?;
    assert!(output.success);

    // Validate with --render and --format json
    let output = project.run_agpm(&["validate", "--render", "--format", "json"])?;
    if !output.success {
        eprintln!("STDERR: {}", output.stderr);
        eprintln!("STDOUT: {}", output.stdout);
    }
    assert!(output.success);

    let stdout = &output.stdout;
    eprintln!("JSON OUTPUT: {}", stdout);

    // Parse JSON output
    let json: serde_json::Value = serde_json::from_str(stdout)
        .context(format!("Failed to parse JSON. Output was: {}", stdout))?;

    assert_eq!(json["valid"], true, "Should be valid");
    assert_eq!(json["templates_valid"], true, "Templates should be valid");
    assert!(json["templates_rendered"].is_number(), "Should have templates_rendered count");
    assert!(json["templates_total"].is_number(), "Should have templates_total count");

    Ok(())
}

/// Test that --frozen mode detects lockfile corruption via duplicate entries.
#[tokio::test]
async fn test_templating_checksum_enforced() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent with template variables
    let template_content = r#"---
title: Test Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent is installed at: {{ agpm.resource.install_path }}
Version: {{ agpm.resource.version }}
Content: Template content with checksum verification
"#;

    test_repo.add_resource("agents", "test-agent", template_content).await?;

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

    // Install to generate lockfile and rendered content
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Initial install should succeed");

    // Read the installed file and verify it was rendered correctly
    let installed_path = project.project_path().join(".claude/agents/test-agent.md");
    let content = fs::read_to_string(&installed_path).await?;
    assert!(content.contains("Content: Template content with checksum verification"));
    assert!(content.contains("# test-agent"));

    // Read the lockfile and manually corrupt it by adding a duplicate entry
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_content = fs::read_to_string(&lockfile_path).await?;

    // Find the agents section and duplicate the first agent entry to simulate corruption
    if let Some(agents_pos) = lockfile_content.find("[[agents]]") {
        let agent_section = &lockfile_content[agents_pos..];

        // Find the end of this agent entry (next [[agents]] or end of file)
        let next_section_start = agent_section.find("\n[[agents]]").unwrap_or(agent_section.len());

        let current_agent_entry = &agent_section[..next_section_start];

        // Create corrupted lockfile with duplicate entry
        let corrupted_lockfile = format!(
            "{}{}{}",
            &lockfile_content[..agents_pos],
            current_agent_entry,
            &lockfile_content[agents_pos..]
        );

        fs::write(&lockfile_path, corrupted_lockfile).await?;
    }

    // Run agpm install --frozen - this should detect the duplicate entry (corruption) and fail
    let output = project.run_agpm(&["install", "--frozen"])?;

    // The install should fail because the lockfile has duplicate entries (corruption)
    assert!(!output.success, "Frozen install should fail with lockfile corruption detected");

    let stderr = &output.stderr;
    assert!(
        stderr.contains("Lockfile has critical issues")
            || stderr.contains("corruption")
            || stderr.contains("duplicate")
            || stderr.contains("Duplicate"),
        "Should report lockfile corruption. Actual stderr: {}",
        stderr
    );

    // Now fix the lockfile by restoring original content and run normal install - should succeed
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Normal install should succeed and fix lockfile");

    // Verify the content is still correct
    let fixed_content = fs::read_to_string(&installed_path).await?;
    assert!(fixed_content.contains("Content: Template content with checksum verification"));

    Ok(())
}

/// Test that template paths are rendered with platform-native separators.
/// On Windows, paths should use backslashes; on Unix, forward slashes.
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

You can find it at the following location:
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

    test_repo.add_resource("snippets/utils", "helper", snippet_content).await?;

    test_repo.commit_all("Add test resources")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("path-test", |d| {
            d.source("test-repo").path("agents/path-test.md").version("v1.0.0")
        })
        .add_snippet("nested-helper", |d| {
            d.source("test-repo").path("snippets/utils/helper.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install resources
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Install should succeed. stderr: {}", output.stderr);

    // Read the installed agent file
    let agent_path = project.project_path().join(".claude/agents/path-test.md");
    let agent_content = fs::read_to_string(&agent_path).await?;

    // Read the installed snippet file
    let snippet_path = project.project_path().join(".agpm/snippets/utils/helper.md");
    let snippet_content = fs::read_to_string(&snippet_path).await?;

    // Verify paths use platform-native separators
    #[cfg(windows)]
    {
        // On Windows, rendered paths should use backslashes
        assert!(
            agent_content.contains(".claude\\agents\\path-test.md"),
            "Agent content should contain Windows-style path with backslashes. Content:\n{}",
            agent_content
        );
        assert!(
            snippet_content.contains(".agpm\\snippets\\utils\\helper.md"),
            "Snippet content should contain Windows-style path with backslashes. Content:\n{}",
            snippet_content
        );

        // Verify no Unix-style paths leaked through
        assert!(
            !agent_content.contains(".claude/agents/path-test.md"),
            "Agent content should NOT contain Unix-style paths on Windows"
        );
    }

    #[cfg(not(windows))]
    {
        // On Unix, rendered paths should use forward slashes
        assert!(
            agent_content.contains(".claude/agents/path-test.md"),
            "Agent content should contain Unix-style path with forward slashes. Content:\n{}",
            agent_content
        );
        assert!(
            snippet_content.contains(".agpm/snippets/utils/helper.md"),
            "Snippet content should contain Unix-style path with forward slashes. Content:\n{}",
            snippet_content
        );
    }

    // Verify lockfile still uses Unix-style paths (cross-platform consistency)
    let lockfile_path = project.project_path().join("agpm.lock");
    let lockfile_content = fs::read_to_string(&lockfile_path).await?;

    // Lockfile should ALWAYS use forward slashes on all platforms
    assert!(
        lockfile_content.contains("installed_at = \".claude/agents/path-test.md\""),
        "Lockfile should use Unix-style paths on all platforms. Lockfile:\n{}",
        lockfile_content
    );
    assert!(
        lockfile_content.contains("installed_at = \".agpm/snippets/utils/helper.md\""),
        "Lockfile should use Unix-style paths for snippets. Lockfile:\n{}",
        lockfile_content
    );

    // On Windows, verify no backslashes in lockfile
    #[cfg(windows)]
    {
        assert!(
            !lockfile_content.contains("installed_at = \".claude\\"),
            "Lockfile should NOT contain Windows-style paths. Lockfile:\n{}",
            lockfile_content
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

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

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

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/project-agent.md");
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

/// Test that templates work without project variables (backward compatibility).
#[tokio::test]
async fn test_templates_without_project_variables() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let test_repo = project.create_source_repo("test-repo").await?;

    // Create an agent without project variables
    test_repo
        .add_resource(
            "agents",
            "simple-agent",
            r#"---
title: Simple Agent
agpm:
  templating: true
---
# {{ agpm.resource.name }}

This agent is simple and doesn't use project variables.
"#,
        )
        .await?;

    test_repo.commit_all("Add simple agent")?;
    test_repo.tag_version("v1.0.0")?;

    let repo_url = test_repo.bare_file_url(project.sources_path())?;

    // Create manifest WITHOUT project section
    let manifest = ManifestBuilder::new()
        .add_source("test-repo", &repo_url)
        .add_agent("simple-agent", |d| {
            d.source("test-repo").path("agents/simple-agent.md").version("v1.0.0")
        })
        .build();

    project.write_manifest(&manifest).await?;

    // Install should work without project variables
    let output = project.run_agpm(&["install"])?;
    assert!(output.success, "Installation should succeed without project variables");

    // Read the installed file
    let installed_path = project.project_path().join(".claude/agents/simple-agent.md");
    let content =
        fs::read_to_string(&installed_path).await.context("Failed to read installed agent file")?;

    // Verify resource variables still work
    assert!(
        content.contains("# simple-agent"),
        "Resource name should be substituted. Content:\n{}",
        content
    );

    Ok(())
}
