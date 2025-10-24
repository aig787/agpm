//! Core types and utilities for dependency resolution.
//!
//! This module provides shared types, context structures, and helper functions
//! used throughout the resolver. It consolidates:
//! - Resolution context and core shared state
//! - Context structures for different resolution phases
//! - Pure helper functions for dependency manipulation

use std::collections::HashMap;
use std::sync::Arc;

use crate::cache::Cache;
use crate::core::ResourceType;
use crate::core::operation_context::OperationContext;
use crate::manifest::{Manifest, ResourceDependency};
use crate::source::SourceManager;
use crate::version::conflict::ConflictDetector;

// ============================================================================
// Core Resolution Context
// ============================================================================

/// Core shared context for dependency resolution.
///
/// This struct holds immutable state that is shared across all
/// resolution services. It does not change during resolution.
pub struct ResolutionCore {
    /// The project manifest with dependencies and configuration
    pub manifest: Manifest,

    /// The cache for worktrees and Git operations
    pub cache: Cache,

    /// The source manager for resolving source URLs
    pub source_manager: SourceManager,

    /// Optional operation context for warnings and progress tracking
    pub operation_context: Option<Arc<OperationContext>>,
}

impl ResolutionCore {
    /// Create a new resolution core.
    pub fn new(
        manifest: Manifest,
        cache: Cache,
        source_manager: SourceManager,
        operation_context: Option<Arc<OperationContext>>,
    ) -> Self {
        Self {
            manifest,
            cache,
            source_manager,
            operation_context,
        }
    }

    /// Get a reference to the manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Get a reference to the cache.
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Get a reference to the source manager.
    pub fn source_manager(&self) -> &SourceManager {
        &self.source_manager
    }

    /// Get a reference to the operation context if present.
    pub fn operation_context(&self) -> Option<&Arc<OperationContext>> {
        self.operation_context.as_ref()
    }
}

// ============================================================================
// Resolution Context Types
// ============================================================================

/// Type alias for dependency keys used in resolution maps.
///
/// Format: (ResourceType, dependency_name, source, tool)
pub type DependencyKey = (ResourceType, String, Option<String>, Option<String>);

/// Base resolution context with immutable shared state.
///
/// This context is passed to most resolution operations and provides access
/// to the manifest, cache, source manager, and operation context.
pub struct ResolutionContext<'a> {
    /// The project manifest with dependencies and configuration
    pub manifest: &'a Manifest,

    /// The cache for worktrees and Git operations
    pub cache: &'a Cache,

    /// The source manager for resolving source URLs
    pub source_manager: &'a SourceManager,

    /// Optional operation context for warnings and progress tracking
    pub operation_context: Option<&'a Arc<OperationContext>>,
}

/// Context for transitive dependency resolution.
///
/// Extends the base resolution context with mutable state needed for
/// transitive dependency traversal and conflict detection.
pub struct TransitiveContext<'a> {
    /// Base immutable context
    pub base: ResolutionContext<'a>,

    /// Map tracking which dependencies are required by which resources
    pub dependency_map: &'a mut HashMap<DependencyKey, Vec<String>>,

    /// Map tracking custom names for transitive dependencies (for template variables)
    pub transitive_custom_names: &'a mut HashMap<DependencyKey, String>,

    /// Conflict detector for version resolution
    pub conflict_detector: &'a mut ConflictDetector,
}

/// Context for pattern expansion operations.
///
/// Extends the base resolution context with pattern alias tracking.
pub struct PatternContext<'a> {
    /// Base immutable context
    pub base: ResolutionContext<'a>,

    /// Map tracking pattern alias relationships (concrete_name -> pattern_name)
    pub pattern_alias_map: &'a mut HashMap<(ResourceType, String), String>,
}

// ============================================================================
// Dependency Helper Functions
// ============================================================================

/// Builds a resource identifier in the format `source:path`.
///
/// Resource identifiers are used for conflict detection and version resolution
/// to uniquely identify resources across different sources.
///
/// # Arguments
///
/// * `dep` - The resource dependency specification
///
/// # Returns
///
/// A string in the format `"source:path"`, or `"unknown:path"` for dependencies
/// without a source (e.g., local dependencies).
pub fn build_resource_id(dep: &ResourceDependency) -> String {
    let source = dep.get_source().unwrap_or("unknown");
    let path = dep.get_path();
    format!("{source}:{path}")
}

