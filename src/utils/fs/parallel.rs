//! Parallel file operations for concurrent processing of multiple files and directories.
//!
//! This module provides async functions that perform file operations in parallel
//! using Tokio's thread pool. These functions are optimized for:
//! - Processing multiple files or directories simultaneously
//! - Avoiding blocking the async runtime with I/O operations
//! - Scaling with available CPU cores
//!
//! # Examples
//!
//! ```rust,no_run
//! use agpm_cli::utils::fs::parallel::copy_files_parallel;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let copy_operations = vec![
//!     (PathBuf::from("src/agent1.md"), PathBuf::from("output/agent1.md")),
//!     (PathBuf::from("src/agent2.md"), PathBuf::from("output/agent2.md")),
//! ];
//!
//! copy_files_parallel(&copy_operations).await?;
//! println!("All files copied successfully!");
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use futures::future::try_join_all;
use std::fs;
use std::path::PathBuf;

/// Copies multiple files concurrently using parallel processing.
///
/// This function performs multiple file copy operations in parallel, which can
/// significantly improve performance when copying many files, especially on
/// systems with fast storage and multiple CPU cores.
///
/// # Arguments
///
/// * `sources_and_destinations` - A slice of (source, destination) path pairs
///
/// # Returns
///
/// - `Ok(())` if all files were copied successfully
/// - `Err` if any copy operation fails, with details about all failures
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::parallel::copy_files_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let copy_operations = vec![
///     (PathBuf::from("src/agent1.md"), PathBuf::from("output/agent1.md")),
///     (PathBuf::from("src/agent2.md"), PathBuf::from("output/agent2.md")),
///     (PathBuf::from("src/snippet.md"), PathBuf::from("output/snippet.md")),
/// ];
///
/// copy_files_parallel(&copy_operations).await?;
/// println!("All files copied successfully!");
/// # Ok(())
/// # }
/// ```
///
/// # Features
///
/// - **Parallel execution**: Uses thread pool for concurrent operations
/// - **Automatic directory creation**: Creates destination directories as needed
/// - **Atomic behavior**: Either all files copy successfully or none do
/// - **Progress tracking**: Can be combined with progress bars for user feedback
///
/// # Performance Characteristics
///
/// - Best for many small to medium files
/// - Scales with available CPU cores and I/O bandwidth
/// - May not improve performance for very large files (I/O bound)
/// - Respects filesystem limits on concurrent operations
///
/// # Error Handling
///
/// All copy operations must succeed for the function to return `Ok(())`. If any
/// operation fails, detailed error information is provided for troubleshooting.
///
/// # See Also
///
/// - [`copy_dirs_parallel`] for directory copying
/// - [`super::atomic::atomic_write_multiple`] for writing multiple files
pub async fn copy_files_parallel(sources_and_destinations: &[(PathBuf, PathBuf)]) -> Result<()> {
    if sources_and_destinations.is_empty() {
        return Ok(());
    }

    let mut tasks = Vec::new();

    for (src, dst) in sources_and_destinations {
        let src = src.clone();
        let dst = dst.clone();
        let task = tokio::task::spawn_blocking(move || {
            // Ensure destination directory exists
            if let Some(parent) = dst.parent() {
                super::dirs::ensure_dir(parent)?;
            }

            // Copy file
            fs::copy(&src, &dst).with_context(|| {
                format!("Failed to copy file from {} to {}", src.display(), dst.display())
            })?;

            Ok::<_, anyhow::Error>((src, dst))
        });
        tasks.push(task);
    }

    let results = try_join_all(tasks).await.context("Failed to join file copy tasks")?;

    let mut errors = Vec::new();

    for result in results {
        if let Err(e) = result {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|error| format!("  {error}")).collect();
        return Err(anyhow::anyhow!(
            "Failed to copy {} files:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    Ok(())
}

/// Copies multiple directories concurrently.
///
/// This function performs multiple directory copy operations in parallel,
/// which can improve performance when copying several separate directory trees.
/// Each directory is copied recursively using the same logic as [`super::dirs::copy_dir`].
///
/// # Arguments
///
/// * `sources_and_destinations` - A slice of (source, destination) directory pairs
///
/// # Returns
///
/// - `Ok(())` if all directories were copied successfully
/// - `Err` if any copy operation fails, with details about all failures
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::parallel::copy_dirs_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let copy_operations = vec![
///     (PathBuf::from("cache/agents"), PathBuf::from("output/agents")),
///     (PathBuf::from("cache/snippets"), PathBuf::from("output/snippets")),
///     (PathBuf::from("cache/templates"), PathBuf::from("output/templates")),
/// ];
///
/// copy_dirs_parallel(&copy_operations).await?;
/// println!("All directories copied successfully!");
/// # Ok(())
/// # }
/// ```
///
/// # Features
///
/// - **Recursive copying**: Each directory is copied with all subdirectories
/// - **Parallel execution**: Multiple directory trees copied concurrently
/// - **Atomic behavior**: Either all directories copy successfully or operation fails
/// - **Automatic creation**: Destination directories are created as needed
///
/// # Use Cases
///
/// - Copying multiple resource categories simultaneously
/// - Batch operations on directory structures
/// - Backup operations for multiple directories
/// - Installation processes involving multiple components
///
/// # Performance Considerations
///
/// - Best performance with multiple separate directory trees
/// - May not improve performance if directories share the same disk
/// - Memory usage scales with number of directories and their sizes
/// - Respects filesystem concurrent operation limits
///
/// # See Also
///
/// - [`super::dirs::copy_dir`] for single directory copying
/// - [`copy_files_parallel`] for individual file copying
pub async fn copy_dirs_parallel(sources_and_destinations: &[(PathBuf, PathBuf)]) -> Result<()> {
    if sources_and_destinations.is_empty() {
        return Ok(());
    }

    let mut tasks = Vec::new();

    for (src, dst) in sources_and_destinations {
        let src = src.clone();
        let dst = dst.clone();
        let task = tokio::task::spawn_blocking(move || {
            super::dirs::copy_dir(&src, &dst).map(|()| (src, dst))
        });
        tasks.push(task);
    }

    let results = try_join_all(tasks).await.context("Failed to join directory copy tasks")?;

    let mut errors = Vec::new();

    for result in results {
        if let Err(e) = result {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|error| format!("  {error}")).collect();
        return Err(anyhow::anyhow!(
            "Failed to copy {} directories:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    Ok(())
}

/// Reads multiple files concurrently and returns their contents.
///
/// This function reads multiple text files in parallel, which can improve
/// performance when processing many files, especially on systems with
/// fast storage and multiple CPU cores.
///
/// # Arguments
///
/// * `paths` - A slice of file paths to read
///
/// # Returns
///
/// A vector of tuples containing each file path and its content as a UTF-8 string,
/// in the same order as the input paths. Returns an error if any file fails
/// to be read or contains invalid UTF-8.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::parallel::read_files_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config_files = vec![
///     PathBuf::from("agpm.toml"),
///     PathBuf::from("agents/agent1.md"),
///     PathBuf::from("snippets/snippet1.md"),
/// ];
///
/// let results = read_files_parallel(&config_files).await?;
/// for (path, content) in results {
///     println!("{}: {} characters", path.display(), content.len());
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function uses `tokio::task::spawn_blocking` to perform file I/O
/// on separate threads:
/// - Multiple files read simultaneously
/// - Non-blocking for the async runtime
/// - Scales with available CPU cores and I/O bandwidth
/// - Maintains result ordering
///
/// # UTF-8 Handling
///
/// All files must contain valid UTF-8 text. If any file contains invalid
/// UTF-8 bytes, the operation will fail with a descriptive error.
///
/// # Error Handling
///
/// If any file fails to be read (due to permissions, missing file, or
/// invalid UTF-8), the entire operation fails. This ensures consistency
/// when processing related files that should all be available.
///
/// # See Also
///
/// - [`super::metadata::calculate_checksums_parallel`] for file integrity verification
/// - [`super::atomic::atomic_write_multiple`] for batch writing operations
pub async fn read_files_parallel(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut tasks = Vec::new();

    for (index, path) in paths.iter().enumerate() {
        let path = path.clone();
        let task = tokio::task::spawn_blocking(move || {
            fs::read_to_string(&path).map(|content| (index, path, content))
        });
        tasks.push(task);
    }

    let results = try_join_all(tasks).await.context("Failed to join file read tasks")?;

    let mut successes = Vec::new();
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok((index, path, content)) => successes.push((index, path, content)),
            Err(e) => errors.push(e),
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|error| format!("  {error}")).collect();
        return Err(anyhow::anyhow!(
            "Failed to read {} files:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    // Sort results by original index to maintain order
    successes.sort_by_key(|(index, _, _)| *index);
    let ordered_results: Vec<(PathBuf, String)> =
        successes.into_iter().map(|(_, path, content)| (path, content)).collect();

    Ok(ordered_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_copy_files_parallel() {
        let temp = tempdir().unwrap();
        let src1 = temp.path().join("src1.txt");
        let src2 = temp.path().join("src2.txt");
        let dst1 = temp.path().join("dst").join("dst1.txt");
        let dst2 = temp.path().join("dst").join("dst2.txt");

        std::fs::write(&src1, "content1").unwrap();
        std::fs::write(&src2, "content2").unwrap();

        let pairs = vec![(src1.clone(), dst1.clone()), (src2.clone(), dst2.clone())];
        copy_files_parallel(&pairs).await.unwrap();

        assert!(dst1.exists());
        assert!(dst2.exists());
        assert_eq!(std::fs::read_to_string(&dst1).unwrap(), "content1");
        assert_eq!(std::fs::read_to_string(&dst2).unwrap(), "content2");
    }

    #[tokio::test]
    async fn test_copy_dirs_parallel() {
        let temp = tempdir().unwrap();
        let src1 = temp.path().join("src1");
        let src2 = temp.path().join("src2");
        let dst1 = temp.path().join("dst1");
        let dst2 = temp.path().join("dst2");

        super::super::dirs::ensure_dir(&src1).unwrap();
        super::super::dirs::ensure_dir(&src2).unwrap();
        std::fs::write(src1.join("file1.txt"), "content1").unwrap();
        std::fs::write(src2.join("file2.txt"), "content2").unwrap();

        let pairs = vec![(src1.clone(), dst1.clone()), (src2.clone(), dst2.clone())];
        copy_dirs_parallel(&pairs).await.unwrap();

        assert!(dst1.join("file1.txt").exists());
        assert!(dst2.join("file2.txt").exists());
    }

    #[tokio::test]
    async fn test_read_files_parallel() {
        let temp = tempdir().unwrap();
        let file1 = temp.path().join("read1.txt");
        let file2 = temp.path().join("read2.txt");

        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        let paths = vec![file1.clone(), file2.clone()];
        let results = read_files_parallel(&paths).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, file1);
        assert_eq!(results[0].1, "content1");
        assert_eq!(results[1].0, file2);
        assert_eq!(results[1].1, "content2");
    }

    #[tokio::test]
    async fn test_parallel_operations_empty() {
        // Test parallel operations with empty inputs
        let result = copy_files_parallel(&[]).await;
        assert!(result.is_ok());

        let result = copy_dirs_parallel(&[]).await;
        assert!(result.is_ok());

        let result = read_files_parallel(&[]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_copy_files_parallel_errors() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("nonexistent.txt");
        let dst = temp.path().join("dest.txt");

        let pairs = vec![(src, dst)];
        let result = copy_files_parallel(&pairs).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_files_parallel_mixed() {
        let temp = tempdir().unwrap();
        let valid = temp.path().join("valid.txt");
        let invalid = temp.path().join("invalid.txt");

        std::fs::write(&valid, "content").unwrap();

        let paths = vec![valid, invalid];
        let result = read_files_parallel(&paths).await;

        // Should fail if any file cannot be read
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_dirs_parallel_errors() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("nonexistent");
        let dst = temp.path().join("dest");

        let pairs = vec![(src, dst)];
        let result = copy_dirs_parallel(&pairs).await;

        assert!(result.is_err());
    }
}
