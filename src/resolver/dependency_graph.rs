//! Dependency graph management for transitive dependency resolution.
//!
//! This module provides the graph data structure and algorithms needed to
//! handle transitive dependencies, including cycle detection and topological
//! ordering for correct installation order.

use anyhow::{Result, anyhow};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use crate::lockfile::lockfile_dependency_ref::LockfileDependencyRef;

/// Represents a dependency node in the graph.
///
/// Each node represents a unique resource that can be installed.
/// Nodes are distinguished by name, resource type, and source to prevent
/// false cycle detection when the same resource name appears in multiple sources.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependencyNode {
    /// Resource type: Agent, Snippet, Command, etc.
    pub resource_type: crate::core::ResourceType,
    /// Dependency name as specified in the manifest.
    pub name: String,
    /// Source name (e.g., "community", "local"). None for direct path dependencies.
    pub source: Option<String>,
}

impl DependencyNode {
    /// Create a new dependency node without a source.
    pub fn new(resource_type: crate::core::ResourceType, name: impl Into<String>) -> Self {
        Self {
            resource_type,
            name: name.into(),
            source: None,
        }
    }

    /// Create a new dependency node with a source.
    pub fn with_source(
        resource_type: crate::core::ResourceType,
        name: impl Into<String>,
        source: Option<String>,
    ) -> Self {
        Self {
            resource_type,
            name: name.into(),
            source,
        }
    }

    /// Get a display name for this node.
    pub fn display_name(&self) -> String {
        if let Some(ref source) = self.source {
            LockfileDependencyRef::git(source.clone(), self.resource_type, self.name.clone(), None)
                .to_string()
        } else {
            LockfileDependencyRef::local(self.resource_type, self.name.clone(), None).to_string()
        }
    }
}

impl fmt::Display for DependencyNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Color states for cycle detection using DFS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    /// Node has not been visited.
    White,
    /// Node is currently being visited (in the DFS stack).
    Gray,
    /// Node has been fully visited.
    Black,
}

/// Dependency graph for managing transitive dependencies.
///
/// This graph tracks dependencies between resources and provides
/// algorithms for cycle detection and topological ordering.
pub struct DependencyGraph {
    /// The underlying directed graph.
    graph: DiGraph<DependencyNode, ()>,
    /// Map from dependency nodes to their graph indices.
    node_map: HashMap<DependencyNode, NodeIndex>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    /// Add a node to the graph if it doesn't already exist.
    ///
    /// Returns the node index in the graph.
    fn ensure_node(&mut self, node: DependencyNode) -> NodeIndex {
        if let Some(&index) = self.node_map.get(&node) {
            index
        } else {
            let index = self.graph.add_node(node.clone());
            self.node_map.insert(node, index);
            index
        }
    }

    /// Add a dependency relationship to the graph.
    ///
    /// `from` depends on `to`, meaning `to` must be installed before `from`.
    pub fn add_dependency(&mut self, from: DependencyNode, to: DependencyNode) {
        let from_idx = self.ensure_node(from);
        let to_idx = self.ensure_node(to);

        // Check if edge already exists to avoid duplicates
        if !self.graph.contains_edge(from_idx, to_idx) {
            self.graph.add_edge(from_idx, to_idx, ());
        }
    }

    /// Detect cycles in the dependency graph using DFS with colors.
    ///
    /// Returns an error containing the cycle path if a cycle is detected.
    pub fn detect_cycles(&self) -> Result<()> {
        let mut colors: HashMap<NodeIndex, Color> = HashMap::new();
        let mut path: Vec<DependencyNode> = Vec::new();

        // Initialize all nodes as white
        for node in self.graph.node_indices() {
            colors.insert(node, Color::White);
        }

        // DFS from each white node
        for node in self.graph.node_indices() {
            if matches!(colors.get(&node), Some(Color::White))
                && let Some(cycle) = self.dfs_visit(node, &mut colors, &mut path)
            {
                let cycle_str =
                    cycle.iter().map(DependencyNode::display_name).collect::<Vec<_>>().join(" → ");
                return Err(anyhow!("Circular dependency detected: {cycle_str}"));
            }
        }

        Ok(())
    }

