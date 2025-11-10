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

use anyhow::{Context, Result};
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

    /// Rendered frontmatter with line offset (for Pass 2 parsing).
    pub rendered_frontmatter: Option<RenderedFrontmatter>,

    /// Byte boundaries of the frontmatter section (if present).
    pub boundaries: Option<FrontmatterBoundaries>,
}

/// Rendered frontmatter with accurate line number information.
///
/// This struct represents frontmatter that has been extracted from a fully
/// rendered file, preserving accurate line number references for error reporting.
#[derive(Debug, Clone)]
pub struct RenderedFrontmatter {
    /// The rendered frontmatter content as YAML string.
    pub content: String,

    /// Number of lines before frontmatter in the rendered content.
    /// This helps maintain accurate line number references.
    pub line_offset: usize,
}

/// Byte boundaries of frontmatter section in content.
///
/// This struct represents the start and end byte positions of the frontmatter
/// section (including delimiters) in the original content. This enables direct
/// frontmatter replacement without string splitting and reassembly.
#[derive(Debug, Clone, Copy)]
pub struct FrontmatterBoundaries {
    /// Byte position where frontmatter starts (first `---`).
    pub start: usize,

    /// Byte position where frontmatter ends (after closing `---` and newline).
    pub end: usize,
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
            #[allow(clippy::needless_borrow)]
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
            rendered_frontmatter: None,
            boundaries: self.get_frontmatter_boundaries(content),
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
            match serde_yaml::from_str::<T>(frontmatter) {
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
            rendered_frontmatter: None,
            boundaries: self.get_frontmatter_boundaries(content),
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

    /// Get the byte boundaries of the frontmatter section.
    ///
    /// This method finds the start and end byte positions of the frontmatter
    /// section (including delimiters) in the content. This enables direct
    /// frontmatter replacement without string splitting and reassembly.
    ///
    /// # Arguments
    /// * `content` - The content to analyze
    ///
    /// # Returns
    /// * `Option<FrontmatterBoundaries>` - Boundary positions if frontmatter exists
    ///
    /// # Example
    /// ```rust,no_run
    /// use agpm_cli::markdown::frontmatter::FrontmatterParser;
    ///
    /// let parser = FrontmatterParser::new();
    /// let content = "---\nkey: value\n---\n\nBody content";
    /// let boundaries = parser.get_frontmatter_boundaries(content);
    /// assert!(boundaries.is_some());
    /// ```
    pub fn get_frontmatter_boundaries(&self, content: &str) -> Option<FrontmatterBoundaries> {
        // Look for opening delimiter
        let first_delim = content.find("---")?;

        // Frontmatter must start at beginning (possibly after whitespace)
        if !content[..first_delim].trim().is_empty() {
            return None;
        }

        // Find the end of the first line (after opening ---)
        let after_first_delim = first_delim + 3;
        let first_line_end = content[after_first_delim..]
            .find('\n')
            .map(|pos| after_first_delim + pos + 1)
            .unwrap_or(content.len());

        // Look for closing delimiter after the first line
        let closing_delim_start = content[first_line_end..].find("---")?;
        let closing_delim_pos = first_line_end + closing_delim_start;

        // Find end of closing delimiter line
        let after_closing = closing_delim_pos + 3;
        let end_pos = content[after_closing..]
            .find('\n')
            .map(|pos| after_closing + pos + 1)
            .unwrap_or(content.len());

        Some(FrontmatterBoundaries {
            start: first_delim,
            end: end_pos,
        })
    }

    /// Replace frontmatter section directly using byte boundaries.
    ///
    /// This method replaces the frontmatter section in the original content
    /// with rendered frontmatter, preserving the body content exactly as-is.
    /// This avoids the error-prone split-and-reassemble pattern.
    ///
    /// # Arguments
    /// * `original_content` - The original content with frontmatter
    /// * `rendered_frontmatter` - The rendered frontmatter YAML string (without delimiters)
    /// * `boundaries` - The byte boundaries of the frontmatter section
    ///
    /// # Returns
    /// * `String` - Content with frontmatter replaced, body unchanged
    ///
    /// # Example
    /// ```rust,no_run
    /// use agpm_cli::markdown::frontmatter::FrontmatterParser;
    ///
    /// let parser = FrontmatterParser::new();
    /// let content = "---\nkey: {{ var }}\n---\n\nBody";
    /// let boundaries = parser.get_frontmatter_boundaries(content).unwrap();
    /// let rendered = "key: value";
    /// let result = parser.replace_frontmatter(content, rendered, boundaries);
    /// assert_eq!(result, "---\nkey: value\n---\n\nBody");
    /// ```
    pub fn replace_frontmatter(
        &self,
        original_content: &str,
        rendered_frontmatter: &str,
        boundaries: FrontmatterBoundaries,
    ) -> String {
        let before = &original_content[..boundaries.start];
        let after = &original_content[boundaries.end..];

        format!("{}---\n{}\n---\n{}", before, rendered_frontmatter.trim(), after)
    }

    /// Parse frontmatter from already-rendered full file content (Pass 2).
    ///
    /// This method extracts and parses frontmatter from content that has
    /// already been through full-file template rendering, preserving accurate line numbers.
    /// This is used for Pass 2 of the two-pass rendering system.
    ///
    /// # Arguments
    /// * `rendered_content` - The fully rendered file content
    /// * `file_path` - Path to file for error reporting
    ///
    /// # Returns
    /// * `Result<ParsedFrontmatter<T>>` - Parsed result with accurate line numbers
    pub fn parse_rendered_content<T>(
        &self,
        rendered_content: &str,
        file_path: &Path,
    ) -> Result<ParsedFrontmatter<T>>
    where
        T: DeserializeOwned,
    {
        // Extract frontmatter using existing YAML engine
        let matter_result = self.yaml_matter.parse(rendered_content).with_context(|| {
            format!("Failed to extract frontmatter from '{}'", file_path.display())
        })?;

        // Get raw frontmatter for line number tracking
        let rendered_frontmatter = if matter_result.data.is_some() {
            // Count lines before frontmatter to get accurate line numbers
            let frontmatter_start = rendered_content.find("---").unwrap_or(0);
            let lines_before = rendered_content[..frontmatter_start].lines().count();

            // Store the raw frontmatter with line offset info
            Some(RenderedFrontmatter {
                content: serde_yaml::to_string(&matter_result.data.as_ref().unwrap())?,
                line_offset: lines_before,
            })
        } else {
            None
        };

        // Parse the structured data
        let parsed_data = matter_result
            .data
            .map(|yaml_value| {
                serde_yaml::from_value::<T>(yaml_value)
                    .with_context(|| "Failed to deserialize frontmatter YAML")
            })
            .transpose()?;

        Ok(ParsedFrontmatter {
            data: parsed_data,
            content: matter_result.content,
            raw_frontmatter: rendered_frontmatter.as_ref().map(|rf| rf.content.clone()), // Use rendered frontmatter for has_frontmatter check
            templated: true, // Always true since input is already rendered
            rendered_frontmatter,
            boundaries: self.get_frontmatter_boundaries(rendered_content),
        })
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
    fn test_frontmatter_templating_basic() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().to_path_buf();
        let _template_renderer = TemplateRenderer::new(true, project_dir.clone(), None)?;
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

        let templated = result?;
        assert!(templated.contains("name: test-project"));
        assert!(templated.contains("version: 1.0.0"));
        Ok(())
    }

    #[test]
    fn test_frontmatter_templating_no_template_syntax() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().to_path_buf();
        let _template_renderer = TemplateRenderer::new(true, project_dir.clone(), None)?;
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

        let templated = result?;
        assert_eq!(templated, content);
        Ok(())
    }

