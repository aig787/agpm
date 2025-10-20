//! Custom Tera filters for AGPM templates.
//!
//! This module provides template filters that extend Tera's functionality for
//! AGPM-specific use cases, such as reading project files, content manipulation,
//! and other template operations.
//!
//! # Security
//!
//! All file access is restricted to the project directory with the following protections:
//! - Only relative paths are allowed (no absolute paths)
//! - Directory traversal outside project root is prevented
//! - Only text file types are permitted (.md, .txt, .json, .toml, .yaml)
//! - Missing files produce hard errors to fail fast
//!
//! # Supported File Types
//!
//! - **Markdown (.md)**: YAML/TOML frontmatter is automatically stripped
//! - **JSON (.json)**: Parsed and pretty-printed
//! - **Text (.txt)**: Raw content
//! - **TOML (.toml)**: Raw content
//! - **YAML (.yaml, .yml)**: Raw content
//!
//! # Examples
//!
//! ## Basic File Reading
//!
//! ```markdown
//! ---
//! agpm.templating: true
//! ---
//! # Code Review Agent
//!
//! ## Style Guide
//! {{ 'project/styleguide.md' | content }}
//!
//! ## Best Practices
//! {{ 'docs/best-practices.txt' | content }}
//! ```
//!
//! ## Combining with Dependency Content Embedding
//!
//! Use both `content` filter and dependency `.content` fields together:
//!
//! ```markdown
//! ---
//! agpm.templating: true
//! dependencies:
//!   snippets:
//!     - path: snippets/rust-patterns.md
//!       name: rust_patterns
//! ---
//! # Rust Code Reviewer
//!
//! ## Shared Rust Patterns (versioned, from AGPM)
//! {{ agpm.deps.snippets.rust_patterns.content }}
//!
//! ## Project-Specific Style Guide (local)
//! {{ 'project/rust-style.md' | content }}
//! ```
//!
//! **When to use each**:
//! - **`agpm.deps.<type>.<name>.content`**: Versioned content from AGPM repositories
//! - **`content` filter**: Project-local files (team docs, company standards)
//!
//! ## Recursive Templates
//!
//! Project files can themselves contain template syntax:
//!
//! **project/styleguide.md**:
//! ```markdown
//! # Coding Standards
//!
//! ## Rust-Specific Rules
//! {{ 'project/rust-style.md' | content }}
//!
//! ## Common Guidelines
//! {{ 'project/common-style.md' | content }}
//! ```
//!
//! The template system will render up to 10 levels of nested references.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// Allowed file extensions for project file access.
///
/// Only text-based formats are permitted to prevent binary file inclusion
/// and ensure content can be safely embedded in templates.
const ALLOWED_EXTENSIONS: &[&str] = &["md", "txt", "json", "toml", "yaml", "yml"];

/// Maximum nesting depth for recursive template rendering.
///
/// This prevents infinite loops and excessive memory usage when files
/// reference each other cyclically or create deep nesting chains.
pub const MAX_RENDER_DEPTH: usize = 10;

