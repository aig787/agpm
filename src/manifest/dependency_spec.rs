//! Transitive dependency specifications for resources.
//!
//! This module defines the structures used to represent transitive dependencies
//! that resources can declare within their files (via YAML frontmatter or JSON fields).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dependency specification without the source field.
///
/// Used within resource files to declare dependencies on other resources
/// from the same source repository. The source is implicit and inherited
/// from the resource that declares the dependency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DependencySpec {
    /// Path to the dependency file within the source repository.
    ///
    /// This can be either:
    /// - A specific file path: `"agents/helper.md"`
    /// - A glob pattern: `"agents/*.md"`, `"agents/**/review*.md"`
    pub path: String,

    /// Optional custom name for the dependency in template context.
    ///
    /// If specified, this name will be used as the key when accessing this
    /// dependency in templates (e.g., `agpm.deps.agents.custom_name`).
    /// If not specified, the name is derived from the path.
    ///
    /// Example:
    /// ```yaml
    /// dependencies:
    ///   agents:
    ///     - path: "agents/complex-path/helper.md"
    ///       name: "helper"
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional version constraint for the dependency.
    ///
    /// If not specified, the version of the declaring resource is used.
    /// Supports the same version formats as manifest dependencies:
    /// - Exact version: `"v1.0.0"`
    /// - Latest: `"latest"`
    /// - Branch: `"main"`
    /// - Commit: `"abc123..."`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Optional tool specification for this dependency.
    ///
    /// If not specified, inherits from parent (if parent's tool supports this resource type)
    /// or falls back to the default tool for this resource type.
    /// - "claude-code" - Install to `.claude/` directories
    /// - "opencode" - Install to `.opencode/` directories
    /// - "agpm" - Install to `.agpm/` directories
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// Metadata extracted from resource files.
///
/// This structure represents the dependency information that can be
/// embedded within resource files themselves, either as YAML frontmatter
/// in Markdown files or as JSON fields in JSON configuration files.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DependencyMetadata {
    /// Maps resource type to list of dependency specifications.
    ///
    /// The keys are resource types: "agents", "snippets", "commands",
    /// "scripts", "hooks", "mcp-servers".
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<HashMap<String, Vec<DependencySpec>>>,
}

impl DependencyMetadata {
    /// Check if this metadata contains any dependencies.
    pub fn has_dependencies(&self) -> bool {
        self.dependencies
            .as_ref()
            .is_some_and(|deps| !deps.is_empty() && deps.values().any(|v| !v.is_empty()))
    }

    /// Get the total count of dependencies.
    pub fn dependency_count(&self) -> usize {
        self.dependencies.as_ref().map_or(0, |deps| deps.values().map(std::vec::Vec::len).sum())
    }

    /// Merge another metadata into this one.
    ///
    /// Used when combining dependencies from multiple sources.
    pub fn merge(&mut self, other: Self) {
        if let Some(other_deps) = other.dependencies {
            let deps = self.dependencies.get_or_insert_with(HashMap::new);
            for (resource_type, specs) in other_deps {
                deps.entry(resource_type).or_default().extend(specs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_spec_serialization() {
        let spec = DependencySpec {
            path: "agents/helper.md".to_string(),
            name: None,
            version: Some("v1.0.0".to_string()),
            tool: None,
        };

        let yaml = serde_yaml::to_string(&spec).unwrap();
        assert!(yaml.contains("path: agents/helper.md"));
        assert!(yaml.contains("version: v1.0.0"));

        let deserialized: DependencySpec = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(spec, deserialized);
    }

    #[test]
    fn test_dependency_spec_with_tool() {
        let spec = DependencySpec {
            path: "agents/helper.md".to_string(),
            name: None,
            version: Some("v1.0.0".to_string()),
            tool: Some("opencode".to_string()),
        };

        let yaml = serde_yaml::to_string(&spec).unwrap();
        assert!(yaml.contains("path: agents/helper.md"));
        assert!(yaml.contains("version: v1.0.0"));
        assert!(yaml.contains("tool: opencode"));

        let deserialized: DependencySpec = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(spec, deserialized);
        assert_eq!(deserialized.tool, Some("opencode".to_string()));
    }

    #[test]
    fn test_dependency_metadata_has_dependencies() {
        let mut metadata = DependencyMetadata::default();
        assert!(!metadata.has_dependencies());

        metadata.dependencies = Some(HashMap::new());
        assert!(!metadata.has_dependencies());

        let mut deps = HashMap::new();
        deps.insert("agents".to_string(), vec![]);
        metadata.dependencies = Some(deps);
        assert!(!metadata.has_dependencies());

        let mut deps = HashMap::new();
        deps.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "test.md".to_string(),
                name: None,
                version: None,
                tool: None,
            }],
        );
        metadata.dependencies = Some(deps);
        assert!(metadata.has_dependencies());
    }

    #[test]
    fn test_dependency_metadata_merge() {
        let mut metadata1 = DependencyMetadata::default();
        let mut deps1 = HashMap::new();
        deps1.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "agent1.md".to_string(),
                name: None,
                version: None,
                tool: None,
            }],
        );
        metadata1.dependencies = Some(deps1);

        let mut metadata2 = DependencyMetadata::default();
        let mut deps2 = HashMap::new();
        deps2.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "agent2.md".to_string(),
                name: None,
                version: None,
                tool: None,
            }],
        );
        deps2.insert(
            "snippets".to_string(),
            vec![DependencySpec {
                path: "snippet1.md".to_string(),
                name: None,
                version: Some("v1.0.0".to_string()),
                tool: None,
            }],
        );
        metadata2.dependencies = Some(deps2);

        metadata1.merge(metadata2);

        assert_eq!(metadata1.dependency_count(), 3);
        let deps = metadata1.dependencies.as_ref().unwrap();
        assert_eq!(deps["agents"].len(), 2);
        assert_eq!(deps["snippets"].len(), 1);
    }
}
