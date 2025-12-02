//! File locking utilities for cache operations.
//!
//! This module provides thread-safe and process-safe file locking for cache directories
//! to prevent corruption during concurrent cache operations. The locks are automatically
//! released when the lock object is dropped.
//!
//! # Async Safety
//!
//! All file operations are wrapped in `spawn_blocking` to avoid blocking the tokio
//! runtime. This is critical for preventing worker thread starvation under high
//! parallelism with slow I/O (e.g., network-attached storage).

use crate::constants::{MAX_BACKOFF_DELAY_MS, STARTING_BACKOFF_DELAY_MS, default_lock_timeout};
use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio_retry::strategy::ExponentialBackoff;
use tracing::debug;

/// A file lock for cache operations.
///
/// The lock is held for the lifetime of this struct and automatically
/// released when dropped. Lock acquisition and release are tracked
/// for deadlock detection.
#[derive(Debug)]
pub struct CacheLock {
    /// The file handle - lock is released when this is dropped
    _file: Arc<File>,
    /// Name of the lock for tracing
    lock_name: String,
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        debug!(lock_name = %self.lock_name, "File lock released");
    }
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
        Self::acquire_with_timeout(cache_dir, source_name, default_lock_timeout()).await
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

        let lock_name = format!("file:{}", source_name);
        debug!(lock_name = %lock_name, "Waiting for file lock");

        // Create locks directory if it doesn't exist
        let locks_dir = cache_dir.join(".locks");
        fs::create_dir_all(&locks_dir).await.with_context(|| {
            format!("Failed to create locks directory: {}", locks_dir.display())
        })?;

        // Create lock file path
        let lock_path = locks_dir.join(format!("{source_name}.lock"));

        // CRITICAL: Use spawn_blocking for file open to avoid blocking tokio runtime
        // This is essential for preventing worker thread starvation under slow I/O
        let lock_path_clone = lock_path.clone();
        let file = tokio::task::spawn_blocking(move || {
            OpenOptions::new().create(true).write(true).truncate(false).open(&lock_path_clone)
        })
        .await
        .with_context(|| "spawn_blocking panicked")?
        .with_context(|| format!("Failed to open lock file: {}", lock_path.display()))?;

        // Wrap file in Arc for sharing with spawn_blocking
        let file = Arc::new(file);

        // Acquire exclusive lock with timeout and exponential backoff
        let start = std::time::Instant::now();

        // Create exponential backoff strategy with platform-specific tuning:
        // - Windows: 25ms, 50ms, 100ms, 200ms (faster retries for AV delays)
        // - Unix: 10ms, 20ms, 40ms, ... 500ms (standard backoff)
        let backoff = ExponentialBackoff::from_millis(STARTING_BACKOFF_DELAY_MS)
            .max_delay(Duration::from_millis(MAX_BACKOFF_DELAY_MS));

        // Add jitter to prevent thundering herd when multiple processes retry simultaneously
        let mut rng_state: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345);

        for delay in backoff {
            // Simple xorshift for jitter: adds 0-25% random variation
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            let jitter_factor = 1.0 + (rng_state % 25) as f64 / 100.0;
            let jittered_delay =
                Duration::from_millis((delay.as_millis() as f64 * jitter_factor) as u64);
            // CRITICAL: Use spawn_blocking for try_lock_exclusive to avoid blocking tokio runtime
            let file_clone = Arc::clone(&file);
            let lock_result = tokio::task::spawn_blocking(move || file_clone.try_lock_exclusive())
                .await
                .with_context(|| "spawn_blocking panicked")?;

            match lock_result {
                Ok(true) => {
                    debug!(
                        lock_name = %lock_name,
                        wait_ms = start.elapsed().as_millis(),
                        "File lock acquired"
                    );
                    return Ok(Self {
                        _file: file,
                        lock_name,
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
                    // Sleep for the shorter of jittered delay or remaining time
                    tokio::time::sleep(jittered_delay.min(remaining)).await;
                }
            }
        }

        // If backoff iterator exhausted without acquiring lock, return timeout error
        Err(anyhow::anyhow!("Timeout acquiring lock for '{}' after {:?}", source_name, timeout))
    }

    /// Acquires a shared (read) lock for a specific source in the cache directory.
    ///
    /// Multiple processes can hold shared locks simultaneously, but a shared lock
    /// blocks exclusive lock acquisition. Use this for operations that can safely
    /// run in parallel, like worktree creation (each SHA writes to a different subdir).
    ///
    /// # Lock Semantics
    ///
    /// - **Shared locks**: Multiple holders allowed simultaneously
    /// - **Exclusive locks**: Blocked while any shared lock is held
    /// - **Shared + Exclusive**: Shared lock blocks until exclusive is released
    ///
    /// # Use Cases
    ///
    /// - Worktree creation: Multiple SHAs can create worktrees in parallel
    /// - Read-only operations on shared state
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::cache::lock::CacheLock;
    /// use std::path::Path;
    /// # async fn example() -> anyhow::Result<()> {
    /// # let cache_dir = Path::new("/tmp/cache");
    /// // Multiple processes can hold this simultaneously
    /// let lock = CacheLock::acquire_shared(cache_dir, "bare-worktree-owner_repo").await?;
    /// // Lock released on drop
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire_shared(cache_dir: &Path, source_name: &str) -> Result<Self> {
        Self::acquire_shared_with_timeout(cache_dir, source_name, default_lock_timeout()).await
    }

    /// Acquires a shared (read) lock with a specified timeout.
    ///
    /// Uses exponential backoff (10ms → 500ms) without blocking the async runtime.
    ///
    /// # Errors
    ///
    /// Returns timeout error if lock cannot be acquired within the specified duration.
    pub async fn acquire_shared_with_timeout(
        cache_dir: &Path,
        source_name: &str,
        timeout: std::time::Duration,
    ) -> Result<Self> {
        use tokio::fs;

        let lock_name = format!("file-shared:{}", source_name);
        debug!(lock_name = %lock_name, "Waiting for shared file lock");

        // Create locks directory if it doesn't exist
        let locks_dir = cache_dir.join(".locks");
        fs::create_dir_all(&locks_dir).await.with_context(|| {
            format!("Failed to create locks directory: {}", locks_dir.display())
        })?;

        // Create lock file path
        let lock_path = locks_dir.join(format!("{source_name}.lock"));

        // CRITICAL: Use spawn_blocking for file open to avoid blocking tokio runtime
        let lock_path_clone = lock_path.clone();
        let file = tokio::task::spawn_blocking(move || {
            OpenOptions::new().create(true).write(true).truncate(false).open(&lock_path_clone)
        })
        .await
        .with_context(|| "spawn_blocking panicked")?
        .with_context(|| format!("Failed to open lock file: {}", lock_path.display()))?;

        // Wrap file in Arc for sharing with spawn_blocking
        let file = Arc::new(file);

        // Acquire shared lock with timeout and exponential backoff
        let start = std::time::Instant::now();

        let backoff = ExponentialBackoff::from_millis(STARTING_BACKOFF_DELAY_MS)
            .max_delay(Duration::from_millis(MAX_BACKOFF_DELAY_MS));

        // Add jitter to prevent thundering herd
        let mut rng_state: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345);

        for delay in backoff {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            let jitter_factor = 1.0 + (rng_state % 25) as f64 / 100.0;
            let jittered_delay =
                Duration::from_millis((delay.as_millis() as f64 * jitter_factor) as u64);

            // CRITICAL: Use spawn_blocking for try_lock_shared to avoid blocking tokio runtime
            // Use FileExt trait method explicitly to avoid std::fs::File::try_lock_shared
            let file_clone = Arc::clone(&file);
            let lock_result =
                tokio::task::spawn_blocking(move || FileExt::try_lock_shared(file_clone.as_ref()))
                    .await
                    .with_context(|| "spawn_blocking panicked")?;

            match lock_result {
                Ok(true) => {
                    debug!(
                        lock_name = %lock_name,
                        wait_ms = start.elapsed().as_millis(),
                        "Shared file lock acquired"
                    );
                    return Ok(Self {
                        _file: file,
                        lock_name,
                    });
                }
                Ok(false) | Err(_) => {
                    // Check remaining time before sleeping
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() {
                        return Err(anyhow::anyhow!(
                            "Timeout acquiring shared lock for '{}' after {:?}",
                            source_name,
                            timeout
                        ));
                    }
                    tokio::time::sleep(jittered_delay.min(remaining)).await;
                }
            }
        }

        Err(anyhow::anyhow!(
            "Timeout acquiring shared lock for '{}' after {:?}",
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

    #[tokio::test]
    async fn test_shared_locks_dont_block_each_other() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let cache_dir1 = cache_dir.clone();
        let barrier1 = barrier.clone();

        // Task 1: Acquire shared lock and hold it
        let handle1 = tokio::spawn(async move {
            let _lock = CacheLock::acquire_shared(&cache_dir1, "shared_test").await.unwrap();
            barrier1.wait().await; // Signal that lock is acquired
            tokio::time::sleep(Duration::from_millis(100)).await; // Hold lock
        });

        let cache_dir2 = cache_dir.clone();

        // Task 2: Acquire another shared lock on same resource (should NOT block)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await; // Wait for first task to acquire lock
            let start = Instant::now();
            let _lock = CacheLock::acquire_shared(&cache_dir2, "shared_test").await.unwrap();
            let elapsed = start.elapsed();

            // Should complete quickly since shared locks don't block each other
            assert!(
                elapsed < Duration::from_millis(200),
                "Shared lock took {:?}, expected < 200ms (no blocking)",
                elapsed
            );
        });

        handle1.await.unwrap();
        handle2.await.unwrap();
    }

    #[tokio::test]
    async fn test_exclusive_blocks_shared() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let cache_dir1 = cache_dir.clone();
        let barrier1 = barrier.clone();

        // Task 1: Acquire EXCLUSIVE lock and hold it
        let handle1 = tokio::spawn(async move {
            let _lock = CacheLock::acquire(&cache_dir1, "exclusive_shared_test").await.unwrap();
            barrier1.wait().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let cache_dir2 = cache_dir.clone();

        // Task 2: Try to acquire SHARED lock (should block until exclusive releases)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await;
            let start = Instant::now();
            let _lock =
                CacheLock::acquire_shared(&cache_dir2, "exclusive_shared_test").await.unwrap();
            let elapsed = start.elapsed();

            // Should have blocked for at least 50ms
            assert!(
                elapsed >= Duration::from_millis(50),
                "Shared lock should have blocked: {:?}",
                elapsed
            );
        });

        handle1.await.unwrap();
        handle2.await.unwrap();
    }

    #[tokio::test]
    async fn test_shared_blocks_exclusive() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let cache_dir1 = cache_dir.clone();
        let barrier1 = barrier.clone();

        // Task 1: Acquire SHARED lock and hold it
        let handle1 = tokio::spawn(async move {
            let _lock =
                CacheLock::acquire_shared(&cache_dir1, "shared_exclusive_test").await.unwrap();
            barrier1.wait().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let cache_dir2 = cache_dir.clone();

        // Task 2: Try to acquire EXCLUSIVE lock (should block until shared releases)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await;
            let start = Instant::now();
            let _lock = CacheLock::acquire(&cache_dir2, "shared_exclusive_test").await.unwrap();
            let elapsed = start.elapsed();

            // Should have blocked for at least 50ms
            assert!(
                elapsed >= Duration::from_millis(50),
                "Exclusive lock should have blocked: {:?}",
                elapsed
            );
        });

        handle1.await.unwrap();
        handle2.await.unwrap();
    }
}
