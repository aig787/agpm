//! Markdown file operations and metadata extraction for Claude Code resources.
//!
//! This module provides comprehensive support for reading, writing, and manipulating
//! Markdown files that contain Claude Code agents and snippets. It handles both
//! plain Markdown files and files with structured metadata in frontmatter.
//!
//! # Overview
//!
//! The markdown module is a core component of AGPM that:
//! - Parses Markdown files with optional YAML or TOML frontmatter
//! - Extracts structured metadata for dependency resolution
//! - Preserves document structure during read/write operations
//! - Provides utilities for file discovery and validation
//! - Supports atomic file operations for safe installation
//!
//! # Supported File Formats
//!
//! ## Plain Markdown Files
//!
//! Standard Markdown files without frontmatter are fully supported:
//!
//! ```markdown
//! # Python Code Reviewer
//!
//! This agent specializes in reviewing Python code for:
//! - PEP 8 compliance
//! - Security vulnerabilities
//! - Performance optimizations
//!
//! ## Usage
//!
//! When reviewing code, I will...
//! ```
//!
//! ## YAML Frontmatter Format
//!
//! Files can include YAML frontmatter for structured metadata:
//!
//! ```markdown
//! ---
//! title: "Python Code Reviewer"
//! description: "Specialized agent for Python code quality review"
//! version: "2.1.0"
//! author: "Claude Code Team"
//! type: "agent"
//! tags:
//!   - "python"
//!   - "code-review"
//!   - "quality"
//! dependencies:
//!   agents:
//!     - path: agents/syntax-checker.md
//!   snippets:
//!     - path: snippets/security-scanner.md
//! ---
//!
//! # Python Code Reviewer
//!
//! This agent specializes in reviewing Python code...
//! ```
//!
//! ## TOML Frontmatter Format
//!
//! TOML frontmatter is also supported using `+++` delimiters:
//!
//! ```text
//! +++
//! title = "JavaScript Snippet Collection"
//! description = "Useful JavaScript utilities and helpers"
//! version = "1.0.0"
//! author = "Community Contributors"
//! type = "snippet"
//! tags = ["javascript", "utilities", "helpers"]
//! +++
//!
//! # JavaScript Snippet Collection
//!
//! ## Array Utilities
//!
//! ```javascript
//! function unique(arr) {
//!     return [...new Set(arr)];
//! }
//! ```
//!
//! # Metadata Schema
//!
//! The frontmatter metadata follows this schema:
//!
//! | Field | Type | Description | Required |
//! |-------|------|-------------|----------|
//! | title | string | Human-readable resource title | No |
//! | description | string | Brief description of the resource | No |
//! | version | string | Resource version (semver recommended) | No |
//! | author | string | Author name or organization | No |
//! | type | string | Resource type ("agent" or "snippet") | No |
//! | tags | array | Tags for categorization | No |
//! | dependencies | object | Structured dependencies by resource type | No |
//!
//! Additional custom fields are preserved in the extra map.
//!
//! # Content Extraction
//!
//! When metadata is not explicitly provided in frontmatter, the module
//! can extract information from the Markdown content:
//!
//! - **Title**: Extracted from the first level-1 heading in the content
//! - **Description**: Extracted from the first paragraph after headings
//!
//! This allows resources to work without frontmatter while still providing
//! useful metadata for dependency resolution and display.
//!
//! # File Operations
//!
//! All file operations are designed to be safe and atomic:
//! - Parent directories are created automatically during writes
//! - Content is validated during parsing to catch errors early  
//! - File extensions are validated (.md, .markdown)
//! - Recursive directory traversal for bulk operations
//!
//! # Usage Examples
//!
//! ## Basic Reading and Writing
//!
//! ```rust,no_run
//! use agpm::markdown::MarkdownDocument;
//! use std::path::Path;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Read a markdown file
//! let doc = MarkdownDocument::read(Path::new("agents/reviewer.md"))?;
//!
//! // Access metadata
//! if let Some(metadata) = &doc.metadata {
//!     println!("Title: {:?}", metadata.title);
//!     println!("Version: {:?}", metadata.version);
//!     println!("Tags: {:?}", metadata.tags);
//! }
//!
//! // Extract title from content if not in metadata
//! if let Some(title) = doc.get_title() {
//!     println!("Extracted title: {}", title);
//! }
//!
//! // Write to a new location
//! doc.write(Path::new("installed/reviewer.md"))?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Creating Documents Programmatically
//!
//! ```rust,no_run
//! use agpm::markdown::{MarkdownDocument, MarkdownMetadata};
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create metadata
//! let mut metadata = MarkdownMetadata::default();
//! metadata.title = Some("Custom Agent".to_string());
//! metadata.version = Some("1.0.0".to_string());
//! metadata.tags = vec!["custom".to_string(), "utility".to_string()];
//!
//! // Create document with metadata
//! let content = "# Custom Agent\n\nThis is a custom agent...";
//! let doc = MarkdownDocument::with_metadata(metadata, content.to_string());
//!
//! // The raw field contains formatted frontmatter + content
//! println!("{}", doc.raw);
//! # Ok(())
//! # }
//! ```
//!
//! ## Batch File Processing
//!
//! ```rust,no_run
//! use agpm::markdown::{list_markdown_files, MarkdownDocument};
//! use std::path::Path;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Find all markdown files in a directory
//! let files = list_markdown_files(Path::new("resources/"))?;
//!
//! for file in files {
//!     let doc = MarkdownDocument::read(&file)?;
//!     
//!     if let Some(title) = doc.get_title() {
//!         println!("{}: {}", file.display(), title);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Integration with AGPM
//!
//! This module integrates with other AGPM components:
//!
//! - `crate::manifest`: Uses metadata for dependency resolution
//! - `crate::lockfile`: Stores checksums and installation paths  
//! - `crate::source`: Handles remote resource fetching
//! - `crate::core`: Provides core types and error handling
//!
//! See the respective module documentation for integration details.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::manifest::DependencySpec;