/// Validates a file path for security and correctness.
///
/// This function ensures that:
/// 1. The path is relative (not absolute)
/// 2. The path doesn't traverse outside the project directory using `..`
/// 3. The file extension is in the allowed list
/// 4. The file exists and is readable
/// 5. The file size doesn't exceed the maximum allowed
///
/// # Arguments
///
/// * `path_str` - The path string from the template
/// * `project_dir` - The project root directory
/// * `max_size` - Maximum file size in bytes (None for no limit)
///
/// # Returns
///
/// Returns the canonicalized absolute path to the file if all checks pass.
///
/// # Errors
///
/// Returns an error if:
/// - Path is absolute
/// - Path contains `..` components that escape project directory
/// - File extension is not in the allowed list
/// - File doesn't exist
/// - File is not accessible (permissions, etc.)
/// - File size exceeds the maximum allowed
///
/// # Security
///
/// This function is critical for preventing directory traversal attacks.
/// It validates paths before any file system access occurs.
///
/// # Examples
///
/// ```rust,no_run
/// # use std::path::Path;
/// # use agpm_cli::templating::filters::validate_content_path;
/// # fn example() -> anyhow::Result<()> {
/// let project_dir = Path::new("/home/user/project");
///
/// // Valid relative path with no size limit
/// let path = validate_content_path("docs/guide.md", project_dir, None)?;
///
/// // With size limit (1 MB)
/// let path = validate_content_path("docs/guide.md", project_dir, Some(1024 * 1024))?;
///
/// // Invalid: absolute path
/// let result = validate_content_path("/etc/passwd", project_dir, None);
/// assert!(result.is_err());
///
/// // Invalid: directory traversal
/// let result = validate_content_path("../../etc/passwd", project_dir, None);
/// assert!(result.is_err());
///
/// // Invalid: wrong extension
/// let result = validate_content_path("script.sh", project_dir, None);
/// assert!(result.is_err());
/// # Ok(())
/// # }
/// ```
pub fn validate_content_path(
    path_str: &str,
    project_dir: &Path,
    max_size: Option<u64>,
) -> Result<PathBuf> {
    // Parse the path
    let path = Path::new(path_str);

    // Reject absolute paths
    if path.is_absolute() {
        bail!(
            "Absolute paths are not allowed in content filter. \
             Path '{}' must be relative to project root.",
            path_str
        );
    }

    // Check for directory traversal attempts
    // We need to resolve the path and ensure it stays within project_dir
    let mut components_count: i32 = 0;
    for component in path.components() {
        match component {
            Component::Normal(_) => components_count += 1,
            Component::ParentDir => {
                components_count -= 1;
                // If we go negative, we're trying to escape the project directory
                if components_count < 0 {
                    bail!(
                        "Path traversal outside project directory is not allowed. \
                         Path '{}' attempts to access parent directories beyond project root.",
                        path_str
                    );
                }
            }
            Component::CurDir => {
                // `.` is fine, just ignore it
            }
            _ => {
                // Prefix, RootDir shouldn't appear in relative paths
                bail!("Invalid path component in '{}'. Only relative paths are allowed.", path_str);
            }
        }
    }

    // Validate file extension
    let extension = path.extension().and_then(|ext| ext.to_str()).ok_or_else(|| {
        anyhow::anyhow!(
            "File '{}' has no extension. Allowed extensions: {}",
            path_str,
            ALLOWED_EXTENSIONS.join(", ")
        )
    })?;

    let extension_lower = extension.to_lowercase();
    if !ALLOWED_EXTENSIONS.contains(&extension_lower.as_str()) {
        bail!(
            "File extension '.{}' is not allowed. \
             Allowed extensions: {}. \
             Path: '{}'",
            extension,
            ALLOWED_EXTENSIONS.join(", "),
            path_str
        );
    }

    // Construct full path relative to project directory
    let full_path = project_dir.join(path);

    // Check if file exists
    if !full_path.exists() {
        bail!(
            "File not found: '{}'. \
             The content filter requires files to exist. \
             Full path attempted: {}",
            path_str,
            full_path.display()
        );
    }

    // Check if it's a regular file (not a directory or symlink)
    if !full_path.is_file() {
        bail!(
            "Path '{}' is not a regular file. \
             The content filter only works with files, not directories or special files.",
            path_str
        );
    }

    // Canonicalize to get absolute path and verify it's still within project_dir
    let canonical_path = full_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize path: {}", full_path.display()))?;

    let canonical_project = project_dir.canonicalize().with_context(|| {
        format!("Failed to canonicalize project directory: {}", project_dir.display())
    })?;

    // Final security check: ensure canonical path is within project directory
    if !canonical_path.starts_with(&canonical_project) {
        bail!(
            "Security violation: Path '{}' resolves to '{}' which is outside project directory '{}'",
            path_str,
            canonical_path.display(),
            canonical_project.display()
        );
    }

    // Check file size if limit is specified
    if let Some(max_bytes) = max_size {
        let metadata = canonical_path.metadata().with_context(|| {
            format!("Failed to read file metadata: {}", canonical_path.display())
        })?;

        let file_size = metadata.len();
        if file_size > max_bytes {
            bail!(
                "File '{}' is too large ({} bytes). Maximum allowed size: {} bytes ({:.2} MB vs {:.2} MB limit).",
                path_str,
                file_size,
                max_bytes,
                file_size as f64 / (1024.0 * 1024.0),
                max_bytes as f64 / (1024.0 * 1024.0)
            );
        }
    }

    Ok(canonical_path)
}

