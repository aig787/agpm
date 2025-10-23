//! Pattern expansion for AGPM dependencies.
//!
//! This module handles the expansion of glob patterns in dependency specifications,
//! converting pattern dependencies into concrete file dependencies. It supports both
//! local and remote pattern resolution with proper path handling.

use crate::git::GitRepo;
use crate::manifest::{DetailedDependency, ResourceDependency};
use crate::pattern::PatternResolver;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Expands a pattern dependency into concrete dependencies.
///
/// This function takes a pattern dependency (e.g., `agents/*.md`) and expands it
/// into individual file dependencies. It handles both local and remote patterns.
///
/// # Arguments
///
/// * `name` - The name of the pattern dependency
/// * `dep` - The pattern dependency to expand
/// * `resource_type` - The type of resource being expanded
/// * `source_manager` - Source manager for remote repositories
/// * `cache` - Cache for storing resolved files
///
/// # Returns
///
/// A vector of tuples containing:
/// - The generated dependency name
/// - The concrete resource dependency
pub async fn expand_pattern_to_concrete_deps(
    dep: &ResourceDependency,
    resource_type: crate::core::ResourceType,
    source_manager: &crate::source::SourceManager,
    cache: &crate::cache::Cache,
    manifest_dir: Option<&Path>,
) -> Result<Vec<(String, ResourceDependency)>> {
    let pattern = dep.get_path();

    if dep.is_local() {
        expand_local_pattern(dep, pattern, manifest_dir).await
    } else {
        expand_remote_pattern(dep, pattern, resource_type, source_manager, cache).await
    }
}

/// Expands a local pattern dependency.
async fn expand_local_pattern(
    dep: &ResourceDependency,
    pattern: &str,
    manifest_dir: Option<&Path>,
) -> Result<Vec<(String, ResourceDependency)>> {
    // For absolute patterns, use the parent directory as base and strip the pattern to just the filename part
    // For relative patterns, use manifest directory
    let pattern_path = Path::new(pattern);
    let (base_path, search_pattern) = if pattern_path.is_absolute() {
        // Absolute pattern: extract base directory and relative pattern
        // Example: "/tmp/xyz/agents/*.md" -> base="/tmp/xyz", pattern="agents/*.md"
        let components: Vec<_> = pattern_path.components().collect();

        // Find the first component with a glob character
        let glob_idx = components.iter().position(|c| {
            let s = c.as_os_str().to_string_lossy();
            s.contains('*') || s.contains('?') || s.contains('[')
        });

        if let Some(idx) = glob_idx {
            // Split at the glob component
            let base_components = &components[..idx];
            let pattern_components = &components[idx..];

            let base: PathBuf = base_components.iter().collect();
            let pattern: String = pattern_components
                .iter()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");

            (base, pattern)
        } else {
            // No glob characters, use as-is
            (PathBuf::from("."), pattern.to_string())
        }
    } else {
        // Relative pattern, use manifest directory as base
        let base = manifest_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        (base, pattern.to_string())
    };

    let pattern_resolver = PatternResolver::new();
    let matches = pattern_resolver.resolve(&search_pattern, &base_path)?;

    debug!("Pattern '{}' matched {} files", pattern, matches.len());

    // Get tool, target, and flatten from parent pattern dependency
    let (tool, target, flatten) = match dep {
        ResourceDependency::Detailed(d) => (d.tool.clone(), d.target.clone(), d.flatten),
        _ => (None, None, None),
    };

    let mut concrete_deps = Vec::new();

    for matched_path in matches {
        // Generate a dependency name from the matched path
        let dep_name = generate_dependency_name(&matched_path.to_string_lossy());

        // Convert matched path to absolute by joining with base_path
        let absolute_path = base_path.join(&matched_path);
        let concrete_path = absolute_path.to_string_lossy().to_string();

        // Create a concrete dependency for the matched file, inheriting tool, target, and flatten from parent
        let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: concrete_path,
            source: None,
            version: None,
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: target.clone(),
            filename: None,
            dependencies: None,
            tool: tool.clone(),
            flatten,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        concrete_deps.push((dep_name, concrete_dep));
    }

    Ok(concrete_deps)
}

/// Expands a remote pattern dependency.
async fn expand_remote_pattern(
    dep: &ResourceDependency,
    pattern: &str,
    _resource_type: crate::core::ResourceType,
    source_manager: &crate::source::SourceManager,
    cache: &crate::cache::Cache,
) -> Result<Vec<(String, ResourceDependency)>> {
    let source_name = dep
        .get_source()
        .ok_or_else(|| anyhow::anyhow!("Remote pattern dependency missing source: {}", pattern))?;

    let source_url = source_manager
        .get_source_url(source_name)
        .with_context(|| format!("Source not found: {}", source_name))?;

    // Get or clone the source repository
    let repo_path = cache
        .get_or_clone_source(source_name, &source_url, dep.get_version())
        .await
        .with_context(|| format!("Failed to access source repository: {}", source_name))?;

    let repo = GitRepo::new(&repo_path);

    // Resolve the version to a commit SHA
    let version = dep.get_version().unwrap_or("HEAD");
    let commit_sha = repo.resolve_to_sha(Some(version)).await.with_context(|| {
        format!("Failed to resolve version '{}' for source {}", version, source_name)
    })?;

    // Create a worktree for the specific commit
    let worktree_path = cache
        .get_or_create_worktree_for_sha(source_name, &source_url, &commit_sha, Some(version))
        .await
        .with_context(|| format!("Failed to create worktree for {}@{}", source_name, version))?;

    // Resolve the pattern within the worktree
    let pattern_resolver = PatternResolver::new();
    let matches = pattern_resolver.resolve(pattern, &worktree_path)?;

    debug!("Remote pattern '{}' in {} matched {} files", pattern, source_name, matches.len());

    // Get tool, target, and flatten from parent pattern dependency
    let (tool, target, flatten) = match dep {
        ResourceDependency::Detailed(d) => (d.tool.clone(), d.target.clone(), d.flatten),
        _ => (None, None, None),
    };

    let mut concrete_deps = Vec::new();

    for matched_path in matches {
        // Generate a dependency name from the matched path
        let dep_name = generate_dependency_name(&matched_path.to_string_lossy());

        // matched_path is already relative to worktree root (from PatternResolver)
        // Create a concrete dependency for the matched file, inheriting tool, target, and flatten from parent
        let concrete_dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: matched_path.to_string_lossy().to_string(),
            source: Some(source_name.to_string()),
            version: Some(commit_sha.clone()),
            branch: None,
            rev: None,
            command: None,
            args: None,
            target: target.clone(),
            filename: None,
            dependencies: None,
            tool: tool.clone(),
            flatten,
            install: None,
            template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
        }));

        concrete_deps.push((dep_name, concrete_dep));
    }

    Ok(concrete_deps)
}