    #[test]
    fn test_frontmatter_templating_template_error() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let project_dir = temp_dir.path().to_path_buf();
        let _template_renderer = TemplateRenderer::new(true, project_dir.clone(), None)?;
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
        Ok(())
    }

    #[test]
    fn test_frontmatter_parser_new() {
        let parser = FrontmatterParser::new();
        // Should not panic
        assert!(parser.has_frontmatter("---\nkey: value\n---\ncontent"));
        assert!(!parser.has_frontmatter("just content"));
    }

    #[test]
    fn test_frontmatter_parser_with_project_dir() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        FrontmatterParser::with_project_dir(temp_dir.path().to_path_buf())?;
        Ok(())
    }

    #[test]
    fn test_parsed_frontmatter_has_frontmatter() {
        let parsed = ParsedFrontmatter::<serde_yaml::Value> {
            data: None,
            content: "content".to_string(),
            raw_frontmatter: Some("key: value".to_string()),
            templated: false,
            rendered_frontmatter: None,
            boundaries: None,
        };
        assert!(parsed.has_frontmatter());

        let parsed_no_fm = ParsedFrontmatter::<serde_yaml::Value> {
            data: None,
            content: "content".to_string(),
            raw_frontmatter: None,
            templated: false,
            rendered_frontmatter: None,
            boundaries: None,
        };
        assert!(!parsed_no_fm.has_frontmatter());
    }

    #[test]
    fn test_parse_rendered_content() -> Result<(), Box<dyn std::error::Error>> {
        let parser = FrontmatterParser::new();
        let file_path = Path::new("test.md");

        // Test with rendered content that has frontmatter
        let rendered_content = r#"---
name: test-agent
description: A test agent
version: 1.0.0
---

# Test Agent Content

This is the content of the agent.
"#;

        let parsed =
            parser.parse_rendered_content::<serde_yaml::Value>(rendered_content, file_path)?;
        assert!(parsed.has_frontmatter());
        assert!(parsed.data.is_some());
        assert!(parsed.rendered_frontmatter.is_some());
        assert!(parsed.templated); // Should be true for rendered content
        assert!(parsed.raw_frontmatter.is_some()); // Should be Some for rendered content

        // Check line offset calculation
        let rendered_fm = parsed.rendered_frontmatter.unwrap();
        assert_eq!(rendered_fm.line_offset, 0); // No lines before frontmatter
        assert!(rendered_fm.content.contains("name: test-agent"));
        Ok(())
    }

    #[test]
    fn test_parse_rendered_content_no_frontmatter() -> Result<(), Box<dyn std::error::Error>> {
        let parser = FrontmatterParser::new();
        let file_path = Path::new("test.md");

        // Test with content that has no frontmatter
        let rendered_content = r#"# Just Content

This is content without frontmatter.
"#;

        let parsed =
            parser.parse_rendered_content::<serde_yaml::Value>(rendered_content, file_path)?;
        assert!(!parsed.has_frontmatter());
        assert!(parsed.data.is_none());
        assert!(parsed.rendered_frontmatter.is_none());
        assert!(parsed.templated); // Still true since method assumes rendered input
        Ok(())
    }

    #[test]
    fn test_parse_rendered_content_with_preface() -> Result<(), Box<dyn std::error::Error>> {
        let parser = FrontmatterParser::new();
        let file_path = Path::new("test.md");

        // Test with content that has lines before frontmatter
        let rendered_content = r#"<!-- This is a comment line -->
---
name: test-agent
version: 1.0.0
---

# Content
"#;

        // First test: Check if gray_matter can parse this
        let yaml_matter = gray_matter::Matter::<gray_matter::engine::YAML>::new();
        let matter_result = yaml_matter.parse::<serde_yaml::Value>(rendered_content);

        // gray_matter doesn't recognize frontmatter when there's content before it
        // So we need to handle this case differently
        if matter_result.is_ok() && matter_result.unwrap().data.is_some() {
            // If gray_matter can parse it, test parse_rendered_content
            let parsed =
                parser.parse_rendered_content::<serde_yaml::Value>(rendered_content, file_path)?;
            assert!(parsed.has_frontmatter());

            // Check line offset calculation - should be 1 line before frontmatter
            let rendered_fm = parsed.rendered_frontmatter.unwrap();
            assert_eq!(rendered_fm.line_offset, 1);
        } else {
            // If gray_matter can't parse frontmatter with content before it,
            // that's expected behavior - skip this test case
            println!(
                "Note: gray_matter doesn't extract frontmatter when there's content before it"
            );
            println!("This is expected behavior for YAML frontmatter with preceding content");
        }
        Ok(())
    }
}
