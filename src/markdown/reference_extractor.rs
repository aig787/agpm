//! File reference extraction and validation for markdown documents.
//!
//! This module provides utilities to extract and validate file path references
//! within markdown content. It helps catch broken cross-references before
//! installation by checking that referenced files actually exist.
//!
//! # Supported Reference Types
//!
//! - **Markdown links**: `[text](path.md)`
//! - **Direct file paths**: `.agpm/snippets/file.md`, `docs/guide.md`
//!
//! # Extraction Rules
//!
//! The extractor intelligently filters references to avoid false positives:
//! - Skips absolute URLs (http://, https://, etc.)
//! - Skips absolute filesystem paths (starting with /)
//! - Skips content inside code blocks (``` delimited)
//! - Skips content inside inline code (` delimited)
//! - Only extracts relative file paths with common extensions
//!
//! # Usage
//!
//! ```rust,no_run
//! use agpm_cli::markdown::reference_extractor::{extract_file_references, validate_file_references};
//! use std::path::Path;
//!
//! # fn example() -> anyhow::Result<()> {
//! let markdown = r#"
//! See [documentation](../docs/guide.md) for details.
//!
//! Also check `.agpm/snippets/example.md` for examples.
//! "#;
//!
//! let references = extract_file_references(markdown);
//! // Returns: ["../docs/guide.md", ".agpm/snippets/example.md"]
//!
//! // Validate references exist
//! let project_dir = Path::new("/path/to/project");
//! let missing = validate_file_references(&references, project_dir)?;
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use regex::Regex;
use std::path::Path;

/// A missing file reference found during validation.
///
/// This struct captures information about a file reference that was found
/// in markdown content but does not exist on the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingReference {
    /// The markdown file that contains the broken reference
    pub source_file: String,

    /// The referenced path that was not found
    pub referenced_path: String,
}

impl MissingReference {
    /// Create a new missing reference record.
    ///
    /// # Arguments
    ///
    /// * `source_file` - The file containing the reference
    /// * `referenced_path` - The path that was referenced but not found
    #[must_use]
    pub fn new(source_file: String, referenced_path: String) -> Self {
        Self {
            source_file,
            referenced_path,
        }
    }
}

