//! Directory operations for creating, copying, and removing directories.
//!
//! This module provides cross-platform directory operations with proper
//! error handling and Windows long path support.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Ensures a directory exists, creating it and all parent directories if necessary.
///
/// This function is cross-platform and handles:
/// - Windows long paths (>260 characters) automatically
/// - Permission errors with helpful error messages
/// - Existing files at the target path (returns error)
///
/// # Arguments
///
/// * `path` - The directory path to create
///
/// # Returns
///
/// - `Ok(())` if the directory exists or was successfully created
/// - `Err` if the path exists but is not a directory, or creation fails
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::ensure_dir;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Create nested directories
/// ensure_dir(Path::new("output/agents/subdir"))?;
/// # Ok(())
/// # }
/// ```
///
/// # Platform Notes
///
/// - **Windows**: Automatically handles long paths and provides specific error guidance
/// - **Unix**: Respects umask for directory permissions
/// - **All platforms**: Creates parent directories recursively
pub fn ensure_dir(path: &Path) -> Result<()> {
    // Handle Windows long paths
    let safe_path = crate::utils::platform::windows_long_path(path);

    if !safe_path.exists() {
        fs::create_dir_all(&safe_path).with_context(|| {
            let platform_help = if crate::utils::platform::is_windows() {
                "On Windows: Check that the path length is < 260 chars or that long path support is enabled"
            } else {
                "Check directory permissions and path validity"
            };

            format!("Failed to create directory: {}\n\n{}", path.display(), platform_help)
        })?;
    } else if !safe_path.is_dir() {
        return Err(anyhow::anyhow!("Path exists but is not a directory: {}", path.display()));
    }
    Ok(())
}

/// Ensures that the parent directory of a file path exists.
///
/// This is a convenience function for creating the directory structure needed
/// for a file before writing to it. It extracts the parent directory from the
/// file path and ensures it exists.
///
/// # Arguments
///
/// * `path` - The file path whose parent directory should exist
///
/// # Returns
///
/// - `Ok(())` if the parent directory exists or was created successfully
/// - `Err` if directory creation fails
/// - `Ok(())` if the path has no parent (e.g., root level files)
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::ensure_parent_dir;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Ensure directory structure exists before writing file
/// ensure_parent_dir(Path::new("output/agents/example.md"))?;
/// std::fs::write("output/agents/example.md", "# Example Agent")?;
/// # Ok(())
/// # }
/// ```
///
/// # Use Cases
///
/// - Preparing directory structure before file operations
/// - Ensuring atomic writes have proper directory structure
/// - Setting up output paths in batch processing
///
/// # See Also
///
/// - [`ensure_dir`] for creating a specific directory
/// - [`crate::utils::fs::atomic_write`] which calls this internally
pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    Ok(())
}

/// Alias for `ensure_dir` for consistency
pub fn ensure_dir_exists(path: &Path) -> Result<()> {
    ensure_dir(path)
}

/// Recursively copies a directory and all its contents to a new location.
///
/// This function performs a deep copy of all files and subdirectories from the source
/// to the destination. It creates the destination directory if it doesn't exist and
/// preserves the directory structure.
///
/// # Arguments
///
/// * `src` - The source directory to copy from
/// * `dst` - The destination directory to copy to
///
/// # Returns
///
/// - `Ok(())` if the directory was copied successfully
/// - `Err` if the copy operation fails for any file or directory
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::copy_dir;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Copy entire agent directory
/// copy_dir(Path::new("cache/agents"), Path::new("output/agents"))?;
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Creates destination directory if it doesn't exist
/// - Recursively copies all subdirectories
/// - Copies only regular files (skips symlinks and special files)
/// - Overwrites existing files in the destination
///
/// # Platform Notes
///
/// - **Windows**: Handles long paths and preserves attributes
/// - **Unix**: Preserves file permissions during copy
/// - **All platforms**: Does not follow symbolic links
///
/// # See Also
///
/// - [`crate::utils::fs::copy_dirs_parallel`] for copying multiple directories concurrently
/// - [`crate::utils::fs::copy_files_parallel`] for batch file copying
pub fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    ensure_dir(dst)?;

    for entry in
        fs::read_dir(src).with_context(|| format!("Failed to read directory: {}", src.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!("Failed to copy file from {} to {}", src_path.display(), dst_path.display())
            })?;
        }
        // Skip symlinks and other file types
    }

    Ok(())
}

/// Copy a directory recursively (alias for consistency)
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    copy_dir(src, dst)
}

