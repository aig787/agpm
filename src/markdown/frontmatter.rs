//! Frontmatter parsing with grey_matter Engine trait and Tera templating.
//!
//! This module provides a custom grey_matter Engine that applies Tera templating
//! to frontmatter content before parsing it as YAML. This enables dynamic frontmatter
//! with template variables while maintaining compatibility with standard YAML frontmatter.
//!
//! # Example
//!
//! ```rust,no_run
//! use agpm_cli::markdown::frontmatter::FrontmatterParser;
//! use agpm_cli::manifest::DependencyMetadata;
//! use agpm_cli::manifest::ProjectConfig;
//! use std::path::Path;
//! use std::str::FromStr;
//! use toml;
//!
//! let mut parser = FrontmatterParser::new();
//! let content = r#"---
//! dependencies:
//!   agents:
//!     - path: helper.md
//!       version: "{{ project.version }}"
//! ---
//! # Content
//! "#;
//!
//! // Create a test project config
//! let toml_content = r#"
//! name = "test-project"
//! version = "1.0.0"
//! language = "rust"
//! "#;
//! let project_config = {
//!     let value = toml::Value::from_str(toml_content).unwrap();
//!     if let toml::Value::Table(table) = value {
//!         ProjectConfig::from(table)
//!     } else {
//!         ProjectConfig::default()
//!     }
//! };
//!
//! let result = parser.parse_with_templating::<DependencyMetadata>(
//!     content,
//!     Some(&project_config.to_json_value()),
//!     Path::new("test.md"),
//!     None
//! ).unwrap_or_else(|e| panic!("Failed to parse: {}", e));
//!
//! assert!(result.has_frontmatter());
//! assert!(result.data.is_some());
//! ```

use anyhow::Result;
use gray_matter::{
    Matter, Pod,
    engine::{Engine, YAML},
};
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::path::Path;
use tera::Context as TeraContext;

use crate::core::OperationContext;
use crate::manifest::ProjectConfig;
use crate::templating::TemplateRenderer;

/// Custom gray_matter engine that returns raw frontmatter text without parsing.
///
/// This engine implements the gray_matter Engine trait but simply returns the
/// raw frontmatter content as a string without any YAML parsing. This allows
/// us to extract frontmatter text even when the YAML is malformed.
struct RawFrontmatter;

impl Engine for RawFrontmatter {
    fn parse(content: &str) -> Result<Pod, gray_matter::Error> {
        // Just return the raw content as a string without any parsing
        Ok(Pod::String(content.to_string()))
    }
}

/// Result of parsing frontmatter from content.
///
/// This struct represents the parsed result from frontmatter extraction,
/// containing both the structured data and the content without frontmatter.
#[derive(Debug, Clone)]
pub struct ParsedFrontmatter<T> {
    /// The parsed frontmatter data, if any was present and successfully parsed.
    pub data: Option<T>,

    /// The content with frontmatter removed.
    pub content: String,

    /// The raw frontmatter string before templating and parsing.
    pub raw_frontmatter: Option<String>,

    /// Whether templating was applied during parsing.
    pub templated: bool,
}

impl<T> ParsedFrontmatter<T> {
    /// Check if frontmatter was present in the original content.
    pub fn has_frontmatter(&self) -> bool {
        self.raw_frontmatter.is_some()
    }
}

/// Helper functions for frontmatter templating.
pub struct FrontmatterTemplating;

impl FrontmatterTemplating {
    /// Build Tera context for frontmatter templating.
    ///
    /// Creates the template context with the agpm.project namespace
    /// based on the provided project configuration.
    ///
    /// # Arguments
    /// * `project_config` - Project configuration for template variables
    ///
    /// # Returns
    /// * `TeraContext` - Configured template context
    pub fn build_template_context(project_config: &ProjectConfig) -> TeraContext {
        let mut context = TeraContext::new();

        // Build agpm.project context (same structure as content templates)
        let mut agpm = serde_json::Map::new();
        agpm.insert("project".to_string(), project_config.to_json_value());
        context.insert("agpm", &agpm);

        // Also provide top-level project namespace for convenience
        context.insert("project", &project_config.to_json_value());

        context
    }

