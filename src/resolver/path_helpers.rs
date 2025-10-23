// ! Path computation and manipulation helpers for dependency resolution.
//!
//! This module provides utilities for computing installation paths, resolving
//! pattern paths, and determining flatten behavior for resources.

use crate::core::ResourceType;
use crate::manifest::{Manifest, ResourceDependency};
use crate::utils::{compute_relative_install_path, normalize_path, normalize_path_for_storage};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parses a pattern string to extract the base path and pattern components.
///
/// Handles three cases:
/// 1. Patterns with path separators and absolute/relative parents
/// 2. Patterns with path separators but simple relative paths
/// 3. Simple patterns without path separators
///
/// # Arguments
///
/// * `pattern` - The glob pattern string (e.g., "agents/*.md", "../foo/*.md")
///
/// # Returns
///
/// A tuple of (base_path, pattern_str) where:
/// - `base_path` is the directory to search in
/// - `pattern_str` is the glob pattern to match files against
///
/// # Examples
///
/// ```
/// use std::path::{Path, PathBuf};
/// use agpm_cli::resolver::path_helpers::parse_pattern_base_path;
///
/// let (base, pattern) = parse_pattern_base_path("agents/*.md");
/// assert_eq!(base, PathBuf::from("."));
/// assert_eq!(pattern, "agents/*.md");
///
/// let (base, pattern) = parse_pattern_base_path("../foo/bar/*.md");
/// assert_eq!(base, PathBuf::from("../foo/bar"));
/// assert_eq!(pattern, "*.md");
/// ```
pub fn parse_pattern_base_path(pattern: &str) -> (PathBuf, String) {
    if pattern.contains('/') || pattern.contains('\\') {
        // Pattern contains path separators, extract base path
        let pattern_path = Path::new(pattern);
        if let Some(parent) = pattern_path.parent() {
            if parent.is_absolute() || parent.starts_with("..") || parent.starts_with(".") {
                // Use the parent as base path and just the filename pattern
                (
                    parent.to_path_buf(),
                    pattern_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(pattern)
                        .to_string(),
                )
            } else {
                // Relative path, use current directory as base
                (PathBuf::from("."), pattern.to_string())
            }
        } else {
            // No parent, use current directory
            (PathBuf::from("."), pattern.to_string())
        }
    } else {
        // Simple pattern without path separators
        (PathBuf::from("."), pattern.to_string())
    }
}

/// Computes the installation path for a merge-target resource (Hook or McpServer).
///
/// These resources are not installed as files but are merged into configuration files.
/// The installation path is determined by the tool's merge target configuration or
/// hardcoded defaults.
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Hook or McpServer)
///
/// # Returns
///
/// The normalized path to the merge target configuration file.
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::path_helpers::compute_merge_target_install_path;
///
/// let manifest = Manifest::new();
/// let path = compute_merge_target_install_path(&manifest, "claude-code", ResourceType::Hook);
/// assert_eq!(path, ".claude/settings.local.json");
/// ```
pub fn compute_merge_target_install_path(
    manifest: &Manifest,
    artifact_type: &str,
    resource_type: ResourceType,
) -> String {
    // Use configured merge target, with fallback to hardcoded defaults
    if let Some(merge_target) = manifest.get_merge_target(artifact_type, resource_type) {
        normalize_path_for_storage(merge_target.display().to_string())
    } else {
        // Fallback to hardcoded defaults if not configured
        match resource_type {
            ResourceType::Hook => ".claude/settings.local.json".to_string(),
            ResourceType::McpServer => {
                if artifact_type == "opencode" {
                    ".opencode/opencode.json".to_string()
                } else {
                    ".mcp.json".to_string()
                }
            }
            _ => unreachable!(
                "compute_merge_target_install_path should only be called for Hook or McpServer"
            ),
        }
    }
}

