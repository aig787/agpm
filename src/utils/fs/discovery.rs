//! File discovery and search operations.
//!
//! This module provides utilities for finding files matching patterns
//! in directory trees.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Recursively finds files matching a pattern in a directory tree.
///
/// This function performs a recursive search through the directory tree,
/// matching files whose names contain the specified pattern. The search
/// is case-sensitive and uses simple string matching (not regex).
///
/// # Arguments
///
/// * `dir` - The directory to search in
/// * `pattern` - The pattern to match in filenames (substring match)
///
/// # Returns
///
/// A vector of [`PathBuf`]s for all matching files, or an error if the directory
/// cannot be read.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::find_files;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Find all Rust source files
/// let rust_files = find_files(Path::new("src"), ".rs")?;
///
/// // Find all markdown files
/// let md_files = find_files(Path::new("docs"), ".md")?;
///
/// // Find files with "test" in the name
/// let test_files = find_files(Path::new("."), "test")?;
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Searches recursively through all subdirectories
/// - Only returns regular files (not directories or symlinks)
/// - Uses substring matching (case-sensitive)
/// - Returns empty vector if no matches found
/// - Continues searching even if some subdirectories are inaccessible
///
/// # Performance
///
/// For large directory trees or when searching for many different patterns,
/// consider using external tools like `fd` or implementing caching for repeated searches.
pub fn find_files(dir: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    find_files_recursive(dir, pattern, &mut files)?;
    Ok(files)
}

fn find_files_recursive(dir: &Path, pattern: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            find_files_recursive(&path, pattern, files)?;
        } else if path.is_file()
            && let Some(name) = path.file_name()
            && name.to_string_lossy().contains(pattern)
        {
            files.push(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_find_files() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create test files
        std::fs::write(root.join("test.rs"), "").unwrap();
        std::fs::write(root.join("main.rs"), "").unwrap();
        crate::utils::fs::ensure_dir(&root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        std::fs::write(root.join("src/test.txt"), "").unwrap();

        let files = find_files(root, ".rs").unwrap();
        assert_eq!(files.len(), 3);

        let files = find_files(root, "test").unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_find_files_with_patterns() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create test files
        std::fs::write(root.join("README.md"), "").unwrap();
        std::fs::write(root.join("test.MD"), "").unwrap(); // Different case
        std::fs::write(root.join("file.txt"), "").unwrap();
        crate::utils::fs::ensure_dir(&root.join("hidden")).unwrap();
        std::fs::write(root.join("hidden/.secret.md"), "").unwrap();

        // Pattern matching
        let files = find_files(root, ".md").unwrap();
        assert_eq!(files.len(), 2); // README.md and .secret.md

        let files = find_files(root, ".MD").unwrap();
        assert_eq!(files.len(), 1); // test.MD

        // Substring matching
        let files = find_files(root, "test").unwrap();
        assert_eq!(files.len(), 1);

        let files = find_files(root, "secret").unwrap();
        assert_eq!(files.len(), 1);
    }
}