    /// Apply Tera templating to frontmatter content.
    ///
    /// Always renders the content as a template, even if no template syntax is present.
    ///
    /// # Arguments
    /// * `content` - The frontmatter content to template
    /// * `project_config` - Project configuration for template variables
    /// * `template_renderer` - Template renderer to use
    /// * `file_path` - Path to file for error reporting
    ///
    /// # Returns
    /// * `Result<String>` - Templated content or error
    pub fn apply_templating(
        content: &str,
        project_config: &ProjectConfig,
        template_renderer: &mut TemplateRenderer,
        file_path: &Path,
    ) -> Result<String> {
        let context = Self::build_template_context(project_config);

        // Always render as template - this handles the case where there's no template syntax
        template_renderer.render_template(content, &context, None).map_err(|e| {
            anyhow::anyhow!(
                "Failed to render frontmatter template in '{}': {}",
                file_path.display(),
                e
            )
        })
    }

    /// Build template context from variant inputs.
    ///
    /// Creates a Tera context from variant_inputs, which contains all template
    /// variables including project config and any overrides.
    ///
    /// # Arguments
    /// * `variant_inputs` - Template variables (project, config, etc.)
    ///
    /// # Returns
    /// * `TeraContext` - Configured template context
    pub fn build_template_context_from_variant_inputs(
        variant_inputs: &serde_json::Value,
    ) -> TeraContext {
        let mut context = TeraContext::new();

        // Build agpm namespace and top-level keys from variant_inputs
        if let Some(obj) = variant_inputs.as_object() {
            let mut agpm = serde_json::Map::new();

            for (key, value) in obj {
                // Insert at top level
                context.insert(key, value);
                // Also add to agpm namespace
                agpm.insert(key.clone(), value.clone());
            }

            context.insert("agpm", &agpm);
        }

        context
    }
}

/// Unified frontmatter parser with templating support.
///
/// This struct provides a centralized interface for parsing frontmatter from
/// content using the grey_matter library, with optional Tera templating support.
/// It handles YAML, TOML, and JSON frontmatter formats automatically.
pub struct FrontmatterParser {
    raw_matter: Matter<RawFrontmatter>,
    yaml_matter: Matter<YAML>,
    template_renderer: TemplateRenderer,
}

impl Clone for FrontmatterParser {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl Debug for FrontmatterParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrontmatterParser").finish()
    }
}

impl Default for FrontmatterParser {
    fn default() -> Self {
        Self::new()
    }
}

impl FrontmatterParser {
    /// Create a new frontmatter parser.
    pub fn new() -> Self {
        let project_dir = std::env::current_dir().unwrap_or_default();
        let template_renderer = TemplateRenderer::new(true, project_dir.clone(), None)
            .unwrap_or_else(|_| {
                // Fallback to disabled renderer if configuration fails
                TemplateRenderer::new(false, project_dir, None).unwrap()
            });

        Self {
            raw_matter: Matter::new(),
            yaml_matter: Matter::new(),
            template_renderer,
        }
    }

    /// Create a new frontmatter parser with custom project directory.
    ///
    /// # Arguments
    /// * `project_dir` - Project root directory for template rendering
    pub fn with_project_dir(project_dir: std::path::PathBuf) -> Result<Self> {
        let template_renderer = TemplateRenderer::new(true, project_dir.clone(), None)?;

        Ok(Self {
            raw_matter: Matter::new(),
            yaml_matter: Matter::new(),
            template_renderer,
        })
    }