/// Computes the installation path for a regular resource (Agent, Command, Snippet, Script).
///
/// Regular resources are installed as files in tool-specific directories. This function
/// determines the final installation path by:
/// 1. Getting the base artifact path from tool configuration
/// 2. Applying any custom target override from the dependency
/// 3. Computing the relative path based on flatten behavior
/// 4. Avoiding redundant directory prefixes
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Agent, Command, etc.)
/// * `filename` - The meaningful path structure extracted from the source file
///
/// # Returns
///
/// The normalized installation path, or an error if the resource type is not supported
/// by the specified tool.
///
/// # Errors
///
/// Returns an error if:
/// - The resource type is not supported by the specified tool
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::path_helpers::compute_regular_resource_install_path;
///
/// # fn example() -> anyhow::Result<()> {
/// let manifest = Manifest::new();
/// # let dep: agpm_cli::manifest::ResourceDependency = todo!();
/// let path = compute_regular_resource_install_path(
///     &manifest,
///     &dep,
///     "claude-code",
///     ResourceType::Agent,
///     "agents/helper.md"
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn compute_regular_resource_install_path(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
    filename: &str,
) -> Result<String> {
    // Get the artifact path for this resource type
    let artifact_path =
        manifest.get_artifact_resource_path(artifact_type, resource_type).ok_or_else(|| {
            anyhow::anyhow!(
                "Resource type '{}' is not supported by tool '{}'",
                resource_type,
                artifact_type
            )
        })?;

    // Determine flatten behavior: use explicit setting or tool config default
    let flatten = get_flatten_behavior(manifest, dep, artifact_type, resource_type);

    // Determine the base target directory
    let base_target = if let Some(custom_target) = dep.get_target() {
        // Custom target is relative to the artifact's resource directory
        PathBuf::from(artifact_path.display().to_string())
            .join(custom_target.trim_start_matches('/'))
    } else {
        artifact_path.to_path_buf()
    };

    // Use compute_relative_install_path to avoid redundant prefixes
    let relative_path = compute_relative_install_path(&base_target, Path::new(filename), flatten);
    Ok(normalize_path_for_storage(normalize_path(&base_target.join(relative_path))))
}

/// Determines the flatten behavior for a resource installation.
///
/// Flatten behavior controls whether directory structure from the source repository
/// is preserved in the installation path. The decision is made by checking:
/// 1. Explicit `flatten` setting on the dependency (highest priority)
/// 2. Tool configuration default for this resource type
/// 3. Global default (false)
///
/// # Arguments
///
/// * `manifest` - The project manifest containing tool configurations
/// * `dep` - The resource dependency specification
/// * `artifact_type` - The tool name (e.g., "claude-code", "opencode")
/// * `resource_type` - The resource type (Agent, Command, etc.)
///
/// # Returns
///
/// `true` if directory structure should be flattened, `false` if it should be preserved.
///
/// # Examples
///
/// ```no_run
/// use agpm_cli::core::ResourceType;
/// use agpm_cli::manifest::Manifest;
/// use agpm_cli::resolver::path_helpers::get_flatten_behavior;
///
/// let manifest = Manifest::new();
/// # let dep: agpm_cli::manifest::ResourceDependency = todo!();
/// let flatten = get_flatten_behavior(&manifest, &dep, "claude-code", ResourceType::Agent);
/// ```
pub fn get_flatten_behavior(
    manifest: &Manifest,
    dep: &ResourceDependency,
    artifact_type: &str,
    resource_type: ResourceType,
) -> bool {
    let dep_flatten = dep.get_flatten();
    let tool_flatten = manifest
        .get_tool_config(artifact_type)
        .and_then(|config| config.resources.get(resource_type.to_plural()))
        .and_then(|resource_config| resource_config.flatten);

    dep_flatten.or(tool_flatten).unwrap_or(false) // Default to false if not configured
}

