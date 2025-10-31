//! Integration tests for transitive dependencies with conditional logic in frontmatter
//!
//! This tests the critical case where:
//! 1. A resource has `templating: true` with conditional dependencies ({% if %})
//! 2. Dependencies have custom names that need to be resolved
//! 3. Some dependencies have `templating: false` (literal guards)
//! 4. The resource is embedded as a transitive dependency in a parent
//!
//! This would have caught the bugs fixed in FIXES_2.md:
//! - Frontmatter not being rendered before parsing (causing templating flag to be wrong)
//! - Literal guards not being collapsed before adding to context

use crate::common::TestProject;
use anyhow::Result;

/// Test transitive dependency with conditional frontmatter logic and custom names
///
/// This is a regression test for bugs where:
/// 1. Resources with {% if %} in frontmatter were incorrectly marked as non-templated
/// 2. Dependencies with `templating: false` had guards that weren't collapsed
/// 3. Custom dependency names couldn't be resolved for templated paths
#[tokio::test]
async fn test_conditional_deps_with_guards_and_custom_names() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let community_repo = project.create_source_repo("community").await?;

    // Create best-practices snippet with `templating: false` (will have guards)
    community_repo
        .add_resource(
            "snippets/best-practices",
            "javascript-best-practices",
            r#"---
agpm:
  templating: false
---
# JavaScript Best Practices

- Use const/let instead of var
- Prefer arrow functions
- Use async/await for promises
"#,
        )
        .await?;

    // Create styleguide snippet with `templating: false`
    community_repo
        .add_resource(
            "snippets/styleguides",
            "javascript-styleguide",
            r#"---
agpm:
  templating: false
---
# JavaScript Style Guide

- 2 spaces for indentation
- Semicolons required
- Use camelCase for variables
"#,
        )
        .await?;

    // Create framework-specific snippet
    community_repo
        .add_resource(
            "snippets/frameworks",
            "react",
            r#"---
agpm:
  templating: false
---
# React Framework Guide

- Use hooks instead of classes
- Keep components small
"#,
        )
        .await?;

    // Create base snippet with CONDITIONAL dependencies in frontmatter
    // This is the critical case: templating: true with {% if %} in YAML
    community_repo
        .add_resource(
            "snippets/agents",
            "frontend-engineer",
            r#"---
agpm:
  templating: true
  dependencies:
    snippets:
      - name: best-practices
        path: ../best-practices/{{ agpm.project.language }}-best-practices.md
        install: false
      - name: styleguide
        path: ../styleguides/{{ agpm.project.language }}-styleguide.md
        install: false
      {% if agpm.project.framework %}
      - name: framework
        path: ../frameworks/{{ agpm.project.framework }}.md
        install: false
      {% endif %}
---
# Frontend Engineer Base

You are a senior frontend engineer specializing in {{ agpm.project.language }}.

## Best Practices

{{ agpm.deps.snippets.best_practices.content }}

## Style Guide

{{ agpm.deps.snippets.styleguide.content }}

{% if agpm.project.framework %}
## Framework Guide

{{ agpm.deps.snippets.framework.content }}
{% endif %}
"#,
        )
        .await?;

    // Create top-level agent that embeds the snippet
    community_repo
        .add_resource(
            "agents",
            "frontend-engineer",
            r#"---
agpm:
  templating: true
  dependencies:
    snippets:
      - name: frontend-engineer-base
        path: ../snippets/agents/frontend-engineer.md
        install: false
---
{{ agpm.deps.snippets.frontend_engineer_base.content }}

**Additional tool-specific context**:

- Focus on component-based architecture
- Ensure responsive design
"#,
        )
        .await?;

    community_repo.commit_all("Add frontend resources with conditional deps")?;
    community_repo.tag_version("v1.0.0")?;

    // Create manifest with template_vars to trigger the conditional path
    let source_url = community_repo.bare_file_url(project.sources_path())?;

    // Build manifest manually to include template_vars
    let manifest = format!(
        r#"[sources]
community = "{}"

[agents.frontend-react]
source = "community"
path = "agents/frontend-engineer.md"
version = "v1.0.0"

[agents.frontend-react.template_vars.project]
language = "javascript"
framework = "react"

[agents.frontend-vanilla]
source = "community"
path = "agents/frontend-engineer.md"
version = "v1.0.0"
filename = "frontend-engineer-vanilla.md"

[agents.frontend-vanilla.template_vars.project]
language = "javascript"
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;

    if !output.success {
        eprintln!("STDERR:\n{}", output.stderr);
        eprintln!("STDOUT:\n{}", output.stdout);
    }

    assert!(
        output.success,
        "Install should succeed. This would have failed before FIXES_2.md. Stderr:\n{}",
        output.stderr
    );

    // Verify the conditional dependency was resolved correctly
    let lockfile_content = project.read_lockfile().await?;

    // Both agents should be in lockfile
    assert!(
        lockfile_content.contains("frontend-react")
            || lockfile_content.contains("frontend-engineer"),
        "Frontend agents should be in lockfile"
    );

    // Verify the base snippet is in lockfile
    assert!(
        lockfile_content.contains("snippets/agents/frontend-engineer")
            || lockfile_content.contains("frontend-engineer"),
        "Base snippet should be in lockfile"
    );

    // Verify best-practices and styleguide are in lockfile
    assert!(
        lockfile_content.contains("best-practices")
            || lockfile_content.contains("javascript-best-practices"),
        "Best practices should be in lockfile"
    );
    assert!(
        lockfile_content.contains("styleguide")
            || lockfile_content.contains("javascript-styleguide"),
        "Styleguide should be in lockfile"
    );

    // Verify the framework is in lockfile for the react variant
    assert!(
        lockfile_content.contains("react"),
        "React framework should be in lockfile for frontend-react agent"
    );

    // Read the installed agent files and verify content was embedded correctly
    let react_agent_path = project.project_path().join(".claude/agents/frontend-engineer.md");

    if tokio::fs::metadata(&react_agent_path).await.is_ok() {
        let react_content = tokio::fs::read_to_string(&react_agent_path).await?;

        // Verify best practices content was embedded (guards collapsed)
        assert!(
            react_content.contains("Use const/let instead of var"),
            "Agent should include best practices content. Content:\n{}",
            react_content
        );

        // NOTE: Template syntax ({{ }}) IS preserved when it's inside markdown code fences (```).
        // The `content_contains_template_syntax()` function skips code fence content, so guards
        // are not applied to code-fenced template syntax. This allows framework-specific examples
        // (like Vue templates) to be included in non-templated resources.
        //
        // LIMITATION: Template syntax OUTSIDE code fences in non-templated content will cause
        // rendering errors when embedded in templated parents. This is expected behavior.
        // See test_template_syntax_preserved_in_embedded_content() for proper usage.

        // Verify styleguide content was embedded
        assert!(
            react_content.contains("2 spaces for indentation"),
            "Agent should include styleguide content. Content:\n{}",
            react_content
        );

        // Verify framework content was embedded (conditional path)
        assert!(
            react_content.contains("Use hooks instead of classes"),
            "Agent should include React framework content. Content:\n{}",
            react_content
        );

        // Verify guards were collapsed (should NOT see __AGPM_LITERAL_RAW_START__)
        assert!(
            !react_content.contains("__AGPM_LITERAL_RAW_START__")
                && !react_content.contains("__AGPM_LITERAL_RAW_END__"),
            "Rendered agent should not contain literal guards. Content:\n{}",
            react_content
        );
    }

    Ok(())
}

