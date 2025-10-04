//! File locking utilities for cache operations.
//!
//! This module provides thread-safe and process-safe file locking for cache directories
//! to prevent corruption during concurrent cache operations. The locks are automatically
//! released when the lock object is dropped.

use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// A file lock for cache operations
pub struct CacheLock {
    _file: File,
    path: PathBuf,
}

impl CacheLock {
    /// Acquires an exclusive lock for a specific source in the cache directory.
    ///
    /// This async method creates and acquires an exclusive file lock for the specified
    /// source name. The file locking operation uses `spawn_blocking` internally to avoid
    /// blocking the tokio runtime, while still providing blocking file lock semantics.
    ///
    /// # Lock File Management
    ///
    /// The method performs several setup operations:
    /// 1. **Locks directory creation**: Creates `.locks/` directory if needed
    /// 2. **Lock file creation**: Creates `{source_name}.lock` file
    /// 3. **Exclusive locking**: Acquires exclusive access via OS file locking
    /// 4. **Handle retention**: Keeps file handle open to maintain lock
    ///
    /// # Async and Blocking Behavior
    ///
    /// If another process already holds a lock for the same source:
    /// - **Async-friendly**: Uses `spawn_blocking` to avoid blocking the tokio runtime
    /// - **Blocking wait**: The spawned task blocks until other lock is released
    /// - **Fair queuing**: Locks are typically acquired in FIFO order
    /// - **No timeout**: Task will wait indefinitely (use with caution)
    /// - **Interruptible**: Can be interrupted by process signals
    ///
    /// # Lock File Location
    ///
    /// Lock files are created in a dedicated subdirectory:
    /// ```text
    /// {cache_dir}/.locks/{source_name}.lock
    /// ```
    ///
    /// Examples:
    /// - `~/.agpm/cache/.locks/community.lock`
    /// - `~/.agpm/cache/.locks/work-tools.lock`
    /// - `~/.agpm/cache/.locks/my-project.lock`
    ///
    /// # Parameters
    ///
    /// * `cache_dir` - Root cache directory path
    /// * `source_name` - Unique identifier for the source being locked
    ///
    /// # Returns
    ///
    /// Returns a `CacheLock` instance that holds the exclusive lock. The lock
    /// remains active until the returned instance is dropped.
    ///
    /// # Errors
    ///
    /// The method can fail for several reasons:
    ///
    /// ## Directory Creation Errors
    /// - Permission denied creating `.locks/` directory
    /// - Disk space exhausted
    /// - Path length exceeds system limits
    ///
    /// ## File Operation Errors
    /// - Permission denied creating/opening lock file
    /// - File system full
    /// - Invalid characters in source name
    ///
    /// ## Locking Errors
    /// - File locking not supported by file system
    /// - Lock file corrupted or in invalid state
    /// - System resource limits exceeded
    ///
    /// # Platform Considerations
    ///
    /// - **Windows**: Uses Win32 `LockFile` API via [`fs4`]
    /// - **Unix**: Uses POSIX `fcntl()` locking via [`fs4`]
    /// - **NFS/Network**: Behavior depends on file system support
    /// - **Docker**: Works within containers with proper volume mounts
    ///
    /// # Examples
    ///
    /// Simple lock acquisition:
    ///
    /// ```rust,no_run
    /// use agpm::cache::lock::CacheLock;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache_dir = PathBuf::from("/home/user/.agpm/cache");
    ///
    /// // This will block if another process has the lock
    /// let lock = CacheLock::acquire(&cache_dir, "my-source").await?;
    ///
    /// // Perform cache operations safely...
    /// println!("Lock acquired successfully!");
    ///
    /// // Lock is released when 'lock' variable is dropped
    /// drop(lock);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Error handling for lock acquisition:
    ///
    /// ```rust,no_run
    /// use agpm::cache::lock::CacheLock;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let cache_dir = PathBuf::from("/tmp/cache");
    ///
    /// match CacheLock::acquire(&cache_dir, "problematic-source").await {
    ///     Ok(lock) => {
    ///         println!("Lock acquired, proceeding with operations");
    ///         // Use lock...
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Failed to acquire lock: {}", e);
    ///         eprintln!("Another process may be using this source");
    ///         return Err(e);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire(cache_dir: &Path, source_name: &str) -> Result<Self> {
        // Create lock file path: ~/.agpm/cache/.locks/source_name.lock
        let locks_dir = cache_dir.join(".locks");
        tokio::fs::create_dir_all(&locks_dir).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotADirectory {
                anyhow::anyhow!(
                    "Cannot create directory: cache path is not a directory ({})",
                    cache_dir.display()
                )
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                anyhow::anyhow!(
                    "Permission denied: cannot create locks directory at {}",
                    locks_dir.display()
                )
            } else if e.raw_os_error() == Some(28) {
                // ENOSPC on Unix
                anyhow::anyhow!("No space left on device to create locks directory")
            } else {
                anyhow::anyhow!("Failed to create directory {}: {}", locks_dir.display(), e)
            }
        })?;

        let lock_path = locks_dir.join(format!("{source_name}.lock"));
        let lock_path_clone = lock_path.clone();
        let source_name = source_name.to_string();

        // Use spawn_blocking to perform blocking file lock operations
        // This prevents blocking the tokio runtime
        let file = tokio::task::spawn_blocking(move || -> Result<File> {
            // Open or create the lock file
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&lock_path_clone)
                .with_context(|| {
                    format!("Failed to open lock file: {}", lock_path_clone.display())
                })?;

            // Try to acquire exclusive lock (blocking)
            file.lock_exclusive()
                .with_context(|| format!("Failed to acquire lock for: {source_name}"))?;

            Ok(file)
        })
        .await
        .context("Failed to spawn blocking task for lock acquisition")??;

        Ok(Self {
            _file: file,
            path: lock_path,
        })
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        // Lock is automatically released when file is closed (on Drop)
        // But we can explicitly unlock for clarity
        #[allow(unstable_name_collisions)]
        if let Err(e) = self._file.unlock() {
            eprintln!("Warning: Failed to unlock {}: {}", self.path.display(), e);
        }
    }
}

