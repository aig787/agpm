//! Tests for the template renderer functionality.

use crate::templating::renderer::TemplateRenderer;
use anyhow::Result;
use std::collections::HashMap;
use tera::Context as TeraContext;

#[test]
fn test_template_renderer() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let mut renderer = TemplateRenderer::new(true, project_dir, None)?;

    // Test rendering without template syntax
    let result = renderer.render_template("# Plain Markdown", &TeraContext::new(), None)?;
    assert_eq!(result, "# Plain Markdown");

    // Test rendering with template syntax
    let mut context = TeraContext::new();
    context.insert("test_var", "test_value");

    let result = renderer.render_template("# {{ test_var }}", &context, None)?;
    assert_eq!(result, "# test_value");

    Ok(())
}

#[test]
fn test_template_renderer_disabled() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let mut renderer = TemplateRenderer::new(false, project_dir, None)?;

    let mut context = TeraContext::new();
    context.insert("test_var", "test_value");

    // Should return content as-is when disabled
    let result = renderer.render_template("# {{ test_var }}", &context, None)?;
    assert_eq!(result, "# {{ test_var }}");

    Ok(())
}

#[test]
fn test_template_error_formatting() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let mut renderer = TemplateRenderer::new(true, project_dir, None)?;
    let context = TeraContext::new();

    // Test with missing variable - should produce detailed error
    let result = renderer.render_template("# {{ missing_var }}", &context, None);
    assert!(result.is_err());

    let error = result.unwrap_err();
    let error_msg = format!("{}", error);

    // Error should NOT contain "__tera_one_off"
    assert!(
        !error_msg.contains("__tera_one_off"),
        "Error should not expose internal Tera template names"
    );

    // Error should contain useful information about what went wrong
    assert!(
        error_msg.contains("Variable") && error_msg.contains("not found"),
        "Error should indicate missing variable. Got: {}",
        error_msg
    );

    Ok(())
}

#[test]
fn test_to_native_path_display() {
    // Test Unix-style path conversion
    let unix_path = ".claude/agents/test.md";
    let native_path = crate::templating::utils::to_native_path_display(unix_path);

    #[cfg(windows)]
    {
        assert_eq!(native_path, ".claude\\agents\\test.md");
    }

    #[cfg(not(windows))]
    {
        assert_eq!(native_path, ".claude/agents/test.md");
    }
}

#[test]
fn test_to_native_path_display_nested() {
    // Test deeply nested path
    let unix_path = ".claude/agents/ai/helpers/test.md";
    let native_path = crate::templating::utils::to_native_path_display(unix_path);

    #[cfg(windows)]
    {
        assert_eq!(native_path, ".claude\\agents\\ai\\helpers\\test.md");
    }

    #[cfg(not(windows))]
    {
        assert_eq!(native_path, ".claude/agents/ai/helpers/test.md");
    }
}

// Tests for literal block functionality (Phase 1)

#[test]
fn test_protect_literal_blocks_basic() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let renderer = TemplateRenderer::new(true, project_dir, None)?;

    let content = r#"# Documentation

Use this syntax:

```literal
{{ agpm.deps.snippets.example.content }}
```

That's how you embed content."#;

    let (protected, placeholders) = renderer.protect_literal_blocks(content);

    // Should have one placeholder
    assert_eq!(placeholders.len(), 1);

    // Protected content should contain placeholder
    assert!(protected.contains("__AGPM_LITERAL_BLOCK_0__"));

    // Protected content should NOT contain the template syntax
    assert!(!protected.contains("{{ agpm.deps.snippets.example.content }}"));

    // Placeholder should contain the original content
    let placeholder_content = placeholders
        .get("__AGPM_LITERAL_BLOCK_0__")
        .ok_or_else(|| anyhow::anyhow!("Placeholder __AGPM_LITERAL_BLOCK_0__ not found"))?;
    assert!(placeholder_content.contains("{{ agpm.deps.snippets.example.content }}"));

    Ok(())
}

#[test]
fn test_protect_literal_blocks_multiple() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let renderer = TemplateRenderer::new(true, project_dir, None)?;

    let content = r#"# First Example

```literal
{{ first.example }}
```

# Second Example

```literal
{{ second.example }}
```"#;

    let (protected, placeholders) = renderer.protect_literal_blocks(content);

    // Should have two placeholders
    assert_eq!(placeholders.len(), 2);

    // Both placeholders should be in the protected content
    assert!(protected.contains("__AGPM_LITERAL_BLOCK_0__"));
    assert!(protected.contains("__AGPM_LITERAL_BLOCK_1__"));

    // Original template syntax should not be in protected content
    assert!(!protected.contains("{{ first.example }}"));
    assert!(!protected.contains("{{ second.example }}"));

    Ok(())
}