/// Normalizes a path by stripping leading `./` prefix.
///
/// This is a simple normalization that makes paths consistent for comparison
/// and lookup operations.
///
/// # Arguments
///
/// * `path` - The path string to normalize
///
/// # Returns
///
/// A normalized path string without leading `./`
///
/// # Examples
///
/// ```
/// use agpm_cli::resolver::types::normalize_lookup_path;
///
/// assert_eq!(normalize_lookup_path("./agents/helper.md"), "agents/helper");
/// assert_eq!(normalize_lookup_path("agents/helper.md"), "agents/helper");
/// assert_eq!(normalize_lookup_path("./foo"), "foo");
/// ```
pub fn normalize_lookup_path(path: &str) -> String {
    use std::path::{Component, Path};

    let path_obj = Path::new(path);

    // Build normalized path by iterating through components
    let mut components = Vec::new();
    for component in path_obj.components() {
        match component {
            Component::CurDir => continue, // Skip "."
            Component::Normal(os_str) => {
                components.push(os_str.to_string_lossy().to_string());
            }
            _ => {}
        }
    }

    // If we have components, strip extension from last one
    if let Some(last) = components.last_mut() {
        // Strip .md extension if present
        if let Some(stem) = Path::new(last.as_str()).file_stem() {
            *last = stem.to_string_lossy().to_string();
        }
    }

    if components.is_empty() {
        path.to_string()
    } else {
        components.join("/")
    }
}

/// Extracts the filename from a path.
///
/// Returns the last component of a slash-separated path.
///
/// # Arguments
///
/// * `path` - The path string (may contain forward slashes)
///
/// # Returns
///
/// The filename if the path contains at least one component, `None` otherwise.
///
/// # Examples
///
/// ```
/// use agpm_cli::resolver::types::extract_filename_from_path;
///
/// assert_eq!(extract_filename_from_path("agents/helper.md"), Some("helper.md".to_string()));
/// assert_eq!(extract_filename_from_path("foo/bar/baz.txt"), Some("baz.txt".to_string()));
/// assert_eq!(extract_filename_from_path("single.md"), Some("single.md".to_string()));
/// assert_eq!(extract_filename_from_path(""), None);
/// ```
pub fn extract_filename_from_path(path: &str) -> Option<String> {
    path.split('/').next_back().filter(|s| !s.is_empty()).map(std::string::ToString::to_string)
}

/// Strips resource type directory prefix from a path.
///
/// This mimics the logic in `generate_dependency_name` to allow dependency
/// lookups to work with dependency names from the dependency map.
///
/// For paths like `agents/helpers/foo.md`, this returns `helpers/foo.md`.
/// For paths without a recognized resource type directory, returns `None`.
///
/// # Arguments
///
/// * `path` - The path string with forward slashes
///
/// # Returns
///
/// The path with the resource type directory prefix stripped, or `None` if
/// no resource type directory is found.
///
/// # Recognized Resource Type Directories
///
/// - agents
/// - snippets
/// - commands
/// - scripts
/// - hooks
/// - mcp-servers
///
/// # Examples
///
/// ```
/// use agpm_cli::resolver::types::strip_resource_type_directory;
///
/// assert_eq!(
///     strip_resource_type_directory("agents/helpers/foo.md"),
///     Some("helpers/foo.md".to_string())
/// );
/// assert_eq!(
///     strip_resource_type_directory("snippets/rust/best-practices.md"),
///     Some("rust/best-practices.md".to_string())
/// );
/// assert_eq!(
///     strip_resource_type_directory("commands/deploy.md"),
///     Some("deploy.md".to_string())
/// );
/// assert_eq!(
///     strip_resource_type_directory("foo/bar.md"),
///     None
/// );
/// assert_eq!(
///     strip_resource_type_directory("agents"),
///     None  // No components after the resource type dir
/// );
/// ```
pub fn strip_resource_type_directory(path: &str) -> Option<String> {
    let components: Vec<&str> = path.split('/').collect();
    if components.len() > 1 {
        // Resource type directories
        let resource_type_dirs =
            ["agents", "snippets", "commands", "scripts", "hooks", "mcp-servers"];

        // Find the index of the first resource type directory
        if let Some(idx) = components.iter().position(|c| resource_type_dirs.contains(c)) {
            // Skip everything up to and including the resource type directory
            if idx + 1 < components.len() {
                return Some(components[idx + 1..].join("/"));
            }
        }
    }
    None
}

