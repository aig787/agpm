//! Conflict detection service for version and path conflicts.
//!
//! This module provides high-level orchestration for conflict detection,
//! wrapping the lower-level ConflictDetector functionality.

use anyhow::Result;
use std::collections::HashMap;

use crate::core::ResourceType;
use crate::manifest::{DetailedDependency, ResourceDependency};
use crate::version::conflict::{ConflictDetector, VersionConflict};

use super::types::DependencyKey;

/// Conflict detection service.
///
/// This service wraps the ConflictDetector and provides high-level methods
/// for detecting version conflicts and path conflicts in dependencies.
#[allow(dead_code)] // detector field not yet used in service-based refactoring
pub struct ConflictService {
    detector: ConflictDetector,
}

impl ConflictService {
    /// Create a new conflict service.
    pub fn new() -> Self {
        Self {
            detector: ConflictDetector::new(),
        }
    }

    /// Detect version conflicts in the provided dependencies.
    ///
    /// # Arguments
    ///
    /// * `dependencies` - Map of dependencies by (type, name) key
    ///
    /// # Returns
    ///
    /// A vector of detected version conflicts
    pub fn detect_version_conflicts(
        &mut self,
        dependencies: &HashMap<DependencyKey, DetailedDependency>,
    ) -> Result<Vec<VersionConflict>> {
        let mut conflicts = Vec::new();

        // Group dependencies by (type, path, source, tool) to find version conflicts
        let mut grouped: HashMap<(ResourceType, String, String, String), Vec<_>> = HashMap::new();

        for (key, dep) in dependencies {
            let source = dep.source.clone().unwrap_or_default();
            let tool = dep.tool.clone().unwrap_or_default();

            let group_key = (
                key.0, // resource_type
                dep.path.clone(),
                source,
                tool,
            );
            grouped.entry(group_key).or_default().push((key, dep));
        }

        // Check each group for version conflicts
        for ((resource_type, path, source, _tool), deps) in grouped {
            if deps.len() > 1 {
                // Multiple versions of the same resource
                let mut conflicting_requirements = Vec::new();

                for (key, dep) in &deps {
                    let requirement = dep.version.clone().unwrap_or_else(|| "latest".to_string());

                    conflicting_requirements.push(
                        crate::version::conflict::ConflictingRequirement {
                            required_by: format!("{}/{}", key.0, key.1), // resource_type, name
                            requirement,
                            resolved_sha: String::new(), // No SHA available at this stage
                            resolved_version: None,
                        },
                    );
                }

                conflicts.push(VersionConflict {
                    resource: format!("{}/{}/{}", resource_type, path, source),
                    conflicting_requirements,
                });
            }
        }

        Ok(conflicts)
    }

    /// Detect path conflicts in the provided dependencies.
    ///
    /// # Arguments
    ///
    /// * `dependencies` - Map of dependencies by (type, name) key
    ///
    /// # Returns
    ///
    /// A vector of detected path conflicts
    pub fn detect_path_conflicts(
        dependencies: &HashMap<DependencyKey, DetailedDependency>,
    ) -> Vec<(String, Vec<String>)> {
        let mut conflicts = Vec::new();
        let mut install_paths: HashMap<String, Vec<String>> = HashMap::new();

        // Group dependencies by install path
        for (key, dep) in dependencies {
            let install_path = format!("{}/{}", key.0, key.1); // resource_type, name
            install_paths.entry(install_path.clone()).or_default().push(format!(
                "{}/{} (from {})",
                key.0,
                key.1,
                dep.source.as_deref().unwrap_or("local")
            ));
        }

        // Find paths with multiple dependencies
        for (path, deps) in install_paths {
            if deps.len() > 1 {
                conflicts.push((path, deps));
            }
        }

        conflicts
    }

    /// Check if a dependency conflicts with existing dependencies.
    ///
    /// # Arguments
    ///
    /// * `dependencies` - Existing dependencies
    /// * `new_dep` - New dependency to check
    /// * `new_key` - Key for the new dependency
    ///
    /// # Returns
    ///
    /// True if there's a conflict, false otherwise
    pub fn has_conflict(
        &mut self,
        dependencies: &HashMap<DependencyKey, DetailedDependency>,
        new_dep: &ResourceDependency,
        new_key: &DependencyKey,
    ) -> bool {
        // For ResourceDependency, we need to extract the path and source info
        let (new_path, new_source, new_tool) = match new_dep {
            ResourceDependency::Simple(path) => (path, None, None),
            ResourceDependency::Detailed(details) => {
                (&details.path, details.source.as_deref(), details.tool.as_deref())
            }
        };

        // Check for version conflicts
        for (key, dep) in dependencies {
            if key.0 == new_key.0 // resource_type
                && key.1 != new_key.1 // name
                && dep.path == *new_path
                && dep.source == new_source.map(String::from)
                && dep.tool == new_tool.map(String::from)
            {
                return true;
            }
        }

        false
    }
}

impl Default for ConflictService {
    fn default() -> Self {
        Self::new()
    }
}