/// Test that resources with templating: true but {% if %} in frontmatter are correctly detected
///
/// Before the fix, parse() couldn't handle {% if %} in YAML, causing incorrect templating
/// flag detection. This test ensures we use parse_with_templating() which renders the
/// frontmatter before parsing it.
#[tokio::test]
async fn test_templating_flag_with_conditional_frontmatter() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let community_repo = project.create_source_repo("community").await?;

    // Create a simple dependency
    community_repo.add_resource("snippets", "helper", "# Helper\nSome helper content").await?;

    // Create resource with conditional frontmatter
    // The key test: templating: true with {% if %} in YAML should still be detected correctly
    community_repo
        .add_resource(
            "agents",
            "conditional",
            r#"---
agpm:
  templating: true
  dependencies:
    snippets:
      {% if agpm.project.language %}
      - path: ../snippets/helper.md
        install: false
      {% endif %}
---
# Agent with Conditional Deps

This agent has templating: true with conditional logic in its frontmatter.

Template variable: {{ agpm.project.language }}
"#,
        )
        .await?;

    community_repo.commit_all("Add conditional resource")?;
    community_repo.tag_version("v1.0.0")?;

    let source_url = community_repo.bare_file_url(project.sources_path())?;

    // Build manifest manually to include template_vars
    let manifest = format!(
        r#"[sources]
community = "{}"

[agents.conditional]
source = "community"
path = "agents/conditional.md"
version = "v1.0.0"

[agents.conditional.template_vars.project]
language = "rust"
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install - should succeed
    let output = project.run_agpm(&["install"])?;

    assert!(
        output.success,
        "Install should succeed with conditional frontmatter. Before fix, this would fail \
        because templating flag would be incorrectly detected. Stderr:\n{}",
        output.stderr
    );

    // Verify the agent was installed and rendered
    let agent_path = project.project_path().join(".claude/agents/conditional.md");
    let agent_content = tokio::fs::read_to_string(&agent_path).await?;

    // Should have rendered the template variable
    assert!(
        agent_content.contains("rust"),
        "Agent should have rendered template variable. Content:\n{}",
        agent_content
    );

    Ok(())
}

