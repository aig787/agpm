//! Cross-platform utilities and helpers
//!
//! This module provides utility functions for file operations, platform-specific
//! code, and user interface elements like progress bars. All utilities are designed
//! to work consistently across Windows, macOS, and Linux.
//!
//! # Modules
//!
//! - [`fs`] - File system operations with atomic writes and safe copying
//! - [`manifest_utils`] - Utilities for loading and validating manifests
//! - [`platform`] - Platform-specific helpers and path resolution
//! - [`progress`] - Multi-phase progress tracking for long-running operations
//!
//! # Cross-Platform Considerations
//!
//! All utilities handle platform differences:
//! - Path separators (`/` vs `\`)
//! - Line endings (`\n` vs `\r\n`)
//! - File permissions and attributes
//! - Shell commands and environment variables
//!
//! # Example
//!
//! ```rust,no_run
//! use agpm_cli::utils::{ensure_dir, atomic_write, MultiPhaseProgress, InstallationPhase};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Ensure directory exists
//! ensure_dir(Path::new("output/agents"))?;
//!
//! // Write file atomically
//! atomic_write(Path::new("output/config.toml"), b"content")?;
//!
//! // Show progress with phases
//! let progress = MultiPhaseProgress::new(true);
//! progress.start_phase(InstallationPhase::Installing, Some("Processing files"));
//! # Ok(())
//! # }
//! ```

pub mod fs;
pub mod manifest_utils;
pub mod path_validation;
pub mod platform;
pub mod progress;
pub mod security;

pub use fs::{
    atomic_write, compare_file_times, copy_dir, create_temp_file, ensure_dir,
    file_exists_and_readable, get_modified_time, normalize_path, read_json_file, read_text_file,
    read_toml_file, read_yaml_file, safe_write, write_json_file, write_text_file, write_toml_file,
    write_yaml_file,
};
pub use manifest_utils::{
    load_and_validate_manifest, load_project_manifest, manifest_exists, manifest_path,
};
pub use path_validation::{
    ensure_directory_exists, ensure_within_directory, find_project_root, safe_canonicalize,
    safe_relative_path, sanitize_file_name, validate_no_traversal, validate_project_path,
    validate_resource_path,
};
pub use platform::{
    get_git_command, get_home_dir, is_windows, normalize_path_for_storage, resolve_path,
};
pub use progress::{InstallationPhase, MultiPhaseProgress, ProgressBar, collect_dependency_names};

/// Determines if a given URL/path is a local filesystem path (not a Git repository URL).
///
/// Local paths are directories on the filesystem that are directly accessible,
/// as opposed to Git repository URLs that need to be cloned/fetched.
///
/// # Examples
///
/// ```
/// use agpm_cli::utils::is_local_path;
///
/// // Unix-style paths
/// assert!(is_local_path("/absolute/path"));
/// assert!(is_local_path("./relative/path"));
/// assert!(is_local_path("../parent/path"));
///
/// // Windows-style paths (with drive letters or UNC)
/// assert!(is_local_path("C:/Users/path"));
/// assert!(is_local_path("C:\\Users\\path"));
/// assert!(is_local_path("//server/share"));
/// assert!(is_local_path("\\\\server\\share"));
///
/// // Git URLs (not local paths)
/// assert!(!is_local_path("https://github.com/user/repo.git"));
/// assert!(!is_local_path("git@github.com:user/repo.git"));
/// assert!(!is_local_path("file:///path/to/repo.git"));
/// ```
#[must_use]
pub fn is_local_path(url: &str) -> bool {
    // file:// URLs are Git repository URLs, not local paths
    if url.starts_with("file://") {
        return false;
    }

    // Unix-style absolute or relative paths
    if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
        return true;
    }

    // Windows-style paths
    // Check for drive letter (e.g., C:/ or C:\)
    if url.len() >= 2 {
        let chars: Vec<char> = url.chars().collect();
        if chars[0].is_ascii_alphabetic() && chars[1] == ':' {
            return true;
        }
    }

    // Check for UNC paths (e.g., //server/share or \\server\share)
    if url.starts_with("//") || url.starts_with("\\\\") {
        return true;
    }

    false
}

/// Determines if a given URL is a Git repository URL (including file:// URLs).
///
/// Git repository URLs need to be cloned/fetched, unlike local filesystem paths.
///
/// # Examples
///
/// ```
/// use agpm_cli::utils::is_git_url;
///
/// assert!(is_git_url("https://github.com/user/repo.git"));
/// assert!(is_git_url("git@github.com:user/repo.git"));
/// assert!(is_git_url("file:///path/to/repo.git"));
/// assert!(is_git_url("ssh://git@server.com/repo.git"));
/// assert!(!is_git_url("/absolute/path"));
/// assert!(!is_git_url("./relative/path"));
/// ```
#[must_use]
pub fn is_git_url(url: &str) -> bool {
    !is_local_path(url)
}

