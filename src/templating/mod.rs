//! Markdown templating engine for AGPM resources.
//!
//! This module provides Tera-based templating functionality for Markdown resources,
//! enabling dynamic content generation during installation. It supports safe, sandboxed
//! template rendering with a rich context containing installation metadata.
//!
//! # Overview
//!
//! The templating system allows resource authors to:
//! - Reference other resources by name and type
//! - Access resolved installation paths and versions
//! - Use conditional logic and loops in templates
//! - Read and embed project-specific files (style guides, best practices, etc.)
//!
//! # Template Context
//!
//! Templates are rendered with a structured context containing:
//! - `agpm.resource`: Current resource information (name, type, install path, etc.)
//! - `agpm.deps`: Nested dependency information by resource type and name
//!
//! # Custom Filters
//!
//! - `content`: Read project-specific files (e.g., `{{ 'docs/guide.md' | content }}`)
//!
//! # Syntax Restrictions
//!
//! For security and safety, the following Tera features are disabled:
//! - `{% include %}` tags (no file system access)
//! - `{% extends %}` tags (no template inheritance)
//! - `{% import %}` tags (no external template imports)
//! - Custom functions that access the file system or network (except content filter)
//!
//! # Supported Features
//!
//! - Variable substitution: `{{ agpm.resource.install_path }}`
//! - Conditional logic: `{% if agpm.resource.source %}...{% endif %}`
//! - Loops: `{% for name, dep in agpm.deps.agents %}...{% endfor %}`
//! - Standard Tera filters (string manipulation, formatting)
//! - Project file embedding: `{{ 'path/to/file.md' | content }}`
//! - Literal blocks: Protect template syntax from rendering for documentation

// Module declarations
pub mod cache;
pub mod content;
pub mod context;
pub mod dependencies;
pub mod filters;
pub mod renderer;
pub mod utils;

// Re-exports for public API
pub use context::{DependencyData, ResourceMetadata, TemplateContextBuilder};
pub use renderer::TemplateRenderer;
pub use utils::{deep_merge_json, to_native_path_display};

#[cfg(test)]
mod tests {
    use super::content::{NON_TEMPLATED_LITERAL_GUARD_END, NON_TEMPLATED_LITERAL_GUARD_START};
    use super::*;
    use crate::core::ResourceType;
    use crate::lockfile::{LockFile, LockedResource, LockedResourceBuilder};

    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tera::Context as TeraContext;

    fn create_test_lockfile() -> LockFile {
        let mut lockfile = LockFile::default();

        // Add a test agent
        lockfile.agents.push(LockedResource {
            name: "test-agent".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum".to_string(),
            context_checksum: None,
            installed_at: ".claude/agents/test-agent.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
        });

        lockfile
    }

    #[tokio::test]
    async fn test_template_context_builder() {
        let lockfile = create_test_lockfile();

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();
        let builder =
            TemplateContextBuilder::new(Arc::new(lockfile), None, Arc::new(cache), project_dir);

        let variant_inputs = serde_json::json!({});
        let hash = crate::utils::compute_variant_inputs_hash(&variant_inputs).unwrap();
        let resource_id = crate::lockfile::ResourceId::new(
            "test-agent",
            Some("community"),
            Some("claude-code"),
            ResourceType::Agent,
            hash,
        );
        let (_context, _checksum) =
            builder.build_context(&resource_id, &variant_inputs).await.unwrap();

        // If we got here without panicking, context building succeeded
        // The actual context structure is tested implicitly by the renderer tests
    }

    #[test]
    fn test_template_renderer() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Test rendering without template syntax
        let result = renderer.render_template("# Plain Markdown", &TeraContext::new()).unwrap();
        assert_eq!(result, "# Plain Markdown");

        // Test rendering with template syntax
        let mut context = TeraContext::new();
        context.insert("test_var", "test_value");