    /// DFS visit for cycle detection.
    ///
    /// Returns `Some(cycle_path)` if a cycle is detected, None otherwise.
    fn dfs_visit(
        &self,
        node: NodeIndex,
        colors: &mut HashMap<NodeIndex, Color>,
        path: &mut Vec<DependencyNode>,
    ) -> Option<Vec<DependencyNode>> {
        colors.insert(node, Color::Gray);
        path.push(self.graph[node].clone());

        for neighbor in self.graph.neighbors(node) {
            match colors.get(&neighbor) {
                Some(Color::Gray) => {
                    // Found a cycle - find where it starts in the path
                    let cycle_start = path.iter().position(|n| *n == self.graph[neighbor]).unwrap();
                    let mut cycle = path[cycle_start..].to_vec();
                    // Add the node again to show the cycle closes
                    cycle.push(self.graph[neighbor].clone());
                    return Some(cycle);
                }
                Some(Color::White) => {
                    if let Some(cycle) = self.dfs_visit(neighbor, colors, path) {
                        return Some(cycle);
                    }
                }
                _ => {}
            }
        }

        path.pop();
        colors.insert(node, Color::Black);
        None
    }

    /// Get the topological order for installation.
    ///
    /// Returns nodes in an order where all dependencies come before their dependents.
    /// This ensures that resources are installed in the correct order.
    pub fn topological_order(&self) -> Result<Vec<DependencyNode>> {
        // First check for cycles
        self.detect_cycles()?;

        // Perform topological sort
        match toposort(&self.graph, None) {
            Ok(indices) => {
                let mut order = Vec::new();
                // Reverse the order so dependencies come first
                for idx in indices.into_iter().rev() {
                    order.push(self.graph[idx].clone());
                }
                Ok(order)
            }
            Err(_) => {
                // This shouldn't happen as we already checked for cycles
                Err(anyhow!("Failed to determine installation order"))
            }
        }
    }