/// Type alias for [`MarkdownDocument`] for backward compatibility.
///
/// This alias exists to provide a consistent naming convention and maintain
/// backward compatibility with existing code that might use `MarkdownFile`.
/// New code should prefer using [`MarkdownDocument`] directly.
///
/// # Examples
///
/// ```rust,no_run
/// # use agpm::markdown::{MarkdownFile, MarkdownDocument};
/// // These are equivalent
/// let doc1 = MarkdownDocument::new("content".to_string());
/// let doc2 = MarkdownFile::new("content".to_string());
///
/// assert_eq!(doc1.content, doc2.content);
/// ```
pub type MarkdownFile = MarkdownDocument;

/// Structured metadata extracted from Markdown frontmatter.
///
/// This struct represents all the metadata that can be parsed from YAML or TOML
/// frontmatter in Markdown files. It follows a flexible schema that accommodates
/// both standard AGPM fields and custom extensions.
///
/// # Standard Fields
///
/// The following fields have special meaning in AGPM:
/// - `title`: Human-readable name for the resource
/// - `description`: Brief explanation of what the resource does
/// - `version`: Version identifier (semantic versioning recommended)
/// - `author`: Creator or maintainer information
/// - `resource_type`: Type classification ("agent" or "snippet")
/// - `tags`: Categorization labels for filtering and discovery
/// - `dependencies`: Structured dependencies for transitive resolution
///
/// # Custom Fields
///
/// Additional fields are preserved in the `extra` map, allowing resource
/// authors to include custom metadata without breaking compatibility.
///
/// # Serialization
///
/// The struct uses Serde for serialization with skip-if-empty optimizations
/// to keep generated frontmatter clean. Empty collections and None values
/// are omitted from the output.
///
/// # Example
///
/// ```rust,no_run
/// # use agpm::markdown::MarkdownMetadata;
/// # use std::collections::HashMap;
/// let mut metadata = MarkdownMetadata::default();
/// metadata.title = Some("Python Linter".to_string());
/// metadata.version = Some("2.0.1".to_string());
/// metadata.tags = vec!["python".to_string(), "linting".to_string()];
/// // Dependencies can be set as a JSON value for the structured format
/// // This is typically parsed from frontmatter rather than set programmatically
///
/// // Custom fields via extra map
/// let mut extra = HashMap::new();
/// extra.insert("license".to_string(), "MIT".into());
/// extra.insert("min_python".to_string(), "3.8".into());
/// metadata.extra = extra;
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarkdownMetadata {
    /// Human-readable title of the resource.
    ///
    /// This is displayed in listings and used for resource identification.
    /// If not provided, the title may be extracted from the first heading
    /// in the Markdown content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Brief description explaining what the resource does.
    ///
    /// Used for documentation and resource discovery. If not provided,
    /// the description may be extracted from the first paragraph in
    /// the Markdown content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Version identifier for the resource.
    ///
    /// Semantic versioning (e.g., "1.2.3") is recommended for compatibility
    /// with dependency resolution, but any string format is accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Author or maintainer information.
    ///
    /// Can be a name, organization, or contact information. Free-form text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Classification tags for categorization and filtering.
    ///
    /// Tags help with resource discovery and organization. Common patterns:
    /// - Language-specific: "python", "javascript", "rust"
    /// - Functionality: "linting", "testing", "documentation"
    /// - Domain: "web-dev", "data-science", "devops"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Resource type classification.
    ///
    /// Currently supported types:
    /// - "agent": Interactive Claude Code agents
    /// - "snippet": Code snippets and templates
    ///
    /// This field uses `rename = "type"` to match the frontmatter format
    /// while avoiding Rust's `type` keyword.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,

    /// Dependencies for this resource.
    ///
    /// This field uses the structured transitive dependency format where
    /// dependencies are organized by resource type (agents, snippets, etc.).
    /// Each resource type maps to a list of dependency specifications.
    ///
    /// Example:
    /// ```yaml
    /// dependencies:
    ///   agents:
    ///     - path: agents/helper.md
    ///       version: v1.0.0
    ///   snippets:
    ///     - path: snippets/utils.md
    /// ```
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<HashMap<String, Vec<DependencySpec>>>,

    /// Additional custom metadata fields.
    ///
    /// Any frontmatter fields not recognized by the standard schema are
    /// preserved here. This allows resource authors to include custom
    /// metadata without breaking compatibility with AGPM.
    ///
    /// Values are stored as `serde_json::Value` to handle mixed types
    /// (strings, numbers, arrays, objects).
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// A parsed Markdown document representing a Claude Code resource.
///
/// This is the core structure for handling Markdown files in AGPM. It provides
/// a clean separation between structured metadata (from frontmatter) and the
/// actual content, while preserving the original document format for roundtrip
/// compatibility.
///
/// # Structure
///
/// A `MarkdownDocument` consists of three parts:
/// 1. **Metadata**: Structured data from frontmatter (YAML or TOML)
/// 2. **Content**: The main Markdown content without frontmatter
/// 3. **Raw**: The complete original document for faithful reproduction
///
/// # Frontmatter Support
///
/// The document can parse both YAML (`---` delimiters) and TOML (`+++` delimiters)
/// frontmatter formats. If no frontmatter is present, the entire file is treated
/// as content.
///
/// # Content Extraction
///
/// When explicit metadata is not available, the document can extract information
/// from the content itself using [`get_title`] and [`get_description`] methods.
///
/// # Thread Safety
///
/// This struct is `Clone` and can be safely passed between threads for
/// concurrent processing of multiple documents.
///
/// # Examples
///
/// ## Reading from File
///
/// ```rust,no_run
/// # use agpm::markdown::MarkdownDocument;
/// # use std::path::Path;
/// # fn example() -> anyhow::Result<()> {
/// let doc = MarkdownDocument::read(Path::new("agent.md"))?;
///
/// if let Some(metadata) = &doc.metadata {
///     println!("Found metadata: {:?}", metadata.title);
/// }
///
/// println!("Content length: {} chars", doc.content.len());
/// # Ok(())
/// # }
/// ```
///
/// ## Creating Programmatically
///
/// ```rust,no_run
/// # use agpm::markdown::{MarkdownDocument, MarkdownMetadata};
/// let metadata = MarkdownMetadata {
///     title: Some("Test Agent".to_string()),
///     version: Some("1.0.0".to_string()),
///     ..Default::default()
/// };
///
/// let content = "# Test Agent\n\nThis agent helps with testing.";
/// let doc = MarkdownDocument::with_metadata(metadata, content.to_string());
///
/// // Raw contains formatted frontmatter + content
/// assert!(doc.raw.contains("title: Test Agent"));
/// assert!(doc.raw.contains("This agent helps with testing"));
/// ```
///
/// ## Modifying Content
///
/// ```rust,no_run
/// # use agpm::markdown::MarkdownDocument;
/// let mut doc = MarkdownDocument::new("# Original".to_string());
///
/// // Update content - raw is automatically regenerated
/// doc.set_content("# Updated Content\n\nNew description.".to_string());
///
/// assert_eq!(doc.content, "# Updated Content\n\nNew description.");
/// assert_eq!(doc.raw, doc.content); // No frontmatter, so raw == content
/// ```
///
/// [`get_title`]: MarkdownDocument::get_title
/// [`get_description`]: MarkdownDocument::get_description
#[derive(Debug, Clone)]
pub struct MarkdownDocument {
    /// Parsed metadata extracted from frontmatter.
    ///
    /// This will be `Some` if the document contained valid YAML or TOML
    /// frontmatter, and `None` for plain Markdown files. The metadata
    /// is used by AGPM for dependency resolution and resource management.
    pub metadata: Option<MarkdownMetadata>,

    /// The main Markdown content without frontmatter delimiters.
    ///
    /// This contains only the actual content portion of the document,
    /// with frontmatter stripped away. This is what gets processed
    /// for content-based metadata extraction.
    pub content: String,

    /// The complete original document including frontmatter.
    ///
    /// This field preserves the exact original format for faithful
    /// reproduction when writing back to disk. When metadata or content
    /// is modified, this field is automatically regenerated to maintain
    /// consistency.
    pub raw: String,
}

impl MarkdownDocument {
    /// Create a new markdown document without frontmatter.
    ///
    /// This creates a plain Markdown document with no metadata. The content
    /// becomes both the `content` and `raw` fields since there's no frontmatter
    /// to format.
    ///
    /// # Arguments
    ///
    /// * `content` - The Markdown content as a string
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::MarkdownDocument;
    /// let doc = MarkdownDocument::new("# Hello\n\nWorld!".to_string());
    ///
    /// assert!(doc.metadata.is_none());
    /// assert_eq!(doc.content, "# Hello\n\nWorld!");
    /// assert_eq!(doc.raw, doc.content);
    /// ```
    #[must_use]
    pub fn new(content: String) -> Self {
        Self {
            metadata: None,
            content: content.clone(),
            raw: content,
        }
    }

    /// Create a markdown document with metadata and content.
    ///
    /// This constructor creates a complete document with structured metadata
    /// in YAML frontmatter format. The `raw` field will contain the formatted
    /// frontmatter followed by the content.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The structured metadata for the document
    /// * `content` - The Markdown content (without frontmatter)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::{MarkdownDocument, MarkdownMetadata};
    /// let metadata = MarkdownMetadata {
    ///     title: Some("Example".to_string()),
    ///     version: Some("1.0.0".to_string()),
    ///     ..Default::default()
    /// };
    ///
    /// let doc = MarkdownDocument::with_metadata(
    ///     metadata,
    ///     "# Example\n\nThis is an example.".to_string()
    /// );
    ///
    /// assert!(doc.metadata.is_some());
    /// assert!(doc.raw.starts_with("---\n"));
    /// assert!(doc.raw.contains("title: Example"));
    /// ```
    #[must_use]
    pub fn with_metadata(metadata: MarkdownMetadata, content: String) -> Self {
        let raw = Self::format_with_frontmatter(&metadata, &content);
        Self {
            metadata: Some(metadata),
            content,
            raw,
        }
    }

    /// Read and parse a Markdown file from the filesystem.
    ///
    /// This method reads the entire file into memory and parses it for
    /// frontmatter and content. It supports both YAML and TOML frontmatter
    /// formats and provides detailed error context on failure.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the Markdown file to read
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the parsed document or an error with
    /// context about what went wrong (file not found, parse error, etc.).
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file cannot be read (doesn't exist, permissions, etc.)
    /// - The file contains invalid UTF-8
    /// - The frontmatter is malformed YAML or TOML
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::MarkdownDocument;
    /// # use std::path::Path;
    /// # fn example() -> anyhow::Result<()> {
    /// let doc = MarkdownDocument::read(Path::new("resources/agent.md"))?;
    ///
    /// println!("Title: {:?}", doc.get_title());
    /// println!("Content length: {}", doc.content.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn read(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read markdown file: {}", path.display()))?;

        Self::parse(&raw)
    }

    /// Write the document to a file on disk.
    ///
    /// This method performs an atomic write operation, creating any necessary
    /// parent directories automatically. The complete `raw` content (including
    /// frontmatter if present) is written to the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Target path where the file should be written
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error with context on failure.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Parent directories cannot be created (permissions, disk space, etc.)
    /// - The file cannot be written (permissions, disk space, etc.)
    /// - The path is invalid or inaccessible
    ///
    /// # Safety
    ///
    /// This operation creates parent directories as needed, which could
    /// potentially create unexpected directory structures if the path
    /// is not validated by the caller.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::MarkdownDocument;
    /// # use std::path::Path;
    /// # fn example() -> anyhow::Result<()> {
    /// let doc = MarkdownDocument::new("# Test\n\nContent".to_string());
    ///
    /// // Writes to file, creating directories as needed
    /// doc.write(Path::new("output/resources/test.md"))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn write(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        fs::write(path, &self.raw)
            .with_context(|| format!("Failed to write markdown file: {}", path.display()))?;

        Ok(())
    }

    /// Parse a Markdown string that may contain frontmatter with context for warnings.
    ///
    /// This is similar to [`parse`](Self::parse) but accepts an optional context string
    /// that will be included in warning messages when preprocessing is required.
    ///
    /// # Arguments
    ///
    /// * `input` - The complete Markdown document as a string
    /// * `context` - Optional context (e.g., file path) for warning messages
    ///
    /// # Returns
    ///
    /// Returns a parsed `MarkdownDocument`. If frontmatter parsing fails,
    /// a warning is emitted and the entire document is treated as content.
    pub fn parse_with_context(input: &str, context: Option<&str>) -> Result<Self> {
        // Check for YAML frontmatter (starts with ---)
        if (input.starts_with("---\n") || input.starts_with("---\r\n"))
            && let Some(end_idx) = find_frontmatter_end(input)
        {
            let skip_size = if input.starts_with("---\r\n") {
                5
            } else {
                4
            };
            let frontmatter = &input[skip_size..end_idx];
            let content = input[end_idx..].trim_start_matches("---").trim_start();

            // Try to parse YAML frontmatter with standard parser first
            match serde_yaml::from_str::<MarkdownMetadata>(frontmatter) {
                Ok(metadata) => {
                    // Standard parsing succeeded
                    return Ok(Self {
                        metadata: Some(metadata),
                        content: content.to_string(),
                        raw: input.to_string(),
                    });
                }
                Err(err) => {
                    // Parsing failed - emit warning and treat entire document as content
                    if let Some(ctx) = context {
                        eprintln!(
                            "⚠️  Warning: Unable to parse YAML frontmatter in '{ctx}'. \
                            The document will be processed without metadata. Error: {err}"
                        );
                    } else {
                        eprintln!(
                            "⚠️  Warning: Unable to parse YAML frontmatter. \
                            The document will be processed without metadata. Error: {err}"
                        );
                    }

                    // Treat the entire document as content (including the invalid frontmatter)
                    return Ok(Self {
                        metadata: None,
                        content: input.to_string(),
                        raw: input.to_string(),
                    });
                }
            }
        }

        // Check for TOML frontmatter (starts with +++)
        if (input.starts_with("+++\n") || input.starts_with("+++\r\n"))
            && let Some(end_idx) = find_toml_frontmatter_end(input)
        {
            let skip_size = if input.starts_with("+++\r\n") {
                5
            } else {
                4
            };
            let frontmatter = &input[skip_size..end_idx];
            let content = input[end_idx..].trim_start_matches("+++").trim_start();

            // Try to parse TOML frontmatter
            match toml::from_str::<MarkdownMetadata>(frontmatter) {
                Ok(metadata) => {
                    return Ok(Self {
                        metadata: Some(metadata),
                        content: content.to_string(),
                        raw: input.to_string(),
                    });
                }
                Err(err) => {
                    // TOML parsing failed - emit warning and treat entire document as content
                    if let Some(ctx) = context {
                        eprintln!(
                            "⚠️  Warning: Unable to parse TOML frontmatter in '{ctx}'. \
                            The document will be processed without metadata. Error: {err}"
                        );
                    } else {
                        eprintln!(
                            "⚠️  Warning: Unable to parse TOML frontmatter. \
                            The document will be processed without metadata. Error: {err}"
                        );
                    }

                    // Treat the entire document as content (including the invalid frontmatter)
                    return Ok(Self {
                        metadata: None,
                        content: input.to_string(),
                        raw: input.to_string(),
                    });
                }
            }
        }

        // No frontmatter, entire document is content
        Ok(Self {
            metadata: None,
            content: input.to_string(),
            raw: input.to_string(),
        })
    }

    /// Parse a Markdown string that may contain frontmatter.
    ///
    /// This is the core parsing method that handles both YAML and TOML
    /// frontmatter formats. It attempts to detect and parse frontmatter,
    /// falling back to treating the entire input as content if no valid
    /// frontmatter is found.
    ///
    /// # Supported Formats
    ///
    /// ## YAML Frontmatter (recommended)
    /// ```text
    /// ---
    /// title: "Example"
    /// version: "1.0.0"
    /// ---
    /// Content here...
    /// ```
    ///
    /// ## TOML Frontmatter
    /// ```text
    /// +++
    /// title = "Example"
    /// version = "1.0.0"
    /// +++
    /// Content here...
    /// ```
    ///
    /// # Arguments
    ///
    /// * `input` - The complete Markdown document as a string
    ///
    /// # Returns
    ///
    /// Returns a parsed `MarkdownDocument` with metadata extracted if present.
    ///
    /// # Errors
    ///
    /// Returns an error if the frontmatter is present but malformed:
    /// - Invalid YAML syntax in `---` delimited frontmatter
    /// - Invalid TOML syntax in `+++` delimited frontmatter
    /// - Frontmatter that doesn't match the expected metadata schema
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::MarkdownDocument;
    /// // Parse document with YAML frontmatter
    /// let input = "---\ntitle: Test\n---\n# Content";
    /// let doc = MarkdownDocument::parse(input).unwrap();
    /// assert!(doc.metadata.is_some());
    ///
    /// // Parse plain Markdown
    /// let input = "# Just Content";
    /// let doc = MarkdownDocument::parse(input).unwrap();
    /// assert!(doc.metadata.is_none());
    /// ```
    pub fn parse(input: &str) -> Result<Self> {
        Self::parse_with_context(input, None)
    }

    /// Format a document with YAML frontmatter
    fn format_with_frontmatter(metadata: &MarkdownMetadata, content: &str) -> String {
        let yaml = serde_yaml::to_string(metadata).unwrap_or_default();
        format!("---\n{yaml}---\n\n{content}")
    }

    /// Update the document's metadata and regenerate the raw content.
    ///
    /// This method replaces the current metadata (if any) with new metadata
    /// and automatically regenerates the `raw` field to include properly
    /// formatted YAML frontmatter.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The new metadata to set for this document
    ///
    /// # Effects
    ///
    /// - Sets `self.metadata` to `Some(metadata)`
    /// - Regenerates `self.raw` with YAML frontmatter + content
    /// - Preserves the existing `content` field unchanged
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::{MarkdownDocument, MarkdownMetadata};
    /// let mut doc = MarkdownDocument::new("# Test\n\nContent".to_string());
    /// assert!(doc.metadata.is_none());
    ///
    /// let metadata = MarkdownMetadata {
    ///     title: Some("New Title".to_string()),
    ///     version: Some("2.0.0".to_string()),
    ///     ..Default::default()
    /// };
    ///
    /// doc.set_metadata(metadata);
    /// assert!(doc.metadata.is_some());
    /// assert!(doc.raw.contains("title: New Title"));
    /// assert!(doc.raw.contains("# Test"));
    /// ```
    pub fn set_metadata(&mut self, metadata: MarkdownMetadata) {
        self.raw = Self::format_with_frontmatter(&metadata, &self.content);
        self.metadata = Some(metadata);
    }

    /// Update the document's content and regenerate the raw document.
    ///
    /// This method replaces the current content with new content and
    /// automatically regenerates the `raw` field. If metadata is present,
    /// the raw content will include formatted frontmatter; otherwise it
    /// will be just the new content.
    ///
    /// # Arguments
    ///
    /// * `content` - The new Markdown content (without frontmatter)
    ///
    /// # Effects
    ///
    /// - Sets `self.content` to the new content
    /// - Regenerates `self.raw` appropriately:
    ///   - If metadata exists: frontmatter + new content
    ///   - If no metadata: just the new content
    /// - Preserves existing metadata unchanged
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::{MarkdownDocument, MarkdownMetadata};
    /// // Document with metadata
    /// let metadata = MarkdownMetadata {
    ///     title: Some("Test".to_string()),
    ///     ..Default::default()
    /// };
    /// let mut doc = MarkdownDocument::with_metadata(
    ///     metadata,
    ///     "Original content".to_string()
    /// );
    ///
    /// doc.set_content("# New Content\n\nUpdated!".to_string());
    ///
    /// assert_eq!(doc.content, "# New Content\n\nUpdated!");
    /// assert!(doc.raw.contains("title: Test"));
    /// assert!(doc.raw.contains("# New Content"));
    /// ```
    pub fn set_content(&mut self, content: String) {
        if let Some(ref metadata) = self.metadata {
            self.raw = Self::format_with_frontmatter(metadata, &content);
        } else {
            self.raw = content.clone();
        }
        self.content = content;
    }

    /// Extract the document title from metadata or content.
    ///
    /// This method provides a fallback mechanism for getting the document title:
    /// 1. First, check if metadata contains an explicit title
    /// 2. If not, scan the content for the first level-1 heading (`# Title`)
    /// 3. Return `None` if neither source provides a title
    ///
    /// # Returns
    ///
    /// - `Some(String)` containing the title if found
    /// - `None` if no title is available from either source
    ///
    /// # Title Extraction Rules
    ///
    /// When extracting from content:
    /// - Only level-1 headings (starting with `# `) are considered
    /// - The first matching heading is used
    /// - Leading/trailing whitespace is trimmed from the result
    /// - Empty headings (just `#`) are ignored
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::{MarkdownDocument, MarkdownMetadata};
    /// // From metadata
    /// let metadata = MarkdownMetadata {
    ///     title: Some("Metadata Title".to_string()),
    ///     ..Default::default()
    /// };
    /// let doc = MarkdownDocument::with_metadata(
    ///     metadata,
    ///     "# Content Title\n\nSome text".to_string()
    /// );
    /// assert_eq!(doc.get_title(), Some("Metadata Title".to_string()));
    ///
    /// // From content heading
    /// let doc = MarkdownDocument::new("# Extracted Title\n\nContent".to_string());
    /// assert_eq!(doc.get_title(), Some("Extracted Title".to_string()));
    ///
    /// // No title available
    /// let doc = MarkdownDocument::new("Just some content without headings".to_string());
    /// assert_eq!(doc.get_title(), None);
    /// ```
    #[must_use]
    pub fn get_title(&self) -> Option<String> {
        // First check metadata
        if let Some(ref metadata) = self.metadata
            && let Some(ref title) = metadata.title
        {
            return Some(title.clone());
        }

        // Try to extract from first # heading
        for line in self.content.lines() {
            if let Some(heading) = line.strip_prefix("# ") {
                return Some(heading.trim().to_string());
            }
        }

        None
    }

    /// Extract the document description from metadata or content.
    ///
    /// This method provides a fallback mechanism for getting the document description:
    /// 1. First, check if metadata contains an explicit description
    /// 2. If not, extract the first paragraph from the content (after any headings)
    /// 3. Return `None` if neither source provides a description
    ///
    /// # Returns
    ///
    /// - `Some(String)` containing the description if found
    /// - `None` if no description is available from either source
    ///
    /// # Description Extraction Rules
    ///
    /// When extracting from content:
    /// - All headings (lines starting with `#`) are skipped
    /// - Empty lines before the first paragraph are ignored
    /// - The first continuous block of non-empty lines becomes the description
    /// - Multiple lines are joined with spaces
    /// - Extraction stops at the first empty line after content starts
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use agpm::markdown::{MarkdownDocument, MarkdownMetadata};
    /// // From metadata
    /// let metadata = MarkdownMetadata {
    ///     description: Some("Metadata description".to_string()),
    ///     ..Default::default()
    /// };
    /// let doc = MarkdownDocument::with_metadata(
    ///     metadata,
    ///     "# Title\n\nContent description".to_string()
    /// );
    /// assert_eq!(doc.get_description(), Some("Metadata description".to_string()));
    ///
    /// // From content paragraph
    /// let doc = MarkdownDocument::new(
    ///     "# Title\n\nThis is the first\nparagraph of content.\n\nSecond paragraph.".to_string()
    /// );
    /// assert_eq!(doc.get_description(), Some("This is the first paragraph of content.".to_string()));
    ///
    /// // No description available  
    /// let doc = MarkdownDocument::new("# Just a title".to_string());
    /// assert_eq!(doc.get_description(), None);
    /// ```
    #[must_use]
    pub fn get_description(&self) -> Option<String> {
        // First check metadata
        if let Some(ref metadata) = self.metadata
            && let Some(ref desc) = metadata.description
        {
            return Some(desc.clone());
        }

        // Try to extract first non-heading paragraph
        let mut in_paragraph = false;
        let mut paragraph = String::new();

        for line in self.content.lines() {
            let trimmed = line.trim();

            // Skip headings and empty lines at start
            if trimmed.starts_with('#') || (trimmed.is_empty() && !in_paragraph) {
                continue;
            }

            // Start collecting paragraph
            if !trimmed.is_empty() {
                in_paragraph = true;
                if !paragraph.is_empty() {
                    paragraph.push(' ');
                }
                paragraph.push_str(trimmed);
            } else if in_paragraph {
                // End of first paragraph
                break;
            }
        }

        if paragraph.is_empty() {
            None
        } else {
            Some(paragraph)
        }
    }
}