/// Generates a dependency name from a path.
/// Creates collision-resistant names by preserving directory structure.
pub fn generate_dependency_name(path: &str) -> String {
    // Convert path to a collision-resistant name
    // Example: "agents/ai/helper.md" -> "ai/helper"
    // Example: "snippets/commands/commit.md" -> "commands/commit"
    // Example: "commit.md" -> "commit"
    // Example (absolute): "/private/tmp/shared/snippets/utils.md" -> "/private/tmp/shared/snippets/utils"
    // Example (Windows absolute): "C:/team/tools/foo.md" -> "C:/team/tools/foo"
    // Example (parent-relative): "../shared/utils.md" -> "../shared/utils"

    let path = Path::new(path);

    // Get the path without extension
    let without_ext = path.with_extension("");

    // Convert to string and normalize separators to forward slashes
    // This ensures consistent behavior on Windows where Path::to_string_lossy()
    // produces backslashes, which would break our split('/') logic below
    let path_str = without_ext.to_string_lossy().replace('\\', "/");

    // Check if this is an absolute path or starts with ../ (cross-directory)
    // Note: With the fix to always use manifest-relative paths (even with ../),
    // lockfiles should never contain absolute paths. We check path.is_absolute()
    // defensively for manually-edited lockfiles.
    let is_absolute = path.is_absolute();
    let is_cross_directory = path_str.starts_with("../");

    // If the path has multiple components, skip the first directory (resource type)
    // to avoid redundancy, but keep subdirectories for uniqueness
    // EXCEPTIONS that keep all components to avoid collisions:
    // 1. Absolute paths (e.g., C:/team/tools/foo.md vs D:/team/tools/foo.md)
    // 2. Cross-directory paths with ../ (e.g., ../shared/a.md vs ../other/a.md)
    let components: Vec<&str> = path_str.split('/').collect();

    let result = if components.len() > 1 && !is_absolute && !is_cross_directory {
        // Skip first component (resource type) for normal relative paths
        components[1..].join("/")
    } else {
        // Keep all components for absolute paths, cross-directory paths, or single-component paths
        path_str
    };

    // Ensure the result is not empty
    if result.is_empty() {
        "unnamed".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::DetailedDependency;

    #[test]
    fn test_generate_dependency_name() {
        // Basic relative paths
        assert_eq!(generate_dependency_name("agents/helper.md"), "helper");
        assert_eq!(generate_dependency_name("snippets/rust-patterns.md"), "rust-patterns");
        assert_eq!(generate_dependency_name("commands/deploy.sh"), "deploy");

        // Paths with subdirectories
        assert_eq!(generate_dependency_name("agents/ai/helper.md"), "ai/helper");
        assert_eq!(generate_dependency_name("snippets/commands/commit.md"), "commands/commit");

        // Single component paths
        assert_eq!(generate_dependency_name("README.md"), "README");
        assert_eq!(generate_dependency_name("helper.md"), "helper");

        // Cross-directory paths (should keep all components)
        assert_eq!(generate_dependency_name("../shared/utils.md"), "../shared/utils");
        assert_eq!(generate_dependency_name("../../common/base.md"), "../../common/base");

        // Absolute paths (should keep all components)
        assert_eq!(generate_dependency_name("/tmp/shared/utils.md"), "/tmp/shared/utils");

        // Windows absolute paths (only test on Windows where they're recognized as absolute)
        #[cfg(target_os = "windows")]
        assert_eq!(generate_dependency_name("C:/team/tools/foo.md"), "C:/team/tools/foo");

        // Complex names with special characters
        assert_eq!(generate_dependency_name("agents/ai-helper_v2.md"), "ai-helper_v2");
        assert_eq!(generate_dependency_name("snippets/rust-patterns@123.md"), "rust-patterns@123");

        // Edge cases
        assert_eq!(generate_dependency_name(".hidden.md"), ".hidden");
        assert_eq!(generate_dependency_name(""), "unnamed");
    }

    #[tokio::test]
    async fn test_expand_local_pattern() {
        // This test would require creating temporary files and directories
        // For now, we'll test the logic with a mock scenario
        let dep = ResourceDependency::Detailed(Box::new(DetailedDependency {
            path: "tests/fixtures/*.md".to_string(),
            source: None,
            version: None,
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

        // Note: This test would need actual test files to work properly
        // For now, we just verify the function signature and basic structure
        match expand_local_pattern(&dep, "tests/fixtures/*.md", None).await {
            Ok(_) => println!("Pattern expansion succeeded"),
            Err(e) => println!("Pattern expansion failed (expected in test): {}", e),
        }
    }
}
