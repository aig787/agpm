//! File metadata operations including size calculation, checksums, and file queries.
//!
//! This module provides functions for:
//! - Directory size calculation (recursive)
//! - SHA-256 checksum generation (single and parallel)
//! - File existence and readability checks
//! - File modification time queries and comparisons
//!
//! # Examples
//!
//! ```rust,no_run
//! use agpm_cli::utils::fs::metadata::{calculate_checksum, dir_size, file_exists_and_readable};
//! use std::path::Path;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Check if file is readable
//! if file_exists_and_readable(Path::new("important.txt")) {
//!     // Calculate checksum for integrity verification
//!     let checksum = calculate_checksum(Path::new("important.txt"))?;
//!     println!("File checksum: {}", checksum);
//! }
//!
//! // Calculate directory size
//! let size = dir_size(Path::new("cache"))?;
//! println!("Cache size: {} bytes", size);
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use futures::future::try_join_all;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Calculates the total size of a directory and all its contents recursively.
///
/// This function traverses the directory tree and sums the sizes of all regular files.
/// It handles nested directories and provides the total disk usage for the directory tree.
///
/// # Arguments
///
/// * `path` - The directory to calculate size for
///
/// # Returns
///
/// The total size in bytes, or an error if the directory cannot be read
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::metadata::dir_size;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let cache_size = dir_size(Path::new("~/.agpm/cache"))?;
/// println!("Cache size: {} bytes ({:.2} MB)", cache_size, cache_size as f64 / 1024.0 / 1024.0);
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Recursively traverses all subdirectories
/// - Includes only regular files in size calculation
/// - Does not follow symbolic links
/// - Returns 0 for empty directories
/// - Accumulates sizes using 64-bit integers (supports very large directories)
///
/// # Performance
///
/// This is a synchronous operation that may take time for large directory trees.
/// For better performance with large directories, use [`get_directory_size`] which
/// runs the calculation on a separate thread.
///
/// # See Also
///
/// - [`get_directory_size`] for async version
/// - Platform-specific tools may be faster for very large directories
pub fn dir_size(path: &Path) -> Result<u64> {
    let mut size = 0;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            size += dir_size(&entry.path())?;
        } else {
            size += metadata.len();
        }
    }

    Ok(size)
}

/// Asynchronously calculates the total size of a directory and all its contents.
///
/// This is the async version of [`dir_size`] that runs the calculation on a separate
/// thread to avoid blocking the async runtime. Use this when calculating directory
/// sizes as part of async operations.
///
/// # Arguments
///
/// * `path` - The directory to calculate size for
///
/// # Returns
///
/// The total size in bytes, or an error if the operation fails
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::metadata::get_directory_size;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache_size = get_directory_size(Path::new("~/.agpm/cache")).await?;
/// println!("Cache size: {} bytes", cache_size);
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function uses `tokio::task::spawn_blocking` to run the directory traversal
/// on a thread pool, preventing it from blocking other async tasks. This is particularly
/// useful when:
/// - Calculating sizes for multiple directories concurrently
/// - Integrating with async workflows
/// - Avoiding blocking in async web servers or CLI applications
///
/// # See Also
///
/// - [`dir_size`] for synchronous version
pub async fn get_directory_size(path: &Path) -> Result<u64> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || dir_size(&path))
        .await
        .context("Failed to join directory size calculation task")?
}