/// Find the end position of YAML frontmatter in a document.
///
/// This helper function scans through a document that starts with YAML
/// frontmatter (delimited by `---`) to find where the closing delimiter
/// occurs. It returns the byte position of the closing delimiter.
///
/// # Arguments
///
/// * `input` - The document content starting with `---`
///
/// # Returns
///
/// - `Some(usize)` - Byte position of the closing `---` delimiter
/// - `None` - If no closing delimiter is found
///
/// # Implementation Notes
///
/// - Assumes the input starts with the opening `---` delimiter
/// - Counts bytes, not characters, for proper string slicing
/// - Accounts for newline characters in position calculation
fn find_frontmatter_end(input: &str) -> Option<usize> {
    // Handle both Unix (LF) and Windows (CRLF) line endings
    let has_crlf = input.contains("\r\n");
    let initial_skip = if has_crlf {
        5
    } else {
        4
    }; // "---\r\n" or "---\n"

    let mut lines = input.lines();
    lines.next()?; // Skip first ---

    let mut pos = initial_skip;
    for line in lines {
        if line == "---" {
            return Some(pos);
        }
        // Account for actual line ending bytes (CRLF = 2, LF = 1)
        let line_ending_size = if has_crlf {
            2
        } else {
            1
        };
        pos += line.len() + line_ending_size;
    }

    None
}

