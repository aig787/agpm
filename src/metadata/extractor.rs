//! Extract dependency metadata from resource files.
//!
//! This module handles the extraction of transitive dependency information
//! from resource files. Supports YAML frontmatter in Markdown files and
//! JSON fields in JSON configuration files.
//!
//! # Template Support
//!
//! When a `ProjectConfig` is provided, frontmatter is rendered as a Tera template
//! before parsing. This allows dependency paths to reference project variables:
//!
//! ```yaml
//! dependencies:
//!   snippets:
//!     - path: standards/{{ agpm.project.language }}-guide.md
//! ```

use anyhow::{Context, Result};
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;
use std::path::Path;
use tera::{Context as TeraContext, Tera};

use crate::core::OperationContext;
use crate::manifest::{DependencyMetadata, ProjectConfig};

/// Metadata extractor for resource files.
///
/// Extracts dependency information embedded in resource files:
/// - Markdown files (.md): YAML frontmatter between `---` delimiters
/// - JSON files (.json): `dependencies` field in the JSON structure
/// - Other files: No dependencies supported
pub struct MetadataExtractor;

impl MetadataExtractor {
    /// Extract dependency metadata from a file's content.
    ///
    /// Uses operation-scoped context for warning deduplication when provided.
    ///
    /// # Arguments
    /// * `path` - Path to the file (used to determine file type)
    /// * `content` - Content of the file
    /// * `project_config` - Optional project configuration for template rendering
    /// * `context` - Optional operation context for warning deduplication
    ///
    /// # Returns
    /// * `DependencyMetadata` - Extracted metadata (may be empty)
    ///
    /// # Template Support
    ///
    /// If `project_config` is provided, frontmatter is rendered as a Tera template
    /// before parsing, allowing references to project variables like:
    /// `{{ agpm.project.language }}`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::core::OperationContext;
    /// use agpm_cli::metadata::MetadataExtractor;
    /// use std::path::Path;
    ///
    /// let ctx = OperationContext::new();
    /// let path = Path::new("agent.md");
    /// let content = "---\ndependencies:\n  agents:\n    - path: helper.md\n---\n# Agent";
    ///
    /// let metadata = MetadataExtractor::extract(
    ///     path,
    ///     content,
    ///     None,
    ///     Some(&ctx)
    /// ).unwrap();
    /// ```
    pub fn extract(
        path: &Path,
        content: &str,
        project_config: Option<&ProjectConfig>,
        context: Option<&OperationContext>,
    ) -> Result<DependencyMetadata> {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        match extension {
            "md" => Self::extract_markdown_frontmatter(content, project_config, path, context),
            "json" => Self::extract_json_field(content, project_config, path, context),
            _ => {
                // Scripts and other files don't support embedded dependencies
                Ok(DependencyMetadata::default())
            }
        }
    }

    /// Extract YAML frontmatter from Markdown content.
    ///
    /// Looks for content between `---` delimiters at the start of the file.
    /// Uses two-phase extraction to respect per-resource templating settings.
    fn extract_markdown_frontmatter(
        content: &str,
        project_config: Option<&ProjectConfig>,
        path: &Path,
        context: Option<&OperationContext>,
    ) -> Result<DependencyMetadata> {
        // Check if content starts with frontmatter delimiter
        if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
            return Ok(DependencyMetadata::default());
        }

        // Find the end of frontmatter
        let search_start = if content.starts_with("---\n") {
            4
        } else {
            5
        };

        let end_pattern = if content.contains("\r\n") {
            "\r\n---\r\n"
        } else {
            "\n---\n"
        };

