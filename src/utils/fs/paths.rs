//! Path utilities for normalization, validation, and discovery.
//!
//! This module provides path manipulation functions with security
//! checks for path traversal and project root discovery.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Normalizes a path by resolving `.` and `..` components.
///
/// This function cleans up path components by:
/// - Removing `.` (current directory) components
/// - Resolving `..` (parent directory) components
/// - Maintaining the path's absolute or relative nature
///
/// Note: This function performs logical path resolution without accessing the filesystem.
/// It does not resolve symbolic links or verify that the path exists.
///
/// # Arguments
///
/// * `path` - The path to normalize
///
/// # Returns
///
/// A normalized [`PathBuf`] with `.` and `..` components resolved
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::normalize_path;
/// use std::path::{Path, PathBuf};
///
/// let path = Path::new("/foo/./bar/../baz");
/// let normalized = normalize_path(path);
/// assert_eq!(normalized, PathBuf::from("/foo/baz"));
///
/// let relative = Path::new("../src/./lib.rs");
/// let normalized_rel = normalize_path(relative);
/// assert_eq!(normalized_rel, PathBuf::from("../src/lib.rs"));
/// ```
///
/// # Use Cases
///
/// - Cleaning user input paths
/// - Path comparison and deduplication
/// - Security checks for path traversal
/// - Canonical path representation
///
/// # See Also
///
/// - `is_safe_path` for security validation using this normalization
/// - `safe_canonicalize` for filesystem-aware path resolution
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {} // Skip .
            std::path::Component::ParentDir => {
                components.pop(); // Remove previous component for ..
            }
            c => components.push(c),
        }
    }

    components.iter().collect()
}

/// Checks if a path is safe and doesn't escape the base directory.
///
/// This function prevents directory traversal attacks by ensuring that the resolved
/// path remains within the base directory. It handles both absolute and relative paths,
/// normalizing them before comparison.
///
/// # Arguments
///
/// * `base` - The base directory that should contain the path
/// * `path` - The path to validate (can be absolute or relative)
///
/// # Returns
///
/// - `true` if the path is safe and stays within the base directory
/// - `false` if the path would escape the base directory
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::is_safe_path;
/// use std::path::Path;
///
/// let base = Path::new("/home/user/project");
///
/// // Safe paths
/// assert!(is_safe_path(base, Path::new("src/main.rs")));
/// assert!(is_safe_path(base, Path::new("./config/settings.toml")));
///
/// // Unsafe paths (directory traversal)
/// assert!(!is_safe_path(base, Path::new("../../../etc/passwd")));
/// assert!(!is_safe_path(base, Path::new("/etc/passwd")));
/// ```
///
/// # Security
///
/// This function is essential for preventing directory traversal vulnerabilities
/// when processing user-provided paths. It should be used whenever:
/// - Extracting archives or packages
/// - Processing configuration files with path references
/// - Handling user input that specifies file locations
///
/// # Implementation
///
/// The function normalizes both paths using `normalize_path` before comparison,
/// ensuring that path traversal attempts using `../` are properly detected.
#[must_use]
pub fn is_safe_path(base: &Path, path: &Path) -> bool {
    let normalized_base = normalize_path(base);
    let normalized_path = if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&base.join(path))
    };

    normalized_path.starts_with(normalized_base)
}

/// Finds the AGPM project root by searching for `agpm.toml` in the directory hierarchy.
///
/// This function starts from the given directory and walks up the directory tree
/// looking for a `agpm.toml` file, which indicates the root of a AGPM project.
/// This is similar to how Git finds the repository root by looking for `.git`.
///
/// # Arguments
///
/// * `start` - The directory to start searching from (typically current directory)
///
/// # Returns
///
/// The path to the directory containing `agpm.toml`, or an error if not found
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::find_project_root;
/// use std::env;
///
/// # fn example() -> anyhow::Result<()> {
/// // Find project root from current directory
/// let current_dir = env::current_dir()?;
/// let project_root = find_project_root(&current_dir)?;
/// println!("Project root: {}", project_root.display());
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Starts from the given directory and searches upward
/// - Returns the first directory containing `agpm.toml`
/// - Canonicalizes the starting path to handle symlinks
/// - Stops at filesystem root if no `agpm.toml` is found
///
/// # Error Cases
///
/// - No `agpm.toml` found in the directory hierarchy
/// - Permission denied accessing parent directories
/// - Invalid or inaccessible starting path
///
/// # Use Cases
///
/// - CLI commands that need to operate on the current project
/// - Finding configuration files relative to project root
/// - Validating that commands are run within a AGPM project
pub fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut current = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());

    loop {
        if current.join("agpm.toml").exists() {
            return Ok(current);
        }

        if !current.pop() {
            return Err(anyhow::anyhow!(
                "No agpm.toml found in current directory or any parent directory"
            ));
        }
    }
}