/// Reads and processes a project file based on its type.
///
/// This function handles different file types appropriately:
/// - Markdown: Strips YAML/TOML frontmatter
/// - JSON: Parses and pretty-prints
/// - Other text files: Returns raw content
///
/// # Arguments
///
/// * `file_path` - Validated absolute path to the file
///
/// # Returns
///
/// Returns the processed file content as a string.
///
/// # Errors
///
/// Returns an error if:
/// - File cannot be read (I/O error)
/// - File contains invalid UTF-8
/// - JSON file has invalid syntax
/// - Markdown frontmatter is malformed
///
/// # Examples
///
/// ```rust,no_run
/// # use std::path::Path;
/// # use agpm_cli::templating::filters::read_and_process_content;
/// # fn example() -> anyhow::Result<()> {
/// let path = Path::new("/home/user/project/docs/guide.md");
/// let content = read_and_process_content(path)?;
/// println!("{}", content);
/// # Ok(())
/// # }
/// ```
pub fn read_and_process_content(file_path: &Path) -> Result<String> {
    // Read file content
    let content = std::fs::read_to_string(file_path).with_context(|| {
        format!(
            "Failed to read project file: {}. \
             Ensure the file is readable and contains valid UTF-8.",
            file_path.display()
        )
    })?;

    // Process based on file extension
    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let processed_content = match extension.as_str() {
        "md" => {
            // Markdown: strip frontmatter
            match crate::markdown::MarkdownDocument::parse(&content) {
                Ok(doc) => doc.content,
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse markdown file '{}': {}. Using raw content.",
                        file_path.display(),
                        e
                    );
                    content
                }
            }
        }
        "json" => {
            // JSON: parse and pretty-print
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => serde_json::to_string_pretty(&json).unwrap_or(content),
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse JSON file '{}': {}. Using raw content.",
                        file_path.display(),
                        e
                    );
                    content
                }
            }
        }
        _ => {
            // Text, TOML, YAML: return raw content
            content
        }
    };

    Ok(processed_content)
}