#[test]
fn test_restore_literal_blocks() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let renderer = TemplateRenderer::new(true, project_dir, None)?;

    let mut placeholders = HashMap::new();
    placeholders.insert(
        "__AGPM_LITERAL_BLOCK_0__".to_string(),
        "{{ agpm.deps.snippets.example.content }}".to_string(),
    );

    let content = "# Example\n\n__AGPM_LITERAL_BLOCK_0__\n\nDone.";
    let restored = renderer.restore_literal_blocks(content, placeholders);

    // Should contain the original content in a code fence
    assert!(restored.contains("```\n{{ agpm.deps.snippets.example.content }}\n```"));

    // Should NOT contain the placeholder
    assert!(!restored.contains("__AGPM_LITERAL_BLOCK_0__"));

    Ok(())
}

#[test]
fn test_literal_blocks_integration_with_rendering() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let mut renderer = TemplateRenderer::new(true, project_dir, None)?;

    let template = r#"# Agent: {{ agent_name }}

## Documentation

Here's how to use template syntax:

```literal
{{ agpm.deps.snippets.helper.content }}
```

The agent name is: {{ agent_name }}"#;

    let mut context = TeraContext::new();
    context.insert("agent_name", "test-agent");

    let result = renderer.render_template(template, &context, None)?;

    // The agent_name variable should be rendered
    assert!(result.contains("# Agent: test-agent"));
    assert!(result.contains("The agent name is: test-agent"));

    // The literal block should be preserved and wrapped in code fence
    assert!(result.contains("```\n{{ agpm.deps.snippets.helper.content }}\n```"));

    // The literal block should NOT be rendered (still has template syntax)
    assert!(result.contains("{{ agpm.deps.snippets.helper.content }}"));

    Ok(())
}

#[test]
fn test_literal_blocks_with_complex_template_syntax() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let mut renderer = TemplateRenderer::new(true, project_dir, None)?;

    let template = r#"# Documentation

```literal
{% for item in agpm.deps.agents %}
{{ item.name }}: {{ item.version }}
{% endfor %}
```"#;

    let context = TeraContext::new();
    let result = renderer.render_template(template, &context, None)?;

    // Should preserve the for loop syntax
    assert!(result.contains("{% for item in agpm.deps.agents %}"));
    assert!(result.contains("{{ item.name }}"));
    assert!(result.contains("{% endfor %}"));

    Ok(())
}

#[test]
fn test_literal_blocks_empty() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let mut renderer = TemplateRenderer::new(true, project_dir, None)?;

    let template = r#"# Example

```literal
```

Done."#;

    let context = TeraContext::new();
    let result = renderer.render_template(template, &context, None)?;

    // Should handle empty literal blocks gracefully
    assert!(result.contains("# Example"));
    assert!(result.contains("Done."));

    Ok(())
}

#[test]
fn test_literal_blocks_unclosed() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let renderer = TemplateRenderer::new(true, project_dir, None)?;

    let content = r#"# Example

```literal
{{ template.syntax }}
This block is not closed"#;

    let (protected, placeholders) = renderer.protect_literal_blocks(content);

    // Should have no placeholders (unclosed block is treated as regular content)
    assert_eq!(placeholders.len(), 0);

    // Content should be preserved as-is
    assert!(protected.contains("```literal"));
    assert!(protected.contains("{{ template.syntax }}"));

    Ok(())
}

#[test]
fn test_literal_blocks_with_indentation() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let renderer = TemplateRenderer::new(true, project_dir, None)?;

    let content = r#"# Example

    ```literal
    {{ indented.template }}
    ```"#;

    let (_protected, placeholders) = renderer.protect_literal_blocks(content);

    // Should detect indented literal blocks
    assert_eq!(placeholders.len(), 1);

    // Should preserve the indented template syntax
    let placeholder_content = placeholders
        .get("__AGPM_LITERAL_BLOCK_0__")
        .ok_or_else(|| anyhow::anyhow!("Placeholder __AGPM_LITERAL_BLOCK_0__ not found"))?;
    assert!(placeholder_content.contains("{{ indented.template }}"));

    Ok(())
}

#[test]
fn test_error_line_numbers_with_frontmatter() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let mut renderer = TemplateRenderer::new(true, project_dir, None)?;
    let context = TeraContext::new();

    // Create markdown with 10 lines of frontmatter + error on line 15
    let template = r#"---
name: test
description: A test
author: Test Author
version: 1.0.0
tags:
  - example
  - test
agpm:
  templating: true
---

# Test Content

{{ variable >+ invalid }}

More content here."#;

    let result = renderer.render_template(template, &context, None);
    assert!(result.is_err(), "Should fail with syntax error");

    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);

    // Debug: Print the error to see what we get
    println!("Error message:\n{}", error_msg);

    // The error should mention line 15 (where the syntax error is)
    // Note: The exact line might be 15 or 16 depending on how Tera counts
    assert!(
        error_msg.contains("15") || error_msg.contains("16"),
        "Error should report line 15 or 16 (near the template error), not line 5. Error: {}",
        error_msg
    );

    Ok(())
}