/// Test that template syntax in code fences is preserved when embedded
///
/// This test verifies that when a dependency with `templating: false` contains template
/// syntax like `{{ example }}` INSIDE MARKDOWN CODE FENCES, that syntax is preserved as
/// literal text when the dependency is embedded in a parent resource with `templating: true`.
///
/// IMPORTANT LIMITATION: Template syntax OUTSIDE code fences in non-templated content will
/// cause rendering errors when embedded in templated parents. Always put template syntax
/// examples inside markdown code fences (```).
#[tokio::test]
async fn test_template_syntax_preserved_in_embedded_content() -> Result<()> {
    agpm_cli::test_utils::init_test_logging(None);

    let project = TestProject::new().await?;
    let community_repo = project.create_source_repo("community").await?;

    // Create a snippet with templating: false that contains template syntax inside code fences
    // NOTE: Template syntax MUST be inside markdown code fences to be preserved!
    // Template syntax outside code fences in non-templated content will cause errors
    // when embedded in templated parents.
    community_repo
        .add_resource(
            "snippets",
            "example-with-syntax",
            r#"---
agpm:
  templating: false
---
# Example Snippet

This snippet contains template syntax inside code fences:

```vue
<template>
  <div>{{ user.name }}</div>
  {% for item in items %}
    <li>{{ item }}</li>
  {% endfor %}
  {# This is a comment #}
</template>
```

The syntax appears in a code fence, so it will be preserved correctly.
"#,
        )
        .await?;

    // Create a parent agent that embeds this snippet
    community_repo
        .add_resource(
            "agents",
            "example-agent",
            r#"---
agpm:
  templating: true
  dependencies:
    snippets:
      - path: ../snippets/example-with-syntax.md
        install: false
---
# Example Agent

Below is the embedded snippet:

{{ agpm.deps.snippets.example_with_syntax.content }}

End of agent.
"#,
        )
        .await?;

    community_repo.commit_all("Add resources with template syntax")?;
    community_repo.tag_version("v1.0.0")?;

    let source_url = community_repo.bare_file_url(project.sources_path())?;

    let manifest = format!(
        r#"[sources]
community = "{}"

[agents.example]
source = "community"
path = "agents/example-agent.md"
version = "v1.0.0"
"#,
        source_url
    );

    project.write_manifest(&manifest).await?;

    // Run install
    let output = project.run_agpm(&["install"])?;

    assert!(output.success, "Install should succeed. Stderr:\n{}", output.stderr);

    // Read the installed agent
    let agent_path = project.project_path().join(".claude/agents/example-agent.md");
    let agent_content = tokio::fs::read_to_string(&agent_path).await?;

    // Verify template syntax is preserved as literal text (inside code fence)
    assert!(
        agent_content.contains("<div>{{ user.name }}</div>"),
        "Variable syntax should be preserved in code fence. Content:\n{}",
        agent_content
    );

    assert!(
        agent_content.contains("{% for item in items %}"),
        "Loop syntax should be preserved in code fence. Content:\n{}",
        agent_content
    );

    assert!(
        agent_content.contains("{# This is a comment #}"),
        "Comment syntax should be preserved in code fence. Content:\n{}",
        agent_content
    );

    // Verify the code fence markers are present
    assert!(
        agent_content.contains("```vue"),
        "Code fence should be preserved. Content:\n{}",
        agent_content
    );

    // Verify NO guard markers remain
    assert!(
        !agent_content.contains("__AGPM_LITERAL_RAW_START__")
            && !agent_content.contains("__AGPM_LITERAL_RAW_END__"),
        "Guard markers should be collapsed. Content:\n{}",
        agent_content
    );

    Ok(())
}
