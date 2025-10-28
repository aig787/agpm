//! Helper functions for manifest file operations.
//!
//! This module provides utility functions for:
//! - URL expansion with environment variable and path resolution
//! - Manifest file discovery in directory hierarchies

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Expand local paths to file:// URLs, preserving standard Git URLs.
///
/// Converts local paths to file:// URLs while leaving http://, https://,
/// git@, and file:// URLs unchanged. Returns original string on expansion failure.
pub fn expand_url(url: &str) -> Result<String> {
    // If it looks like a standard protocol URL (http, https, git@, file://), don't expand
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("git@")
        || url.starts_with("file://")
    {
        return Ok(url.to_string());
    }

    // Only try to expand if it looks like a local path (contains path separators, starts with ~, or contains env vars)
    if url.contains('/') || url.contains('\\') || url.starts_with('~') || url.contains('$') {
        // For cases that look like local paths, try to expand as a local path and convert to file:// URL
        match crate::utils::platform::resolve_path(url) {
            Ok(expanded_path) => {
                // Convert to file:// URL
                let path_str = expanded_path.to_string_lossy();
                if expanded_path.is_absolute() {
                    Ok(format!("file://{path_str}"))
                } else {
                    Ok(format!(
                        "file://{}",
                        std::env::current_dir()?.join(expanded_path).to_string_lossy()
                    ))
                }
            }
            Err(_) => {
                // If path expansion fails, return the original URL
                // This allows the validation to catch the error with a proper message
                Ok(url.to_string())
            }
        }
    } else {
        // For strings that don't look like paths, return as-is to let validation catch the error
        Ok(url.to_string())
    }
}

/// Find manifest by searching up directory tree from current directory.
///
/// Searches for `agpm.toml` starting from the current working directory
/// and walking up until found or filesystem root is reached.
///
/// Mirrors Cargo, Git, and NPM project file discovery behavior.
///
/// # Search Algorithm
///
/// 1. Start from the current working directory
/// 2. Look for `agpm.toml` in the current directory
/// 3. If not found, move to the parent directory
/// 4. Repeat until found or reach the filesystem root
/// 5. Return error if no manifest is found
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::manifest::find_manifest;
///
/// // Find manifest from current directory
/// match find_manifest() {
///     Ok(path) => println!("Found manifest at: {}", path.display()),
///     Err(e) => println!("No manifest found: {}", e),
/// }
/// ```
///
/// # Directory Structure Example
///
/// ```text
/// /home/user/project/
/// ├── agpm.toml          ← Found here
/// └── subdir/
///     └── deep/
///         └── nested/     ← Search started here, walks up
/// ```
///
/// If called from `/home/user/project/subdir/deep/nested/`, this function
/// will find and return `/home/user/project/agpm.toml`.
///
/// # Error Conditions
///
/// - **No manifest found**: Searched to filesystem root without finding `agpm.toml`
/// - **Permission denied**: Cannot read current directory or traverse up
/// - **Filesystem corruption**: Cannot determine current working directory
///
/// # Use Cases
///
/// This function is typically called by CLI commands that need to locate the
/// project configuration, allowing users to run AGPM commands from any
/// subdirectory within their project.
pub fn find_manifest() -> Result<PathBuf> {
    let current = std::env::current_dir()
        .context("Cannot determine current working directory. This may indicate a permission issue or corrupted filesystem")?;
    find_manifest_from(current)
}

/// Find manifest using explicit path or directory search.
///
/// Uses explicit path if provided and exists, otherwise searches from current directory.
///
/// # Errors
///
/// - Explicit path provided but doesn't exist
/// - No explicit path and no manifest found via search
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::manifest::find_manifest_with_optional;
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Use explicit path
/// let explicit = Some(PathBuf::from("/path/to/agpm.toml"));
/// let manifest = find_manifest_with_optional(explicit)?;
///
/// // Search from current directory
/// let manifest = find_manifest_with_optional(None)?;
/// # Ok(())
/// # }
/// ```
pub fn find_manifest_with_optional(explicit_path: Option<PathBuf>) -> Result<PathBuf> {
    match explicit_path {
        Some(path) => {
            if path.exists() {
                Ok(path)
            } else {
                Err(crate::core::AgpmError::ManifestNotFound.into())
            }
        }
        None => find_manifest(),
    }
}

/// Find manifest by searching up from a specific starting directory.
///
/// Core discovery function implementing directory traversal from a given
/// starting point. Used internally by [`find_manifest`].
///
/// # Algorithm
///
/// 1. Check for `agpm.toml` in current directory
/// 2. If found, return full path
/// 3. If not found, move to parent directory
/// 4. Repeat until found or filesystem root reached
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::manifest::find_manifest_from;
/// use std::path::PathBuf;
///
/// // Search from a specific directory
/// let start_dir = PathBuf::from("/home/user/project/subdir");
/// match find_manifest_from(start_dir) {
///     Ok(manifest_path) => {
///         println!("Found manifest: {}", manifest_path.display());
///     }
///     Err(e) => {
///         println!("No manifest found: {}", e);
///     }
/// }
/// ```
///
/// # Performance Considerations
///
/// - Each directory check involves a filesystem stat operation
/// - Search depth is limited by filesystem hierarchy (typically < 20 levels)
/// - Function returns immediately upon finding the first manifest
/// - No filesystem locks are held during the search
///
/// # Cross-Platform Behavior
///
/// - Works correctly on Windows, macOS, and Linux
/// - Handles filesystem roots appropriately (`/` on Unix, `C:\` on Windows)
/// - Respects platform-specific path separators and conventions
/// - Works with network filesystems and mounted volumes
///
/// # Error Handling
///
/// Returns [`crate::core::AgpmError::ManifestNotFound`] wrapped in an [`anyhow::Error`]
/// if no manifest file is found after searching to the filesystem root.
pub fn find_manifest_from(mut current: PathBuf) -> Result<PathBuf> {
    loop {
        let manifest_path = current.join("agpm.toml");
        if manifest_path.exists() {
            return Ok(manifest_path);
        }

        if !current.pop() {
            return Err(crate::core::AgpmError::ManifestNotFound.into());
        }
    }
}
