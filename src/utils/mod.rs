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
//! use ccpm::utils::{ensure_dir, atomic_write, MultiPhaseProgress, InstallationPhase};
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
pub use platform::{get_git_command, get_home_dir, is_windows, resolve_path};
pub use progress::{InstallationPhase, MultiPhaseProgress, ProgressBar, collect_dependency_names};

/// Determines if a given URL/path is a local filesystem path (not a Git repository URL).
///
/// Local paths are directories on the filesystem that are directly accessible,
/// as opposed to Git repository URLs that need to be cloned/fetched.
///
/// # Examples
///
/// ```
/// use ccpm::utils::is_local_path;
///
/// assert!(is_local_path("/absolute/path"));
/// assert!(is_local_path("./relative/path"));
/// assert!(is_local_path("../parent/path"));
/// assert!(!is_local_path("https://github.com/user/repo.git"));
/// assert!(!is_local_path("git@github.com:user/repo.git"));
/// assert!(!is_local_path("file:///path/to/repo.git"));
/// ```
#[must_use]
pub fn is_local_path(url: &str) -> bool {
    // file:// URLs are Git repository URLs, not local paths
    !url.starts_with("file://")
        && (url.starts_with('/') || url.starts_with("./") || url.starts_with("../"))
}

/// Determines if a given URL is a Git repository URL (including file:// URLs).
///
/// Git repository URLs need to be cloned/fetched, unlike local filesystem paths.
///
/// # Examples
///
/// ```
/// use ccpm::utils::is_git_url;
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
