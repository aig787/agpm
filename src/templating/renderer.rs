//! Template rendering engine with Tera.
//!
//! This module provides the TemplateRenderer struct that wraps Tera with
//! AGPM-specific configuration, custom filters, and literal block handling.

use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::PathBuf;
use tera::{Context as TeraContext, Tera};

use super::content::NON_TEMPLATED_LITERAL_GUARD_END;
use super::content::NON_TEMPLATED_LITERAL_GUARD_START;
use super::filters;

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
    /// The underlying Tera template engine
    tera: Tera,
    /// Whether templating is enabled globally
    enabled: bool,
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
        let mut tera = Tera::default();

        // Register custom filters
        tera.register_filter(
            "content",
            filters::create_content_filter(project_dir.clone(), max_content_file_size),
        );

        Ok(Self {
            tera,
            enabled,
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

        // Split content by lines to find both ```literal fences and RAW guards
        let mut in_literal_fence = false;
        let mut in_raw_guard = false;
        let mut current_block = String::new();
        let lines = content.lines();

        for line in lines {
            let trimmed = line.trim();

            if trimmed == NON_TEMPLATED_LITERAL_GUARD_START {
                // Start of RAW guard block
                in_raw_guard = true;
                current_block.clear();
                tracing::debug!("Found start of RAW guard block");
                // Skip the guard line
            } else if in_raw_guard && trimmed == NON_TEMPLATED_LITERAL_GUARD_END {
                // End of RAW guard block
                in_raw_guard = false;

                // Generate unique placeholder
                let placeholder_id = format!("__AGPM_LITERAL_BLOCK_{}__", counter);
                counter += 1;

                // Store original content (keep the guards for later processing)
                let guarded_content = format!(
                    "{}\n{}\n{}",
                    NON_TEMPLATED_LITERAL_GUARD_START,
                    current_block,
                    NON_TEMPLATED_LITERAL_GUARD_END
                );
                placeholders.insert(placeholder_id.clone(), guarded_content);

                // Insert placeholder
                result.push_str(&placeholder_id);
                result.push('\n');

                tracing::debug!(
                    "Protected RAW guard block with placeholder {} ({} bytes)",
                    placeholder_id,
                    current_block.len()
                );

                current_block.clear();
                // Skip the guard line
            } else if in_raw_guard {
                // Inside RAW guard - accumulate content
                if !current_block.is_empty() {
                    current_block.push('\n');
                }
                current_block.push_str(line);
            } else if trimmed.starts_with("```literal") {
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
        if in_raw_guard {
            tracing::warn!("Unclosed RAW guard found - treating as regular content");
            result.push_str(NON_TEMPLATED_LITERAL_GUARD_START);
            result.push('\n');
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
            if original_content.starts_with(NON_TEMPLATED_LITERAL_GUARD_START) {
                result = result.replace(&placeholder_id, &original_content);
            } else {
                // Wrap in markdown code fence for display
                let replacement = format!("```\n{}\n```", original_content);
                result = result.replace(&placeholder_id, &replacement);
            }

            tracing::debug!(
                "Restored literal block {} ({} bytes)",
                placeholder_id,
                original_content.len()
            );
        }

        result
    }

    /// Collapse literal fences that were injected to protect non-templated dependency content.
    ///
    /// Any block that starts with ```literal, contains the sentinel marker on its first line,
    /// and ends with ``` will be replaced by the inner content without the sentinel or fences.
    fn collapse_non_templated_literal_guards(content: String) -> String {
        let mut result = String::with_capacity(content.len());
        let mut in_guard = false;

        for chunk in content.split_inclusive('\n') {
            let trimmed = chunk.trim_end_matches(['\r', '\n']);

            if !in_guard {
                if trimmed == NON_TEMPLATED_LITERAL_GUARD_START {
                    in_guard = true;
                } else {
                    result.push_str(chunk);
                }
            } else if trimmed == NON_TEMPLATED_LITERAL_GUARD_END {
                in_guard = false;
            } else {
                result.push_str(chunk);
            }
        }

        // If guard never closed, re-append the start marker and captured content to avoid dropping data.
        if in_guard {
            result.push_str(NON_TEMPLATED_LITERAL_GUARD_START);
        }

        result
    }

    /// Render a Markdown template with the given context.
    ///
    /// This method supports recursive template rendering where project files
    /// can reference other project files using the `content` filter.
    /// Rendering continues up to [`filters::MAX_RENDER_DEPTH`] levels deep.
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
    /// ````
    ///
    /// This is useful for documentation that shows template syntax examples.
    ///
    /// # Recursive Rendering
    ///
    /// When a template contains `content` filter references, those files
    /// may themselves contain template syntax. The renderer automatically
    /// detects this and performs multiple rendering passes until either:
    /// - No template syntax remains in the output
    /// - Maximum depth is reached (error)
    ///
    /// Example recursive template chain:
    /// ```markdown
    /// # Main Agent
    /// {{ 'docs/guide.md' | content }}
    /// ```
    ///
    /// Where `docs/guide.md` contains:
    /// ```markdown
    /// # Guide
    /// {{ 'docs/common.md' | content }}
    /// ```
    ///
    /// This will render up to 10 levels deep.
    pub fn render_template(
        &mut self,
        template_content: &str,
        context: &TeraContext,
    ) -> Result<String> {
        tracing::debug!("render_template called, enabled={}", self.enabled);

        if !self.enabled {
            // If templating is disabled, return content as-is
            tracing::debug!("Templating disabled, returning content as-is");
            return Ok(template_content.to_string());
        }

        // Step 1: Protect literal blocks before any rendering
        let (protected_content, placeholders) = self.protect_literal_blocks(template_content);

        // Check if content contains template syntax (after protecting literals)
        if !self.contains_template_syntax(&protected_content) {
            // No template syntax found, restore literals and return
            tracing::debug!(
                "No template syntax found after protecting literals, returning content"
            );
            return Ok(self.restore_literal_blocks(&protected_content, placeholders));
        }

        // Log the template context for debugging
        tracing::debug!("Rendering template with context");
        Self::log_context_as_kv(context);

        // Step 2: Multi-pass rendering for recursive templates
        // This allows project files to reference other project files
        let mut current_content = protected_content;
        let mut depth = 0;
        let max_depth = filters::MAX_RENDER_DEPTH;

        let rendered = loop {
            depth += 1;

            // Check depth limit
            if depth > max_depth {
                bail!(
                    "Template rendering exceeded maximum recursion depth of {}. \
                     This usually indicates circular dependencies between project files. \
                     Please check your content filter references for cycles.",
                    max_depth
                );
            }

            tracing::debug!("Rendering pass {} of max {}", depth, max_depth);

            // Render the current content
            let rendered = self.tera.render_str(&current_content, context).map_err(|e| {
                // Extract detailed error information from Tera error
                let error_msg = Self::format_tera_error(&e);

                // Return just the error without verbose template context
                anyhow::Error::new(e).context(format!(
                    "Template rendering failed at depth {}:\n{}",
                    depth, error_msg
                ))
            })?;

            // Check if the rendered output still contains template syntax OUTSIDE code fences
            // This prevents re-rendering template syntax that was embedded as code examples
            if !self.contains_template_syntax_outside_fences(&rendered) {
                // No more template syntax outside fences - we're done with rendering
                tracing::debug!("Template rendering complete after {} pass(es)", depth);
                break rendered;
            }

            // More template syntax found outside fences - prepare for next iteration
            tracing::debug!("Template syntax detected in output, continuing to pass {}", depth + 1);
            current_content = rendered;
        };

        // Step 3: Restore literal blocks after all rendering is complete
        let restored = self.restore_literal_blocks(&rendered, placeholders);

        // Step 4: Collapse any literal guards that were added for non-templated dependencies
        Ok(Self::collapse_non_templated_literal_guards(restored))
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

    /// Check if content contains Tera template syntax.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the content contains template delimiters.
    pub(crate) fn contains_template_syntax(&self, content: &str) -> bool {
        let has_vars = content.contains("{{");
        let has_tags = content.contains("{%");
        let has_comments = content.contains("{#");
        let result = has_vars || has_tags || has_comments;
        tracing::debug!(
            "Template syntax check: vars={}, tags={}, comments={}, result={}",
            has_vars,
            has_tags,
            has_comments,
            result
        );
        result
    }

    /// Check if content contains template syntax outside of code fences.
    ///
    /// This is used after rendering to determine if another pass is needed.
    /// It ignores template syntax inside code fences to prevent re-rendering
    /// content that has already been processed (like embedded dependency content).
    pub(crate) fn contains_template_syntax_outside_fences(&self, content: &str) -> bool {
        let mut in_code_fence = false;
        let mut in_guard = 0usize;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed == NON_TEMPLATED_LITERAL_GUARD_START {
                in_guard = in_guard.saturating_add(1);
                continue;
            } else if trimmed == NON_TEMPLATED_LITERAL_GUARD_END {
                in_guard = in_guard.saturating_sub(1);
                continue;
            }

            if in_guard > 0 {
                continue;
            }

            // Track code fence boundaries
            if trimmed.starts_with("```") {
                in_code_fence = !in_code_fence;
                continue;
            }

            // Skip lines inside code fences
            if in_code_fence {
                continue;
            }

            // Check for template syntax in non-fenced content
            if line.contains("{{") || line.contains("{%") || line.contains("{#") {
                tracing::debug!(
                    "Template syntax found outside code fences: {:?}",
                    &line[..line.len().min(80)]
                );
                return true;
            }
        }

        tracing::debug!("No template syntax found outside code fences");
        false
    }
}
