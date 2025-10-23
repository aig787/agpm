//! Transitive dependency resolution for AGPM.
//!
//! This module handles the discovery and resolution of transitive dependencies,
//! building dependency graphs, and detecting cycles. It processes dependencies
//! declared within resource files and resolves them in topological order.

use crate::core::ResourceType;
use crate::manifest::ResourceDependency;
use crate::metadata::MetadataExtractor;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::debug;

use super::dependency_graph::{DependencyGraph, DependencyNode};

/// Resolves transitive dependencies for a set of base dependencies.
///
/// This function discovers dependencies declared within resource files,
/// builds a dependency graph, detects cycles, and returns all dependencies
/// in topological order.
///
/// # Arguments
///
/// * `base_deps` - The initial dependencies from the manifest
/// * `fetch_content` - Async function to fetch resource content for metadata extraction
/// * `enable_transitive` - Whether to enable transitive dependency resolution
///
/// # Returns
///
/// A vector of all dependencies (direct + transitive) in topological order, including
/// all same-named resources from different sources. Each tuple contains:
/// - Dependency name
/// - Resource dependency details
/// - Resource type (to avoid ambiguity when same name exists across types)
pub async fn resolve_transitive_dependencies<F, Fut>(
    base_deps: &[(String, ResourceDependency, ResourceType)],
    mut fetch_content: F,
    enable_transitive: bool,
) -> Result<Vec<(String, ResourceDependency, ResourceType)>>
where
    F: FnMut(&str, &ResourceDependency) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    if !enable_transitive {
        // If transitive resolution is disabled, return base dependencies as-is
        // with their resource types already threaded from the manifest
        return Ok(base_deps.to_vec());
    }

    let mut graph = DependencyGraph::new();
    // Use (resource_type, name, source, tool) as key to distinguish same-named resources from different sources and tools
    let mut all_deps: HashMap<
        (ResourceType, String, Option<String>, Option<String>),
        ResourceDependency,
    > = HashMap::new();
    let mut processed: HashSet<(ResourceType, String, Option<String>, Option<String>)> =
        HashSet::new();
    let mut queue: Vec<(String, ResourceDependency, Option<ResourceType>)> = Vec::new();

    // Add initial dependencies to queue with their threaded types
    for (name, dep, resource_type) in base_deps {
        let source = dep.get_source().map(std::string::ToString::to_string);
        let tool = dep.get_tool().map(std::string::ToString::to_string);
        queue.push((name.clone(), dep.clone(), Some(*resource_type)));
        all_deps.insert((*resource_type, name.clone(), source, tool), dep.clone());
    }

    // Process queue to discover transitive dependencies
    while let Some((name, dep, resource_type)) = queue.pop() {
        let source = dep.get_source().map(std::string::ToString::to_string);
        let tool = dep.get_tool().map(std::string::ToString::to_string);
        // resource_type must be provided - cannot be derived from path
        let Some(resource_type) = resource_type else {
            debug!("Skipping dependency '{}' without resource type", name);
            continue;
        };

        let key = (resource_type, name.clone(), source.clone(), tool.clone());

        // Skip if already processed
        if processed.contains(&key) {
            debug!("[QUEUE_POP] SKIPPED (already processed): '{}'", name);
            continue;
        }

        debug!("[QUEUE_POP] PROCESSING: '{}'", name);
        processed.insert(key.clone());

        // Handle pattern dependencies by expanding them to concrete files
        if dep.is_pattern() {
            debug!("[QUEUE_POP] '{}' is a PATTERN, expanding to concrete deps", name);
            // Note: Pattern expansion should be handled before calling this function
            // For now, we'll skip patterns in transitive resolution
            continue;
        }

        // Add node to graph (node will be added via add_dependency if there are edges)
        let node = DependencyNode::with_source(resource_type, name.clone(), source.clone());

        // Extract dependencies from resource content
        let content = fetch_content(&name, &dep)
            .await
            .with_context(|| format!("Failed to fetch content for dependency: {}", name))?;

        let path_string = dep.get_path().to_string();
        let path = Path::new(&path_string);
        let metadata = MetadataExtractor::extract(path, &content, None, None)
            .with_context(|| format!("Failed to extract metadata from: {}", path_string))?;

        if let Some(dependencies) = metadata.get_dependencies() {
            for (dep_type_str, deps) in dependencies {
                // Convert resource type string to ResourceType enum
                let dep_resource_type: ResourceType = match dep_type_str.parse() {
                    Ok(rt) => rt,
                    Err(_) => {
                        debug!("Skipping unknown resource type: {}", dep_type_str);
                        continue;
                    }
                };

                for dep_spec in deps {
                    // Convert DependencySpec to ResourceDependency
                    let transitive_dep = create_resource_dependency(dep_spec, &source, &tool)?;

                    let transitive_name = generate_dependency_name(transitive_dep.get_path());
                    let transitive_source =
                        transitive_dep.get_source().map(std::string::ToString::to_string);
                    let transitive_tool =
                        transitive_dep.get_tool().map(std::string::ToString::to_string);

                    // Add edge from current dependency to transitive dependency
                    let transitive_node = DependencyNode::with_source(
                        dep_resource_type,
                        transitive_name.clone(),
                        transitive_source.clone(),
                    );
                    graph.add_dependency(node.clone(), transitive_node);

                    // Add to queue if not processed
                    let transitive_key = (
                        dep_resource_type,
                        transitive_name.clone(),
                        transitive_source.clone(),
                        transitive_tool.clone(),
                    );
                    if !processed.contains(&transitive_key) {
                        queue.push((
                            transitive_name,
                            transitive_dep.clone(),
                            Some(dep_resource_type),
                        ));
                        all_deps.insert(transitive_key, transitive_dep);
                    }
                }
            }
        }
    }

    // Detect cycles
    graph.detect_cycles()?;

    // Return dependencies in topological order
    let ordered_nodes = graph.topological_order()?;
    let mut result = Vec::new();
    let mut added_keys = HashSet::new();

    debug!(
        "Transitive resolution - topological order has {} nodes, all_deps has {} entries",
        ordered_nodes.len(),
        all_deps.len()
    );

    for node in ordered_nodes {
        debug!(
            "Processing ordered node: {}/{} (source: {:?})",
            node.resource_type, node.name, node.source
        );
        // Find matching dependency - now that nodes include source, we can match precisely
        for (key, dep) in &all_deps {
            if key.0 == node.resource_type && key.1 == node.name && key.2 == node.source {
                debug!(
                    "  -> Found match in all_deps, adding to result with type {:?}",
                    node.resource_type
                );
                result.push((node.name.clone(), dep.clone(), node.resource_type));
                added_keys.insert(key.clone());
                break; // Exact match found, no need to continue
            }
        }
    }

    // Add remaining dependencies that weren't in the graph (no transitive deps)
    // These can be added in any order since they have no dependencies
    // IMPORTANT: Filter out patterns - they should only serve as expansion points,
    // not final dependencies. The concrete deps from expansion are what we want.
    for (key, dep) in all_deps {
        if !added_keys.contains(&key) {
            // Skip pattern dependencies - they were expanded to concrete deps
            if dep.is_pattern() {
                debug!(
                    "Skipping pattern dependency in final result: {}/{} (source: {:?})",
                    key.0, key.1, key.2
                );
                continue;
            }

            debug!(
                "Adding non-graph dependency: {}/{} (source: {:?}) with type {:?}",
                key.0, key.1, key.2, key.0
            );
            result.push((key.1.clone(), dep.clone(), key.0));
        }
    }

    debug!("Transitive resolution returning {} dependencies", result.len());

    Ok(result)
}

