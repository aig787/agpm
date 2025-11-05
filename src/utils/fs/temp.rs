//! Temporary directory management with RAII cleanup.
//!
//! This module provides a `TempDir` struct that automatically cleans up
//! temporary directories when dropped.

use crate::utils::fs::dirs::{ensure_dir, remove_dir_all};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// A temporary directory that automatically cleans up when dropped.
///
/// This struct provides RAII (Resource Acquisition Is Initialization) semantics
/// for temporary directories. The directory is created when the struct is created
/// and automatically removed when the struct is dropped, even if the program panics.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::TempDir;
///
/// # fn example() -> anyhow::Result<()> {
/// {
///     let temp = TempDir::new("test")?;
///     let temp_path = temp.path();
///
///     // Use the temporary directory
///     std::fs::write(temp_path.join("file.txt"), "temporary data")?;
///
///     // Directory exists here
///     assert!(temp_path.exists());
/// } // TempDir is dropped here, directory is automatically cleaned up
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// Each `TempDir` instance creates a unique directory using UUID generation,
/// making it safe to use across multiple threads without naming conflicts.
///
/// # Cleanup Behavior
///
/// - Directory is removed recursively when dropped
/// - Cleanup happens even if the program panics
/// - If cleanup fails (rare), the error is silently ignored
/// - Uses the system temporary directory as the parent
///
/// # Use Cases
///
/// - Unit testing with temporary files
/// - Staging areas for atomic operations
/// - Scratch space for temporary processing
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a new temporary directory with the given prefix.
    ///
    /// The directory is created immediately and will have a name like
    /// `agpm_{prefix}_{uuid}` in the system temporary directory.
    ///
    /// # Arguments
    ///
    /// * `prefix` - A prefix for the directory name (for identification)
    ///
    /// # Returns
    ///
    /// A new `TempDir` instance, or an error if directory creation fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::utils::fs::TempDir;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let temp = TempDir::new("cache")?;
    /// println!("Temporary directory: {}", temp.path().display());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(prefix: &str) -> Result<Self> {
        let temp_dir = std::env::temp_dir();
        let unique_name = format!("agpm_{}_{}", prefix, uuid::Uuid::new_v4());
        let path = temp_dir.join(unique_name);

        ensure_dir(&path)?;

        Ok(Self {
            path,
        })
    }

    /// Returns the path to the temporary directory.
    ///
    /// The directory is guaranteed to exist as long as this `TempDir` instance exists.
    ///
    /// # Returns
    ///
    /// A reference to the temporary directory path
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir() {
        let temp_dir = TempDir::new("test").unwrap();
        let path = temp_dir.path().to_path_buf();

        assert!(path.exists());
        assert!(path.is_dir());

        // Write a file to verify it's a real directory
        std::fs::write(path.join("test.txt"), "test").unwrap();
        assert!(path.join("test.txt").exists());

        drop(temp_dir);
        // Directory should be cleaned up
        assert!(!path.exists());
    }

    #[test]
    fn test_temp_dir_custom_prefix() {
        let temp1 = TempDir::new("prefix1").unwrap();
        let temp2 = TempDir::new("prefix2").unwrap();

        assert!(temp1.path().to_string_lossy().contains("prefix1"));
        assert!(temp2.path().to_string_lossy().contains("prefix2"));

        let path1 = temp1.path().to_path_buf();
        let path2 = temp2.path().to_path_buf();

        assert_ne!(path1, path2);
        assert!(path1.exists());
        assert!(path2.exists());
    }
}