/// Resolves a file-relative path from a transitive dependency.
///
/// This function resolves paths that start with `./` or `../` relative to the
/// directory containing the parent resource file. This provides a unified way to
/// resolve transitive dependencies for both Git-backed and path-only resources.
///
/// # Arguments
///
/// * `parent_file_path` - Absolute path to the file declaring the dependency
/// * `relative_path` - Path from the transitive dep spec (must start with `./` or `../`)
///
/// # Returns
///
/// Canonical absolute path to the dependency.
///
/// # Errors
///
/// Returns an error if:
/// - `relative_path` doesn't start with `./` or `../`
/// - The resolved path doesn't exist
/// - Canonicalization fails
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agpm_cli::utils::resolve_file_relative_path;
///
/// let parent = Path::new("/project/agents/helper.md");
/// let resolved = resolve_file_relative_path(parent, "./snippets/utils.md")?;
/// // Returns: /project/agents/snippets/utils.md
///
/// let resolved = resolve_file_relative_path(parent, "../common/base.md")?;
/// // Returns: /project/common/base.md
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn resolve_file_relative_path(
    parent_file_path: &std::path::Path,
    relative_path: &str,
) -> anyhow::Result<std::path::PathBuf> {
    use anyhow::{Context, anyhow};

    // Validate it's a file-relative path
    if !relative_path.starts_with("./") && !relative_path.starts_with("../") {
        return Err(anyhow!(
            "Transitive dependency path must start with './' or '../': {}",
            relative_path
        ));
    }

    // Get parent directory
    let parent_dir = parent_file_path
        .parent()
        .ok_or_else(|| anyhow!("Parent file has no directory: {}", parent_file_path.display()))?;

    // Resolve relative to parent's directory
    let resolved = parent_dir.join(relative_path);

    // Canonicalize (resolves .. and ., checks existence)
    resolved.canonicalize().with_context(|| {
        format!(
            "Transitive dependency does not exist: {} (resolved from '{}' relative to '{}')",
            resolved.display(),
            relative_path,
            parent_dir.display()
        )
    })
}

/// Resolves a path relative to the manifest directory.
///
/// This function handles shell expansion and both relative and absolute paths,
/// resolving them relative to the directory containing the manifest file.
///
/// # Arguments
///
/// * `manifest_dir` - The directory containing the agpm.toml manifest
/// * `rel_path` - The path to resolve (can be relative or absolute)
///
/// # Returns
///
/// Canonical absolute path to the resource.
///
/// # Errors
///
/// Returns an error if:
/// - Shell expansion fails
/// - The path doesn't exist
/// - Canonicalization fails
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agpm_cli::utils::resolve_path_relative_to_manifest;
///
/// let manifest_dir = Path::new("/project");
/// let resolved = resolve_path_relative_to_manifest(manifest_dir, "../shared/agents/helper.md")?;
/// // Returns: /shared/agents/helper.md
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn resolve_path_relative_to_manifest(
    manifest_dir: &std::path::Path,
    rel_path: &str,
) -> anyhow::Result<std::path::PathBuf> {
    use anyhow::Context;

    let expanded = shellexpand::full(rel_path)
        .with_context(|| format!("Failed to expand path: {}", rel_path))?;
    let path = std::path::PathBuf::from(expanded.as_ref());

    let resolved = if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    };

    resolved.canonicalize().with_context(|| {
        format!(
            "Path does not exist: {} (resolved from manifest dir '{}')",
            resolved.display(),
            manifest_dir.display()
        )
    })
}

/// Computes a relative path from a base directory to a target path.
///
/// This function handles paths both inside and outside the base directory,
/// using `../` notation when the target is outside. Both paths should be
/// absolute and canonicalized for correct results.
///
/// This is critical for lockfile portability - we must store manifest-relative
/// paths even when they go outside the project with `../`.
///
/// # Arguments
///
/// * `base` - The base directory (should be absolute and canonicalized)
/// * `target` - The target path (should be absolute and canonicalized)
///
/// # Returns
///
/// A relative path from base to target, using `../` notation if needed.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agpm_cli::utils::compute_relative_path;
///
/// let base = Path::new("/project");
/// let target = Path::new("/project/agents/helper.md");
/// let relative = compute_relative_path(base, target);
/// // Returns: "agents/helper.md"
///
/// let target_outside = Path::new("/shared/utils.md");
/// let relative = compute_relative_path(base, target_outside);
/// // Returns: "../shared/utils.md"
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn compute_relative_path(base: &std::path::Path, target: &std::path::Path) -> String {
    use std::path::Component;

    // Try simple strip_prefix first (common case: target inside base)
    if let Ok(relative) = target.strip_prefix(base) {
        return relative.to_string_lossy().to_string();
    }

    // Target is outside base - need to compute path with ../
    let base_components: Vec<_> = base.components().collect();
    let target_components: Vec<_> = target.components().collect();

    // Find the common prefix
    let mut common_prefix_len = 0;
    for (b, t) in base_components.iter().zip(target_components.iter()) {
        if b == t {
            common_prefix_len += 1;
        } else {
            break;
        }
    }

    // Use slices instead of drain for better performance (avoid reallocation)
    let base_remainder = &base_components[common_prefix_len..];
    let target_remainder = &target_components[common_prefix_len..];

    // Build the relative path
    let mut result = std::path::PathBuf::new();

    // Add ../ for each remaining base component
    for _ in base_remainder {
        result.push("..");
    }

    // Add the remaining target components
    for component in target_remainder {
        if let Component::Normal(c) = component {
            result.push(c);
        }
    }

    result.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_compute_relative_path_inside_base() {
        // Target inside base directory
        let base = Path::new("/project");
        let target = Path::new("/project/agents/helper.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "agents/helper.md");
    }

    #[test]
    fn test_compute_relative_path_outside_base() {
        // Target outside base directory (sibling)
        let base = Path::new("/project");
        let target = Path::new("/shared/utils.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "../shared/utils.md");
    }

    #[test]
    fn test_compute_relative_path_multiple_levels_up() {
        // Target multiple levels up
        let base = Path::new("/project/subdir");
        let target = Path::new("/other/file.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "../../other/file.md");
    }

    #[test]
    fn test_compute_relative_path_same_directory() {
        // Base and target are the same
        let base = Path::new("/project");
        let target = Path::new("/project");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "");
    }

    #[test]
    fn test_compute_relative_path_nested() {
        // Complex nesting
        let base = Path::new("/a/b/c");
        let target = Path::new("/a/d/e/f.md");
        let result = compute_relative_path(base, target);
        assert_eq!(result, "../../d/e/f.md");
    }
}
