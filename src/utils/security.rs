//! Security utilities for path validation and access control
//!
//! This module provides security functions to prevent unauthorized access
//! to sensitive system directories and validate paths for safe operations.

use std::path::Path;

/// Security blacklist for local paths
/// Prevents access to sensitive system directories while allowing normal development paths
pub static BLACKLISTED_PATHS: &[&str] = &[
    "/etc",                    // System configuration
    "/sys",                    // System information
    "/proc",                   // Process information
    "/dev",                    // Device files
    "/boot",                   // Boot files
    "/root",                   // Root home
    "/bin",                    // System binaries
    "/sbin",                   // System binaries
    "/usr/bin",                // User binaries
    "/usr/sbin",               // User system binaries
    "/System",                 // macOS system
    "/Library",                // macOS system libraries
    "/private/etc",            // macOS etc
    "/private/var/db",         // macOS system databases
    "C:\\Windows",             // Windows system
    "C:\\Program Files",       // Windows programs
    "C:\\Program Files (x86)", // Windows 32-bit programs
    "C:\\ProgramData",         // Windows program data
    "C:\\System",              // Windows system
    "C:\\System32",            // Windows system32
    "C:\\Windows\\System32",   // Windows system32
];

/// Check if a path is blacklisted (points to sensitive system directories)
///
/// # Arguments
/// * `path` - The path to check
///
/// # Returns
/// * `true` if the path is blacklisted, `false` otherwise
///
/// # Examples
/// ```
/// use agpm_cli::utils::security::is_path_blacklisted;
/// use std::path::Path;
///
/// assert!(is_path_blacklisted(Path::new("/etc/passwd")));
/// assert!(is_path_blacklisted(Path::new("/System/Library")));
/// assert!(!is_path_blacklisted(Path::new("/home/user/project")));
/// ```
#[must_use]
pub fn is_path_blacklisted(path: &Path) -> bool {
    for blacklisted in BLACKLISTED_PATHS {
        if path.starts_with(blacklisted) {
            return true;
        }
    }
    false
}

/// Validates a path for security constraints
///
/// Checks if the path:
/// - Is not blacklisted
/// - Does not contain symlinks (if `check_symlinks` is true)
///
/// # Arguments
/// * `path` - The path to validate (should be the original path before canonicalization)
/// * `check_symlinks` - Whether to check for symlinks in the path
///
/// # Returns
/// * `Ok(())` if the path is safe
/// * `Err` with a descriptive error message if the path fails validation
pub fn validate_path_security(path: &Path, check_symlinks: bool) -> anyhow::Result<()> {
    // Check blacklist
    if is_path_blacklisted(path) {
        return Err(anyhow::anyhow!("Security error: Access to system directories is not allowed"));
    }

    // Check for symlinks if requested
    if check_symlinks && path.exists() {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| anyhow::anyhow!("Failed to check path metadata"))?;
        if metadata.file_type().is_symlink() {
            return Err(anyhow::anyhow!(
                "Security error: Symlinks are not allowed in local dependency paths"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blacklisted_paths() {
        // System paths should be blacklisted
        assert!(is_path_blacklisted(Path::new("/etc/passwd")));
        assert!(is_path_blacklisted(Path::new("/sys/kernel")));
        assert!(is_path_blacklisted(Path::new("/System/Library/CoreServices")));
        assert!(is_path_blacklisted(Path::new("/private/etc/hosts")));

        #[cfg(windows)]
        {
            assert!(is_path_blacklisted(Path::new("C:\\Windows\\System32")));
            assert!(is_path_blacklisted(Path::new("C:\\Program Files\\App")));
        }

        // Normal development paths should not be blacklisted
        assert!(!is_path_blacklisted(Path::new("/home/user/project")));
        assert!(!is_path_blacklisted(Path::new("/tmp/test")));
        assert!(!is_path_blacklisted(Path::new("/var/folders/temp")));
        assert!(!is_path_blacklisted(Path::new("/Users/developer/work")));
    }

    #[test]
    fn test_validate_path_security() {
        // Safe paths should pass
        assert!(validate_path_security(Path::new("/home/user/project"), false).is_ok());
        assert!(validate_path_security(Path::new("/tmp/test"), false).is_ok());

        // Blacklisted paths should fail
        assert!(validate_path_security(Path::new("/etc/passwd"), false).is_err());
        assert!(validate_path_security(Path::new("/System/Library"), false).is_err());
    }
}