        let result = renderer.render_template("# {{ test_var }}", &context).unwrap();
        assert_eq!(result, "# test_value");
    }

    #[test]
    fn test_template_renderer_disabled() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(false, project_dir, None).unwrap();

        let mut context = TeraContext::new();
        context.insert("test_var", "test_value");

        // Should return content as-is when disabled
        let result = renderer.render_template("# {{ test_var }}", &context).unwrap();
        assert_eq!(result, "# {{ test_var }}");
    }

    #[test]
    fn test_template_error_formatting() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();
        let context = TeraContext::new();

        // Test with missing variable - should produce detailed error
        let result = renderer.render_template("# {{ missing_var }}", &context);
        assert!(result.is_err());

        let error = result.unwrap_err();
        let error_msg = format!("{}", error);

        // Error should NOT contain "__tera_one_off"
        assert!(
            !error_msg.contains("__tera_one_off"),
            "Error should not expose internal Tera template names"
        );

        // Error should contain useful information about the missing variable
        assert!(
            error_msg.contains("missing_var") || error_msg.contains("Variable"),
            "Error should mention the problematic variable or that a variable is missing. Got: {}",
            error_msg
        );
    }

    #[test]
    fn test_to_native_path_display() {
        // Test Unix-style path conversion
        let unix_path = ".claude/agents/test.md";
        let native_path = to_native_path_display(unix_path);

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
        let native_path = to_native_path_display(unix_path);

        #[cfg(windows)]
        {
            assert_eq!(native_path, ".claude\\agents\\ai\\helpers\\test.md");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(native_path, ".claude/agents/ai/helpers/test.md");
        }
    }

    #[tokio::test]
    async fn test_template_context_uses_native_paths() {
        use tempfile::TempDir;
        use tokio::fs;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();

        let mut lockfile = create_test_lockfile();

        // Add another resource with a nested path
        lockfile.snippets.push(LockedResource {
            name: "test-snippet".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "snippets/utils/test.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum".to_string(),
            installed_at: ".claude/snippets/utils/test.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Snippet,
            context_checksum: None,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            variant_inputs: crate::resolver::lockfile_builder::VariantInputs::default(),
        });

        // Add the snippet as a dependency of the test-agent
        if let Some(agent) = lockfile.agents.first_mut() {
            agent.dependencies.push("snippet:test-snippet".to_string());
        }

        // Create the snippet file at the installed location
        let snippet_path = project_dir.join(".claude/snippets/utils/test.md");
        fs::create_dir_all(snippet_path.parent().unwrap()).await.unwrap();
        let snippet_content = "# Test Snippet\n\nSome content here.";
        fs::write(&snippet_path, snippet_content).await.unwrap();

        let cache = crate::cache::Cache::new().unwrap();
        let builder =
            TemplateContextBuilder::new(Arc::new(lockfile), None, Arc::new(cache), project_dir);
        let variant_inputs = serde_json::json!({});
        let hash = crate::utils::compute_variant_inputs_hash(&variant_inputs).unwrap();
        let resource_id = crate::lockfile::ResourceId::new(
            "test-agent",
            Some("community"),
            Some("claude-code"),
            ResourceType::Agent,
            hash,
        );
        let (context, _checksum) =
            builder.build_context(&resource_id, &variant_inputs).await.unwrap();

        // Extract the agpm.resource.install_path from context
        let agpm_value = context.get("agpm").expect("agpm context should exist");
        let agpm_obj = agpm_value.as_object().expect("agpm should be an object");
        let resource_value = agpm_obj.get("resource").expect("resource should exist");
        let resource_obj = resource_value.as_object().expect("resource should be an object");
        let install_path = resource_obj
            .get("install_path")
            .expect("install_path should exist")
            .as_str()
            .expect("install_path should be a string");

        // Verify the path uses platform-native separators
        #[cfg(windows)]
        {
            assert_eq!(install_path, ".claude\\agents\\test-agent.md");
            assert!(install_path.contains('\\'), "Windows paths should use backslashes");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(install_path, ".claude/agents/test-agent.md");
            assert!(install_path.contains('/'), "Unix paths should use forward slashes");
        }

        // Also verify dependency paths
        let deps_value = agpm_obj.get("deps").expect("deps should exist");
        let deps_obj = deps_value.as_object().expect("deps should be an object");
        let snippets = deps_obj.get("snippets").expect("snippets should exist");
        let snippets_obj = snippets.as_object().expect("snippets should be an object");
        let test_snippet = snippets_obj.get("test_snippet").expect("test_snippet should exist");
        let snippet_obj = test_snippet.as_object().expect("test_snippet should be an object");
        let snippet_path = snippet_obj
            .get("install_path")
            .expect("install_path should exist")
            .as_str()
            .expect("install_path should be a string");

        #[cfg(windows)]
        {
            assert_eq!(snippet_path, ".claude\\snippets\\utils\\test.md");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(snippet_path, ".claude/snippets/utils/test.md");
        }
    }

    // Tests for literal block functionality (Phase 1)

    #[test]
    fn test_protect_literal_blocks_basic() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

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
        let placeholder_content = placeholders.get("__AGPM_LITERAL_BLOCK_0__").unwrap();
        assert!(placeholder_content.contains("{{ agpm.deps.snippets.example.content }}"));
    }

    #[test]
    fn test_protect_literal_blocks_multiple() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

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
    }

    #[test]
    fn test_restore_literal_blocks() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

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
    }

    #[test]
    fn test_literal_blocks_integration_with_rendering() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = r#"# Agent: {{ agent_name }}

