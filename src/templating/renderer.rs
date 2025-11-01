//! Template rendering engine with Tera.
//!
//! This module provides the TemplateRenderer struct that wraps Tera with
//! AGPM-specific configuration, custom filters, and literal block handling.

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use strsim::levenshtein;
use tera::{Context as TeraContext, Tera};

use super::error::{ErrorLocation, TemplateError};
use super::filters;
use crate::core::ResourceType;

/// Maximum allowed Levenshtein distance as a percentage of target length for suggestions.
/// This represents a 50% similarity threshold for variable name suggestions.
const SIMILARITY_THRESHOLD_PERCENT: usize = 50;

/// Context information about the current rendering operation
#[derive(Debug, Clone)]
pub struct RenderingMetadata {
    /// The resource currently being rendered
    pub resource_name: String,
    /// The type of resource (agent, command, snippet, etc.)
    pub resource_type: ResourceType,
    /// Full dependency chain from root to current resource
    pub dependency_chain: Vec<DependencyChainEntry>,
    /// Source file path if available
    pub source_path: Option<PathBuf>,
    /// Current rendering depth (for content filter recursion)
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct DependencyChainEntry {
    pub resource_type: ResourceType,
    pub name: String,
    pub path: Option<String>,
}

/// Template renderer with Tera engine and custom functions.
///
/// This struct wraps a Tera instance with AGPM-specific configuration,
/// custom functions, and filters. It provides a safe, sandboxed environment
/// for rendering Markdown templates.
///
/// # Security
///
/// The renderer is configured with security restrictions:
/// - No file system access via includes/extends (except content filter)
/// - No network access
/// - Sandboxed template execution
/// - Custom functions are carefully vetted
/// - Project file access restricted to project directory with validation
pub struct TemplateRenderer {
    /// Whether templating is enabled globally
    enabled: bool,
    /// Project directory for content filter validation
    project_dir: PathBuf,
    /// Maximum file size for content filter
    max_content_file_size: Option<u64>,
}

impl TemplateRenderer {
    /// Create a new template renderer with AGPM-specific configuration.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether templating is enabled globally
    /// * `project_dir` - Project root directory for content filter validation
    /// * `max_content_file_size` - Maximum file size in bytes for content filter (None for no limit)
    ///
    /// # Returns
    ///
    /// Returns a configured `TemplateRenderer` instance with custom filters registered.
    ///
    /// # Filters
    ///
    /// The following custom filters are registered:
    /// - `content`: Read project-specific files with path validation and size limits
    pub fn new(
        enabled: bool,
        project_dir: PathBuf,
        max_content_file_size: Option<u64>,
    ) -> Result<Self> {
        Ok(Self {
            enabled,
            project_dir,
            max_content_file_size,
        })
    }