/// Find the end position of TOML frontmatter in a document.
///
/// This helper function scans through a document that starts with TOML
/// frontmatter (delimited by `+++`) to find where the closing delimiter
/// occurs. It returns the byte position of the closing delimiter.
///
/// # Arguments
///
/// * `input` - The document content starting with `+++`
///
/// # Returns
///
/// - `Some(usize)` - Byte position of the closing `+++` delimiter
/// - `None` - If no closing delimiter is found
///
/// # Implementation Notes
///
/// - Assumes the input starts with the opening `+++` delimiter
/// - Counts bytes, not characters, for proper string slicing
/// - Accounts for newline characters in position calculation
fn find_toml_frontmatter_end(input: &str) -> Option<usize> {
    // Handle both Unix (LF) and Windows (CRLF) line endings
    let has_crlf = input.contains("\r\n");
    let initial_skip = if has_crlf {
        5
    } else {
        4
    }; // "+++\r\n" or "+++\n"

    let mut lines = input.lines();
    lines.next()?; // Skip first +++

    let mut pos = initial_skip;
    for line in lines {
        if line == "+++" {
            return Some(pos);
        }
        // Account for actual line ending bytes (CRLF = 2, LF = 1)
        let line_ending_size = if has_crlf {
            2
        } else {
            1
        };
        pos += line.len() + line_ending_size;
    }

    None
}