/// Constructs the full relative path for a matched pattern file.
///
/// Combines the base path with the matched file path, normalizing path separators
/// for storage in the lockfile.
///
/// # Arguments
///
/// * `base_path` - The base directory the pattern was resolved in
/// * `matched_path` - The path to the matched file (relative to base_path)
///
/// # Returns
///
/// A normalized path string suitable for storage in the lockfile.
///
/// # Examples
///
/// ```
/// use std::path::{Path, PathBuf};
/// use agpm_cli::resolver::path_helpers::construct_full_relative_path;
///
/// let base = PathBuf::from(".");
/// let matched = Path::new("agents/helper.md");
/// let path = construct_full_relative_path(&base, matched);
/// assert_eq!(path, "agents/helper.md");
///
/// let base = PathBuf::from("../foo");
/// let matched = Path::new("bar.md");
/// let path = construct_full_relative_path(&base, matched);
/// assert_eq!(path, "../foo/bar.md");
/// ```
pub fn construct_full_relative_path(base_path: &Path, matched_path: &Path) -> String {
    if base_path == Path::new(".") {
        crate::utils::normalize_path_for_storage(matched_path.to_string_lossy().to_string())
    } else {
        crate::utils::normalize_path_for_storage(format!(
            "{}/{}",
            base_path.display(),
            matched_path.display()
        ))
    }
}

/// Extracts the meaningful path for pattern matching.
///
/// Constructs the full path from base path and matched path, then extracts
/// the meaningful structure using `extract_meaningful_path`.
///
/// # Arguments
///
/// * `base_path` - The base directory the pattern was resolved in
/// * `matched_path` - The path to the matched file (relative to base_path)
///
/// # Returns
///
/// The meaningful path structure string.
///
/// # Examples
///
/// ```
/// use std::path::{Path, PathBuf};
/// use agpm_cli::resolver::path_helpers::extract_pattern_filename;
///
/// let base = PathBuf::from(".");
/// let matched = Path::new("agents/helper.md");
/// let filename = extract_pattern_filename(&base, matched);
/// assert_eq!(filename, "agents/helper.md");
/// ```
pub fn extract_pattern_filename(base_path: &Path, matched_path: &Path) -> String {
    let full_path = if base_path == Path::new(".") {
        matched_path.to_path_buf()
    } else {
        base_path.join(matched_path)
    };
    crate::resolver::extract_meaningful_path(&full_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pattern_base_path_simple() {
        let (base, pattern) = parse_pattern_base_path("*.md");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "*.md");
    }

    #[test]
    fn test_parse_pattern_base_path_with_directory() {
        let (base, pattern) = parse_pattern_base_path("agents/*.md");
        assert_eq!(base, PathBuf::from("."));
        assert_eq!(pattern, "agents/*.md");
    }

    #[test]
    fn test_parse_pattern_base_path_with_parent() {
        let (base, pattern) = parse_pattern_base_path("../foo/*.md");
        assert_eq!(base, PathBuf::from("../foo"));
        assert_eq!(pattern, "*.md");
    }

    #[test]
    fn test_parse_pattern_base_path_with_current_dir() {
        let (base, pattern) = parse_pattern_base_path("./foo/*.md");
        assert_eq!(base, PathBuf::from("./foo"));
        assert_eq!(pattern, "*.md");
    }

    #[test]
    fn test_construct_full_relative_path_current_dir() {
        let base = PathBuf::from(".");
        let matched = Path::new("agents/helper.md");
        let path = construct_full_relative_path(&base, matched);
        assert_eq!(path, "agents/helper.md");
    }

    #[test]
    fn test_construct_full_relative_path_with_base() {
        let base = PathBuf::from("../foo");
        let matched = Path::new("bar.md");
        let path = construct_full_relative_path(&base, matched);
        assert_eq!(path, "../foo/bar.md");
    }

    #[test]
    fn test_extract_pattern_filename_current_dir() {
        let base = PathBuf::from(".");
        let matched = Path::new("agents/helper.md");
        let filename = extract_pattern_filename(&base, matched);
        assert_eq!(filename, "agents/helper.md");
    }
}