    /// Protect literal blocks from template rendering by replacing them with placeholders.
    ///
    /// This method scans for ```literal fenced code blocks and replaces them with
    /// unique placeholders that won't be affected by template rendering. The original
    /// content is stored in a HashMap that can be used to restore the blocks later.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to process
    ///
    /// # Returns
    ///
    /// Returns a tuple of:
    /// - Modified content with placeholders instead of literal blocks
    /// - HashMap mapping placeholder IDs to original content
    ///
    /// # Examples
    ///
    /// ````markdown
    /// # Documentation Example
    ///
    /// Use this syntax in templates:
    ///
    /// ```literal
    /// {{ agpm.deps.snippets.example.content }}
    /// ```
    /// ````
    ///
    /// The content inside the literal block will be protected from rendering.
    pub(crate) fn protect_literal_blocks(
        &self,
        content: &str,
    ) -> (String, HashMap<String, String>) {
        let mut placeholders = HashMap::new();
        let mut counter = 0;
        let mut result = String::with_capacity(content.len());

        // Split content by lines to find ```literal fences
        let mut in_literal_fence = false;
        let mut current_block = String::new();
        let lines = content.lines();

        for line in lines {
            let trimmed = line.trim();

            if trimmed.starts_with("```literal") {
                // Start of ```literal fence
                in_literal_fence = true;
                current_block.clear();
                tracing::debug!("Found start of literal fence");
                // Skip the fence line
            } else if in_literal_fence && trimmed.starts_with("```") {
                // End of ```literal fence
                in_literal_fence = false;

                // Generate unique placeholder
                let placeholder_id = format!("__AGPM_LITERAL_BLOCK_{}__", counter);
                counter += 1;

                // Store original content
                placeholders.insert(placeholder_id.clone(), current_block.clone());

                // Insert placeholder
                result.push_str(&placeholder_id);
                result.push('\n');

                tracing::debug!(
                    "Protected literal fence with placeholder {} ({} bytes)",
                    placeholder_id,
                    current_block.len()
                );

                current_block.clear();
                // Skip the fence line
            } else if in_literal_fence {
                // Inside ```literal fence - accumulate content
                if !current_block.is_empty() {
                    current_block.push('\n');
                }
                current_block.push_str(line);
            } else {
                // Regular content - pass through
                result.push_str(line);
                result.push('\n');
            }
        }

        // Handle unclosed blocks (add back as-is)
        if in_literal_fence {
            tracing::warn!("Unclosed literal fence found - treating as regular content");
            result.push_str("```literal\n");
            result.push_str(&current_block);
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        tracing::debug!("Protected {} literal block(s)", placeholders.len());
        (result, placeholders)
    }

    /// Restore literal blocks by replacing placeholders with original content.
    ///
    /// This method takes rendered content and restores any literal blocks that were
    /// protected during the rendering process.
    ///
    /// # Arguments
    ///
    /// * `content` - The rendered content containing placeholders
    /// * `placeholders` - HashMap mapping placeholder IDs to original content
    ///
    /// # Returns
    ///
    /// Returns the content with placeholders replaced by original literal blocks,
    /// wrapped in markdown code fences for proper display.
    pub(crate) fn restore_literal_blocks(
        &self,
        content: &str,
        placeholders: HashMap<String, String>,
    ) -> String {
        let mut result = content.to_string();

        for (placeholder_id, original_content) in placeholders {
            // Wrap in markdown code fence for display
            let replacement = format!("```\n{}\n```", original_content);
            result = result.replace(&placeholder_id, &replacement);

            tracing::debug!(
                "Restored literal block {} ({} bytes)",
                placeholder_id,
                original_content.len()
            );
        }

        result
    }

    /// Render a Markdown template with the given context.
    ///
    /// This method supports recursive template rendering where project files
    /// can reference other project files using the `content` filter.
    /// Rendering continues up to [`filters::MAX_RENDER_DEPTH`] levels deep.
    ///
    /// Render a Markdown template with the given context.
    ///
    /// This method processes template syntax using the Tera engine. Content within
    /// ```literal fences is protected from rendering by replacing it with unique
    /// placeholders before processing, then restoring it afterwards.
    ///
    /// # Arguments
    ///
    /// * `template_content` - The raw Markdown template content
    /// * `context` - The template context containing variables
    ///
    /// # Returns
    ///
    /// Returns the rendered Markdown content.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Template syntax is invalid
    /// - Context variables are missing
    /// - Custom functions/filters fail
    /// - Recursive rendering exceeds maximum depth (10 levels)
    ///
    /// # Literal Blocks
    ///
    /// Content wrapped in ```literal fences will be protected from
    /// template rendering and displayed literally:
    ///
    /// ````markdown
    /// ```literal
    /// {{ agpm.deps.snippets.example.content }}
    /// ```
    /// ```ignore
    /// // This is a documentation example showing template syntax
    /// // The actual template content would be in a separate file
    ///
    /// This is useful for documentation that shows template syntax examples.
    ///
    /// # Recursive Rendering
    ///
    /// When a template contains 'content' filter references, those files
    /// may themselves contain template syntax. The renderer automatically
    /// detects this and performs multiple rendering passes until either:
    /// - No template syntax remains in the output
    /// - Maximum depth is reached (error)
    ///
    /// Example recursive template chain:
    /// // In a markdown file:
    /// // # Main Agent
    /// // {{ "docs/guide.md" | content }}
    ///
    /// Where 'docs/guide.md' contains:
    /// // # Guide
    /// // {{ "docs/common.md" | content }}
    ///
    /// This will render up to 10 levels deep.
    pub fn render_template(
        &mut self,
        template_content: &str,
        context: &TeraContext,
        metadata: Option<&RenderingMetadata>,
    ) -> Result<String, TemplateError> {
        tracing::debug!("render_template called, enabled={}", self.enabled);

        if !self.enabled {
            // If templating is disabled, return content as-is
            tracing::debug!("Templating disabled, returning content as-is");
            return Ok(template_content.to_string());
        }

        // Step 1: Protect literal blocks before any rendering
        let (protected_content, placeholders) = self.protect_literal_blocks(template_content);

        // Log the template context for debugging
        tracing::debug!("Rendering template with context");
        Self::log_context_as_kv(context);

        // Step 2: Single-pass rendering
        // Dependencies are pre-rendered with their own contexts before being embedded,
        // so parent templates only need one rendering pass. The content filter returns
        // literal content by default (no template processing).
        tracing::debug!("Rendering template (single pass)");

        // Create fresh Tera instance per render (very cheap - just empty HashMaps)
        // This enables future context-capturing filters without global state
        let mut tera = Tera::default();

        // Register content filter (currently returns literal content only)
        tera.register_filter(
            "content",
            filters::create_content_filter(self.project_dir.clone(), self.max_content_file_size),
        );

        let rendered = tera.render_str(&protected_content, context).map_err(|e| {
            // Parse into structured error
            Self::parse_tera_error(&e, &protected_content, context, metadata)
        })?;

        tracing::debug!("Template rendering complete");

        // Step 3: Restore literal blocks after rendering is complete
        let restored = self.restore_literal_blocks(&rendered, placeholders);

        // Return restored content (literal blocks have been restored)
        Ok(restored)
    }

    /// Parse a Tera error into a structured TemplateError
    fn parse_tera_error(
        error: &tera::Error,
        template_content: &str,
        context: &TeraContext,
        metadata: Option<&RenderingMetadata>,
    ) -> TemplateError {
        // Extract line number from Tera error (if available)
        let line_number = Self::extract_line_from_tera_error(error);

        // Extract context lines around the error
        let context_lines = if let Some(line) = line_number {
            let lines = Self::extract_context_lines(template_content, line, 5);
            if lines.is_empty() {
                None
            } else {
                Some(lines)
            }
        } else {
            None
        };

        // Try to extract more specific error information based on the error kind
        match &error.kind {
            tera::ErrorKind::Msg(msg) => {
                // Check if this is an undefined variable error in disguise
                if msg.contains("Variable") && msg.contains("not found") {
                    // Try to extract variable name
                    if let Some(name) = Self::extract_variable_name(msg) {
                        let available_variables = Self::extract_available_variables(context);
                        let suggestions = Self::find_similar_variables(&name, &available_variables);
                        return TemplateError::VariableNotFound {
                            variable: name.clone(),
                            available_variables: Box::new(available_variables),
                            suggestions: Box::new(suggestions),
                            location: Box::new(Self::build_error_location(
                                metadata,
                                line_number,
                                context_lines,
                            )),
                        };
                    }
                }

                // For other message types, use the format_tera_error function to clean them up
                TemplateError::SyntaxError {
                    message: Self::format_tera_error(error),
                    location: Box::new(Self::build_error_location(
                        metadata,
                        line_number,
                        context_lines,
                    )),
                }
            }
            _ => {
                // Fallback to syntax error with detailed error formatting
                TemplateError::SyntaxError {
                    message: Self::format_tera_error(error),
                    location: Box::new(Self::build_error_location(
                        metadata,
                        line_number,
                        context_lines,
                    )),
                }
            }
        }
    }

    /// Extract variable name from "Variable `foo` not found" message
    fn extract_variable_name(error_msg: &str) -> Option<String> {
        // Pattern: "Variable `<name>` not found"
        let re = Regex::new(r"Variable `([^`]+)` not found").ok()?;
        if let Some(caps) = re.captures(error_msg) {
            if let Some(m) = caps.get(1) {
                return Some(m.as_str().to_string());
            }
        }

        // Try other patterns if needed
        // Pattern: "Unknown variable `foo`"
        let re2 = Regex::new(r"Unknown variable `([^`]+)`").ok()?;
        if let Some(caps) = re2.captures(error_msg) {
            if let Some(m) = caps.get(1) {
                return Some(m.as_str().to_string());
            }
        }

        None
    }

    /// Extract available variables from Tera context
    fn extract_available_variables(context: &TeraContext) -> Vec<String> {
        // Tera context doesn't implement Serialize directly
        // We need to access its internal data structure
        let mut vars = Vec::new();

        // Get the context as a Value by using Tera's internal data access
        // TeraContext stores data as a tera::Value internally
        if let Some(_data) = context.get("agpm") {
            // This is a simplified version - we should walk the actual structure
            vars.push("agpm.resource.name".to_string());
            vars.push("agpm.resource.path".to_string());
            vars.push("agpm.resource.install_path".to_string());
        }

        // Add common project variables if they exist
        if context.contains_key("project") {
            vars.push("project.language".to_string());
            vars.push("project.framework".to_string());
        }

        // Add dependency variables (simplified check)
        if context.contains_key("deps") {
            vars.push("agpm.deps.*".to_string());
        }

        vars
    }

    /// Find similar variable names using Levenshtein distance
    fn find_similar_variables(target: &str, available: &[String]) -> Vec<String> {
        let mut scored: Vec<_> = available
            .iter()
            .map(|var| {
                let distance = levenshtein(target, var);
                (var.clone(), distance)
            })
            .collect();

        // Sort by distance (closest first)
        scored.sort_by_key(|(_, dist)| *dist);

        // Return top 3 suggestions within reasonable distance
        scored
            .into_iter()
            .filter(|(_, dist)| *dist <= target.len() * SIMILARITY_THRESHOLD_PERCENT / 100)
            .take(3)
            .map(|(var, _)| var)
            .collect()
    }

    /// Extract context lines around an error location
    ///
    /// Returns up to `context_size` lines before and after the error line,
    /// along with their line numbers (1-indexed).
    fn extract_context_lines(
        content: &str,
        error_line: usize,
        context_size: usize,
    ) -> Vec<(usize, String)> {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Tera uses 1-indexed line numbers
        if error_line == 0 || error_line > total_lines {
            return Vec::new();
        }

        // Calculate range (convert to 0-indexed for array access)
        let start = error_line.saturating_sub(context_size + 1);
        let end = (error_line + context_size).min(total_lines);

        // Extract lines with their line numbers (1-indexed for display)
        lines[start..end]
            .iter()
            .enumerate()
            .map(|(idx, line)| (start + idx + 1, line.to_string()))
            .collect()
    }

    /// Extract line number from Tera error message
    ///
    /// Tera includes line:column information in parse error messages.
    /// Examples: "1:7", "15:23", "864:1"
    fn extract_line_from_tera_error(error: &tera::Error) -> Option<usize> {
        let error_msg = format!("{:?}", error);

        // Look for pattern like "1:7" or "864:1" in the error message
        let re = Regex::new(r"(\d+):(\d+)").ok()?;
        if let Some(caps) = re.captures(&error_msg) {
            if let Some(line_str) = caps.get(1) {
                return line_str.as_str().parse::<usize>().ok();
            }
        }
        None
    }

    /// Build ErrorLocation from metadata
    fn build_error_location(
        metadata: Option<&RenderingMetadata>,
        line_number: Option<usize>,
        context_lines: Option<Vec<(usize, String)>>,
    ) -> ErrorLocation {
        let meta = metadata.cloned().unwrap_or_else(|| RenderingMetadata {
            resource_name: "unknown".to_string(),
            resource_type: ResourceType::Snippet, // Default to snippet
            dependency_chain: vec![],
            source_path: None,
            depth: 0,
        });

        ErrorLocation {
            resource_name: meta.resource_name,
            resource_type: meta.resource_type,
            dependency_chain: meta.dependency_chain,
            file_path: meta.source_path,
            line_number,
            context_lines,
        }
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
    pub fn format_tera_error(error: &tera::Error) -> String {
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

    /// Format the template context as a string for error messages.
    ///
    /// # Arguments
    ///
    /// * `context` - The Tera context to format
    fn format_context_as_string(context: &TeraContext) -> String {
        let context_clone = context.clone();
        let json_value = context_clone.into_json();
        let mut output = String::new();

        // Recursively format the JSON structure with indentation
        fn format_value(key: &str, value: &serde_json::Value, indent: usize) -> Vec<String> {
            let prefix = "  ".repeat(indent);
            let mut lines = Vec::new();

            match value {
                serde_json::Value::Object(map) => {
                    lines.push(format!("{}{}:", prefix, key));
                    for (k, v) in map {
                        lines.extend(format_value(k, v, indent + 1));
                    }
                }
                serde_json::Value::Array(arr) => {
                    lines.push(format!("{}{}: [{} items]", prefix, key, arr.len()));
                    // Only show first few items to avoid spam
                    for (i, item) in arr.iter().take(3).enumerate() {
                        lines.extend(format_value(&format!("[{}]", i), item, indent + 1));
                    }
                    if arr.len() > 3 {
                        lines.push(format!("{}  ... {} more items", prefix, arr.len() - 3));
                    }
                }
                serde_json::Value::String(s) => {
                    // Truncate long strings
                    if s.len() > 100 {
                        lines.push(format!(
                            "{}{}: \"{}...\" ({} chars)",
                            prefix,
                            key,
                            &s[..97],
                            s.len()
                        ));
                    } else {
                        lines.push(format!("{}{}: \"{}\"", prefix, key, s));
                    }
                }
                serde_json::Value::Number(n) => {
                    lines.push(format!("{}{}: {}", prefix, key, n));
                }
                serde_json::Value::Bool(b) => {
                    lines.push(format!("{}{}: {}", prefix, key, b));
                }
                serde_json::Value::Null => {
                    lines.push(format!("{}{}: null", prefix, key));
                }
            }
            lines
        }

        if let serde_json::Value::Object(map) = &json_value {
            for (key, value) in map {
                output.push_str(&format_value(key, value, 1).join("\n"));
                output.push('\n');
            }
        }

        output
    }

    /// Log the template context as key-value pairs at debug level.
    ///
    /// # Arguments
    ///
    /// * `context` - The Tera context to log
    fn log_context_as_kv(context: &TeraContext) {
        let formatted = Self::format_context_as_string(context);
        for line in formatted.lines() {
            tracing::debug!("{}", line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tera_embedding_preserves_template_syntax() {
        // This test verifies that when we embed content containing template syntax
        // (like {{ }}) as a STRING VALUE in a template context, Tera treats it as
        // literal text and does NOT try to process it.
        //
        // This is critical for our guard collapsing logic: when we collapse guards
        // and add the content to the parent's context, any {{ }} in that content
        // should remain as literal text in the final output.

        let mut tera = Tera::default();

        // Simulate a dependency's content that contains template syntax
        let dependency_content = "Value is {{ some.variable }} here";

        // Create a parent template that embeds the dependency
        let parent_template = r#"
# Parent Document

Embedded content below:
{{ deps.foo.content }}

Done.
"#;

        // Add template to Tera
        tera.add_raw_template("parent", parent_template).unwrap();

        // Create context with the dependency content as a STRING
        let mut context = TeraContext::new();
        context.insert(
            "deps",
            &serde_json::json!({
                "foo": {
                    "content": dependency_content
                }
            }),
        );

        // Render the parent
        let result = tera.render("parent", &context).unwrap();

        println!("Rendered output:\n{}", result);
        println!(
            "\nDoes it contain literal '{{{{ some.variable }}}}'? {}",
            result.contains("{{ some.variable }}")
        );

        // THE KEY ASSERTION: The template syntax should be preserved as literal text
        assert!(
            result.contains("{{ some.variable }}"),
            "Template syntax should be preserved as literal text when embedded as a string value.\n\
            This test failing means Tera tried to process the {{ }} syntax, which would break our guard collapsing.\n\
            Rendered output:\n{}",
            result
        );
    }
}
