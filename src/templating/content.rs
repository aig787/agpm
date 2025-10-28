//! Content extraction from resource files for template rendering.
//!
//! This module handles reading and processing resource files (Markdown, JSON, etc.)
//! to extract content for template rendering.

use std::path::PathBuf;
use std::sync::Arc;

/// Sentinel markers used to guard non-templated dependency content.
/// Content enclosed between these markers should be treated as literal text
/// and never passed through the templating engine.
pub(crate) const NON_TEMPLATED_LITERAL_GUARD_START: &str = "__AGPM_LITERAL_RAW_START__";
pub(crate) const NON_TEMPLATED_LITERAL_GUARD_END: &str = "__AGPM_LITERAL_RAW_END__";

/// Helper trait for content extraction methods.
///
/// This trait is implemented on `TemplateContextBuilder` to provide
/// content extraction functionality.
pub(crate) trait ContentExtractor {
    /// Get the cache instance
    fn cache(&self) -> &Arc<crate::cache::Cache>;

    /// Get the project directory
    fn project_dir(&self) -> &PathBuf;

    /// Extract and process content from a resource file.
    ///
    /// Reads the source file and processes it based on file type:
    /// - Markdown (.md): Strips YAML frontmatter, returns content only
    /// - JSON (.json): Removes metadata fields like `dependencies`
    /// - Other files: Returns raw content
    ///
    /// # Arguments
    ///
    /// * `resource` - The locked resource to extract content from
    ///
    /// # Returns
    ///
    /// Returns `Some(content)` if extraction succeeded, `None` on error (with warning logged)
    async fn extract_content(&self, resource: &crate::lockfile::LockedResource) -> Option<String> {
        tracing::debug!(
            "Attempting to extract content for resource '{}' (type: {:?})",
            resource.name,
            resource.resource_type
        );

        // Determine source path
        let source_path = if let Some(source_name) = &resource.source {
            let url = resource.url.as_ref()?;

            // Check if this is a local directory source
            let is_local_source = resource.resolved_commit.as_deref().is_none_or(str::is_empty);

            tracing::debug!(
                "Resource '{}': source='{}', url='{}', is_local={}",
                resource.name,
                source_name,
                url,
                is_local_source
            );

            if is_local_source {
                // Local directory source - use URL as path directly
                let path = std::path::PathBuf::from(url).join(&resource.path);
                tracing::debug!("Using local source path: {}", path.display());
                path
            } else {
                // Git-based source - get worktree path
                let sha = resource.resolved_commit.as_deref()?;

                tracing::debug!(
                    "Resource '{}': Getting worktree for SHA {}...",
                    resource.name,
                    &sha[..8.min(sha.len())]
                );

                // Use centralized worktree path construction
                let worktree_dir = match self.cache().get_worktree_path(url, sha) {
                    Ok(path) => {
                        tracing::debug!("Worktree path: {}", path.display());
                        path
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to construct worktree path for resource '{}': {}",
                            resource.name,
                            e
                        );
                        return None;
                    }
                };

                let full_path = worktree_dir.join(&resource.path);
                tracing::debug!(
                    "Full source path for '{}': {} (worktree exists: {})",
                    resource.name,
                    full_path.display(),
                    worktree_dir.exists()
                );
                full_path
            }
        } else {
            // Local file - path is relative to project or absolute
            let local_path = std::path::Path::new(&resource.path);
            let resolved_path = if local_path.is_absolute() {
                local_path.to_path_buf()
            } else {
                self.project_dir().join(local_path)
            };

            tracing::debug!(
                "Resource '{}': Using local file path: {}",
                resource.name,
                resolved_path.display()
            );

            resolved_path
        };

        // Read file content
        let content = match tokio::fs::read_to_string(&source_path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "Failed to read content for resource '{}' from {}: {}",
                    resource.name,
                    source_path.display(),
                    e
                );
                return None;
            }
        };

        // Process based on file type
        let processed_content = if resource.path.ends_with(".md") {
            // Markdown: strip frontmatter and guard non-templated content that contains template syntax
            match crate::markdown::MarkdownDocument::parse(&content) {
                Ok(doc) => {
                    let templating_enabled = is_markdown_templating_enabled(doc.metadata.as_ref());
                    let mut stripped_content = doc.content;

                    if !templating_enabled && content_contains_template_syntax(&stripped_content) {
                        tracing::debug!(
                            "Protecting non-templated markdown content for '{}'",
                            resource.name
                        );
                        stripped_content = wrap_content_in_literal_guard(stripped_content);
                    }

                    stripped_content
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse markdown for resource '{}': {}. Using raw content.",
                        resource.name,
                        e
                    );
                    content
                }
            }
        } else if resource.path.ends_with(".json") {
            // JSON: parse and remove metadata fields
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(mut json) => {
                    if let Some(obj) = json.as_object_mut() {
                        // Remove metadata fields that shouldn't be in embedded content
                        obj.remove("dependencies");
                    }
                    serde_json::to_string_pretty(&json).unwrap_or(content)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse JSON for resource '{}': {}. Using raw content.",
                        resource.name,
                        e
                    );
                    content
                }
            }
        } else {
            // Other files: use raw content
            content
        };

        Some(processed_content)
    }
}

/// Determine whether templating is explicitly enabled in Markdown frontmatter.
pub(crate) fn is_markdown_templating_enabled(
    metadata: Option<&crate::markdown::MarkdownMetadata>,
) -> bool {
    metadata
        .and_then(|md| md.extra.get("agpm"))
        .and_then(|agpm| agpm.as_object())
        .and_then(|agpm_obj| agpm_obj.get("templating"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// Detect if content contains Tera template syntax markers.
pub(crate) fn content_contains_template_syntax(content: &str) -> bool {
    content.contains("{{") || content.contains("{%") || content.contains("{#")
}

/// Wrap non-templated content in a literal fence so it renders safely without being evaluated.
pub(crate) fn wrap_content_in_literal_guard(content: String) -> String {
    let mut wrapped = String::with_capacity(
        content.len()
            + NON_TEMPLATED_LITERAL_GUARD_START.len()
            + NON_TEMPLATED_LITERAL_GUARD_END.len()
            + 2, // newline separators
    );

    wrapped.push_str(NON_TEMPLATED_LITERAL_GUARD_START);
    wrapped.push('\n');
    wrapped.push_str(&content);
    if !content.ends_with('\n') {
        wrapped.push('\n');
    }
    wrapped.push_str(NON_TEMPLATED_LITERAL_GUARD_END);

    wrapped
}