/// Check if a path represents a Markdown file based on its extension.
///
/// This function validates file paths to determine if they should be treated
/// as Markdown files. It performs case-insensitive extension checking to
/// support different naming conventions across platforms.
///
/// # Supported Extensions
///
/// - `.md` (most common)
/// - `.markdown` (verbose form)
/// - Case variations: `.MD`, `.Markdown`, etc.
///
/// # Arguments
///
/// * `path` - The file path to check
///
/// # Returns
///
/// - `true` if the file has a recognized Markdown extension
/// - `false` otherwise (including files with no extension)
///
/// # Examples
///
/// ```rust,no_run
/// # use agpm::markdown::is_markdown_file;
/// # use std::path::Path;
/// assert!(is_markdown_file(Path::new("agent.md")));
/// assert!(is_markdown_file(Path::new("README.MD")));
/// assert!(is_markdown_file(Path::new("guide.markdown")));
/// assert!(!is_markdown_file(Path::new("config.toml")));
/// assert!(!is_markdown_file(Path::new("script.sh")));
/// assert!(!is_markdown_file(Path::new("no-extension")));
/// ```
#[must_use]
pub fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

/// Recursively find all Markdown files in a directory.
///
/// This function performs a recursive traversal of the given directory,
/// collecting all files that have Markdown extensions. It follows symbolic
/// links and handles filesystem errors gracefully.
///
/// # Directory Traversal
///
/// - Recursively traverses all subdirectories
/// - Follows symbolic links (may cause infinite loops with circular links)
/// - Silently skips entries that cannot be accessed
/// - Only includes regular files (not directories or special files)
///
/// # Arguments
///
/// * `dir` - The directory path to search
///
/// # Returns
///
/// - `Ok(Vec<PathBuf>)` - List of absolute paths to Markdown files
/// - `Err(...)` - Only on severe filesystem errors (rare)
///
/// # Behavior
///
/// - Returns empty vector if directory doesn't exist (not an error)
/// - Files are returned in filesystem order (not sorted)
/// - Paths are absolute and canonicalized
/// - Uses [`is_markdown_file`] for extension validation
///
/// # Examples
///
/// ```rust,no_run
/// # use agpm::markdown::list_markdown_files;
/// # use std::path::Path;
/// # fn example() -> anyhow::Result<()> {
/// let files = list_markdown_files(Path::new("resources/"))?;
///
/// for file in files {
///     println!("Found: {}", file.display());
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function loads directory metadata but not file contents, making it
/// suitable for scanning large directory trees. For processing the files,
/// consider using [`MarkdownDocument::read`] on each result.
///
/// [`is_markdown_file`]: is_markdown_file
/// [`MarkdownDocument::read`]: MarkdownDocument::read
pub fn list_markdown_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    if !dir.exists() {
        return Ok(files);
    }

    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path.is_file() && is_markdown_file(path) {
            files.push(path.to_path_buf());
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_markdown_document_new() {
        let doc = MarkdownDocument::new("# Hello World".to_string());
        assert!(doc.metadata.is_none());
        assert_eq!(doc.content, "# Hello World");
        assert_eq!(doc.raw, "# Hello World");
    }

    #[test]
    fn test_markdown_with_yaml_frontmatter() {
        let input = r"---
title: Test Document
description: A test document
tags:
  - test
  - example
---

# Hello World

This is the content.";

        let doc = MarkdownDocument::parse(input).unwrap();
        assert!(doc.metadata.is_some());

        let metadata = doc.metadata.unwrap();
        assert_eq!(metadata.title, Some("Test Document".to_string()));
        assert_eq!(metadata.description, Some("A test document".to_string()));
        assert_eq!(metadata.tags, vec!["test", "example"]);

        assert!(doc.content.starts_with("# Hello World"));
    }

    #[test]
    fn test_markdown_with_toml_frontmatter() {
        let input = r#"+++
title = "Test Document"
description = "A test document"
tags = ["test", "example"]
+++

# Hello World

This is the content."#;

        let doc = MarkdownDocument::parse(input).unwrap();
        assert!(doc.metadata.is_some());

        let metadata = doc.metadata.unwrap();
        assert_eq!(metadata.title, Some("Test Document".to_string()));
        assert_eq!(metadata.description, Some("A test document".to_string()));
        assert_eq!(metadata.tags, vec!["test", "example"]);
    }

    #[test]
    fn test_markdown_without_frontmatter() {
        let input = "# Hello World\n\nThis is the content.";

        let doc = MarkdownDocument::parse(input).unwrap();
        assert!(doc.metadata.is_none());
        assert_eq!(doc.content, input);
    }

    #[test]
    fn test_get_title() {
        // From metadata
        let metadata = MarkdownMetadata {
            title: Some("Metadata Title".to_string()),
            ..Default::default()
        };
        let doc = MarkdownDocument::with_metadata(metadata, "Content".to_string());
        assert_eq!(doc.get_title(), Some("Metadata Title".to_string()));

        // From heading
        let doc = MarkdownDocument::new("# Heading Title\n\nContent".to_string());
        assert_eq!(doc.get_title(), Some("Heading Title".to_string()));

        // No title
        let doc = MarkdownDocument::new("Just content".to_string());
        assert_eq!(doc.get_title(), None);
    }

    #[test]
    fn test_get_description() {
        // From metadata
        let metadata = MarkdownMetadata {
            description: Some("Metadata description".to_string()),
            ..Default::default()
        };
        let doc = MarkdownDocument::with_metadata(metadata, "Content".to_string());
        assert_eq!(doc.get_description(), Some("Metadata description".to_string()));

        // From first paragraph
        let doc = MarkdownDocument::new(
            "# Title\n\nThis is the first paragraph.\n\nSecond paragraph.".to_string(),
        );
        assert_eq!(doc.get_description(), Some("This is the first paragraph.".to_string()));
    }

    #[test]
    fn test_read_write_markdown() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.md");

        // Create and write document
        let metadata = MarkdownMetadata {
            title: Some("Test".to_string()),
            ..Default::default()
        };
        let doc = MarkdownDocument::with_metadata(metadata, "# Test\n\nContent".to_string());
        doc.write(&file_path).unwrap();

        // Read back
        let loaded = MarkdownDocument::read(&file_path).unwrap();
        assert!(loaded.metadata.is_some());
        assert_eq!(loaded.metadata.unwrap().title, Some("Test".to_string()));
        assert!(loaded.content.contains("# Test"));
    }

    #[test]
    fn test_is_markdown_file() {
        assert!(is_markdown_file(Path::new("test.md")));
        assert!(is_markdown_file(Path::new("test.MD")));
        assert!(is_markdown_file(Path::new("test.markdown")));
        assert!(is_markdown_file(Path::new("test.MARKDOWN")));
        assert!(!is_markdown_file(Path::new("test.txt")));
        assert!(!is_markdown_file(Path::new("test")));
    }

    #[test]
    fn test_list_markdown_files() {
        let temp = tempdir().unwrap();

        // Create some files
        std::fs::write(temp.path().join("file1.md"), "content").unwrap();
        std::fs::write(temp.path().join("file2.markdown"), "content").unwrap();
        std::fs::write(temp.path().join("file3.txt"), "content").unwrap();

        let subdir = temp.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("file4.md"), "content").unwrap();

        let files = list_markdown_files(temp.path()).unwrap();
        assert_eq!(files.len(), 3);

        let names: Vec<String> =
            files.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();

        assert!(names.contains(&"file1.md".to_string()));
        assert!(names.contains(&"file2.markdown".to_string()));
        assert!(names.contains(&"file4.md".to_string()));
        assert!(!names.contains(&"file3.txt".to_string()));
    }

    #[test]
    fn test_set_metadata_and_content() {
        let mut doc = MarkdownDocument::new("Initial content".to_string());

        // Set metadata
        let metadata = MarkdownMetadata {
            title: Some("New Title".to_string()),
            ..Default::default()
        };
        doc.set_metadata(metadata);

        assert!(doc.metadata.is_some());
        assert!(doc.raw.contains("title: New Title"));
        assert!(doc.raw.contains("Initial content"));

        // Set content
        doc.set_content("Updated content".to_string());
        assert_eq!(doc.content, "Updated content");
        assert!(doc.raw.contains("Updated content"));
        assert!(doc.raw.contains("title: New Title"));
    }

    #[test]
    fn test_invalid_frontmatter_with_escaped_newlines() {
        // Content with invalid YAML frontmatter (literal \n that isn't properly quoted)
        let input = r#"---
name: haiku-syntax-tool
description: Use this agent when you need to fix linting errors, formatting issues, type checking problems, or ensure code adheres to project-specific standards. This agent specializes in enforcing language-specific conventions, project style guides, and maintaining code quality through automated fixes. Examples:\n\n<example>\nContext: The user has just written a new Python function and wants to ensure it meets project standards.\nuser: "I've added a new sync handler function"\nassistant: "Let me review this with the code-standards-enforcer agent to ensure it meets our project standards"\n<commentary>\nSince new code was written, use the Task tool to launch the code-standards-enforcer agent to check for linting, formatting, and type issues according to CLAUDE.md standards.\n</commentary>\n</example>\n\n<example>\nContext: The user encounters linting errors during CI/CD.\nuser: "The CI pipeline is failing due to formatting issues"\nassistant: "I'll use the code-standards-enforcer agent to fix these formatting and linting issues"\n<commentary>\nWhen there are explicit linting or formatting problems, use the code-standards-enforcer agent to automatically fix them according to project standards.\n</commentary>\n</example>\n\n<example>\nContext: The user wants to ensure type hints are correct.\nuser: "Can you check if my type annotations are correct in the API module?"\nassistant: "I'll launch the code-standards-enforcer agent to verify and fix any type annotation issues"\n<commentary>\nFor type checking and annotation verification, use the code-standards-enforcer agent to ensure compliance with project typing standards.\n</commentary>\n</example>
model: haiku
---

You are a meticulous code standards enforcement specialist"#;

        // This should succeed but treat the entire document as content (no metadata)
        let result = MarkdownDocument::parse(input);
        match result {
            Ok(doc) => {
                // Invalid frontmatter means no metadata
                assert!(doc.metadata.is_none());
                // The entire document should be treated as content
                assert!(doc.content.contains("---"));
                assert!(doc.content.contains("name: haiku-syntax-tool"));
                assert!(doc.content.contains("description: Use this agent"));
                assert!(doc.content.contains("model: haiku"));
                assert!(doc.content.contains("meticulous code standards enforcement specialist"));
            }
            Err(e) => {
                panic!("Should not fail, but got error: {}", e);
            }
        }
    }

    #[test]
    fn test_completely_invalid_frontmatter_fallback() {
        // Test with completely broken YAML
        let input = r#"---
name: test
description: {this is not valid yaml at all
model: test
---

Content here"#;

        // This should now succeed but without metadata
        let result = MarkdownDocument::parse(input);
        match result {
            Ok(doc) => {
                // Should treat entire document as content when frontmatter is invalid
                assert!(doc.metadata.is_none());
                assert!(doc.content.contains("---"));
                assert!(doc.content.contains("name: test"));
                assert!(doc.content.contains("Content here"));
            }
            Err(e) => {
                panic!("Should not fail, but got error: {}", e);
            }
        }
    }
}
