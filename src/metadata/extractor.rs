//! Extract dependency metadata from resource files.
//!
//! This module handles the extraction of transitive dependency information
//! from resource files. Supports YAML frontmatter in Markdown files and
//! JSON fields in JSON configuration files.

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::path::Path;

use crate::manifest::DependencyMetadata;

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
    ///
    /// # Returns
    /// * `DependencyMetadata` - Extracted metadata (may be empty)
    pub fn extract(path: &Path, content: &str) -> Result<DependencyMetadata> {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        match extension {
            "md" => Self::extract_markdown_frontmatter(content),
            "json" => Self::extract_json_field(content),
            _ => {
                // Scripts and other files don't support embedded dependencies
                Ok(DependencyMetadata::default())
            }
        }
    }

    /// Extract YAML frontmatter from Markdown content.
    ///
    /// Looks for content between `---` delimiters at the start of the file.
    fn extract_markdown_frontmatter(content: &str) -> Result<DependencyMetadata> {
        // Check if content starts with frontmatter delimiter
        if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
            return Ok(DependencyMetadata::default());
        }

        // Find the end of frontmatter
        let search_start = if content.starts_with("---\n") { 4 } else { 5 };

        let end_pattern = if content.contains("\r\n") {
            "\r\n---\r\n"
        } else {
            "\n---\n"
        };

        if let Some(end_pos) = content[search_start..].find(end_pattern) {
            let frontmatter = &content[search_start..search_start + end_pos];

            // Parse YAML frontmatter
            match serde_yaml::from_str::<DependencyMetadata>(frontmatter) {
                Ok(metadata) => Ok(metadata),
                Err(e) => {
                    // Log warning but don't fail - malformed frontmatter is not fatal
                    tracing::warn!("Warning: Unable to parse YAML frontmatter: {}", e);
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
    fn extract_json_field(content: &str) -> Result<DependencyMetadata> {
        let json: JsonValue =
            serde_json::from_str(content).with_context(|| "Failed to parse JSON content")?;

        if let Some(deps) = json.get("dependencies") {
            // The dependencies field should match our expected structure
            let dependencies = serde_json::from_value(deps.clone())
                .with_context(|| "Failed to parse dependencies field")?;

            Ok(DependencyMetadata {
                dependencies: Some(dependencies),
            })
        } else {
            Ok(DependencyMetadata::default())
        }
    }

    /// Extract metadata from file content without knowing the file type.
    ///
    /// Tries to detect the format automatically.
    pub fn extract_auto(content: &str) -> Result<DependencyMetadata> {
        // Try YAML frontmatter first (for Markdown)
        if (content.starts_with("---\n") || content.starts_with("---\r\n"))
            && let Ok(metadata) = Self::extract_markdown_frontmatter(content)
            && metadata.has_dependencies()
        {
            return Ok(metadata);
        }

        // Try JSON format
        if content.trim_start().starts_with('{')
            && let Ok(metadata) = Self::extract_json_field(content)
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
        let metadata = MetadataExtractor::extract(path, content).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_json_dependencies() {
        let content = r#"{
  "events": ["UserPromptSubmit"],
  "type": "command",
  "command": ".claude/ccpm/scripts/test.js",
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
        let metadata = MetadataExtractor::extract(path, content).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content).unwrap();

        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_extract_script_file() {
        let content = r#"#!/bin/bash
echo "This is a script file"
# Scripts don't support dependencies"#;

        let path = Path::new("script.sh");
        let metadata = MetadataExtractor::extract(path, content).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content).unwrap();

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
        let metadata = MetadataExtractor::extract(path, content).unwrap();

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
        let result = MetadataExtractor::extract(path, content);

        // Should succeed but return empty metadata (with warning logged)
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert!(metadata.dependencies.is_none());
    }
}