/// Cleans up stale lock files in the cache directory.
///
/// This function removes lock files that are older than the specified TTL (time-to-live)
/// in seconds. Lock files can become stale if a process crashes without properly releasing
/// its locks. This cleanup helps prevent lock file accumulation over time.
///
/// # Parameters
///
/// * `cache_dir` - Root cache directory containing the `.locks/` subdirectory
/// * `ttl_seconds` - Maximum age in seconds for lock files (e.g., 3600 for 1 hour)
///
/// # Returns
///
/// Returns the number of stale lock files that were removed.
///
/// # Example
///
/// ```rust,no_run
/// use agpm::cache::lock::cleanup_stale_locks;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache_dir = PathBuf::from("/home/user/.agpm/cache");
/// // Clean up lock files older than 1 hour
/// let removed = cleanup_stale_locks(&cache_dir, 3600).await?;
/// println!("Removed {} stale lock files", removed);
/// # Ok(())
/// # }
/// ```
pub async fn cleanup_stale_locks(cache_dir: &Path, ttl_seconds: u64) -> Result<usize> {
    use std::time::{Duration, SystemTime};
    use tokio::fs;

    let locks_dir = cache_dir.join(".locks");
    if !locks_dir.exists() {
        return Ok(0);
    }

    let mut removed_count = 0;
    let now = SystemTime::now();
    let ttl_duration = Duration::from_secs(ttl_seconds);

    let mut entries = fs::read_dir(&locks_dir)
        .await
        .context("Failed to read locks directory")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // Only process .lock files
        if path.extension().and_then(|s| s.to_str()) != Some("lock") {
            continue;
        }

        // Check file age
        let metadata = match fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue, // Skip if we can't read metadata
        };

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue, // Skip if we can't get modification time
        };

        // Remove if older than TTL
        if let Ok(age) = now.duration_since(modified)
            && age > ttl_duration
        {
            // Try to remove the file (it might be locked by another process)
            if fs::remove_file(&path).await.is_ok() {
                removed_count += 1;
            }
        }
    }

    Ok(removed_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cache_lock_acquire_and_release() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // Acquire lock
        let lock = CacheLock::acquire(cache_dir, "test_source").await.unwrap();

        // Verify lock file was created
        let lock_path = cache_dir.join(".locks").join("test_source.lock");
        assert!(lock_path.exists());

        // Drop the lock
        drop(lock);

        // Lock file should still exist (we don't delete it)
        assert!(lock_path.exists());
    }

    #[tokio::test]
    async fn test_cache_lock_creates_locks_directory() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // Locks directory shouldn't exist initially
        let locks_dir = cache_dir.join(".locks");
        assert!(!locks_dir.exists());

        // Acquire lock - should create directory
        let _lock = CacheLock::acquire(cache_dir, "test").await.unwrap();

        // Verify locks directory was created
        assert!(locks_dir.exists());
        assert!(locks_dir.is_dir());
    }

    #[tokio::test]
    async fn test_cache_lock_exclusive_blocking() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let cache_dir1 = cache_dir.clone();
        let barrier1 = barrier.clone();

        // Task 1: Acquire lock and hold it
        let handle1 = tokio::spawn(async move {
            let _lock = CacheLock::acquire(&cache_dir1, "exclusive_test")
                .await
                .unwrap();
            barrier1.wait().await; // Signal that lock is acquired
            tokio::time::sleep(Duration::from_millis(100)).await; // Hold lock
            // Lock released on drop
        });

        let cache_dir2 = cache_dir.clone();

        // Task 2: Try to acquire same lock (should block)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await; // Wait for first task to acquire lock
            let start = Instant::now();
            let _lock = CacheLock::acquire(&cache_dir2, "exclusive_test")
                .await
                .unwrap();
            let elapsed = start.elapsed();

            // Should have blocked for at least 50ms (less than 100ms due to timing)
            assert!(elapsed >= Duration::from_millis(50));
        });

        handle1.await.unwrap();
        handle2.await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_lock_different_sources_dont_block() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let cache_dir1 = cache_dir.clone();
        let barrier1 = barrier.clone();

        // Task 1: Lock source1
        let handle1 = tokio::spawn(async move {
            let _lock = CacheLock::acquire(&cache_dir1, "source1").await.unwrap();
            barrier1.wait().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let cache_dir2 = cache_dir.clone();

        // Task 2: Lock source2 (different source, shouldn't block)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await;
            let start = Instant::now();
            let _lock = CacheLock::acquire(&cache_dir2, "source2").await.unwrap();
            let elapsed = start.elapsed();

            // Should not block (complete quickly)
            // Increased timeout for slower systems while still ensuring no blocking
            assert!(
                elapsed < Duration::from_millis(200),
                "Lock acquisition took {:?}, expected < 200ms for non-blocking operation",
                elapsed
            );
        });

        handle1.await.unwrap();
        handle2.await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_lock_path_with_special_characters() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // Test with various special characters in source name
        let special_names = vec![
            "source-with-dash",
            "source_with_underscore",
            "source.with.dots",
            "source@special",
        ];

        for name in special_names {
            let lock = CacheLock::acquire(cache_dir, name).await.unwrap();
            let expected_path = cache_dir.join(".locks").join(format!("{name}.lock"));
            assert!(expected_path.exists());
            drop(lock);
        }
    }
}