## Documentation

Here's how to use template syntax:

```literal
{{ agpm.deps.snippets.helper.content }}
```

The agent name is: {{ agent_name }}"#;

        let mut context = TeraContext::new();
        context.insert("agent_name", "test-agent");

        let result = renderer.render_template(template, &context).unwrap();

        // The agent_name variable should be rendered
        assert!(result.contains("# Agent: test-agent"));
        assert!(result.contains("The agent name is: test-agent"));

        // The literal block should be preserved and wrapped in code fence
        assert!(result.contains("```\n{{ agpm.deps.snippets.helper.content }}\n```"));

        // The literal block should NOT be rendered (still has template syntax)
        assert!(result.contains("{{ agpm.deps.snippets.helper.content }}"));
    }

    #[test]
    fn test_literal_blocks_with_complex_template_syntax() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = r#"# Documentation

```literal
{% for item in agpm.deps.agents %}
{{ item.name }}: {{ item.version }}
{% endfor %}
```"#;

        let context = TeraContext::new();
        let result = renderer.render_template(template, &context).unwrap();

        // Should preserve the for loop syntax
        assert!(result.contains("{% for item in agpm.deps.agents %}"));
        assert!(result.contains("{{ item.name }}"));
        assert!(result.contains("{% endfor %}"));
    }

    #[test]
    fn test_literal_blocks_empty() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = r#"# Example

```literal
```

Done."#;

        let context = TeraContext::new();
        let result = renderer.render_template(template, &context).unwrap();

        // Should handle empty literal blocks gracefully
        assert!(result.contains("# Example"));
        assert!(result.contains("Done."));
    }

    #[test]
    fn test_literal_blocks_unclosed() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

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
    }

    #[test]
    fn test_literal_blocks_with_indentation() {
        let project_dir = std::env::current_dir().unwrap();
        let renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let content = r#"# Example

    ```literal
    {{ indented.template }}
    ```"#;

        let (_protected, placeholders) = renderer.protect_literal_blocks(content);

        // Should detect indented literal blocks
        assert_eq!(placeholders.len(), 1);

        // Should preserve the indented template syntax
        let placeholder_content = placeholders.get("__AGPM_LITERAL_BLOCK_0__").unwrap();
        assert!(placeholder_content.contains("{{ indented.template }}"));
    }

    #[test]
    fn test_literal_blocks_in_transitive_dependency_content() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();

        // Create a dependency file with literal blocks containing template syntax
        let dep_content = r#"---
agpm.templating: true
---
# Dependency Documentation

Here's a template example:

```literal
{{ nonexistent_variable }}
{{ agpm.deps.something.else }}
```

This should appear literally."#;

        // Write the dependency file
        let dep_path = project_dir.join("dependency.md");
        fs::write(&dep_path, dep_content).unwrap();

        // First, render the dependency content (simulating what happens when processing a dependency)
        let mut dep_renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let dep_context = TeraContext::new();
        let rendered_dep = dep_renderer.render_template(dep_content, &dep_context).unwrap();

        // The rendered dependency should have the literal block converted to a regular code fence
        assert!(rendered_dep.contains("```\n{{ nonexistent_variable }}"));
        assert!(rendered_dep.contains("{{ agpm.deps.something.else }}\n```"));

        // Now simulate embedding this in a parent resource
        let parent_template = r#"# Parent Resource

## Embedded Documentation

{{ dependency_content }}

