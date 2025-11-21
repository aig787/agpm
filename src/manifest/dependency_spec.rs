//! Transitive dependency specifications for resources.
//!
//! This module defines the structures used to represent transitive dependencies
//! that resources can declare within their files (via YAML frontmatter or JSON fields).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

    /// Optional flatten flag to control directory structure preservation.
    ///
    /// When `true`, only the filename is used for installation (e.g., `nested/dir/file.md` → `file.md`).
    /// When `false` (default for most resources), the full relative path is preserved.
    ///
    /// Default values by resource type:
    /// - `agents`: `true` (flatten by default)
    /// - `commands`: `true` (flatten by default)
    /// - All others: `false` (preserve directory structure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flatten: Option<bool>,

    /// Optional flag to control whether the dependency should be installed to disk.
    ///
    /// When `false`, the dependency will be resolved, fetched, and its content made available
    /// in template context via `agpm.deps.<type>.<name>.content`, but the file will not be
    /// written to the project directory. This is useful for snippet embedding use cases where
    /// you want to include content inline rather than as a separate file.
    ///
    /// See templating module for details on how content is accessed
    /// in templates.
    ///
    /// Default: `true` (install the file)
    ///
    /// Example:
    /// ```yaml
    /// dependencies:
    ///   snippets:
    ///     - path: "snippets/rust-best-practices.md"
    ///       install: false  # Don't create a separate file
    ///       name: "best_practices"
    /// ```
    /// Then use in template: `{{ agpm.deps.snippets.best_practices.content }}`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<bool>,
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
    pub dependencies: Option<BTreeMap<String, Vec<DependencySpec>>>,

    /// AGPM-specific metadata wrapper supporting templating and nested dependencies.
    ///
    /// Example with templating flag and nested dependencies:
    /// ```yaml
    /// agpm:
    ///   templating: true
    ///   dependencies:
    ///     snippets:
    ///       - path: snippets/utils.md
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agpm: Option<AgpmMetadata>,

    /// Cached merged dependencies for efficient access.
    /// This field is not serialized and is computed on demand.
    #[serde(skip)]
    merged_cache: std::cell::OnceCell<BTreeMap<String, Vec<DependencySpec>>>,
}

/// AGPM-specific metadata for templating and nested dependency declarations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgpmMetadata {
    /// Enable templating for this resource (default: false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub templating: Option<bool>,

    /// Dependencies nested under `agpm` section (takes precedence over root-level).
    ///
    /// Example:
    /// ```yaml
    /// agpm:
    ///   templating: true
    ///   dependencies:
    ///     snippets:
    ///       - path: snippets/utils.md
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<BTreeMap<String, Vec<DependencySpec>>>,
}

impl DependencyMetadata {
    /// Create a new DependencyMetadata with the given dependencies and agpm metadata.
    pub fn new(
        dependencies: Option<BTreeMap<String, Vec<DependencySpec>>>,
        agpm: Option<AgpmMetadata>,
    ) -> Self {
        Self {
            dependencies,
            agpm,
            merged_cache: std::cell::OnceCell::new(),
        }
    }

    /// Get merged dependencies from both nested and root-level locations.
    ///
    /// Merges `agpm.dependencies` and `dependencies` into a unified view.
    /// Root-level dependencies are added first, then nested dependencies.
    /// Duplicates (same path and name) are removed, keeping the first occurrence.
    pub fn get_dependencies(&self) -> Option<&BTreeMap<String, Vec<DependencySpec>>> {
        // Check if we have any dependencies at all
        let has_root_deps = self.dependencies.is_some();
        let has_nested_deps =
            self.agpm.as_ref().and_then(|agpm| agpm.dependencies.as_ref()).is_some();

        if !has_root_deps && !has_nested_deps {
            return None;
        }

        // Use OnceCell for lazy caching of merged dependencies
        let merged = self
            .merged_cache
            .get_or_init(|| self.compute_merged_dependencies().unwrap_or_default());

        // Return None if the merged result is empty
        if merged.is_empty() {
            None
        } else {
            Some(merged)
        }
    }