/// Recursively removes a directory and all its contents.
///
/// This function safely removes a directory tree, handling the case where the
/// directory doesn't exist (no error). It's designed to be safe for cleanup
/// operations where the directory may or may not exist.
///
/// # Arguments
///
/// * `path` - The directory to remove
///
/// # Returns
///
/// - `Ok(())` if the directory was removed or didn't exist
/// - `Err` if the removal failed due to permissions or other filesystem errors
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::remove_dir_all;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Safe cleanup - won't error if directory doesn't exist
/// remove_dir_all(Path::new("temp/cache"))?;
/// # Ok(())
/// # }
/// ```
///
/// # Safety
///
/// - Does not follow symbolic links outside the directory tree
/// - Handles permission errors with descriptive messages
/// - Safe to call on non-existent directories
///
/// # Platform Notes
///
/// - **Windows**: Handles long paths and readonly files
/// - **Unix**: Respects file permissions
/// - **All platforms**: Atomic operation where supported by filesystem
pub fn remove_dir_all(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory: {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ensure_dir() {
        let temp = tempdir().unwrap();
        let test_dir = temp.path().join("test_dir");

        assert!(!test_dir.exists());
        ensure_dir(&test_dir).unwrap();
        assert!(test_dir.exists());
        assert!(test_dir.is_dir());
    }

    #[test]
    fn test_ensure_dir_on_file() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();

        let result = ensure_dir(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_parent_dir() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("parent").join("child").join("file.txt");

        ensure_parent_dir(&file_path).unwrap();
        assert!(file_path.parent().unwrap().exists());
    }

    #[test]
    fn test_ensure_parent_dir_edge_cases() {
        use std::path::PathBuf;

        let temp = tempdir().unwrap();

        // File at root (no parent)
        let root_file = if cfg!(windows) {
            PathBuf::from("C:\\file.txt")
        } else {
            PathBuf::from("/file.txt")
        };
        ensure_parent_dir(&root_file).unwrap(); // Should not panic

        // Current directory file
        let current_file = PathBuf::from("file.txt");
        ensure_parent_dir(&current_file).unwrap();

        // Already existing parent
        let existing = temp.path().join("file.txt");
        ensure_parent_dir(&existing).unwrap();
        ensure_parent_dir(&existing).unwrap(); // Second call should be ok
    }

    #[test]
    fn test_ensure_dir_exists() {
        let temp = tempdir().unwrap();
        let test_dir = temp.path().join("test_dir_alias");

        assert!(!test_dir.exists());
        ensure_dir_exists(&test_dir).unwrap();
        assert!(test_dir.exists());
    }

    #[test]
    fn test_copy_dir() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        // Create source structure
        ensure_dir(&src).unwrap();
        ensure_dir(&src.join("subdir")).unwrap();
        std::fs::write(src.join("file1.txt"), "content1").unwrap();
        std::fs::write(src.join("subdir/file2.txt"), "content2").unwrap();

        // Copy directory
        copy_dir(&src, &dst).unwrap();

        // Verify copy
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir/file2.txt").exists());

        let content1 = std::fs::read_to_string(dst.join("file1.txt")).unwrap();
        assert_eq!(content1, "content1");

        let content2 = std::fs::read_to_string(dst.join("subdir/file2.txt")).unwrap();
        assert_eq!(content2, "content2");
    }

    #[test]
    fn test_copy_dir_all() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src_alias");
        let dst = temp.path().join("dst_alias");

        ensure_dir(&src).unwrap();
        std::fs::write(src.join("file.txt"), "content").unwrap();

        copy_dir_all(&src, &dst).unwrap();
        assert!(dst.join("file.txt").exists());
    }

    #[test]
    fn test_copy_dir_with_permissions() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        ensure_dir(&src).unwrap();
        std::fs::write(src.join("file.txt"), "content").unwrap();

        // Set specific permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(src.join("file.txt")).unwrap().permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(src.join("file.txt"), perms).unwrap();
        }

        copy_dir(&src, &dst).unwrap();

        assert!(dst.join("file.txt").exists());

        // Verify permissions were preserved on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(dst.join("file.txt")).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o644);
        }
    }

    #[test]
    fn test_remove_dir_all() {
        let temp = tempdir().unwrap();
        let dir = temp.path().join("to_remove");

        ensure_dir(&dir).unwrap();
        std::fs::write(dir.join("file.txt"), "content").unwrap();

        assert!(dir.exists());
        remove_dir_all(&dir).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn test_remove_dir_all_nonexistent() {
        let temp = tempdir().unwrap();
        let dir = temp.path().join("nonexistent");

        // Should not error on non-existent directory
        remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_remove_dir_all_symlink() {
        // Test that remove_dir_all doesn't follow symlinks
        let temp = tempdir().unwrap();
        let target = temp.path().join("target");
        let link = temp.path().join("link");

        ensure_dir(&target).unwrap();
        std::fs::write(target.join("important.txt"), "data").unwrap();

        std::os::unix::fs::symlink(&target, &link).unwrap();
        remove_dir_all(&link).unwrap();

        // Target should still exist
        assert!(target.exists());
        assert!(target.join("important.txt").exists());
    }
}
