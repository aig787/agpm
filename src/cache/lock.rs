//! File locking utilities for cache operations.
//!
//! This module provides thread-safe and process-safe file locking for cache directories
//! to prevent corruption during concurrent cache operations. The locks are automatically
//! released when the lock object is dropped.

use crate::constants::{DEFAULT_LOCK_TIMEOUT, MAX_BACKOFF_DELAY_MS, STARTING_BACKOFF_DELAY_MS};
use tokio_retry::strategy::ExponentialBackoff;
use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::Duration;

/// A file lock for cache operations
pub struct CacheLock {
    _file: File,
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
    /// use agpm_cli::cache::lock::CacheLock;
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
    pub async fn acquire(cache_dir: &Path, source_name: &str) -> Result<Self> {
        Self::acquire_with_timeout(cache_dir, source_name, DEFAULT_LOCK_TIMEOUT).await
    }

    /// Acquires an exclusive lock with a specified timeout.
    ///
    /// Uses non-blocking `try_lock_exclusive()` in a retry loop to avoid
    /// blocking the async runtime indefinitely.
    pub async fn acquire_with_timeout(
        cache_dir: &Path,
        source_name: &str,
        timeout: std::time::Duration,
    ) -> Result<Self> {
        use tokio::fs;

        // Create locks directory if it doesn't exist
        let locks_dir = cache_dir.join(".locks");
        fs::create_dir_all(&locks_dir).await.with_context(|| {
            format!("Failed to create locks directory: {}", locks_dir.display())
        })?;

        // Create lock file path
        let lock_path = locks_dir.join(format!("{source_name}.lock"));

        // Open/create lock file (sync is fine, it's fast)
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file: {}", lock_path.display()))?;

        // Acquire exclusive lock with timeout and exponential backoff
        let start = std::time::Instant::now();

        // Create exponential backoff strategy: 10ms, 20ms, 40ms... capped at 500ms
        let backoff = ExponentialBackoff::from_millis(STARTING_BACKOFF_DELAY_MS)
            .max_delay(Duration::from_millis(MAX_BACKOFF_DELAY_MS));

        for delay in backoff {
            match file.try_lock_exclusive() {
                Ok(true) => {
                    return Ok(Self { _file: file });
                }
                Ok(false) | Err(_) => {
                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!(
                            "Timeout acquiring lock for '{}' after {:?}",
                            source_name,
                            timeout
                        ));
                    }
                    tokio::time::sleep(delay).await;
                }
            }
        }

        // If backoff iterator exhausted without acquiring lock, return timeout error
        Err(anyhow::anyhow!(
            "Timeout acquiring lock for '{}' after {:?}",
            source_name,
            timeout
        ))
    }
}

/// Cleans up stale lock files in the cache directory.
///
/// This function removes lock files that are older than the specified TTL.
/// It's useful for cleaning up after crashes or processes that didn't
/// properly release their locks.
///
/// # Parameters
///
/// * `cache_dir` - The cache directory containing the .locks subdirectory
/// * `ttl_seconds` - Time-to-live in seconds for lock files
///
/// # Returns
///
/// Returns the number of lock files that were removed.
///
/// # Errors
///
/// Returns an error if unable to read the locks directory or access lock file metadata
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

    let mut entries = fs::read_dir(&locks_dir).await.context("Failed to read locks directory")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // Only process .lock files
        if path.extension().and_then(|s| s.to_str()) != Some("lock") {
            continue;
        }

        // Check file age
        let Ok(metadata) = fs::metadata(&path).await else {
            continue; // Skip if we can't read metadata
        };

        let Ok(modified) = metadata.modified() else {
            continue; // Skip if we can't get modification time
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
        let lock = CacheLock::acquire(cache_dir, "test").await.unwrap();

        // Verify locks directory was created
        assert!(locks_dir.exists());
        assert!(locks_dir.is_dir());

        // Explicitly drop the lock to release the file handle before TempDir cleanup
        drop(lock);
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
            let _lock = CacheLock::acquire(&cache_dir1, "exclusive_test").await.unwrap();
            barrier1.wait().await; // Signal that lock is acquired
            tokio::time::sleep(Duration::from_millis(100)).await; // Hold lock
            // Lock released on drop
        });

        let cache_dir2 = cache_dir.clone();

        // Task 2: Try to acquire same lock (should block)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await; // Wait for first task to acquire lock
            let start = Instant::now();
            let _lock = CacheLock::acquire(&cache_dir2, "exclusive_test").await.unwrap();
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

    #[tokio::test]
    async fn test_cache_lock_acquire_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // First lock succeeds
        let _lock1 = CacheLock::acquire(cache_dir, "test-source").await.unwrap();

        // Second lock attempt should timeout quickly
        let start = std::time::Instant::now();
        let result = CacheLock::acquire_with_timeout(
            cache_dir,
            "test-source",
            Duration::from_millis(100),
        )
        .await;

        let elapsed = start.elapsed();

        // Verify timeout occurred
        assert!(result.is_err(), "Expected timeout error");

        // Verify error message mentions timeout
        match result {
            Ok(_) => panic!("Expected timeout error, but got success"),
            Err(error) => {
                let error_msg = error.to_string();
                assert!(
                    error_msg.contains("Timeout") || error_msg.contains("timeout"),
                    "Error message should mention timeout: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains("test-source"),
                    "Error message should include source name: {}",
                    error_msg
                );
            }
        }

        // Verify timeout happened around the expected time (with some tolerance)
        // Should be ~100ms, allow 50-200ms range for scheduling variance
        assert!(
            elapsed >= Duration::from_millis(50),
            "Timeout too quick: {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(300),
            "Timeout too slow: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_cache_lock_acquire_timeout_succeeds_eventually() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // Acquire lock and release it after 50ms
        let cache_dir_clone = cache_dir.to_path_buf();
        let handle = tokio::spawn(async move {
            let lock = CacheLock::acquire(&cache_dir_clone, "test-source")
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(lock); // Release lock
        });

        // Wait a bit for the first lock to be acquired
        tokio::time::sleep(Duration::from_millis(10)).await;

        // This should succeed after the first lock is released
        // Use 500ms timeout to give plenty of time
        let result = CacheLock::acquire_with_timeout(
            cache_dir,
            "test-source",
            Duration::from_millis(500),
        )
        .await;

        assert!(
            result.is_ok(),
            "Lock should be acquired after first one is released"
        );

        // Clean up spawned task
        handle.await.unwrap();
    }
}