/// Creates a Tera filter function for reading and embedding file content.
///
/// This function returns a closure that can be registered as a Tera filter.
/// The closure captures the project directory and uses it to validate and
/// read files during template rendering.
///
/// # Arguments
///
/// * `project_dir` - The project root directory for path validation
///
/// # Returns
///
/// Returns a boxed closure compatible with Tera's filter registration API.
///
/// # Filter Usage
///
/// In templates, use the filter with a string value containing the relative path:
///
/// ```markdown
/// {{ 'docs/styleguide.md' | content }}
/// ```
///
/// # Errors
///
/// The returned filter will produce template rendering errors if:
/// - The input value is not a string
/// - Path validation fails (absolute path, traversal, invalid extension, etc.)
/// - File cannot be read or processed
///
/// # Examples
///
/// ```rust,no_run
/// # use std::path::Path;
/// # use agpm_cli::templating::filters::create_content_filter;
/// # fn example() -> anyhow::Result<()> {
/// let project_dir = Path::new("/home/user/project");
/// let max_size = Some(10 * 1024 * 1024); // 10 MB limit
/// let filter = create_content_filter(project_dir.to_path_buf(), max_size);
///
/// // Filter is registered in Tera:
/// // tera.register_filter("content", filter);
/// # Ok(())
/// # }
/// ```
pub fn create_content_filter(
    project_dir: PathBuf,
    max_size: Option<u64>,
) -> impl tera::Filter + 'static {
    move |value: &tera::Value, _args: &HashMap<String, tera::Value>| -> tera::Result<tera::Value> {
        // Extract path string from filter input
        let path_str = value
            .as_str()
            .ok_or_else(|| tera::Error::msg("content filter requires a string path"))?;

        // Validate and read the file
        let file_path = validate_content_path(path_str, &project_dir, max_size)
            .map_err(|e| tera::Error::msg(format!("content filter error: {}", e)))?;

        let content = read_and_process_content(&file_path)
            .map_err(|e| tera::Error::msg(format!("content filter error: {}", e)))?;

        // Return content as string value
        Ok(tera::Value::String(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_project() -> TempDir {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        // Create directory structure
        fs::create_dir_all(project_dir.join("docs")).unwrap();
        fs::create_dir_all(project_dir.join("project")).unwrap();

        // Create test files
        fs::write(project_dir.join("docs/guide.md"), "# Guide\n\nContent here").unwrap();
        fs::write(project_dir.join("docs/notes.txt"), "Plain text notes").unwrap();
        fs::write(project_dir.join("project/config.json"), r#"{"key": "value"}"#).unwrap();

        // Create markdown with frontmatter
        fs::write(
            project_dir.join("docs/with-frontmatter.md"),
            "---\ntitle: Test\n---\n\n# Content",
        )
        .unwrap();

        temp
    }

    #[test]
    fn test_validate_valid_path() {
        let temp = create_test_project();
        let project_dir = temp.path();

        let result = validate_content_path("docs/guide.md", project_dir, None);
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.ends_with("docs/guide.md"));
        assert!(path.is_absolute());
    }

    #[test]
    fn test_validate_rejects_absolute_path() {
        let temp = create_test_project();
        let project_dir = temp.path();

        let result = validate_content_path("/etc/passwd", project_dir, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Absolute paths"));
    }

    #[test]
    fn test_validate_rejects_traversal() {
        let temp = create_test_project();
        let project_dir = temp.path();

        let result = validate_content_path("../../etc/passwd", project_dir, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[test]
    fn test_validate_rejects_invalid_extension() {
        let temp = create_test_project();
        let project_dir = temp.path();

        // Create a .sh file
        fs::write(project_dir.join("script.sh"), "#!/bin/bash").unwrap();

        let result = validate_content_path("script.sh", project_dir, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    #[test]
    fn test_validate_rejects_missing_file() {
        let temp = create_test_project();
        let project_dir = temp.path();

        let result = validate_content_path("docs/missing.md", project_dir, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_validate_rejects_file_too_large() {
        let temp = create_test_project();
        let project_dir = temp.path();

        // Create a file with known size (1000 bytes)
        let large_file = project_dir.join("large.md");
        fs::write(&large_file, "a".repeat(1000)).unwrap();

        // Should succeed with larger limit
        let result = validate_content_path("large.md", project_dir, Some(1001));
        assert!(result.is_ok());

        // Should fail with smaller limit
        let result = validate_content_path("large.md", project_dir, Some(999));
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("too large"));
        assert!(err_msg.contains("1000 bytes"));
        assert!(err_msg.contains("999 bytes"));
    }

    #[test]
    fn test_read_markdown_strips_frontmatter() {
        let temp = create_test_project();
        let project_dir = temp.path();

        let path = project_dir.join("docs/with-frontmatter.md");
        let content = read_and_process_content(&path).unwrap();

        assert!(!content.contains("---"));
        assert!(!content.contains("title: Test"));
        assert!(content.contains("# Content"));
    }

    #[test]
    fn test_read_json_pretty_prints() {
        let temp = create_test_project();
        let project_dir = temp.path();

        let path = project_dir.join("project/config.json");
        let content = read_and_process_content(&path).unwrap();

        // Should be pretty-printed (contains newlines)
        assert!(content.contains('\n'));
        assert!(content.contains("\"key\""));
        assert!(content.contains("\"value\""));
    }

    #[test]
    fn test_read_text_returns_raw() {
        let temp = create_test_project();
        let project_dir = temp.path();

        let path = project_dir.join("docs/notes.txt");
        let content = read_and_process_content(&path).unwrap();

        assert_eq!(content, "Plain text notes");
    }

    #[test]
    fn test_filter_function() {
        use tera::Tera;

        let temp = create_test_project();
        let project_dir = temp.path().to_path_buf();

        // Register the filter in a Tera instance
        let mut tera = Tera::default();
        tera.register_filter("content", create_content_filter(project_dir, None));

        // Test with valid path using Tera's template rendering
        let template = r#"{{ 'docs/guide.md' | content }}"#;
        let context = tera::Context::new();

        let result = tera.render_str(template, &context);
        assert!(result.is_ok(), "Filter should render successfully");

        let content = result.unwrap();
        assert!(content.contains("# Guide"));
        assert!(content.contains("Content here"));
    }

    #[test]
    fn test_filter_rejects_non_string() {
        use tera::Tera;

        let temp = create_test_project();
        let project_dir = temp.path().to_path_buf();

        // Register the filter in a Tera instance
        let mut tera = Tera::default();
        tera.register_filter("content", create_content_filter(project_dir, None));

        // Test with number instead of string (this will be caught at template render time)
        let template = r#"{{ 42 | content }}"#;
        let context = tera::Context::new();

        let result = tera.render_str(template, &context);
        // The important thing is that it fails - Tera may wrap our error message
        assert!(result.is_err(), "Filter should reject non-string values");
    }

    #[test]
    fn test_recursive_template_rendering() {
        // This test is in the templating module tests
        // See test_recursive_content_rendering in mod.rs
    }
}

// Integration tests for recursive rendering
#[cfg(test)]
mod recursive_tests {
    use std::fs;

    #[test]
    fn test_two_level_recursion() {
        use crate::templating::TemplateRenderer;
        use tempfile::TempDir;
        use tera::Context;

        // Create test files with recursive references
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        fs::create_dir_all(project_dir.join("docs")).unwrap();

        // Level 1: References level 2
        fs::write(
            project_dir.join("docs/level1.md"),
            "# Level 1\n{{ 'docs/level2.md' | content }}",
        )
        .unwrap();

        // Level 2: Final content (no more references)
        fs::write(project_dir.join("docs/level2.md"), "Content from level 2").unwrap();

        // Create renderer and render
        let mut renderer = TemplateRenderer::new(true, project_dir.to_path_buf(), None).unwrap();
        let context = Context::new();

        let template = "{{ 'docs/level1.md' | content }}";
        let result = renderer.render_template(template, &context);

        assert!(result.is_ok(), "Two-level recursion should succeed");
        let content = result.unwrap();
        assert!(content.contains("# Level 1"));
        assert!(content.contains("Content from level 2"));
        assert!(!content.contains("{{"), "No template syntax should remain");
    }

    #[test]
    fn test_three_level_recursion() {
        use crate::templating::TemplateRenderer;
        use tempfile::TempDir;
        use tera::Context;

        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        fs::create_dir_all(project_dir.join("docs")).unwrap();

        // Level 1 → Level 2 → Level 3
        fs::write(project_dir.join("docs/level1.md"), "L1: {{ 'docs/level2.md' | content }}")
            .unwrap();

        fs::write(project_dir.join("docs/level2.md"), "L2: {{ 'docs/level3.md' | content }}")
            .unwrap();

        fs::write(project_dir.join("docs/level3.md"), "L3: Final").unwrap();

        let mut renderer = TemplateRenderer::new(true, project_dir.to_path_buf(), None).unwrap();
        let context = Context::new();

        let template = "{{ 'docs/level1.md' | content }}";
        let result = renderer.render_template(template, &context);

        assert!(result.is_ok(), "Three-level recursion should succeed");
        let content = result.unwrap();
        assert!(content.contains("L1:"));
        assert!(content.contains("L2:"));
        assert!(content.contains("L3: Final"));
    }

    #[test]
    fn test_depth_limit_exceeded() {
        use crate::templating::TemplateRenderer;
        use tempfile::TempDir;
        use tera::Context;

        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        fs::create_dir_all(project_dir.join("docs")).unwrap();

        // Create a file that references itself (infinite loop)
        fs::write(project_dir.join("docs/loop.md"), "Loop: {{ 'docs/loop.md' | content }}")
            .unwrap();

        let mut renderer = TemplateRenderer::new(true, project_dir.to_path_buf(), None).unwrap();
        let context = Context::new();

        let template = "{{ 'docs/loop.md' | content }}";
        let result = renderer.render_template(template, &context);

        assert!(result.is_err(), "Circular reference should cause error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("maximum recursion depth") || err.contains("depth"),
            "Error should mention depth limit. Got: {}",
            err
        );
    }

    #[test]
    fn test_multiple_file_references_same_level() {
        use crate::templating::TemplateRenderer;
        use tempfile::TempDir;
        use tera::Context;

        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        fs::create_dir_all(project_dir.join("docs")).unwrap();

        // Main file references two files at the same level
        fs::write(
            project_dir.join("docs/main.md"),
            "# Main\n\n{{ 'docs/part1.md' | content }}\n\n{{ 'docs/part2.md' | content }}",
        )
        .unwrap();

        fs::write(project_dir.join("docs/part1.md"), "Part 1 content").unwrap();
        fs::write(project_dir.join("docs/part2.md"), "Part 2 content").unwrap();

        let mut renderer = TemplateRenderer::new(true, project_dir.to_path_buf(), None).unwrap();
        let context = Context::new();

        let template = "{{ 'docs/main.md' | content }}";
        let result = renderer.render_template(template, &context);

        assert!(result.is_ok(), "Multiple file references should succeed");
        let content = result.unwrap();
        assert!(content.contains("# Main"));
        assert!(content.contains("Part 1 content"));
        assert!(content.contains("Part 2 content"));
    }
}