/// Returns the path to the global AGPM configuration file.
///
/// This function constructs the path to the global configuration file following
/// platform conventions. The global config contains user-specific settings like
/// authentication tokens and private repository URLs.
///
/// # Returns
///
/// The path to `~/.config/agpm/config.toml`, or an error if the home directory
/// cannot be determined
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::get_global_config_path;
///
/// # fn example() -> anyhow::Result<()> {
/// let config_path = get_global_config_path()?;
/// println!("Global config at: {}", config_path.display());
///
/// // Check if global config exists
/// if config_path.exists() {
///     let config = std::fs::read_to_string(&config_path)?;
///     println!("Config contents: {}", config);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Platform Paths
///
/// - **Linux**: `~/.config/agpm/config.toml`
/// - **macOS**: `~/.config/agpm/config.toml`
/// - **Windows**: `%USERPROFILE%\.config\agpm\config.toml`
///
/// # Use Cases
///
/// - Loading global user configuration
/// - Storing authentication tokens securely
/// - Sharing settings across multiple projects
///
/// # Security Note
///
/// This file may contain sensitive information like API tokens. It should never
/// be committed to version control or shared publicly.
pub fn get_global_config_path() -> Result<PathBuf> {
    let home = crate::utils::platform::get_home_dir()?;
    Ok(home.join(".config").join("agpm").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_normalize_path() {
        let path = Path::new("/foo/./bar/../baz");
        let normalized = normalize_path(path);
        assert_eq!(normalized, PathBuf::from("/foo/baz"));
    }

    #[test]
    fn test_normalize_path_complex() {
        // Test various path normalization scenarios
        assert_eq!(normalize_path(Path::new("/")), PathBuf::from("/"));
        assert_eq!(normalize_path(Path::new("/foo/bar")), PathBuf::from("/foo/bar"));
        assert_eq!(normalize_path(Path::new("/foo/./bar")), PathBuf::from("/foo/bar"));
        assert_eq!(normalize_path(Path::new("/foo/../bar")), PathBuf::from("/bar"));
        assert_eq!(normalize_path(Path::new("/foo/bar/..")), PathBuf::from("/foo"));
        assert_eq!(normalize_path(Path::new("foo/./bar")), PathBuf::from("foo/bar"));
        assert_eq!(normalize_path(Path::new("./foo/bar")), PathBuf::from("foo/bar"));
    }

    #[test]
    fn test_is_safe_path() {
        let base = Path::new("/home/user/project");

        assert!(is_safe_path(base, Path::new("subdir/file.txt")));
        assert!(is_safe_path(base, Path::new("./subdir/file.txt")));
        assert!(!is_safe_path(base, Path::new("../other/file.txt")));
        assert!(!is_safe_path(base, Path::new("/etc/passwd")));
    }

    #[test]
    fn test_is_safe_path_edge_cases() {
        let base = Path::new("/home/user/project");

        // Safe paths
        assert!(is_safe_path(base, Path::new("")));
        assert!(is_safe_path(base, Path::new(".")));
        assert!(is_safe_path(base, Path::new("./nested/./path")));

        // Unsafe paths
        assert!(!is_safe_path(base, Path::new("..")));
        assert!(!is_safe_path(base, Path::new("../../etc")));
        assert!(!is_safe_path(base, Path::new("/absolute/path")));

        // Windows-style paths (on Unix these are relative)
        if cfg!(windows) {
            assert!(!is_safe_path(base, Path::new("C:\\Windows")));
        }
    }

    #[test]
    fn test_find_project_root() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let subdir = project.join("src").join("subdir");

        crate::utils::fs::ensure_dir(&subdir).unwrap();
        std::fs::write(project.join("agpm.toml"), "[sources]").unwrap();

        let root = find_project_root(&subdir).unwrap();
        assert_eq!(root.canonicalize().unwrap(), project.canonicalize().unwrap());
    }

    #[test]
    fn test_find_project_root_not_found() {
        let temp = tempdir().unwrap();
        let result = find_project_root(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_find_project_root_multiple_markers() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        let subproject = root.join("subproject");
        let deep = subproject.join("src");

        crate::utils::fs::ensure_dir(&deep).unwrap();
        std::fs::write(root.join("agpm.toml"), "[sources]").unwrap();
        std::fs::write(subproject.join("agpm.toml"), "[sources]").unwrap();

        // Should find the closest agpm.toml
        let found = find_project_root(&deep).unwrap();
        assert_eq!(found.canonicalize().unwrap(), subproject.canonicalize().unwrap());
    }

    #[test]
    fn test_get_global_config_path() {
        let config_path = get_global_config_path().unwrap();
        assert!(config_path.to_string_lossy().contains(".config"));
        assert!(config_path.to_string_lossy().contains("agpm"));
    }
}
