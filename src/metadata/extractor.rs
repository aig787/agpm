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
    /// # Arguments
    /// * `path` - Path to the file (used to determine file type)
    /// * `content` - Content of the file
    /// * `project_config` - Optional project configuration for template rendering
    ///
    /// # Returns
    /// * `DependencyMetadata` - Extracted metadata (may be empty)
    ///
    /// # Template Support
    ///
    /// If `project_config` is provided, frontmatter is rendered as a Tera template
    /// before parsing, allowing references to project variables like:
    /// `{{ agpm.project.language }}`
    pub fn extract(
        path: &Path,
        content: &str,
        project_config: Option<&ProjectConfig>,
    ) -> Result<DependencyMetadata> {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        match extension {
            "md" => Self::extract_markdown_frontmatter(content, project_config, path),
            "json" => Self::extract_json_field(content, project_config, path),
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

            // Phase 1: Check if templating is disabled via agpm.templating field
            let templating_disabled = if let Some(_config) = project_config {
                Self::is_templating_disabled_yaml(frontmatter)
            } else {
                false
            };

            // Phase 2: Template the frontmatter if config available and not disabled
            let templated_frontmatter = if let Some(config) = project_config {
                if templating_disabled {
                    tracing::debug!("Templating disabled via agpm.templating field in frontmatter");
                    frontmatter.to_string()
                } else {
                    Self::template_content(frontmatter, config, path)?
                }
            } else {
                frontmatter.to_string()
            };

            // Parse YAML frontmatter
            match serde_yaml::from_str::<DependencyMetadata>(&templated_frontmatter) {
                Ok(metadata) => Ok(metadata),
                Err(e) => {
                    // Provide detailed error message for common issues
                    let error_msg = e.to_string();
                    if error_msg.contains("unknown field") {
                        tracing::warn!(
                            "Warning: YAML frontmatter contains unknown field(s): {}. \
                            Supported fields are: path, version, tool",
                            e
                        );
                        eprintln!(
                            "Warning: YAML frontmatter contains unknown field(s).\n\
                            Supported fields in dependencies are:\n\
                            - path: Path to the dependency file (required)\n\
                            - version: Version constraint (optional)\n\
                            - tool: Target tool (optional: claude-code, opencode, agpm)\n\
                            \nError: {}",
                            e
                        );
                    } else {
                        tracing::warn!("Warning: Unable to parse YAML frontmatter: {}", e);
                        eprintln!("Warning: Unable to parse YAML frontmatter: {}", e);
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
    ) -> Result<DependencyMetadata> {
        // Phase 1: Check if templating is disabled via agpm.templating field
        let templating_disabled = if let Some(_config) = project_config {
            Self::is_templating_disabled_json(content)
        } else {
            false
        };

        // Phase 2: Template the content if config available and not disabled
        let templated_content = if let Some(config) = project_config {
            if templating_disabled {
                tracing::debug!("Templating disabled via agpm.templating field in JSON");
                content.to_string()
            } else {
                Self::template_content(content, config, path)?
            }
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
                Ok(dependencies) => Ok(DependencyMetadata {
                    dependencies: Some(dependencies),
                }),
                Err(e) => {
                    // Provide detailed error message for common issues
                    let error_msg = e.to_string();
                    if error_msg.contains("unknown field") {
                        tracing::warn!(
                            "Warning: JSON dependencies contain unknown field(s): {}. \
                            Supported fields are: path, version, tool",
                            e
                        );
                        eprintln!(
                            "Warning: JSON dependencies contain unknown field(s).\n\
                            Supported fields in dependencies are:\n\
                            - path: Path to the dependency file (required)\n\
                            - version: Version constraint (optional)\n\
                            - tool: Target tool (optional: claude-code, opencode, agpm)\n\
                            \nError: {}",
                            e
                        );
                    } else {
                        tracing::warn!("Warning: Unable to parse dependencies field: {}", e);
                        eprintln!("Warning: Unable to parse dependencies field: {}", e);
                    }
                    Ok(DependencyMetadata::default())
                }
            }
        } else {
            Ok(DependencyMetadata::default())
        }
    }

    /// Check if templating is disabled in YAML frontmatter.
    ///
    /// Parses the YAML to check for `agpm.templating: false` field.
    fn is_templating_disabled_yaml(frontmatter: &str) -> bool {
        // Try to parse as raw YAML value to check agpm.templating field
        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(frontmatter) {
            value
                .get("agpm")
                .and_then(|agpm| agpm.get("templating"))
                .and_then(|v| v.as_bool())
                .map(|b| !b)
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Check if templating is disabled in JSON content.
    ///
    /// Parses the JSON to check for `agpm.templating: false` field.
    fn is_templating_disabled_json(content: &str) -> bool {
        // Try to parse JSON to check agpm.templating field
        if let Ok(json) = serde_json::from_str::<JsonValue>(content) {
            json.get("agpm")
                .and_then(|agpm| agpm.get("templating"))
                .and_then(|v| v.as_bool())
                .map(|b| !b)
                .unwrap_or(false)
        } else {
            false
        }
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

        let mut context = TeraContext::new();

        // Build agpm.project context (same structure as content templates)
        let mut agpm = Map::new();
        agpm.insert("project".to_string(), project_config.to_json_value());
        context.insert("agpm", &agpm);

        // Render template - errors (including undefined vars) are returned to caller
        tera.render_str(content, &context).with_context(|| {
            format!(
                "Failed to render frontmatter template in '{}'. \
                 Hint: Use {{{{ var | default(value=\"fallback\") }}}} for optional variables",
                path.display()
            )
        })
    }

    /// Extract metadata from file content without knowing the file type.
    ///
    /// Tries to detect the format automatically.
    pub fn extract_auto(content: &str) -> Result<DependencyMetadata> {
        use std::path::PathBuf;

        // Try YAML frontmatter first (for Markdown)
        if (content.starts_with("---\n") || content.starts_with("---\r\n"))
            && let Ok(metadata) =
                Self::extract_markdown_frontmatter(content, None, &PathBuf::from("unknown.md"))
            && metadata.has_dependencies()
        {
            return Ok(metadata);
        }

        // Try JSON format
        if content.trim_start().starts_with('{')
            && let Ok(metadata) =
                Self::extract_json_field(content, None, &PathBuf::from("unknown.json"))
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
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_script_file() {
        let content = r#"#!/bin/bash
echo "This is a script file"
# Scripts don't support dependencies"#;

        let path = Path::new("script.sh");
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

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
        let result = MetadataExtractor::extract(path, content, None);

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
        let metadata = MetadataExtractor::extract(path, content, None).unwrap();

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
        let result = MetadataExtractor::extract(path, content, None);

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
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-guide.md
      version: v1.0.0
  commands:
    - path: configs/{{ agpm.project.framework }}-setup.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata = MetadataExtractor::extract(path, content, Some(&project_config)).unwrap();

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
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-{{ agpm.project.undefined }}-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let result = MetadataExtractor::extract(path, content, Some(&project_config));

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
dependencies:
  snippets:
    - path: standards/{{ agpm.project.language }}-{{ agpm.project.style | default(value="standard") }}-guide.md
---

# My Agent"#;

        let path = Path::new("agent.md");
        let metadata = MetadataExtractor::extract(path, content, Some(&project_config)).unwrap();

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
  "dependencies": {
    "scripts": [
      { "path": "scripts/{{ agpm.project.tool }}.js", "version": "v1.0.0" }
    ]
  }
}"#;

        let path = Path::new("hook.json");
        let metadata = MetadataExtractor::extract(path, content, Some(&project_config)).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content, Some(&project_config)).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content, Some(&project_config)).unwrap();

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
        let result = MetadataExtractor::extract(&path, content, Some(&config));

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
        let metadata = MetadataExtractor::extract(path, content, Some(&project_config)).unwrap();

        assert!(metadata.has_dependencies());
        let deps = metadata.dependencies.unwrap();

        // Template syntax should be preserved (not rendered)
        assert_eq!(deps["scripts"].len(), 1);
        assert_eq!(deps["scripts"][0].path, "scripts/{{ agpm.project.tool }}.js");
    }
}