    /// Parse content and extract frontmatter with optional templating.
    ///
    /// This method provides the complete parsing pipeline:
    /// 1. Extract frontmatter using gray_matter
    /// 2. Apply Tera templating if variant_inputs is provided
    /// 3. Deserialize the result to the target type
    ///
    /// # Arguments
    /// * `content` - The content to parse
    /// * `variant_inputs` - Optional template variables (project, config, etc.)
    /// * `file_path` - Path to the file (used for error reporting)
    /// * `context` - Optional operation context for warning deduplication
    ///
    /// # Returns
    /// * `ParsedFrontmatter<T>` - The parsed result with data and content
    pub fn parse_with_templating<T>(
        &mut self,
        content: &str,
        variant_inputs: Option<&serde_json::Value>,
        file_path: &Path,
        context: Option<&OperationContext>,
    ) -> Result<ParsedFrontmatter<T>>
    where
        T: DeserializeOwned,
    {
        // Step 1: Extract raw frontmatter text first (before any YAML parsing)
        let raw_frontmatter_text = self.extract_raw_frontmatter(content);
        let content_without_frontmatter = self.strip_frontmatter(content);

        // Step 2: Always apply templating if frontmatter is present
        let (templated_frontmatter, was_templated) = if let Some(raw_fm) =
            raw_frontmatter_text.as_ref()
        {
            // Always apply templating to catch invalid Jinja syntax
            let templated = if let Some(inputs) = variant_inputs {
                let ctx = FrontmatterTemplating::build_template_context_from_variant_inputs(inputs);
                self.template_renderer.render_template(raw_fm, &ctx, None).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to render frontmatter template in '{}': {}",
                        file_path.display(),
                        e
                    )
                })?
            } else {
                // Even without variant_inputs, render to catch syntax errors
                let empty_context = TeraContext::new();
                self.template_renderer.render_template(raw_fm, &empty_context, None).map_err(
                    |e| {
                        anyhow::anyhow!(
                            "Failed to render frontmatter template in '{}': {}",
                            file_path.display(),
                            e
                        )
                    },
                )?
            };
            (Some(templated), true)
        } else {
            (None, false)
        };

        // Step 3: Deserialize to target type
        let parsed_data = if let Some(frontmatter) = templated_frontmatter {
            match serde_yaml::from_str::<T>(&frontmatter) {
                Ok(data) => Some(data),
                Err(e) => {
                    // Only warn once per file to avoid spam during transitive dependency resolution
                    if let Some(ctx) = context {
                        if ctx.should_warn_file(file_path) {
                            eprintln!(
                                "Warning: Unable to parse YAML frontmatter in '{}'.

The document will be processed without metadata, and any declared dependencies
will NOT be resolved or installed.

Parse error: {}

For the correct dependency format, see:
https://github.com/aig787/agpm#transitive-dependencies",
                                file_path.display(),
                                e
                            );
                        }
                    }
                    None
                }
            }
        } else {
            None
        };

        Ok(ParsedFrontmatter {
            data: parsed_data,
            content: content_without_frontmatter,
            raw_frontmatter: raw_frontmatter_text,
            templated: was_templated,
        })
    }

    /// Simple parse without templating, just extract frontmatter and content.
    ///
    /// # Arguments
    /// * `content` - The content to parse
    ///
    /// # Returns
    /// * `ParsedFrontmatter<T>` - The parsed result with data and content
    pub fn parse<T>(&self, content: &str) -> Result<ParsedFrontmatter<T>>
    where
        T: DeserializeOwned,
    {
        let matter_result = self.yaml_matter.parse(content)?;

        let raw_frontmatter = matter_result
            .data
            .map(|data: serde_yaml::Value| serde_yaml::to_string(&data).unwrap_or_default());

        let content_without_frontmatter = matter_result.content;

        // Parse the data if frontmatter was present
        let parsed_data = if let Some(frontmatter) = raw_frontmatter.as_ref() {
            match serde_yaml::from_str::<T>(&frontmatter) {
                Ok(data) => Some(data),
                Err(e) => {
                    eprintln!(
                        "Warning: Unable to parse YAML frontmatter.

Parse error: {}

The document will be processed without metadata.",
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        Ok(ParsedFrontmatter {
            data: parsed_data,
            content: content_without_frontmatter,
            raw_frontmatter,
            templated: false,
        })
    }

    /// Check if content has frontmatter.
    ///
    /// # Arguments
    /// * `content` - The content to check
    ///
    /// # Returns
    /// * `bool` - True if frontmatter is present
    pub fn has_frontmatter(&self, content: &str) -> bool {
        // Use the raw_matter engine to check for frontmatter without YAML parsing
        if let Ok(result) = self.raw_matter.parse::<String>(content) {
            result.data.is_some()
        } else {
            false
        }
    }

    /// Extract just the content without frontmatter.
    ///
    /// # Arguments
    /// * `content` - The content to process
    ///
    /// # Returns
    /// * `String` - Content with frontmatter removed
    pub fn strip_frontmatter(&self, content: &str) -> String {
        // Use the raw_matter engine to strip frontmatter without YAML parsing
        self.raw_matter
            .parse::<String>(content)
            .map(|result| result.content)
            .unwrap_or_else(|_| content.to_string())
    }

    /// Extract just the raw frontmatter string.
    ///
    /// # Arguments
    /// * `content` - The content to process
    ///
    /// # Returns
    /// * `Option<String>` - Raw frontmatter as YAML string, if present
    pub fn extract_raw_frontmatter(&self, content: &str) -> Option<String> {
        // Use the RawFrontmatter engine to extract raw frontmatter text without YAML parsing
        match self.raw_matter.parse::<String>(content) {
            Ok(result) => {
                // The RawFrontmatter engine returns the raw frontmatter in the data field
                result.data.filter(|frontmatter_text| !frontmatter_text.is_empty())
            }
            Err(_) => None,
        }
    }

    /// Apply Tera templating to content.
    ///
    /// Always renders the content as a template to catch syntax errors.
    /// If variant_inputs is provided, it's used for template variables.
    /// Otherwise, renders with an empty context.
    ///
    /// # Arguments
    /// * `content` - The content to template
    /// * `variant_inputs` - Optional template variables (project, config, etc.)
    /// * `file_path` - Path to file for error reporting
    ///
    /// # Returns
    /// * `Result<String>` - Templated content or error
    pub fn apply_templating(
        &mut self,
        content: &str,
        variant_inputs: Option<&serde_json::Value>,
        file_path: &Path,
    ) -> Result<String> {
        if let Some(inputs) = variant_inputs {
            let context = FrontmatterTemplating::build_template_context_from_variant_inputs(inputs);
            self.template_renderer.render_template(content, &context, None).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to render frontmatter template in '{}': {}",
                    file_path.display(),
                    e
                )
            })
        } else {
            // Render with empty context to catch syntax errors
            let empty_context = TeraContext::new();
            self.template_renderer.render_template(content, &empty_context, None).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to render frontmatter template in '{}': {}",
                    file_path.display(),
                    e
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_project_config() -> ProjectConfig {
        let mut config_map = toml::map::Map::new();
        config_map.insert("name".to_string(), toml::Value::String("test-project".into()));
        config_map.insert("version".to_string(), toml::Value::String("1.0.0".into()));
        config_map.insert("language".to_string(), toml::Value::String("rust".into()));
        ProjectConfig::from(config_map)
    }

    #[test]
    fn test_frontmatter_templating_basic() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let _template_renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let project_config = create_test_project_config();
        let file_path = Path::new("test.md");

        // Convert ProjectConfig to JSON Value for variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // Test simple template variable substitution
        let content = "name: {{ project.name }}\nversion: {{ project.version }}";
        let mut parser = FrontmatterParser::new();
        let result = parser.apply_templating(content, Some(&variant_inputs_value), file_path);

        assert!(result.is_ok());
        let templated = result.unwrap();
        assert!(templated.contains("name: test-project"));
        assert!(templated.contains("version: 1.0.0"));
    }

    #[test]
    fn test_frontmatter_templating_no_template_syntax() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let _template_renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let project_config = create_test_project_config();
        let file_path = Path::new("test.md");

        // Convert ProjectConfig to JSON Value for variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // Test plain YAML without template syntax
        let content = "name: static\nversion: 1.0.0";
        let mut parser = FrontmatterParser::new();
        let result = parser.apply_templating(content, Some(&variant_inputs_value), file_path);

        assert!(result.is_ok());
        let templated = result.unwrap();
        assert_eq!(templated, content);
    }

    #[test]
    fn test_frontmatter_templating_template_error() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let _template_renderer = TemplateRenderer::new(true, project_dir.clone(), None).unwrap();
        let project_config = create_test_project_config();
        let file_path = Path::new("test.md");

        // Convert ProjectConfig to JSON Value for variant_inputs
        let mut variant_inputs = serde_json::Map::new();
        variant_inputs.insert("project".to_string(), project_config.to_json_value());
        let variant_inputs_value = serde_json::Value::Object(variant_inputs);

        // Test template with undefined variable
        let content = "name: {{ undefined_var }}";
        let mut parser = FrontmatterParser::new();
        let result = parser.apply_templating(content, Some(&variant_inputs_value), file_path);

        assert!(result.is_err());
    }

    #[test]
    fn test_frontmatter_parser_new() {
        let parser = FrontmatterParser::new();
        // Should not panic
        assert!(parser.has_frontmatter("---\nkey: value\n---\ncontent"));
        assert!(!parser.has_frontmatter("just content"));
    }

    #[test]
    fn test_frontmatter_parser_with_project_dir() {
        let temp_dir = TempDir::new().unwrap();
        let parser = FrontmatterParser::with_project_dir(temp_dir.path().to_path_buf());
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parsed_frontmatter_has_frontmatter() {
        let parsed = ParsedFrontmatter::<serde_yaml::Value> {
            data: None,
            content: "content".to_string(),
            raw_frontmatter: Some("key: value".to_string()),
            templated: false,
        };
        assert!(parsed.has_frontmatter());

        let parsed_no_fm = ParsedFrontmatter::<serde_yaml::Value> {
            data: None,
            content: "content".to_string(),
            raw_frontmatter: None,
            templated: false,
        };
        assert!(!parsed_no_fm.has_frontmatter());
    }
}