    /// Get all transitive dependencies for a given node.
    ///
    /// Returns a set of all nodes that the given node depends on,
    /// directly or indirectly.
    pub fn get_transitive_deps(&self, node: &DependencyNode) -> HashSet<DependencyNode> {
        let mut deps = HashSet::new();
        let mut queue = VecDeque::new();

        // Find the node index
        if let Some(&node_idx) = self.node_map.get(node) {
            queue.push_back(node_idx);

            while let Some(current) = queue.pop_front() {
                // Add all neighbors (dependencies) to the queue
                for neighbor in self.graph.neighbors(current) {
                    let dep_node = &self.graph[neighbor];
                    if deps.insert(dep_node.clone()) {
                        // Only process if we haven't seen this node before
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        deps
    }

    /// Get direct dependencies for a given node.
    ///
    /// Returns only the immediate dependencies, not transitive ones.
    pub fn get_direct_deps(&self, node: &DependencyNode) -> Vec<DependencyNode> {
        if let Some(&node_idx) = self.node_map.get(node) {
            self.graph.neighbors(node_idx).map(|idx| self.graph[idx].clone()).collect()
        } else {
            Vec::new()
        }
    }

    /// Check if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    /// Get the total number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get the total number of edges (dependencies) in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Get all nodes in the graph.
    pub fn nodes(&self) -> Vec<DependencyNode> {
        self.graph.node_indices().map(|idx| self.graph[idx].clone()).collect()
    }

    /// Build a human-readable dependency tree representation.
    ///
    /// Returns a string showing the dependency hierarchy.
    pub fn to_tree_string(&self, root: &DependencyNode) -> String {
        let mut result = String::new();
        let mut visited = HashSet::new();
        self.build_tree_string(root, &mut result, "", true, &mut visited);
        result
    }

    fn build_tree_string(
        &self,
        node: &DependencyNode,
        result: &mut String,
        prefix: &str,
        is_last: bool,
        visited: &mut HashSet<DependencyNode>,
    ) {
        let connector = if is_last {
            "└── "
        } else {
            "├── "
        };
        result.push_str(&format!("{}{}{}\n", prefix, connector, node.display_name()));

        if !visited.insert(node.clone()) {
            // Already visited - indicate circular reference
            let child_prefix = if is_last {
                format!("{prefix}    ")
            } else {
                format!("{prefix}│   ")
            };
            result.push_str(&format!("{child_prefix}└── (circular reference)\n"));
            return;
        }

        let deps = self.get_direct_deps(node);
        let child_prefix = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };

        for (i, dep) in deps.iter().enumerate() {
            let is_last_child = i == deps.len() - 1;
            self.build_tree_string(dep, result, &child_prefix, is_last_child, visited);
        }
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dependency_chain() {
        let mut graph = DependencyGraph::new();

        // A -> B -> C
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Command, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
            DependencyNode::new(crate::core::ResourceType::Snippet, "C"),
        );

        assert!(graph.detect_cycles().is_ok());

        let order = graph.topological_order().unwrap();
        assert_eq!(order.len(), 3);

        // C should come before B, and B before A
        let c_idx = order.iter().position(|n| n.name == "C").unwrap();
        let b_idx = order.iter().position(|n| n.name == "B").unwrap();
        let a_idx = order.iter().position(|n| n.name == "A").unwrap();
        assert!(c_idx < b_idx);
        assert!(b_idx < a_idx);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut graph = DependencyGraph::new();

        // A -> B -> C -> A (circular)
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
            DependencyNode::new(crate::core::ResourceType::Agent, "C"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "C"),
            DependencyNode::new(crate::core::ResourceType::Agent, "A"),
        );

        let result = graph.detect_cycles();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Circular dependency"));
        assert!(error_msg.contains("agent:A"));
    }

    #[test]
    fn test_diamond_dependency() {
        let mut graph = DependencyGraph::new();

        // A -> B, A -> C, B -> D, C -> D (diamond)
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Command, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Command, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "C"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
            DependencyNode::new(crate::core::ResourceType::Snippet, "D"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "C"),
            DependencyNode::new(crate::core::ResourceType::Snippet, "D"),
        );

        assert!(graph.detect_cycles().is_ok());

        let order = graph.topological_order().unwrap();
        assert_eq!(order.len(), 4);

        // D should come before both B and C
        let d_idx = order.iter().position(|n| n.name == "D").unwrap();
        let b_idx = order.iter().position(|n| n.name == "B").unwrap();
        let c_idx = order.iter().position(|n| n.name == "C").unwrap();
        let a_idx = order.iter().position(|n| n.name == "A").unwrap();

        assert!(d_idx < b_idx);
        assert!(d_idx < c_idx);
        assert!(b_idx < a_idx);
        assert!(c_idx < a_idx);
    }

    #[test]
    fn test_get_transitive_deps() {
        let mut graph = DependencyGraph::new();

        // A -> B -> C, A -> D
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Command, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
            DependencyNode::new(crate::core::ResourceType::Snippet, "C"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Command, "A"),
            DependencyNode::new(crate::core::ResourceType::Snippet, "D"),
        );

        let deps = graph
            .get_transitive_deps(&DependencyNode::new(crate::core::ResourceType::Command, "A"));
        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&DependencyNode::new(crate::core::ResourceType::Agent, "B")));
        assert!(deps.contains(&DependencyNode::new(crate::core::ResourceType::Snippet, "C")));
        assert!(deps.contains(&DependencyNode::new(crate::core::ResourceType::Snippet, "D")));
    }

    #[test]
    fn test_self_dependency() {
        let mut graph = DependencyGraph::new();

        // A -> A (self-dependency)
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "A"),
        );

        let result = graph.detect_cycles();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Circular dependency"));
    }

    #[test]
    fn test_empty_graph() {
        let graph = DependencyGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert!(graph.detect_cycles().is_ok());
        assert!(graph.topological_order().unwrap().is_empty());
    }

    #[test]
    fn test_duplicate_edges() {
        let mut graph = DependencyGraph::new();

        // Add the same dependency twice
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
        );
        graph.add_dependency(
            DependencyNode::new(crate::core::ResourceType::Agent, "A"),
            DependencyNode::new(crate::core::ResourceType::Agent, "B"),
        );

        assert_eq!(graph.edge_count(), 1); // Should only have one edge
        assert_eq!(graph.node_count(), 2);
    }

    #[test]
    fn test_cross_source_no_false_cycle() {
        let mut graph = DependencyGraph::new();

        // Same resource name "helper" from two different sources should NOT create a cycle
        // sourceA: A -> helper@sourceA
        // sourceB: B -> helper@sourceB
        graph.add_dependency(
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "A",
                Some("sourceA".to_string()),
            ),
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "helper",
                Some("sourceA".to_string()),
            ),
        );
        graph.add_dependency(
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "B",
                Some("sourceB".to_string()),
            ),
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "helper",
                Some("sourceB".to_string()),
            ),
        );

        // Should have 4 distinct nodes (A@sourceA, helper@sourceA, B@sourceB, helper@sourceB)
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 2);

        // Should NOT detect a cycle
        assert!(graph.detect_cycles().is_ok());

        // Topological order should succeed
        let order = graph.topological_order().unwrap();
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn test_cross_source_real_cycle() {
        let mut graph = DependencyGraph::new();

        // Same source, real cycle: A -> B -> A (both from sourceX)
        graph.add_dependency(
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "A",
                Some("sourceX".to_string()),
            ),
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "B",
                Some("sourceX".to_string()),
            ),
        );
        graph.add_dependency(
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "B",
                Some("sourceX".to_string()),
            ),
            DependencyNode::with_source(
                crate::core::ResourceType::Agent,
                "A",
                Some("sourceX".to_string()),
            ),
        );

        // Should detect a cycle
        let result = graph.detect_cycles();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Circular dependency"));
    }
}