/// Extract file references from markdown content.
///
/// This function scans markdown content for file path references and returns
/// a deduplicated list of relative file paths. It intelligently filters out
/// URLs, absolute paths, and references inside code blocks.
///
/// # Extracted Reference Types
///
/// - Markdown links: `[text](path.md)` → extracts `path.md`
/// - Direct file paths: `.agpm/snippets/file.md` → extracts `.agpm/snippets/file.md`
///
/// # Filtering Rules
///
/// References are excluded if they:
/// - Start with URL schemes (http://, https://, ftp://, etc.)
/// - Are absolute paths (starting with /)
/// - Appear inside code blocks (``` delimited)
/// - Appear inside inline code (` delimited)
/// - Contain URL-like patterns (://)
///
/// # Arguments
///
/// * `content` - The markdown content to scan
///
/// # Returns
///
/// A vector of unique relative file paths found in the content
///
/// # Examples
///
/// ```rust,no_run
/// # use agpm_cli::markdown::reference_extractor::extract_file_references;
/// let markdown = r#"
/// Check [docs](./guide.md) and `.agpm/snippets/example.md`.
///
/// But not this [external link](https://example.com) or `inline code .md`.
/// "#;
///
/// let refs = extract_file_references(markdown);
/// assert_eq!(refs.len(), 2);
/// assert!(refs.contains(&"./guide.md".to_string()));
/// assert!(refs.contains(&".agpm/snippets/example.md".to_string()));
/// ```
#[must_use]
pub fn extract_file_references(content: &str) -> Vec<String> {
    let mut references = Vec::new();

    // Remove code blocks first to avoid extracting paths from code
    let content_without_code = remove_code_blocks(content);

    // Extract markdown links: [text](path)
    if let Ok(link_regex) = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)") {
        for cap in link_regex.captures_iter(&content_without_code) {
            if let Some(path) = cap.get(2) {
                let path_str = path.as_str();
                if is_valid_file_reference(path_str) {
                    references.push(path_str.to_string());
                }
            }
        }
    }

    // Extract direct file paths with common extensions
    // Pattern: paths containing / with file extensions
    if let Ok(path_regex) = Regex::new(
        r#"(?:^|\s|["'`])([./a-zA-Z_][\w./-]*\.(?:md|json|sh|js|py|toml|yaml|yml|rs|ts|tsx|jsx))(?:\s|["'`]|$)"#,
    ) {
        for cap in path_regex.captures_iter(&content_without_code) {
            if let Some(path) = cap.get(1) {
                let path_str = path.as_str();
                if is_valid_file_reference(path_str) {
                    references.push(path_str.to_string());
                }
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    references.retain(|r| seen.insert(r.clone()));

    references
}

/// Remove code blocks from markdown content.
///
/// This helps prevent extracting file paths that appear in code block examples,
/// which should not be validated as actual file references. Inline code (single
/// backticks) is preserved since it may contain legitimate file path references.
///
/// # Arguments
///
/// * `content` - The markdown content
///
/// # Returns
///
/// Content with code blocks removed (``` delimited)
fn remove_code_blocks(content: &str) -> String {
    let mut result = String::new();
    let mut in_code_block = false;
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        // Check for code block delimiter (```)
        if ch == '`' {
            let mut backtick_count = 1;

            // Count consecutive backticks
            while chars.peek() == Some(&'`') {
                backtick_count += 1;
                chars.next();
            }

            // Three or more backticks toggle code block mode
            if backtick_count >= 3 {
                in_code_block = !in_code_block;
                // Replace code block delimiter with spaces
                for _ in 0..backtick_count {
                    result.push(' ');
                }
                continue;
            } else {
                // It's inline code (1-2 backticks), preserve it
                for _ in 0..backtick_count {
                    result.push('`');
                }
                continue;
            }
        }

        // Skip content inside code blocks
        if in_code_block {
            result.push(' '); // Maintain structure with spaces
        } else {
            result.push(ch);
        }
    }

    result
}

/// Check if a path string is a valid file reference to validate.
///
/// This function filters out URLs, absolute paths, and other patterns
/// that should not be validated as local file references.
///
/// # Valid References
///
/// - Relative paths: `./file.md`, `../docs/guide.md`
/// - Dot-prefixed paths: `.agpm/snippets/file.md`
/// - Simple paths: `docs/guide.md`
///
/// # Invalid References (Filtered Out)
///
/// - URLs: `http://example.com`, `https://github.com/...`
/// - Absolute paths: `/usr/local/file.md`
/// - Paths with URL schemes: `file://...`, `ftp://...`
/// - Empty or whitespace-only strings
///
/// # Arguments
///
/// * `path` - The path string to validate
///
/// # Returns
///
/// `true` if the path should be validated, `false` otherwise
#[must_use]
pub fn is_valid_file_reference(path: &str) -> bool {
    let trimmed = path.trim();

    // Skip empty strings
    if trimmed.is_empty() {
        return false;
    }

    // Skip URLs (any scheme://...)
    if trimmed.contains("://") {
        return false;
    }

    // Skip absolute paths
    if trimmed.starts_with('/') {
        return false;
    }

    // Skip anchor links
    if trimmed.starts_with('#') {
        return false;
    }

    // Must have a file extension
    if !trimmed.contains('.') {
        return false;
    }

    // Must contain a path separator (/) to be considered a file path
    // This filters out simple filenames like "example.md" that aren't paths
    if !trimmed.contains('/') {
        return false;
    }

    true
}

/// Validate that file references exist on the filesystem.
///
/// This function takes a list of relative file paths and checks if they
/// exist relative to the given project directory. It returns a list of
/// missing references for error reporting.
///
/// # Arguments
///
/// * `references` - List of relative file paths to validate
/// * `project_dir` - Base directory to resolve relative paths against
///
/// # Returns
///
/// A list of references that were not found
///
/// # Errors
///
/// Returns an error if the project directory cannot be accessed
///
/// # Examples
///
/// ```rust,no_run
/// # use agpm_cli::markdown::reference_extractor::validate_file_references;
/// # use std::path::Path;
/// # fn example() -> anyhow::Result<()> {
/// let references = vec![
///     ".agpm/snippets/existing.md".to_string(),
///     ".agpm/snippets/missing.md".to_string(),
/// ];
///
/// let project_dir = Path::new("/path/to/project");
/// let missing = validate_file_references(&references, project_dir)?;
/// // Returns only the missing.md entry
/// # Ok(())
/// # }
/// ```
pub fn validate_file_references(references: &[String], project_dir: &Path) -> Result<Vec<String>> {
    let mut missing = Vec::new();

    for reference in references {
        let full_path = project_dir.join(reference);

        if !full_path.exists() {
            missing.push(reference.clone());
        }
    }

    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_extract_markdown_links() {
        let content = r#"
Check the [documentation](./docs/guide.md) for more info.
Also see [examples](../examples/demo.md).
"#;

        let refs = extract_file_references(content);
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"./docs/guide.md".to_string()));
        assert!(refs.contains(&"../examples/demo.md".to_string()));
    }

    #[test]
    fn test_extract_direct_file_paths() {
        let content = r#"
See `.agpm/snippets/example.md` for the implementation.
Check `./src/main.rs` and `.claude/agents/test.md`.
"#;

        let refs = extract_file_references(content);
        assert!(refs.contains(&".agpm/snippets/example.md".to_string()));
        assert!(refs.contains(&".claude/agents/test.md".to_string()));
        assert!(refs.contains(&"./src/main.rs".to_string()));
    }

    #[test]
    fn test_skip_urls() {
        let content = r#"
Visit [GitHub](https://github.com/user/repo) for source.
Or check http://example.com/page.html.
"#;

        let refs = extract_file_references(content);
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_skip_code_blocks() {
        let content = r#"
Normal reference: `.agpm/snippets/real.md`

```bash
# This should be skipped: `.agpm/snippets/code.md`
cat .agpm/snippets/example.md
```

Another real reference: `docs/guide.md`
"#;

        let refs = extract_file_references(content);
        assert!(refs.contains(&".agpm/snippets/real.md".to_string()));
        assert!(refs.contains(&"docs/guide.md".to_string()));
        // Should not contain references from code block
        assert!(!refs.iter().any(|r| r.contains("code.md")));
    }

    #[test]
    fn test_inline_code_path_extraction() {
        let content = "Check `.agpm/real.md` for details.";

        let refs = extract_file_references(content);
        // File paths in inline code are still extracted if they look like actual paths
        assert!(refs.contains(&".agpm/real.md".to_string()));
    }

    #[test]
    fn test_deduplication() {
        let content = r#"
See `.agpm/snippets/example.md` for details.
Also check `.agpm/snippets/example.md` again.
"#;

        let refs = extract_file_references(content);
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn test_is_valid_file_reference() {
        // Valid references
        assert!(is_valid_file_reference("./docs/guide.md"));
        assert!(is_valid_file_reference(".agpm/snippets/file.md"));
        assert!(is_valid_file_reference("../parent/file.json"));

        // Invalid references
        assert!(!is_valid_file_reference("https://example.com"));
        assert!(!is_valid_file_reference("http://test.com/file.md"));
        assert!(!is_valid_file_reference("/absolute/path.md"));
        assert!(!is_valid_file_reference("#anchor"));
        assert!(!is_valid_file_reference(""));
        assert!(!is_valid_file_reference("no-extension"));
    }

    #[test]
    fn test_validate_file_references() -> Result<()> {
        let temp_dir = tempdir()?;
        let project_dir = temp_dir.path();

        // Create some test files
        let existing_dir = project_dir.join(".agpm").join("snippets");
        fs::create_dir_all(&existing_dir)?;
        fs::write(existing_dir.join("existing.md"), "content")?;

        let references = vec![
            ".agpm/snippets/existing.md".to_string(),
            ".agpm/snippets/missing.md".to_string(),
            "nonexistent/file.md".to_string(),
        ];

        let missing = validate_file_references(&references, project_dir)?;

        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&".agpm/snippets/missing.md".to_string()));
        assert!(missing.contains(&"nonexistent/file.md".to_string()));
        assert!(!missing.contains(&".agpm/snippets/existing.md".to_string()));

        Ok(())
    }

    #[test]
    fn test_remove_code_blocks() {
        let content = r#"
Normal text with `.agpm/file.md`

```rust
let path = ".agpm/in_code.md";
```

More normal text `.agpm/another.md`
"#;

        let cleaned = remove_code_blocks(content);
        assert!(cleaned.contains(".agpm/file.md"));
        assert!(cleaned.contains(".agpm/another.md"));
        // Code block content should be replaced with spaces
        assert!(
            !cleaned.contains("in_code.md")
                || cleaned.split_whitespace().all(|word| !word.contains("in_code.md"))
        );
    }

    #[test]
    fn test_complex_markdown_with_mixed_references() {
        let content = r#"
# Documentation

See the [main guide](./docs/guide.md) for details.

## Implementation

The core logic is in `.agpm/snippets/core.md` file.

```rust
// This code reference should be ignored
let path = ".agpm/snippets/ignored.md";
```

Also check:
- [Examples](../examples/demo.md)
- External: https://github.com/user/repo
- `.claude/agents/helper.md`

Inline code like `example.md` should be skipped.
"#;

        let refs = extract_file_references(content);

        // Should extract these
        assert!(refs.contains(&"./docs/guide.md".to_string()));
        assert!(refs.contains(&".agpm/snippets/core.md".to_string()));
        assert!(refs.contains(&"../examples/demo.md".to_string()));
        assert!(refs.contains(&".claude/agents/helper.md".to_string()));

        // Should NOT extract these
        assert!(!refs.iter().any(|r| r.contains("github.com")));
        assert!(!refs.iter().any(|r| r.contains("ignored.md")));
        assert!(!refs.contains(&"example.md".to_string())); // Was in inline code
    }
}