## End"#;

        // Create context with the rendered dependency content
        let mut parent_context = TeraContext::new();
        parent_context.insert("dependency_content", &rendered_dep);

        // Render the parent (with templating enabled)
        let mut parent_renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let final_output =
            parent_renderer.render_template(parent_template, &parent_context).unwrap();

        // Verify the final output contains the template syntax literally
        assert!(
            final_output.contains("{{ nonexistent_variable }}"),
            "Template syntax from literal block should appear literally in final output"
        );
        assert!(
            final_output.contains("{{ agpm.deps.something.else }}"),
            "Template syntax from literal block should appear literally in final output"
        );

        // Verify it's in a code fence
        assert!(
            final_output.contains("```\n{{ nonexistent_variable }}"),
            "Literal content should be in a code fence"
        );

        // Verify it doesn't cause rendering errors
        assert!(!final_output.contains("__AGPM_LITERAL_BLOCK_"), "No placeholders should remain");
    }

    #[test]
    fn test_literal_blocks_with_nested_dependencies() {
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Simulate a dependency that was already rendered with literal blocks
        let dep_content = r#"# Helper Snippet

Use this syntax:

```
{{ agpm.deps.snippets.example.content }}
{{ missing.variable }}
```

Done."#;

        // Now embed this in a parent template
        let parent_template = r#"# Main Agent

## Documentation

{{ helper_content }}

The agent uses templating."#;

        let mut context = TeraContext::new();
        context.insert("helper_content", dep_content);

        let result = renderer.render_template(parent_template, &context).unwrap();

        // The template syntax from the dependency should be preserved
        assert!(result.contains("{{ agpm.deps.snippets.example.content }}"));
        assert!(result.contains("{{ missing.variable }}"));

        // It should be in a code fence
        assert!(result.contains("```\n{{ agpm.deps.snippets.example.content }}"));
    }

    #[tokio::test]
    async fn test_non_templated_dependency_content_is_guarded() {
        use tempfile::TempDir;
        use tokio::fs;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();

        let snippets_dir = project_dir.join("snippets");
        fs::create_dir_all(&snippets_dir).await.unwrap();
        let snippet_path = snippets_dir.join("non-templated.md");
        let snippet_content = r#"---
agpm:
  templating: false
---
# Example Snippet

This should show {{ agpm.deps.some.content }} literally.
"#;
        fs::write(&snippet_path, snippet_content).await.unwrap();

        // Also create the installed file at the location referenced in the lockfile
        let installed_snippets_dir = project_dir.join(".claude/snippets");
        fs::create_dir_all(&installed_snippets_dir).await.unwrap();
        let installed_snippet_path = installed_snippets_dir.join("non-templated.md");
        fs::write(&installed_snippet_path, snippet_content).await.unwrap();

        let mut lockfile = LockFile::default();
        lockfile.commands.push(
            LockedResourceBuilder::new(
                "test-command".to_string(),
                "commands/test.md".to_string(),
                "sha256:test-command".to_string(),
                ".claude/commands/test.md".to_string(),
                ResourceType::Command,
            )
            .dependencies(vec!["snippet:non_templated".to_string()])
            .tool(Some("claude-code".to_string()))
            .applied_patches(std::collections::HashMap::new())
            .variant_inputs(crate::resolver::lockfile_builder::VariantInputs::default())
            .build(),
        );
        lockfile.snippets.push(
            LockedResourceBuilder::new(
                "non_templated".to_string(),
                "snippets/non-templated.md".to_string(),
                "sha256:test-snippet".to_string(),
                ".claude/snippets/non-templated.md".to_string(),
                ResourceType::Snippet,
            )
            .dependencies(vec![])
            .tool(Some("claude-code".to_string()))
            .applied_patches(std::collections::HashMap::new())
            .variant_inputs(crate::resolver::lockfile_builder::VariantInputs::default())
            .build(),
        );

        let cache = crate::cache::Cache::new().unwrap();
        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            None,
            Arc::new(cache),
            project_dir.clone(),
        );
        let variant_inputs = serde_json::json!({});
        let hash = crate::utils::compute_variant_inputs_hash(&variant_inputs).unwrap();
        let resource_id = crate::lockfile::ResourceId::new(
            "test-command",
            None::<String>,
            Some("claude-code"),
            ResourceType::Command,
            hash,
        );
        let (context, _checksum) =
            builder.build_context(&resource_id, &variant_inputs).await.unwrap();

        let mut renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let template = r#"# Combined Output