    /// Get merged dependencies with ResourceType keys instead of strings.
    ///
    /// This is a type-safe version of `get_dependencies()` that parses the
    /// string keys into ResourceType enums. Invalid resource type strings are logged
    /// and skipped.
    ///
    /// # Returns
    ///
    /// HashMap with ResourceType keys, or None if no valid dependencies
    pub fn get_dependencies_typed(
        &self,
    ) -> Option<std::collections::HashMap<crate::core::ResourceType, Vec<DependencySpec>>> {
        let deps = self.get_dependencies()?;
        let mut result = std::collections::HashMap::new();

        for (resource_type_str, specs) in deps {
            // Parse string to ResourceType
            if let Ok(resource_type) = resource_type_str.parse::<crate::core::ResourceType>() {
                result.insert(resource_type, specs.clone());
            } else {
                tracing::warn!("Unknown resource type in dependencies: {}", resource_type_str);
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Compute merged dependencies from both sources.
    ///
    /// This method performs the actual merging of dependencies from both sources.
    /// Returns None if neither source has dependencies.
    fn compute_merged_dependencies(&self) -> Option<BTreeMap<String, Vec<DependencySpec>>> {
        let mut merged: BTreeMap<String, Vec<DependencySpec>> = BTreeMap::new();
        let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Add root-level dependencies first
        if let Some(root_deps) = &self.dependencies {
            for (resource_type, specs) in root_deps {
                let filtered_specs: Vec<DependencySpec> = specs
                    .iter()
                    .filter(|spec| seen_paths.insert(spec.path.clone()))
                    .cloned()
                    .collect();

                if !filtered_specs.is_empty() {
                    merged.insert(resource_type.clone(), filtered_specs);
                }
            }
        }

        // Add nested dependencies second (root takes precedence for duplicates)
        if let Some(agpm) = &self.agpm {
            if let Some(nested_deps) = &agpm.dependencies {
                for (resource_type, specs) in nested_deps {
                    let existing_specs = merged.entry(resource_type.clone()).or_default();
                    let filtered_specs: Vec<DependencySpec> = specs
                        .iter()
                        .filter(|spec| seen_paths.insert(spec.path.clone()))
                        .cloned()
                        .collect();

                    existing_specs.extend(filtered_specs);

                    // Remove empty resource type entries
                    if existing_specs.is_empty() {
                        merged.remove(resource_type);
                    }
                }
            }
        }

        // Return None if no actual dependencies were added
        if merged.is_empty() {
            None
        } else {
            Some(merged)
        }
    }

    /// Check if metadata contains any non-empty dependencies.
    pub fn has_dependencies(&self) -> bool {
        self.get_dependencies()
            .is_some_and(|deps| !deps.is_empty() && deps.values().any(|v| !v.is_empty()))
    }

    /// Count total dependencies across all resource types.
    pub fn dependency_count(&self) -> usize {
        self.get_dependencies().map_or(0, |deps| deps.values().map(std::vec::Vec::len).sum())
    }

    /// Merge another metadata into this one.
    ///
    /// Used when combining dependencies from multiple sources.
    pub fn merge(&mut self, other: Self) {
        // Clear cache since we're modifying the dependencies
        self.merged_cache = std::cell::OnceCell::new();

        if let Some(other_deps) = other.dependencies {
            let deps = self.dependencies.get_or_insert_with(BTreeMap::new);
            for (resource_type, specs) in other_deps {
                deps.entry(resource_type).or_default().extend(specs);
            }
        }

        // Also merge agpm dependencies if present
        if let Some(other_agpm) = other.agpm {
            if let Some(other_agpm_deps) = other_agpm.dependencies {
                let agpm = self.agpm.get_or_insert(AgpmMetadata {
                    templating: None,
                    dependencies: None,
                });
                let agpm_deps = agpm.dependencies.get_or_insert_with(BTreeMap::new);
                for (resource_type, specs) in other_agpm_deps {
                    agpm_deps.entry(resource_type).or_default().extend(specs);
                }
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
            flatten: None,
            install: None,
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
            flatten: None,
            install: None,
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
        let metadata = DependencyMetadata::default();
        assert!(!metadata.has_dependencies());

        let metadata = DependencyMetadata::new(Some(BTreeMap::new()), None);
        assert!(!metadata.has_dependencies());

        let mut deps = BTreeMap::new();
        deps.insert("agents".to_string(), vec![]);
        let metadata = DependencyMetadata::new(Some(deps), None);
        assert!(!metadata.has_dependencies());

        let mut deps = BTreeMap::new();
        deps.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "test.md".to_string(),
                name: None,
                version: None,
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        let metadata = DependencyMetadata::new(Some(deps), None);
        assert!(metadata.has_dependencies());
    }

    #[test]
    fn test_dependency_metadata_merge() {
        let mut metadata1 = DependencyMetadata::default();
        let mut deps1 = BTreeMap::new();
        deps1.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "agent1.md".to_string(),
                name: None,
                version: None,
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        metadata1.dependencies = Some(deps1);

        let mut metadata2 = DependencyMetadata::default();
        let mut deps2 = BTreeMap::new();
        deps2.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "agent2.md".to_string(),
                name: None,
                version: None,
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        deps2.insert(
            "snippets".to_string(),
            vec![DependencySpec {
                path: "snippet1.md".to_string(),
                name: None,
                version: Some("v1.0.0".to_string()),
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        metadata2.dependencies = Some(deps2);

        metadata1.merge(metadata2);

        assert_eq!(metadata1.dependency_count(), 3);
        let deps = metadata1.get_dependencies().unwrap();
        assert_eq!(deps["agents"].len(), 2);
        assert_eq!(deps["snippets"].len(), 1);
    }

    #[test]
    fn test_merged_dependencies_root_only() {
        let mut root_deps = BTreeMap::new();
        root_deps.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "agent1.md".to_string(),
                name: None,
                version: Some("v1.0.0".to_string()),
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        let metadata = DependencyMetadata::new(Some(root_deps), None);

        let merged = metadata.get_dependencies().unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged["agents"].len(), 1);
        assert_eq!(merged["agents"][0].path, "agent1.md");
        assert_eq!(metadata.dependency_count(), 1);
        assert!(metadata.has_dependencies());
    }

    #[test]
    fn test_merged_dependencies_nested_only() {
        let mut nested_deps = BTreeMap::new();
        nested_deps.insert(
            "snippets".to_string(),
            vec![DependencySpec {
                path: "utils.md".to_string(),
                name: Some("utils".to_string()),
                version: Some("v2.0.0".to_string()),
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        let agpm = AgpmMetadata {
            templating: Some(true),
            dependencies: Some(nested_deps),
        };
        let metadata = DependencyMetadata::new(None, Some(agpm));

        let merged = metadata.get_dependencies().unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged["snippets"].len(), 1);
        assert_eq!(merged["snippets"][0].path, "utils.md");
        assert_eq!(merged["snippets"][0].name, Some("utils".to_string()));
        assert_eq!(metadata.dependency_count(), 1);
        assert!(metadata.has_dependencies());
    }

    #[test]
    fn test_merged_dependencies_both_sources() {
        // Root-level dependencies
        let mut root_deps = BTreeMap::new();
        root_deps.insert(
            "agents".to_string(),
            vec![
                DependencySpec {
                    path: "agent1.md".to_string(),
                    name: None,
                    version: Some("v1.0.0".to_string()),
                    tool: None,
                    flatten: None,
                    install: None,
                },
                DependencySpec {
                    path: "shared.md".to_string(),
                    name: Some("shared_root".to_string()),
                    version: Some("v1.0.0".to_string()),
                    tool: None,
                    flatten: None,
                    install: None,
                },
            ],
        );

        // Nested dependencies
        let mut nested_deps = BTreeMap::new();
        nested_deps.insert(
            "snippets".to_string(),
            vec![DependencySpec {
                path: "utils.md".to_string(),
                name: None,
                version: Some("v2.0.0".to_string()),
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        nested_deps.insert(
            "agents".to_string(),
            vec![
                DependencySpec {
                    path: "agent2.md".to_string(),
                    name: None,
                    version: Some("v2.0.0".to_string()),
                    tool: None,
                    flatten: None,
                    install: None,
                },
                // Duplicate path with different name - should be filtered out
                DependencySpec {
                    path: "shared.md".to_string(),
                    name: Some("shared_nested".to_string()),
                    version: Some("v2.0.0".to_string()),
                    tool: None,
                    flatten: None,
                    install: None,
                },
            ],
        );
        let agpm = AgpmMetadata {
            templating: Some(true),
            dependencies: Some(nested_deps),
        };
        let metadata = DependencyMetadata::new(Some(root_deps), Some(agpm));

        let merged = metadata.get_dependencies().unwrap();

        // Should have both resource types
        assert_eq!(merged.len(), 2);

        // Agents should have 3 total (2 root + 1 nested, duplicate filtered)
        assert_eq!(merged["agents"].len(), 3);
        assert_eq!(merged["agents"][0].path, "agent1.md");
        assert_eq!(merged["agents"][1].path, "shared.md");
        assert_eq!(merged["agents"][1].name, Some("shared_root".to_string()));
        assert_eq!(merged["agents"][2].path, "agent2.md");

        // Snippets should have 1 from nested
        assert_eq!(merged["snippets"].len(), 1);
        assert_eq!(merged["snippets"][0].path, "utils.md");

        assert_eq!(metadata.dependency_count(), 4);
        assert!(metadata.has_dependencies());
    }

    #[test]
    fn test_merged_dependencies_no_duplicates() {
        // Root-level dependencies
        let mut root_deps = BTreeMap::new();
        root_deps.insert(
            "agents".to_string(),
            vec![
                DependencySpec {
                    path: "agent.md".to_string(),
                    name: None,
                    version: Some("v1.0.0".to_string()),
                    tool: None,
                    flatten: None,
                    install: None,
                },
                DependencySpec {
                    path: "agent.md".to_string(),
                    name: Some("custom".to_string()),
                    version: Some("v1.0.0".to_string()),
                    tool: None,
                    flatten: None,
                    install: None,
                },
            ],
        );

        // Nested dependencies with same path
        let mut nested_deps = BTreeMap::new();
        nested_deps.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "agent.md".to_string(),
                name: Some("nested".to_string()),
                version: Some("v2.0.0".to_string()),
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        let agpm = AgpmMetadata {
            templating: None,
            dependencies: Some(nested_deps),
        };
        let metadata = DependencyMetadata::new(Some(root_deps), Some(agpm));

        let merged = metadata.get_dependencies().unwrap();

        // Should only have 1 dependency (duplicates filtered)
        assert_eq!(merged.len(), 1);
        assert_eq!(merged["agents"].len(), 1);
        assert_eq!(merged["agents"][0].path, "agent.md");
        assert_eq!(merged["agents"][0].name, None); // First occurrence kept

        assert_eq!(metadata.dependency_count(), 1);
    }

    #[test]
    fn test_merged_dependencies_empty() {
        let metadata = DependencyMetadata::default();

        assert!(metadata.get_dependencies().is_none());
        assert_eq!(metadata.dependency_count(), 0);
        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_merged_dependencies_empty_maps() {
        let agpm = AgpmMetadata {
            templating: None,
            dependencies: Some(BTreeMap::new()),
        };
        let metadata = DependencyMetadata::new(Some(BTreeMap::new()), Some(agpm));

        assert!(metadata.get_dependencies().is_none());
        assert_eq!(metadata.dependency_count(), 0);
        assert!(!metadata.has_dependencies());
    }

    #[test]
    fn test_merged_dependencies_with_agpm_merge() {
        let mut metadata1 = DependencyMetadata::default();
        let mut root_deps = BTreeMap::new();
        root_deps.insert(
            "agents".to_string(),
            vec![DependencySpec {
                path: "agent1.md".to_string(),
                name: None,
                version: None,
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        metadata1.dependencies = Some(root_deps);

        let mut metadata2 = DependencyMetadata::default();
        let mut nested_deps = BTreeMap::new();
        nested_deps.insert(
            "snippets".to_string(),
            vec![DependencySpec {
                path: "snippet1.md".to_string(),
                name: None,
                version: None,
                tool: None,
                flatten: None,
                install: None,
            }],
        );
        metadata2.agpm = Some(AgpmMetadata {
            templating: Some(true),
            dependencies: Some(nested_deps),
        });

        metadata1.merge(metadata2);

        let merged = metadata1.get_dependencies().unwrap();
        assert_eq!(merged.len(), 2); // Both resource types present
        assert_eq!(metadata1.dependency_count(), 2);
        assert!(metadata1.agpm.is_some());
        assert!(metadata1.agpm.unwrap().dependencies.is_some());
    }
}