/// Creates a ResourceDependency from a DependencySpec.
fn create_resource_dependency(
    dep_spec: &crate::manifest::DependencySpec,
    default_source: &Option<String>,
    default_tool: &Option<String>,
) -> Result<ResourceDependency> {
    use crate::manifest::DetailedDependency;

    // Create a DetailedDependency from DependencySpec
    let detailed = DetailedDependency {
        path: dep_spec.path.clone(),
        source: default_source.clone(),
        version: dep_spec.version.clone(),
        branch: None,
        rev: None,
        command: None,
        args: None,
        target: None,
        filename: None,
        dependencies: None,
        tool: dep_spec.tool.clone().or_else(|| default_tool.clone()),
        flatten: dep_spec.flatten,
        install: dep_spec.install,
        template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    };

    Ok(ResourceDependency::Detailed(Box::new(detailed)))
}

/// Generates a dependency name from a path.
fn generate_dependency_name(path: &str) -> String {
    // Extract the base name without extension
    let path_buf = Path::new(path);
    let stem = path_buf.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");

    // Convert to valid identifier (replace invalid chars with underscores)
    stem.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DependencySpec;

    #[test]
    fn test_generate_dependency_name() {
        assert_eq!(generate_dependency_name("agents/helper.md"), "helper");
        assert_eq!(generate_dependency_name("commands/deploy.sh"), "deploy");
        assert_eq!(generate_dependency_name("snippets/rust-patterns.md"), "rust_patterns");
        assert_eq!(generate_dependency_name("complex-file-name@123.md"), "complex_file_name_123");
    }

    #[test]
    fn test_create_resource_dependency() {
        let default_source = Some("test-source".to_string());
        let default_tool = Some("claude-code".to_string());

        // Test dependency with inheritance
        let dep_spec = DependencySpec {
            path: "test.md".to_string(),
            name: None,
            version: Some("v1.0.0".to_string()),
            tool: None,
            flatten: None,
            install: None,
        };
        let dep = create_resource_dependency(&dep_spec, &default_source, &default_tool).unwrap();

        if let ResourceDependency::Detailed(d) = dep {
            assert_eq!(d.path, "test.md");
            assert_eq!(d.version, Some("v1.0.0".to_string()));
            assert_eq!(d.source, default_source);
            assert_eq!(d.tool, default_tool);
        } else {
            panic!("Expected Detailed dependency");
        }
    }
}