{{ agpm.deps.snippets.non_templated.content }}
"#;
        let rendered = renderer.render_template(template, &context).unwrap();

        assert!(
            rendered.contains("# Example Snippet"),
            "Rendered output should include the snippet heading"
        );
        assert!(
            rendered.contains("{{ agpm.deps.some.content }}"),
            "Template syntax inside non-templated dependency should remain literal"
        );
        assert!(
            !rendered.contains(NON_TEMPLATED_LITERAL_GUARD_START)
                && !rendered.contains(NON_TEMPLATED_LITERAL_GUARD_END),
            "Internal literal guard markers should not leak into rendered output"
        );
        assert!(
            !rendered.contains("```literal"),
            "Synthetic literal fences should be removed after rendering"
        );
    }

    #[tokio::test]
    async fn test_template_vars_override() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Add a second agent with template_vars to test overrides
        let template_vars = json!({
            "project": {
                "language": "python",
                "framework": "fastapi"
            },
            "custom": {
                "style": "functional"
            }
        });

        let variant_inputs_obj =
            crate::resolver::lockfile_builder::VariantInputs::new(template_vars.clone());
        lockfile.agents.push(LockedResource {
            name: "test-agent-python".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum2".to_string(),
            installed_at: ".claude/agents/test-agent-python.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            variant_inputs: variant_inputs_obj.clone(),
            context_checksum: None,
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with project config
        let project_config = {
            let mut map = toml::map::Map::new();
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            map.insert("framework".to_string(), toml::Value::String("tokio".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        // Build context without template_vars
        let variant_inputs_empty = serde_json::json!({});
        let hash_empty = crate::utils::compute_variant_inputs_hash(&variant_inputs_empty).unwrap();
        let resource_id_no_override = crate::lockfile::ResourceId::new(
            "test-agent",
            Some("community"),
            Some("claude-code"),
            ResourceType::Agent,
            hash_empty,
        );
        let (context_without_override, _checksum) =
            builder.build_context(&resource_id_no_override, &variant_inputs_empty).await.unwrap();

        // Build context WITH template_vars (different lockfile entry)
        // Must use the same variant_inputs that was stored in the lockfile
        let hash_with_override = crate::utils::compute_variant_inputs_hash(&template_vars).unwrap();
        let resource_id_with_override = crate::lockfile::ResourceId::new(
            "test-agent-python",
            Some("community"),
            Some("claude-code"),
            ResourceType::Agent,
            hash_with_override,
        );
        let (context_with_override, _checksum) =
            builder.build_context(&resource_id_with_override, &template_vars).await.unwrap();

        // Test without overrides - should use project defaults
        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        let template = "Language: {{ project.language }}, Framework: {{ project.framework }}";

        let rendered_without =
            renderer.render_template(template, &context_without_override).unwrap();
        assert_eq!(rendered_without, "Language: rust, Framework: tokio");

        // Test with overrides - should use overridden values
        let rendered_with = renderer.render_template(template, &context_with_override).unwrap();
        assert_eq!(rendered_with, "Language: python, Framework: fastapi");

        // Test new namespace from overrides
        let custom_template = "Style: {{ custom.style }}";
        let rendered_custom =
            renderer.render_template(custom_template, &context_with_override).unwrap();
        assert_eq!(rendered_custom, "Style: functional");
    }

    #[tokio::test]
    async fn test_template_vars_deep_merge() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Override only some database fields, leaving others unchanged
        let template_vars = json!({
            "project": {
                "database": {
                    "host": "db.example.com",
                    "ssl": true
                }
            }
        });

        // Add a second agent with template_vars for deep merge testing
        let variant_inputs =
            crate::resolver::lockfile_builder::VariantInputs::new(template_vars.clone());
        lockfile.agents.push(LockedResource {
            name: "test-agent-merged".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:testchecksum3".to_string(),
            installed_at: ".claude/agents/test-agent-merged.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            variant_inputs,
            context_checksum: None,
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with nested project config
        let project_config = {
            let mut map = toml::map::Map::new();
            let mut db_table = toml::map::Map::new();
            db_table.insert("type".to_string(), toml::Value::String("postgres".into()));
            db_table.insert("host".to_string(), toml::Value::String("localhost".into()));
            db_table.insert("port".to_string(), toml::Value::Integer(5432));
            map.insert("database".to_string(), toml::Value::Table(db_table));
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        // Must use the same variant_inputs that was stored in the lockfile
        let hash_for_id = crate::utils::compute_variant_inputs_hash(&template_vars).unwrap();
        let resource_id = crate::lockfile::ResourceId::new(
            "test-agent-merged",
            Some("community"),
            Some("claude-code"),
            ResourceType::Agent,
            hash_for_id,
        );
        let (context, _checksum) =
            builder.build_context(&resource_id, &template_vars).await.unwrap();

        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Test that merge kept original values and added new ones
        let template = r#"
DB Type: {{ project.database.type }}
DB Host: {{ project.database.host }}
DB Port: {{ project.database.port }}
DB SSL: {{ project.database.ssl }}
Language: {{ project.language }}
"#;

        let rendered = renderer.render_template(template, &context).unwrap();

        // Original values should be preserved
        assert!(rendered.contains("DB Type: postgres"));
        assert!(rendered.contains("DB Port: 5432"));
        assert!(rendered.contains("Language: rust"));

        // Overridden value should be used
        assert!(rendered.contains("DB Host: db.example.com"));

        // New value should be added
        assert!(rendered.contains("DB SSL: true"));
    }

    #[tokio::test]
    async fn test_template_vars_empty_object_noop() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Empty object should be a no-op
        let template_vars = json!({});
        let variant_inputs =
            crate::resolver::lockfile_builder::VariantInputs::new(template_vars.clone());

        lockfile.agents.push(LockedResource {
            name: "test-agent-empty".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:empty".to_string(),
            installed_at: ".claude/agents/test-agent-empty.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            variant_inputs,
            context_checksum: None,
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with project config
        let project_config = {
            let mut map = toml::map::Map::new();
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            map.insert("version".to_string(), toml::Value::String("1.0".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        // Must use the same variant_inputs that was stored in the lockfile
        let hash_for_id = crate::utils::compute_variant_inputs_hash(&template_vars).unwrap();
        let resource_id = crate::lockfile::ResourceId::new(
            "test-agent-empty",
            Some("community"),
            Some("claude-code"),
            ResourceType::Agent,
            hash_for_id,
        );

        let (context, _checksum) =
            builder.build_context(&resource_id, &template_vars).await.unwrap();

        let project_dir = std::env::current_dir().unwrap();
        let mut renderer = TemplateRenderer::new(true, project_dir, None).unwrap();

        // Empty template_vars should not change project config
        let template = "Language: {{ project.language }}, Version: {{ project.version }}";
        let rendered = renderer.render_template(template, &context).unwrap();
        assert_eq!(rendered, "Language: rust, Version: 1.0");
    }

    #[tokio::test]
    async fn test_template_vars_null_values() {
        use serde_json::json;

        let mut lockfile = create_test_lockfile();

        // Null value should replace field with JSON null
        let template_vars = json!({
            "project": {
                "optional_field": null
            }
        });
        let variant_inputs =
            crate::resolver::lockfile_builder::VariantInputs::new(template_vars.clone());

        lockfile.agents.push(LockedResource {
            name: "test-agent-null".to_string(),
            source: Some("community".to_string()),
            url: Some("https://github.com/example/community.git".to_string()),
            path: "agents/test-agent.md".to_string(),
            version: Some("v1.0.0".to_string()),
            resolved_commit: Some("abc123def456".to_string()),
            checksum: "sha256:null".to_string(),
            installed_at: ".claude/agents/test-agent-null.md".to_string(),
            dependencies: vec![],
            resource_type: ResourceType::Agent,
            tool: Some("claude-code".to_string()),
            manifest_alias: None,
            applied_patches: std::collections::HashMap::new(),
            install: None,
            variant_inputs,
            context_checksum: None,
        });

        let cache = crate::cache::Cache::new().unwrap();
        let project_dir = std::env::current_dir().unwrap();

        // Create manifest with project config
        let project_config = {
            let mut map = toml::map::Map::new();
            map.insert("language".to_string(), toml::Value::String("rust".into()));
            crate::manifest::ProjectConfig::from(map)
        };

        let builder = TemplateContextBuilder::new(
            Arc::new(lockfile),
            Some(project_config),
            Arc::new(cache),
            project_dir.clone(),
        );

        // Must use the same variant_inputs that was stored in the lockfile
        let hash_for_id = crate::utils::compute_variant_inputs_hash(&template_vars).unwrap();
        let resource_id = crate::lockfile::ResourceId::new(
            "test-agent-null",
            Some("community"),
            Some("claude-code"),
            ResourceType::Agent,
            hash_for_id,
        );

        let (context, _checksum) =
            builder.build_context(&resource_id, &template_vars).await.unwrap();

        // Verify null is present in context
        let agpm_value = context.get("agpm").expect("agpm should exist");
        let agpm_obj = agpm_value.as_object().expect("agpm should be an object");

        // Check both agpm.project and project namespaces
        let project_value = agpm_obj.get("project").expect("project should exist");
        let project_obj = project_value.as_object().expect("project should be an object");
        assert!(project_obj.get("optional_field").is_some());
        assert!(project_obj["optional_field"].is_null());

        // Also verify in top-level project namespace
        let top_project = context.get("project").expect("top-level project should exist");
        let top_project_obj = top_project.as_object().expect("should be object");
        assert!(top_project_obj.get("optional_field").is_some());
        assert!(top_project_obj["optional_field"].is_null());
    }
}