        if let Some(end_pos) = content[search_start..].find(end_pattern) {
            let frontmatter = &content[search_start..search_start + end_pos];

            // Phase 1: Check if templating should be enabled
            let should_template =
                project_config.is_some() && Self::should_template_frontmatter(frontmatter);

            // Phase 2: Template the frontmatter if templating should be enabled
            let templated_frontmatter = if should_template {
                Self::template_content(frontmatter, project_config.unwrap(), path)?
            } else {
                frontmatter.to_string()
            };

            // Parse YAML frontmatter
            match serde_yaml::from_str::<DependencyMetadata>(&templated_frontmatter) {
                Ok(metadata) => {
                    // Validate resource types (catch tool names used as types)
                    Self::validate_resource_types(&metadata, path)?;
                    Ok(metadata)
                }
                Err(e) => {
                    // Only warn once per file to avoid spam during transitive dependency resolution
                    if let Some(ctx) = context {
                        if ctx.should_warn_file(path) {
                            eprintln!(
                                "Warning: Unable to parse YAML frontmatter in '{}'.

The document will be processed without metadata, and any declared dependencies
will NOT be resolved or installed.

Parse error: {}

For the correct dependency format, see:
https://github.com/aig787/agpm#transitive-dependencies",
                                path.display(),
                                e
                            );
                        }
                    }
                    Ok(DependencyMetadata::default())
                }
            }
        } else {
            // No closing delimiter found
            Ok(DependencyMetadata::default())
        }
    }

    /// Extract dependencies field from JSON content.
    ///
    /// Looks for a `dependencies` field in the top-level JSON object.
    /// Uses two-phase extraction to respect per-resource templating settings.
    fn extract_json_field(
        content: &str,
        project_config: Option<&ProjectConfig>,
        path: &Path,
        context: Option<&OperationContext>,
    ) -> Result<DependencyMetadata> {
        // Phase 1: Check if templating should be enabled
        let should_template = project_config.is_some() && Self::should_template_json(content);

        // Phase 2: Template the content if templating should be enabled
        let templated_content = if should_template {
            Self::template_content(content, project_config.unwrap(), path)?
        } else {
            content.to_string()
        };

        let json: JsonValue = serde_json::from_str(&templated_content)
            .with_context(|| "Failed to parse JSON content")?;

        if let Some(deps) = json.get("dependencies") {
            // The dependencies field should match our expected structure
            match serde_json::from_value::<HashMap<String, Vec<crate::manifest::DependencySpec>>>(
                deps.clone(),
            ) {
                Ok(dependencies) => {
                    let metadata = DependencyMetadata {
                        dependencies: Some(dependencies),
                        agpm: None,
                    };
                    // Validate resource types (catch tool names used as types)
                    Self::validate_resource_types(&metadata, path)?;
                    Ok(metadata)
                }
                Err(e) => {
                    // Only warn once per file to avoid spam during transitive dependency resolution
                    if let Some(ctx) = context {
                        if ctx.should_warn_file(path) {
                            eprintln!(
                                "Warning: Unable to parse dependencies field in '{}'.

The document will be processed without metadata, and any declared dependencies
will NOT be resolved or installed.

Parse error: {}

For the correct dependency format, see:
https://github.com/aig787/agpm#transitive-dependencies",
                                path.display(),
                                e
                            );
                        }
                    }
                    Ok(DependencyMetadata::default())
                }
            }
        } else {
            Ok(DependencyMetadata::default())
        }
    }

    /// Check if templating should be enabled in YAML frontmatter.
    ///
    /// First tries to parse the YAML and honor explicit `agpm.templating` boolean.
    /// If parsing fails, falls back to textual scan for template syntax.
    fn should_template_frontmatter(frontmatter: &str) -> bool {
        // Try to parse as raw YAML value to check agpm.templating field
        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(frontmatter) {
            // Honor explicit boolean if present
            if let Some(templating) =
                value.get("agpm").and_then(|agpm| agpm.get("templating")).and_then(|v| v.as_bool())
            {
                return templating;
            }
        }

        // Fallback: textual scan
        // Look for explicit false first (higher priority)
        if frontmatter.contains("templating: false")
            || frontmatter.contains("\"templating\": false")
        {
            return false;
        }

        // Look for explicit true or template syntax
        frontmatter.contains("templating: true")
            || frontmatter.contains("\"templating\": true")
            || frontmatter.contains("{{")
            || frontmatter.contains("{%")
    }

    /// Check if templating should be enabled in JSON content.
    ///
    /// First tries to parse the JSON and honor explicit `agpm.templating` boolean.
    /// If parsing fails, falls back to textual scan for template syntax.
    fn should_template_json(content: &str) -> bool {
        // Try to parse JSON to check agpm.templating field
        if let Ok(json) = serde_json::from_str::<JsonValue>(content) {
            // Honor explicit boolean if present
            if let Some(templating) =
                json.get("agpm").and_then(|agpm| agpm.get("templating")).and_then(|v| v.as_bool())
            {
                return templating;
            }
        }

        // Fallback: textual scan
        // Look for explicit false first (higher priority)
        if content.contains("\"templating\": false") {
            return false;
        }

        // Look for explicit true or template syntax
        content.contains("\"templating\": true") || content.contains("{{") || content.contains("{%")
    }

    /// Template content using project variables.
    ///
    /// Renders the content as a Tera template with project variables available
    /// under `agpm.project.*`.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to template
    /// * `project_config` - Project configuration containing template variables
    ///
    /// # Returns
    ///
    /// Templated content string, or an error if templating fails
    ///
    /// # Error Handling
    ///
    /// If a template variable is undefined, returns an error with a helpful message.
    /// Use Tera's `default` filter for optional variables:
    /// ```yaml
    /// path: standards/{{ agpm.project.language | default(value="generic") }}-guide.md
    /// ```
    fn template_content(
        content: &str,
        project_config: &ProjectConfig,
        path: &Path,
    ) -> Result<String> {
        // Only template if content contains template syntax
        if !content.contains("{{") && !content.contains("{%") {
            return Ok(content.to_string());
        }

        let mut tera = Tera::default();
        tera.autoescape_on(vec![]); // Disable autoescaping for raw content

        let mut template_context = TeraContext::new();

        // Build agpm.project context (same structure as content templates)
        let mut agpm = Map::new();
        agpm.insert("project".to_string(), project_config.to_json_value());
        template_context.insert("agpm", &agpm);

        // Render template - errors (including undefined vars) are returned to caller
        tera.render_str(content, &template_context).map_err(|e| {
            // Extract detailed error information from Tera error
            let error_details = Self::format_tera_error(&e);

            anyhow::Error::new(e).context(format!(
                "Failed to render frontmatter template in '{}'.\n\
                 Error details:\n{}\n\n\
                 Hint: Use {{{{ var | default(value=\"fallback\") }}}} for optional variables",
                path.display(),
                error_details
            ))
        })
    }

    /// Format a Tera error with detailed information about what went wrong.
    ///
    /// Tera errors can contain various types of issues:
    /// - Missing variables (e.g., "Variable `foo` not found")
    /// - Syntax errors (e.g., "Unexpected end of template")
    /// - Filter/function errors (e.g., "Filter `unknown` not found")
    ///
    /// This function extracts the root cause and formats it in a user-friendly way,
    /// filtering out unhelpful internal template names like '__tera_one_off'.
    ///
    /// # Arguments
    ///
    /// * `error` - The Tera error to format
    fn format_tera_error(error: &tera::Error) -> String {
        use std::error::Error;

        let mut messages = Vec::new();

        // Walk the entire error chain and collect all messages
        let mut all_messages = vec![error.to_string()];
        let mut current_error: Option<&dyn Error> = error.source();
        while let Some(err) = current_error {
            all_messages.push(err.to_string());
            current_error = err.source();
        }

        // Process messages to extract useful information
        for msg in all_messages {
            // Clean up the message by removing internal template names
            let cleaned = msg
                .replace("while rendering '__tera_one_off'", "")
                .replace("Failed to render '__tera_one_off'", "Template rendering failed")
                .replace("Failed to parse '__tera_one_off'", "Template syntax error")
                .replace("'__tera_one_off'", "template")
                .trim()
                .to_string();

            // Only keep non-empty, useful messages
            if !cleaned.is_empty()
                && cleaned != "Template rendering failed"
                && cleaned != "Template syntax error"
            {
                messages.push(cleaned);
            }
        }

        // If we got useful messages, return them
        if !messages.is_empty() {
            messages.join("\n  → ")
        } else {
            // Fallback: extract just the error kind
            "Template syntax error (see details above)".to_string()
        }
    }

    /// Validate that resource type names are correct (not tool names).
    ///
    /// Common mistake: using tool names (claude-code, opencode) as section headers
    /// instead of resource types (agents, snippets, commands).
    ///
    /// # Arguments
    /// * `metadata` - The metadata to validate
    /// * `file_path` - Path to the file being validated (for error messages)
    ///
    /// # Returns
    /// * `Ok(())` if validation passes
    /// * `Err` with helpful error message if tool names detected
    fn validate_resource_types(metadata: &DependencyMetadata, file_path: &Path) -> Result<()> {
        const VALID_RESOURCE_TYPES: &[&str] =
            &["agents", "commands", "snippets", "hooks", "mcp-servers", "scripts"];
        const TOOL_NAMES: &[&str] = &["claude-code", "opencode", "agpm"];

        // Check both root-level and nested dependencies
        if let Some(dependencies) = metadata.get_dependencies() {
            for resource_type in dependencies.keys() {
                if !VALID_RESOURCE_TYPES.contains(&resource_type.as_str()) {
                    if TOOL_NAMES.contains(&resource_type.as_str()) {
                        // Specific error for tool name confusion
                        anyhow::bail!(
                            "Invalid resource type '{}' in dependencies section of '{}'.\n\n\
                            You used a tool name ('{}') as a section header, but AGPM expects resource types.\n\n\
                            ✗ Wrong:\n  dependencies:\n    {}:\n      - path: ...\n\n\
                            ✓ Correct:\n  dependencies:\n    agents:  # or snippets, commands, etc.\n      - path: ...\n        tool: {}  # Specify tool here\n\n\
                            Valid resource types: {}",
                            resource_type,
                            file_path.display(),
                            resource_type,
                            resource_type,
                            resource_type,
                            VALID_RESOURCE_TYPES.join(", ")
                        );
                    }
                    // Generic error for unknown types
                    anyhow::bail!(
                        "Unknown resource type '{}' in dependencies section of '{}'.\n\
                        Valid resource types: {}",
                        resource_type,
                        file_path.display(),
                        VALID_RESOURCE_TYPES.join(", ")
                    );
                }
            }
        }
        Ok(())
    }

    /// Extract metadata from file content without knowing the file type.
    ///
    /// Tries to detect the format automatically.
    pub fn extract_auto(content: &str) -> Result<DependencyMetadata> {
        use std::path::PathBuf;

        // Try YAML frontmatter first (for Markdown)
        if (content.starts_with("---\n") || content.starts_with("---\r\n"))
            && let Ok(metadata) = Self::extract_markdown_frontmatter(
                content,
                None,
                &PathBuf::from("unknown.md"),
                None,
            )
            && metadata.has_dependencies()
        {
            return Ok(metadata);
        }

        // Try JSON format
        if content.trim_start().starts_with('{')
            && let Ok(metadata) =
                Self::extract_json_field(content, None, &PathBuf::from("unknown.json"), None)
            && metadata.has_dependencies()
        {
            return Ok(metadata);
        }

        // No metadata found
        Ok(DependencyMetadata::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_markdown_frontmatter() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
    - path: agents/reviewer.md
  snippets:
    - path: snippets/utils.md
---

# My Command

This is the command documentation."#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["agents"].len(), 2);
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["agents"][0].path, "agents/helper.md");
        assert_eq!(deps["agents"][0].version, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_extract_markdown_no_frontmatter() {
        let content = r#"# My Command

This is a command without frontmatter."#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_json_dependencies() {
        let content = r#"{
  "events": ["UserPromptSubmit"],
  "type": "command",
  "command": ".claude/scripts/test.js",
  "dependencies": {
    "scripts": [
      { "path": "scripts/test-runner.sh", "version": "v1.0.0" },
      { "path": "scripts/validator.py" }
    ],
    "agents": [
      { "path": "agents/code-analyzer.md", "version": "~1.2.0" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["scripts"].len(), 2);
        assert_eq!(deps["agents"].len(), 1);
        assert_eq!(deps["scripts"][0].path, "scripts/test-runner.sh");
        assert_eq!(deps["scripts"][0].version, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_extract_json_no_dependencies() {
        let content = r#"{
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-github"]
}"#;

        let path = Path::new("mcp.json");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_script_file() {
        let content = r#"#!/bin/bash
echo "This is a script file"
# Scripts don't support dependencies"#;

        let path = Path::new("script.sh");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_auto_markdown() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/test.md
---

# Content"#;

        let metadata = MetadataExtractor::extract_auto(content).unwrap();
        assert!(metadata.has_dependencies());
        assert_eq!(metadata.dependency_count(), 1);
    }

    #[test]
    fn test_extract_auto_json() {
        let content = r#"{
  "dependencies": {
    "snippets": [
      { "path": "snippets/test.md" }
    ]
  }
}"#;

        let metadata = MetadataExtractor::extract_auto(content).unwrap();
        assert!(metadata.has_dependencies());
        assert_eq!(metadata.dependency_count(), 1);
    }

    #[test]
    fn test_windows_line_endings() {
        let content = "---\r\ndependencies:\r\n  agents:\r\n    - path: agents/test.md\r\n---\r\n\r\n# Content";

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["agents"].len(), 1);
        assert_eq!(deps["agents"][0].path, "agents/test.md");
    }

    #[test]
    fn test_empty_dependencies() {
        let content = r#"---
dependencies:
---

# Content"#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        // Should parse successfully but have no dependencies
        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_malformed_yaml() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/test.md
    version: missing dash
---

# Content"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        // Should succeed but return empty metadata (with warning logged)
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert!(metadata.dependencies.is_none());
    }

    #[test]
    fn test_extract_with_tool_field() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/backend.md
      version: v1.0.0
      tool: opencode
    - path: agents/frontend.md
      tool: claude-code
---

# Command with multi-tool dependencies"#;

        let path = Path::new("command.md");
        let metadata = MetadataExtractor::extract(path, content, None, None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();
        assert_eq!(deps["agents"].len(), 2);

        // Verify tool fields are preserved
        assert_eq!(deps["agents"][0].path, "agents/backend.md");
        assert_eq!(deps["agents"][0].tool, Some("opencode".to_string()));

        assert_eq!(deps["agents"][1].path, "agents/frontend.md");
        assert_eq!(deps["agents"][1].tool, Some("claude-code".to_string()));
    }

    #[test]
    fn test_extract_unknown_field_warning() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/test.md
      version: v1.0.0
      invalid_field: should_warn
---

# Content"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        // Should succeed but return empty metadata due to unknown field
        assert!(result.is_ok());
        let metadata = result.unwrap();
        // With deny_unknown_fields, the parsing fails and we get empty metadata
        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_template_frontmatter_with_project_vars() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        config_map.insert("framework".to_string(), toml::Value::String("tokio".into()));
        let project_config = ProjectConfig::from(config_map);

        // Markdown with templated dependency path
        let content = r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-guide.md
      version: v1.0.0
  commands:
    - path: configs/{{ agpm.project.framework }}-setup.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&project_config), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Check that templates were resolved
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "standards/rust-guide.md");

        assert_eq!(deps["commands"].len(), 1);
        assert_eq!(deps["commands"][0].path, "configs/tokio-setup.md");
    }

    #[test]
    fn test_template_frontmatter_with_missing_vars() {
        // Create a project config with only one variable
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        let project_config = ProjectConfig::from(config_map);

        // Template references undefined variable (should error with helpful message)
        let content = r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-{{ agpm.project.undefined }}-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let result = MetadataExtractor::extract(path, content, Some(&project_config), None);

        // Should error on undefined variable
        assert!(result.is_err());
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Failed to render frontmatter template"));
        assert!(error_msg.contains("default")); // Suggests using default filter
    }

    #[test]
    fn test_template_frontmatter_with_default_filter() {
        // Create a project config with only one variable
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        let project_config = ProjectConfig::from(config_map);

        // Use default filter for undefined variable (recommended pattern)
        let content = r#"---
agpm:
  templating: true
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-{{ agpm.project.style | default(value="standard") }}-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&project_config), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Default filter provides fallback value
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "standards/rust-standard-guide.md");
    }

    #[test]
    fn test_template_json_dependencies() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("tool".to_string(), toml::Value::String("linter".into()));
        let project_config = ProjectConfig::from(config_map);

        // JSON with templated dependency path
        let content = r#"{
  "events": ["UserPromptSubmit"],
  "command": "node",
  "agpm": {
    "templating": true
  },
  "dependencies": {
    "scripts": [
      { "path": "scripts/{{ agpm.project.tool }}.js", "version": "v1.0.0" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&project_config), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Check that template was resolved
        assert_eq!(deps["scripts"].len(), 1);
        assert_eq!(deps["scripts"][0].path, "scripts/linter.js");
    }

    #[test]
    fn test_template_with_no_template_syntax() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        let project_config = ProjectConfig::from(config_map);

        // Content without template syntax - should work normally
        let content = r#"---
dependencies:
  snippets:
    - path: standards/plain-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&project_config), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Path should remain unchanged
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "standards/plain-guide.md");
    }

    #[test]
    fn test_template_opt_out_via_agpm_field() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        let project_config = ProjectConfig::from(config_map);

        // Content with template syntax BUT templating disabled via agpm.templating field
        let content = r#"---