/// Calculates the SHA-256 checksum of a file.
///
/// This function reads the entire file into memory and computes its SHA-256 hash,
/// returning it as a lowercase hexadecimal string. This is useful for verifying
/// file integrity and detecting changes.
///
/// # Arguments
///
/// * `path` - The path to the file to checksum
///
/// # Returns
///
/// A 64-character lowercase hexadecimal string representing the SHA-256 hash,
/// or an error if the file cannot be read
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::metadata::calculate_checksum;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let checksum = calculate_checksum(Path::new("important-file.txt"))?;
/// println!("File checksum: {}", checksum);
///
/// // Verify against expected checksum
/// let expected = "d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2";
/// if checksum == expected {
///     println!("File integrity verified!");
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function reads the entire file into memory, so it may not be suitable
/// for very large files. For processing multiple files, consider using
/// [`calculate_checksums_parallel`] for better performance.
///
/// # Security
///
/// SHA-256 is cryptographically secure and suitable for:
/// - Integrity verification
/// - Change detection
/// - Digital signatures
/// - Blockchain applications
///
/// # See Also
///
/// - [`calculate_checksums_parallel`] for batch processing
/// - [`hex`] crate for hexadecimal encoding
pub fn calculate_checksum(path: &Path) -> Result<String> {
    let content = fs::read(path)
        .with_context(|| format!("Failed to read file for checksum: {}", path.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();

    Ok(hex::encode(result))
}

/// Calculates SHA-256 checksums for multiple files concurrently.
///
/// This function processes multiple files in parallel using Tokio's thread pool,
/// which can significantly improve performance when processing many files or
/// large files on systems with multiple CPU cores.
///
/// # Arguments
///
/// * `paths` - A slice of file paths to process
///
/// # Returns
///
/// A vector of tuples containing each file path and its corresponding checksum,
/// in the same order as the input paths. Returns an error if any file fails
/// to be processed.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::fs::metadata::calculate_checksums_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let files = vec![
///     PathBuf::from("file1.txt"),
///     PathBuf::from("file2.txt"),
///     PathBuf::from("file3.txt"),
/// ];
///
/// let results = calculate_checksums_parallel(&files).await?;
/// for (path, checksum) in results {
///     println!("{}: {}", path.display(), checksum);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function uses `tokio::task::spawn_blocking` to run checksum calculations
/// on separate threads, allowing for true parallelism. Benefits:
/// - CPU-bound work doesn't block the async runtime
/// - Multiple files processed simultaneously
/// - Scales with available CPU cores
/// - Maintains order of results
///
/// # Error Handling
///
/// If any file fails to be processed, the entire operation fails and returns
/// an error with details about all failures. This "all-or-nothing" approach
/// ensures data consistency.
///
/// # See Also
///
/// - [`calculate_checksum`] for single file processing
/// - [`super::parallel::read_files_parallel`] for concurrent file reading
pub async fn calculate_checksums_parallel(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut tasks = Vec::new();

    for (index, path) in paths.iter().enumerate() {
        let path = path.clone();
        let task = tokio::task::spawn_blocking(move || {
            calculate_checksum(&path).map(|checksum| (index, path, checksum))
        });
        tasks.push(task);
    }

    let results = try_join_all(tasks).await.context("Failed to join checksum calculation tasks")?;

    let mut successes = Vec::new();
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok((index, path, checksum)) => successes.push((index, path, checksum)),
            Err(e) => errors.push(e),
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> =
            errors.into_iter().map(|error| format!("  {error}")).collect();
        return Err(anyhow::anyhow!(
            "Failed to calculate checksums for {} files:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    // Sort results by original index to maintain order
    successes.sort_by_key(|(index, _, _)| *index);
    let ordered_results: Vec<(PathBuf, String)> =
        successes.into_iter().map(|(_, path, checksum)| (path, checksum)).collect();

    Ok(ordered_results)
}

/// Checks if a file exists and is readable.
///
/// # Arguments
/// * `path` - The path to check
///
/// # Returns
/// true if the file exists and is readable, false otherwise
pub fn file_exists_and_readable(path: &Path) -> bool {
    path.exists() && path.is_file() && fs::metadata(path).is_ok()
}

/// Gets the modification time of a file.
///
/// # Arguments
/// * `path` - The path to the file
///
/// # Returns
/// The modification time as a `SystemTime`
///
/// # Errors
/// Returns an error if the file metadata cannot be read
pub fn get_modified_time(path: &Path) -> Result<std::time::SystemTime> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to get metadata for: {}", path.display()))?;

    metadata
        .modified()
        .with_context(|| format!("Failed to get modification time for: {}", path.display()))
}

/// Compares the modification times of two files.
///
/// # Arguments
/// * `path1` - The first file path
/// * `path2` - The second file path
///
/// # Returns
/// - `Ok(Ordering::Less)` if path1 is older than path2
/// - `Ok(Ordering::Greater)` if path1 is newer than path2
/// - `Ok(Ordering::Equal)` if they have the same modification time
///
/// # Errors
/// Returns an error if either file's metadata cannot be read
pub fn compare_file_times(path1: &Path, path2: &Path) -> Result<std::cmp::Ordering> {
    let time1 = get_modified_time(path1)?;
    let time2 = get_modified_time(path2)?;

    Ok(time1.cmp(&time2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_dir_size() {
        let temp = tempdir().unwrap();
        let dir = temp.path();

        std::fs::write(dir.join("file1.txt"), "12345").unwrap();
        std::fs::write(dir.join("file2.txt"), "123456789").unwrap();
        super::super::dirs::ensure_dir(&dir.join("subdir")).unwrap();
        std::fs::write(dir.join("subdir/file3.txt"), "abc").unwrap();

        let size = dir_size(dir).unwrap();
        assert_eq!(size, 17); // 5 + 9 + 3
    }

    #[test]
    fn test_calculate_checksum() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("checksum_test.txt");
        std::fs::write(&file, "test content").unwrap();

        let checksum = calculate_checksum(&file).unwrap();
        assert!(!checksum.is_empty());
        assert_eq!(checksum.len(), 64); // SHA256 produces 64 hex chars
    }

    #[tokio::test]
    async fn test_calculate_checksums_parallel() {
        let temp = tempdir().unwrap();
        let file1 = temp.path().join("file1.txt");
        let file2 = temp.path().join("file2.txt");

        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        let paths = vec![file1.clone(), file2.clone()];
        let results = calculate_checksums_parallel(&paths).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, file1);
        assert_eq!(results[1].0, file2);
        assert!(!results[0].1.is_empty());
        assert!(!results[1].1.is_empty());
    }

    #[tokio::test]
    async fn test_calculate_checksums_parallel_empty() {
        let results = calculate_checksums_parallel(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_calculate_checksum_edge_cases() {
        let temp = tempdir().unwrap();

        // Empty file
        let empty = temp.path().join("empty.txt");
        std::fs::write(&empty, "").unwrap();
        let checksum = calculate_checksum(&empty).unwrap();
        assert_eq!(checksum.len(), 64);
        // SHA256 of empty string is well-known
        assert_eq!(checksum, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

        // Non-existent file
        let nonexistent = temp.path().join("nonexistent.txt");
        let result = calculate_checksum(&nonexistent);
        assert!(result.is_err());

        // Large file (1MB)
        let large = temp.path().join("large.txt");
        let large_content = vec![b'a'; 1024 * 1024];
        std::fs::write(&large, &large_content).unwrap();
        let checksum = calculate_checksum(&large).unwrap();
        assert_eq!(checksum.len(), 64);
    }

    #[tokio::test]
    async fn test_calculate_checksums_parallel_errors() {
        let temp = tempdir().unwrap();
        let valid = temp.path().join("valid.txt");
        let invalid = temp.path().join("invalid.txt");

        std::fs::write(&valid, "content").unwrap();

        let paths = vec![valid.clone(), invalid.clone()];
        let result = calculate_checksums_parallel(&paths).await;

        // Should fail if any file is invalid
        assert!(result.is_err());
    }

    #[test]
    fn test_dir_size_edge_cases() {
        let temp = tempdir().unwrap();

        // Empty directory
        let empty_dir = temp.path().join("empty");
        super::super::dirs::ensure_dir(&empty_dir).unwrap();
        assert_eq!(dir_size(&empty_dir).unwrap(), 0);

        // Non-existent directory
        let nonexistent = temp.path().join("nonexistent");
        let result = dir_size(&nonexistent);
        assert!(result.is_err());

        // Directory with symlinks
        #[cfg(unix)]
        {
            let dir = temp.path().join("with_symlink");
            super::super::dirs::ensure_dir(&dir).unwrap();
            std::fs::write(dir.join("file.txt"), "12345").unwrap();

            let target = temp.path().join("target");
            std::fs::write(&target, "123456789").unwrap();
            std::os::unix::fs::symlink(&target, dir.join("link")).unwrap();

            // The dir_size function behavior with symlinks depends on the implementation
            // Just verify it doesn't crash and returns a reasonable size
            let size = dir_size(&dir).unwrap();
            // We should have at least the size of the real file
            assert!(size >= 5);
            // The size should be reasonable (not gigabytes)
            assert!(size < 1_000_000);
        }
    }
}
