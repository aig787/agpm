//! Resource registry for tracking resources during backtracking.
//!
//! This module provides data structures for tracking resources and their dependency
//! relationships during conflict resolution. It maintains a complete view of all
//! resources in the dependency graph, enabling accurate conflict detection after
//! backtracking changes versions.

use anyhow::Result;
use std::collections::HashMap;

use crate::lockfile::ResourceId;

/// Tracks resources whose versions changed during backtracking.
///
/// These resources need their transitive dependencies re-extracted and re-resolved
/// because changing a resource's version may change which transitive dependencies
/// it declares.
#[derive(Debug, Clone)]
pub struct TransitiveChangeTracker {
    /// Map: resource_id → (old_version, new_version, new_sha, variant_inputs)
    changed_resources: HashMap<String, (String, String, String, Option<serde_json::Value>)>,
}

impl TransitiveChangeTracker {
    pub fn new() -> Self {
        Self {
            changed_resources: HashMap::new(),
        }
    }

    pub fn record_change(
        &mut self,
        resource_id: &str,
        old_version: &str,
        new_version: &str,
        new_sha: &str,
        variant_inputs: Option<serde_json::Value>,
    ) {
        self.changed_resources.insert(
            resource_id.to_string(),
            (old_version.to_string(), new_version.to_string(), new_sha.to_string(), variant_inputs),
        );
    }

    pub fn get_changed_resources(
        &self,
    ) -> &HashMap<String, (String, String, String, Option<serde_json::Value>)> {
        &self.changed_resources
    }

    pub fn clear(&mut self) {
        self.changed_resources.clear();
    }
}

impl Default for TransitiveChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Parameters for adding or updating a resource in the registry.
#[derive(Debug, Clone)]
pub struct ResourceParams {
    pub resource_id: ResourceId,
    pub version: String,
    pub sha: String,
    pub version_constraint: String,
    pub required_by: String,
}

/// Entry for a single resource in the registry.
#[derive(Debug, Clone)]
pub struct ResourceEntry {
    /// Full ResourceId structure - used for ConflictDetector
    pub resource_id: ResourceId,

    /// Current version (may change during backtracking)
    pub version: String,

    /// Resolved SHA for this version
    pub sha: String,

    /// Version constraint originally requested
    pub version_constraint: String,

    /// Resources that depend on this one
    pub required_by: Vec<String>,
}

/// Tracks all resources and their dependency relationships for conflict detection.
///
/// This registry maintains a complete view of all resources in the dependency graph,
/// including their current versions, SHAs, and required_by relationships. This enables
/// accurate conflict detection after backtracking changes versions.
#[derive(Debug, Clone)]
pub struct ResourceRegistry {
    /// Map: resource_id → ResourceEntry
    resources: HashMap<String, ResourceEntry>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    /// Add or update a resource in the registry.
    ///
    /// If the resource already exists, updates its version and SHA, and adds the
    /// required_by entry if not already present.
    pub fn add_or_update_resource(&mut self, params: ResourceParams) {
        let ResourceParams {
            resource_id,
            version,
            sha,
            version_constraint,
            required_by,
        } = params;

        // Convert ResourceId to string for HashMap key
        let resource_id_string =
            resource_id_to_string(&resource_id).expect("ResourceId should have a valid source");

        self.resources
            .entry(resource_id_string.clone())
            .and_modify(|entry| {
                entry.version = version.clone();
                entry.sha = sha.clone();
                if !entry.required_by.contains(&required_by) {
                    entry.required_by.push(required_by.clone());
                }
            })
            .or_insert_with(|| ResourceEntry {
                resource_id: resource_id.clone(),
                version,
                sha,
                version_constraint,
                required_by: vec![required_by],
            });
    }

    /// Iterate over all resources in the registry.
    pub fn all_resources(&self) -> impl Iterator<Item = &ResourceEntry> {
        self.resources.values()
    }

    /// Update the version and SHA for an existing resource.
    ///
    /// This is used during backtracking when a resource's version changes.
    /// The required_by relationships and version_constraint are preserved.
    pub fn update_version_and_sha(
        &mut self,
        resource_id: &str,
        new_version: String,
        new_sha: String,
    ) {
        if let Some(entry) = self.resources.get_mut(resource_id) {
            entry.version = new_version;
            entry.sha = new_sha;
        }
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a resource_id string (format: "source:path") into components.
pub fn parse_resource_id_string(resource_id: &str) -> Result<(&str, &str)> {
    let parts: Vec<&str> = resource_id.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid resource_id format: {}", resource_id));
    }
    Ok((parts[0], parts[1]))
}

/// Convert a ResourceId to the legacy string format "source:name".
pub fn resource_id_to_string(resource_id: &ResourceId) -> Result<String> {
    let source = resource_id
        .source()
        .ok_or_else(|| anyhow::anyhow!("Resource {} has no source", resource_id))?;
    Ok(format!("{}:{}", source, resource_id.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resource_id() {
        let (source, path) = parse_resource_id_string("community:agents/helper.md").unwrap();
        assert_eq!(source, "community");
        assert_eq!(path, "agents/helper.md");
    }

    #[test]
    fn test_parse_resource_id_invalid() {
        let result = parse_resource_id_string("invalid");
        assert!(result.is_err());
    }
}