agpm:
  templating: false
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&project_config), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Template syntax should be preserved (not rendered)
        assert_eq!(deps["snippets"].len(), 1);
        assert_eq!(deps["snippets"][0].path, "standards/{{ agpm.project.language }}-guide.md");
    }

    #[test]
    fn test_template_transitive_dep_path() {
        use std::path::PathBuf;

        // Test that dependency paths in frontmatter are templated correctly
        let content = r#"---
agpm:
  templating: true
dependencies:
  agents:
    - path: agents/{{ agpm.project.language }}-helper.md
      version: v1.0.0
---

# Main Agent
"#;

        let mut config_map = toml::map::Map::new();
        config_map.insert("language".to_string(), toml::Value::String("rust".to_string()));
        let config = ProjectConfig::from(config_map);

        let path = PathBuf::from("agents/main.md");
        let result = MetadataExtractor::extract(&path, content, Some(&config), None);

        assert!(result.is_ok(), "Should extract metadata: {:?}", result.err());
        let metadata = result.unwrap();

        // Should have dependencies
        assert!(metadata.dependencies.is_some(), "Should have dependencies");
        let deps = metadata.dependencies.unwrap();

        // Should have agents key
        assert!(deps.contains_key("agents"), "Should have agents dependencies");
        let agents = &deps["agents"];

        // Should have one agent dependency
        assert_eq!(agents.len(), 1, "Should have one agent dependency");

        // Path should be templated (not contain template syntax)
        let dep_path = &agents[0].path;
        assert_eq!(
            dep_path, "agents/rust-helper.md",
            "Path should be templated to rust-helper, got: {}",
            dep_path
        );
        assert!(!dep_path.contains("{{"), "Path should not contain template syntax");
        assert!(!dep_path.contains("}}"), "Path should not contain template syntax");
    }

    #[test]
    fn test_template_opt_out_json() {
        // Create a project config
        let mut config_map = toml::map::Map::new();
        config_map.insert("tool".to_string(), toml::Value::String("linter".into()));
        let project_config = ProjectConfig::from(config_map);

        // JSON with template syntax BUT templating disabled
        let content = r#"{
  "agpm": {
    "templating": false
  },
  "events": ["UserPromptSubmit"],
  "dependencies": {
    "scripts": [
      { "path": "scripts/{{ agpm.project.tool }}.js" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let metadata =
            MetadataExtractor::extract(path, content, Some(&project_config), None).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Template syntax should be preserved (not rendered)
        assert_eq!(deps["scripts"].len(), 1);
        assert_eq!(deps["scripts"][0].path, "scripts/{{ agpm.project.tool }}.js");
    }

    #[test]
    fn test_validate_tool_name_as_resource_type_yaml() {
        // YAML using tool name 'opencode' instead of resource type 'agents'
        let content = r#"---
dependencies:
  opencode:
    - path: agents/helper.md
---
# Command"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid resource type 'opencode'"));
        assert!(err_msg.contains("tool name"));
        assert!(err_msg.contains("agents:"));
    }

    #[test]
    fn test_validate_tool_name_as_resource_type_json() {
        // JSON using tool name 'claude-code' instead of resource type 'snippets'
        let content = r#"{
  "dependencies": {
    "claude-code": [
      { "path": "snippets/helper.md" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid resource type 'claude-code'"));
        assert!(err_msg.contains("tool name"));
    }

    #[test]
    fn test_validate_unknown_resource_type() {
        // Using a completely unknown resource type
        let content = r#"---
dependencies:
  foobar:
    - path: something/test.md
---
# Command"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unknown resource type 'foobar'"));
        assert!(err_msg.contains("Valid resource types"));
    }

    #[test]
    fn test_validate_correct_resource_types() {
        // All valid resource types should pass
        let content = r#"---
dependencies:
  agents:
    - path: agents/helper.md
  snippets:
    - path: snippets/util.md
  commands:
    - path: commands/deploy.md
---
# Command"#;

        let path = Path::new("command.md");
        let result = MetadataExtractor::extract(path, content, None, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_warning_deduplication_with_context() {
        use std::path::PathBuf;

        // Create an operation context
        let ctx = OperationContext::new();

        let path = PathBuf::from("test-file.md");
        let different_path = PathBuf::from("different-file.md");

        // First call should return true (first warning)
        assert!(ctx.should_warn_file(&path));

        // Second call should return false (already warned)
        assert!(!ctx.should_warn_file(&path));

        // Third call should also return false
        assert!(!ctx.should_warn_file(&path));

        // Different file should still warn
        assert!(ctx.should_warn_file(&different_path));
    }

    #[test]
    fn test_context_isolation() {
        use std::path::PathBuf;

        // Two separate contexts should be isolated
        let ctx1 = OperationContext::new();
        let ctx2 = OperationContext::new();
        let path = PathBuf::from("test-isolation.md");

        // Both contexts should warn the first time
        assert!(ctx1.should_warn_file(&path));
        assert!(ctx2.should_warn_file(&path));

        // Both should deduplicate independently
        assert!(!ctx1.should_warn_file(&path));
        assert!(!ctx2.should_warn_file(&path));
    }

    #[test]
    fn test_should_template_frontmatter_explicit_true() {
        let content = r#"---
agpm:
  templating: true
dependencies:
  agents:
    - path: helper.md
---"#;

        assert!(MetadataExtractor::should_template_frontmatter(content));
    }

    #[test]
    fn test_should_template_frontmatter_explicit_false() {
        let content = r#"---
agpm:
  templating: false
dependencies:
  agents:
    - path: {{ template }}.md
---"#;

        assert!(!MetadataExtractor::should_template_frontmatter(content));
    }

    #[test]
    fn test_should_template_frontmatter_with_template_syntax() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/{{ agpm.project.language }}-helper.md
---"#;

        assert!(MetadataExtractor::should_template_frontmatter(content));
    }

    #[test]
    fn test_should_template_frontmatter_malformed_with_template_syntax() {
        let content = r#"---
agpm:
  templating: not-a-boolean
dependencies:
  agents:
    - path: agents/{{ agpm.project.language }}-helper.md
  invalid_yaml: [unclosed array
---"#;

        assert!(MetadataExtractor::should_template_frontmatter(content));
    }

    #[test]
    fn test_should_template_frontmatter_malformed_no_template_syntax() {
        let content = r#"---
agpm:
  templating: not-a-boolean
dependencies:
  agents:
    - path: agents/helper.md
  invalid_yaml: [unclosed array
---"#;

        assert!(!MetadataExtractor::should_template_frontmatter(content));
    }

    #[test]
    fn test_should_template_frontmatter_no_agpm_section() {
        let content = r#"---
dependencies:
  agents:
    - path: agents/helper.md
---"#;

        assert!(!MetadataExtractor::should_template_frontmatter(content));
    }

    #[test]
    fn test_should_template_json_explicit_true() {
        let content = r#"{
  "agpm": {
    "templating": true
  },
  "dependencies": {
    "agents": [
      { "path": "helper.md" }
    ]
  }
}"#;

        assert!(MetadataExtractor::should_template_json(content));
    }

    #[test]
    fn test_should_template_json_explicit_false() {
        let content = r#"{
  "agpm": {
    "templating": false
  },
  "dependencies": {
    "agents": [
      { "path": "{{ template }}.md" }
    ]
  }
}"#;

        assert!(!MetadataExtractor::should_template_json(content));
    }

    #[test]
    fn test_should_template_json_with_template_syntax() {
        let content = r#"{
  "dependencies": {
    "agents": [
      { "path": "agents/{{ agpm.project.language }}-helper.md" }
    ]
  }
}"#;

        assert!(MetadataExtractor::should_template_json(content));
    }

    #[test]
    fn test_should_template_json_malformed_with_template_syntax() {
        let content = r#"{
  "agpm": {
    "templating": not-a-boolean
  },
  "dependencies": {
    "agents": [
      { "path": "agents/{{ agpm.project.language }}-helper.md" }
    ]
  },
  "invalid_json": "unclosed string
}"#;

        assert!(MetadataExtractor::should_template_json(content));
    }

    #[test]
    fn test_should_template_json_malformed_no_template_syntax() {
        let content = r#"{
  "agpm": {
    "templating": not-a-boolean
  },
  "dependencies": {
    "agents": [
      { "path": "agents/helper.md" }
    ]
  },
  "invalid_json": "unclosed string
}"#;

        assert!(!MetadataExtractor::should_template_json(content));
    }

    #[test]
    fn test_should_template_json_no_agpm_section() {
        let content = r#"{
  "dependencies": {
    "agents": [
      { "path": "helper.md" }
    ]
  }
}"#;

        assert!(!MetadataExtractor::should_template_json(content));
    }
}
