//! Transitive dependency extraction.
//!
//! This module provides reusable functions for extracting transitive dependencies
//! from resource files. Used by both the main transitive resolver and the
//! backtracking resolver for re-extracting dependencies after version changes.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::core::ResourceType;
use crate::manifest::DependencySpec;
use crate::metadata::MetadataExtractor;

/// Extract transitive dependencies from a resource file.
///
/// This is a simplified extraction function that reads a resource file,
/// extracts its metadata, and returns the raw dependency specifications
/// grouped by resource type.
///
/// # Arguments
///
/// * `worktree_path` - Path to the worktree containing the resource
/// * `resource_path` - Relative path to the resource file within worktree
/// * `variant_inputs` - Optional template variables for frontmatter rendering
///
/// # Returns
///
/// Map of resource_type → Vec<DependencySpec>
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use agpm_cli::resolver::transitive_extractor::extract_transitive_deps;
///
/// # async fn example() -> anyhow::Result<()> {
/// let worktree = Path::new("/path/to/worktree");
/// let resource = "agents/helper.md";
///
/// let deps = extract_transitive_deps(worktree, resource, None).await?;
/// for (resource_type, specs) in deps {
///     println!("{:?}: {} dependencies", resource_type, specs.len());
/// }
/// # Ok(())
/// # }
/// ```
pub async fn extract_transitive_deps(
    worktree_path: &Path,
    resource_path: &str,
    variant_inputs: Option<&serde_json::Value>,
) -> Result<HashMap<ResourceType, Vec<DependencySpec>>> {
    // Build full path to the resource file
    let file_path = worktree_path.join(resource_path);

    // Read file content
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .with_context(|| format!("Failed to read resource file: {}", file_path.display()))?;

    // Extract metadata (no operation context needed for backtracking)
    let metadata = MetadataExtractor::extract(&file_path, &content, variant_inputs, None)
        .with_context(|| format!("Failed to extract metadata from: {}", file_path.display()))?;

    // Get typed dependencies (with ResourceType keys)
    Ok(metadata.get_dependencies_typed().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[tokio::test]
    async fn test_extract_from_markdown_with_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r#"---
dependencies:
  agents:
    - path: agents/helper.md
      version: v1.0.0
  snippets:
    - path: snippets/guide.md
---
# Test Agent
"#;

        tokio::fs::write(&file_path, content).await.unwrap();

        let deps = extract_transitive_deps(temp_dir.path(), "test.md", None).await.unwrap();

        assert_eq!(deps.len(), 2);
        assert!(deps.contains_key(&ResourceType::Agent));
        assert!(deps.contains_key(&ResourceType::Snippet));

        let agents = &deps[&ResourceType::Agent];
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].path, "agents/helper.md");
        assert_eq!(agents[0].version.as_deref(), Some("v1.0.0"));
    }

    #[tokio::test]
    async fn test_extract_from_file_without_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = "# Simple Agent\n\nNo dependencies here.";
        tokio::fs::write(&file_path, content).await.unwrap();

        let deps = extract_transitive_deps(temp_dir.path(), "test.md", None).await.unwrap();

        assert_eq!(deps.len(), 0);
    }

    #[tokio::test]
    async fn test_extract_from_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let content = r#"{
  "name": "test",
  "dependencies": {
    "agents": [
      {
        "path": "agents/helper.md",
        "version": "v1.0.0"
      }
    ]
  }
}"#;

        tokio::fs::write(&file_path, content).await.unwrap();

        let deps = extract_transitive_deps(temp_dir.path(), "test.json", None).await.unwrap();

        assert_eq!(deps.len(), 1);
        assert!(deps.contains_key(&ResourceType::Agent));

        let agents = &deps[&ResourceType::Agent];
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].path, "agents/helper.md");
    }

    #[tokio::test]
    async fn test_extract_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();

        let result = extract_transitive_deps(temp_dir.path(), "nonexistent.md", None).await;

        assert!(result.is_err());
    }
}