/// Formats a dependency reference with version suffix.
///
/// Creates a string in the format `"resource_type/name@version"` for use in
/// lockfile dependency lists.
///
/// # Arguments
///
/// * `resource_type` - The type of resource (Agent, Snippet, etc.)
/// * `name` - The resource name
/// * `version` - The version string (can be a semver tag, commit SHA, or "HEAD")
///
/// # Returns
///
/// A formatted dependency string with version.
///
/// # Examples
///
/// ```
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::resolver::types::format_dependency_with_version;
///
/// let formatted = format_dependency_with_version(
///     ResourceType::Agent,
///     "helper",
///     "v1.0.0"
/// );
/// assert_eq!(formatted, "agents/helper@v1.0.0");
///
/// let formatted = format_dependency_with_version(
///     ResourceType::Snippet,
///     "utils",
///     "abc123"
/// );
/// assert_eq!(formatted, "snippets/utils@abc123");
/// ```
pub fn format_dependency_with_version(
    resource_type: ResourceType,
    name: &str,
    version: &str,
) -> String {
    format!("{}/{}@{}", resource_type.to_plural(), name, version)
}

/// Formats a dependency reference without version suffix.
///
/// Creates a string in the format `"resource_type/name"` for use in
/// dependency tracking before version resolution.
///
/// # Arguments
///
/// * `resource_type` - The type of resource (Agent, Snippet, etc.)
/// * `name` - The resource name
///
/// # Returns
///
/// A formatted dependency string without version.
///
/// # Examples
///
/// ```
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::resolver::types::format_dependency_without_version;
///
/// let formatted = format_dependency_without_version(ResourceType::Agent, "helper");
/// assert_eq!(formatted, "agents/helper");
///
/// let formatted = format_dependency_without_version(ResourceType::Command, "deploy");
/// assert_eq!(formatted, "commands/deploy");
/// ```
pub fn format_dependency_without_version(resource_type: ResourceType, name: &str) -> String {
    format!("{}/{}", resource_type.to_plural(), name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DetailedDependency;

    #[test]
    fn test_normalize_lookup_path() {
        // Extensions are stripped for consistent lookup
        assert_eq!(normalize_lookup_path("./agents/helper.md"), "agents/helper");
        assert_eq!(normalize_lookup_path("agents/helper.md"), "agents/helper");
        assert_eq!(normalize_lookup_path("snippets/helpers/foo.md"), "snippets/helpers/foo");
        assert_eq!(normalize_lookup_path("./foo.md"), "foo");
        assert_eq!(normalize_lookup_path("./foo"), "foo");
        assert_eq!(normalize_lookup_path("foo"), "foo");
    }

    #[test]
    fn test_extract_filename_from_path() {
        assert_eq!(extract_filename_from_path("agents/helper.md"), Some("helper.md".to_string()));
        assert_eq!(extract_filename_from_path("foo/bar/baz.txt"), Some("baz.txt".to_string()));
        assert_eq!(extract_filename_from_path("single.md"), Some("single.md".to_string()));
        assert_eq!(extract_filename_from_path(""), None);
        assert_eq!(extract_filename_from_path("trailing/"), None);
    }

    #[test]
    fn test_strip_resource_type_directory() {
        assert_eq!(
            strip_resource_type_directory("agents/helpers/foo.md"),
            Some("helpers/foo.md".to_string())
        );
        assert_eq!(
            strip_resource_type_directory("snippets/rust/best-practices.md"),
            Some("rust/best-practices.md".to_string())
        );
        assert_eq!(
            strip_resource_type_directory("commands/deploy.md"),
            Some("deploy.md".to_string())
        );
        assert_eq!(strip_resource_type_directory("foo/bar.md"), None);
        assert_eq!(strip_resource_type_directory("agents"), None);
        assert_eq!(
            strip_resource_type_directory("mcp-servers/filesystem.json"),
            Some("filesystem.json".to_string())
        );
    }

    #[test]
    fn test_format_dependency_with_version() {
        assert_eq!(
            format_dependency_with_version(ResourceType::Agent, "helper", "v1.0.0"),
            "agents/helper@v1.0.0"
        );
        assert_eq!(
            format_dependency_with_version(ResourceType::Snippet, "utils", "abc123"),
            "snippets/utils@abc123"
        );
    }

    #[test]
    fn test_format_dependency_without_version() {
        assert_eq!(
            format_dependency_without_version(ResourceType::Agent, "helper"),
            "agents/helper"
        );
        assert_eq!(
            format_dependency_without_version(ResourceType::Command, "deploy"),
            "commands/deploy"
        );
    }

    #[test]
    fn test_build_resource_id() {
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            source: Some("test-source".to_string()),
            path: "agents/helper.md".to_string(),
            version: Some("v1.0.0".to_string()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: None,
            filename: None,
            dependencies: None,
            tool: None,
            flatten: None,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));
        let resource_id = build_resource_id(&dep);
        assert!(resource_id.contains("agents/helper.md"));
    }
}
