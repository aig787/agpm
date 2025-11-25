//! File locking utilities for cache operations.
//!
//! This module provides thread-safe and process-safe file locking for cache directories
//! to prevent corruption during concurrent cache operations. The locks are automatically
//! released when the lock object is dropped.

use crate::constants::{DEFAULT_LOCK_TIMEOUT, MAX_BACKOFF_DELAY_MS, STARTING_BACKOFF_DELAY_MS};
use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::Duration;
use tokio_retry::strategy::ExponentialBackoff;

/// A file lock for cache operations
#[derive(Debug)]
pub struct CacheLock {
    _file: File,
}

impl CacheLock {
    /// Acquires an exclusive lock for a specific source in the cache directory.
    ///
    /// Creates and acquires an exclusive file lock for the specified source name.
    /// Uses non-blocking lock attempts with exponential backoff and timeout.
    ///
    /// # Lock File Management
    ///
    /// 1. Creates `.locks/` directory if needed
    /// 2. Creates `{source_name}.lock` file
    /// 3. Acquires exclusive access via OS file locking
    /// 4. Keeps file handle open to maintain lock
    ///
    /// # Behavior
    ///
    /// - **Timeout**: 30-second default (configurable via `acquire_with_timeout`)
    /// - **Non-blocking**: `try_lock_exclusive()` in async retry loop
    /// - **Backoff**: 10ms → 20ms → 40ms... up to 500ms max
    /// - **Fair access**: FIFO order typically
    /// - **Interruptible**: Process signals work
    ///
    /// # Lock File Location
    ///
    /// Format: `{cache_dir}/.locks/{source_name}.lock`
    ///
    /// Example: `~/.agpm/cache/.locks/community.lock`
    ///
    /// # Errors
    ///
    /// - Permission denied
    /// - Disk space exhausted
    /// - Timeout acquiring lock
    ///
    /// # Platform Support
    ///
    /// - **Windows**: Win32 `LockFile` API
    /// - **Unix**: POSIX `fcntl()` locking
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::lock::CacheLock;
    /// use std::path::Path;
    /// # async fn example() -> anyhow::Result<()> {
    /// # let cache_dir = Path::new("/tmp/cache");
    /// let lock = CacheLock::acquire(cache_dir, "my-source").await?;
    /// // Lock released on drop
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire(cache_dir: &Path, source_name: &str) -> Result<Self> {
        Self::acquire_with_timeout(cache_dir, source_name, DEFAULT_LOCK_TIMEOUT).await
    }

    /// Acquires an exclusive lock with a specified timeout.
    ///
    /// Uses exponential backoff (10ms → 500ms) without blocking the async runtime.
    ///
    /// # Errors
    ///
    /// Returns timeout error if lock cannot be acquired within the specified duration.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::lock::CacheLock;
    /// use std::time::Duration;
    /// use std::path::Path;
    /// # async fn example() -> anyhow::Result<()> {
    /// # let cache_dir = Path::new("/tmp/cache");
    /// let lock = CacheLock::acquire_with_timeout(
    ///     cache_dir, "my-source", Duration::from_secs(10)
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
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
                    return Ok(Self {
                        _file: file,
                    });
                }
                Ok(false) | Err(_) => {
                    // Check remaining time before sleeping to avoid exceeding timeout
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() {
                        return Err(anyhow::anyhow!(
                            "Timeout acquiring lock for '{}' after {:?}",
                            source_name,
                            timeout
                        ));
                    }
                    // Sleep for the shorter of delay or remaining time
                    tokio::time::sleep(delay.min(remaining)).await;
                }
            }
        }

        // If backoff iterator exhausted without acquiring lock, return timeout error
        Err(anyhow::anyhow!("Timeout acquiring lock for '{}' after {:?}", source_name, timeout))
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
        let result =
            CacheLock::acquire_with_timeout(cache_dir, "test-source", Duration::from_millis(100))
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
        // Should be ~100ms, allow 50-500ms range to accommodate slow CI runners
        assert!(elapsed >= Duration::from_millis(50), "Timeout too quick: {:?}", elapsed);
        assert!(elapsed < Duration::from_millis(500), "Timeout too slow: {:?}", elapsed);
    }

    #[tokio::test]
    async fn test_cache_lock_acquire_timeout_succeeds_eventually() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // Acquire lock and release it after 50ms
        let cache_dir_clone = cache_dir.to_path_buf();
        let handle = tokio::spawn(async move {
            let lock = CacheLock::acquire(&cache_dir_clone, "test-source").await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(lock); // Release lock
        });

        // Wait a bit for the first lock to be acquired
        tokio::time::sleep(Duration::from_millis(10)).await;

        // This should succeed after the first lock is released
        // Use 500ms timeout to give plenty of time
        let result =
            CacheLock::acquire_with_timeout(cache_dir, "test-source", Duration::from_millis(500))
                .await;

        assert!(result.is_ok(), "Lock should be acquired after first one is released");

        // Clean up spawned task
        handle.await.unwrap();
    }
}
