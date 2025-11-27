//! Project-level file locking for cross-process coordination.
//!
//! This module provides process-safe file locking for project operations
//! like resource installation. The locks are automatically released when
//! the lock object is dropped.
//!
//! # Async Safety
//!
//! All file operations are wrapped in `spawn_blocking` to avoid blocking the tokio
//! runtime. This is critical for preventing worker thread starvation under high
//! parallelism with slow I/O.

use crate::constants::{MAX_BACKOFF_DELAY_MS, STARTING_BACKOFF_DELAY_MS, default_lock_timeout};
use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_retry::strategy::ExponentialBackoff;
use tracing::debug;

/// A file lock for project-level operations.
///
/// Provides cross-process synchronization for operations like resource
/// installation. Uses OS-level file locking via the fs4 crate.
///
/// # Lock File Location
///
/// Lock files are stored in `{project_dir}/.agpm/.locks/{lock_name}.lock`
///
/// # Example
///
/// ```rust,no_run
/// use agpm_cli::installer::project_lock::ProjectLock;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let project_dir = Path::new("/path/to/project");
///
/// // Acquire resource lock for file writes
/// let _lock = ProjectLock::acquire(project_dir, "resource").await?;
///
/// // Perform file operations...
/// // Lock is automatically released when _lock goes out of scope
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ProjectLock {
    /// The file handle - lock is released when this is dropped
    _file: Arc<File>,
    /// Name of the lock for tracing
    lock_name: String,
    /// Path to the lock file for cleanup on drop
    lock_path: PathBuf,
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        debug!(lock_name = %self.lock_name, "Project lock released");
        // Clean up the lock file to prevent accumulation
        if let Err(e) = std::fs::remove_file(&self.lock_path) {
            // Only log if it's not a "file not found" error (race condition with another cleanup)
            if e.kind() != std::io::ErrorKind::NotFound {
                debug!(lock_name = %self.lock_name, error = %e, "Failed to remove lock file");
            }
        }
    }
}

impl ProjectLock {
    /// Acquires an exclusive lock for a project operation.
    ///
    /// Creates and acquires an exclusive file lock for the specified lock name.
    /// Uses non-blocking lock attempts with exponential backoff and timeout.
    ///
    /// # Lock File Management
    ///
    /// 1. Creates `.agpm/.locks/` directory if needed
    /// 2. Creates `{lock_name}.lock` file
    /// 3. Acquires exclusive access via OS file locking
    /// 4. Keeps file handle open to maintain lock
    ///
    /// # Behavior
    ///
    /// - **Timeout**: 30-second default (configurable via `acquire_with_timeout`)
    /// - **Non-blocking**: `try_lock_exclusive()` in async retry loop
    /// - **Backoff**: 10ms → 20ms → 40ms... up to 500ms max
    ///
    /// # Errors
    ///
    /// - Permission denied creating lock directory
    /// - Disk space exhausted
    /// - Timeout acquiring lock
    ///
    /// # Platform Support
    ///
    /// - **Windows**: Win32 `LockFile` API
    /// - **Unix**: POSIX `fcntl()` locking
    pub async fn acquire(project_dir: &Path, lock_name: &str) -> Result<Self> {
        Self::acquire_with_timeout(project_dir, lock_name, default_lock_timeout()).await
    }

    /// Acquires an exclusive lock with a specified timeout.
    ///
    /// Uses exponential backoff (10ms → 500ms) without blocking the async runtime.
    ///
    /// # Errors
    ///
    /// Returns timeout error if lock cannot be acquired within the specified duration.
    pub async fn acquire_with_timeout(
        project_dir: &Path,
        lock_name: &str,
        timeout: Duration,
    ) -> Result<Self> {
        use tokio::fs;

        let display_name = format!("project:{}", lock_name);
        debug!(lock_name = %display_name, "Waiting for project lock");

        // Create .agpm/.locks directory if it doesn't exist
        let locks_dir = project_dir.join(".agpm").join(".locks");
        fs::create_dir_all(&locks_dir).await.with_context(|| {
            format!("Failed to create project locks directory: {}", locks_dir.display())
        })?;

        // Create lock file path
        let lock_path = locks_dir.join(format!("{lock_name}.lock"));

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

        // Acquire exclusive lock with timeout and exponential backoff
        let start = std::time::Instant::now();

        // Create exponential backoff strategy: 10ms, 20ms, 40ms... capped at 500ms
        let backoff = ExponentialBackoff::from_millis(STARTING_BACKOFF_DELAY_MS)
            .max_delay(Duration::from_millis(MAX_BACKOFF_DELAY_MS));

        for delay in backoff {
            // CRITICAL: Use spawn_blocking for try_lock_exclusive to avoid blocking tokio runtime
            let file_clone = Arc::clone(&file);
            let lock_result = tokio::task::spawn_blocking(move || file_clone.try_lock_exclusive())
                .await
                .with_context(|| "spawn_blocking panicked")?;

            match lock_result {
                Ok(true) => {
                    debug!(
                        lock_name = %display_name,
                        wait_ms = start.elapsed().as_millis(),
                        "Project lock acquired"
                    );
                    return Ok(Self {
                        _file: file,
                        lock_name: display_name,
                        lock_path,
                    });
                }
                Ok(false) | Err(_) => {
                    // Check remaining time before sleeping to avoid exceeding timeout
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() {
                        return Err(anyhow::anyhow!(
                            "Timeout acquiring project lock '{}' after {:?}",
                            lock_name,
                            timeout
                        ));
                    }
                    // Sleep for the shorter of delay or remaining time
                    tokio::time::sleep(delay.min(remaining)).await;
                }
            }
        }

        // If backoff iterator exhausted without acquiring lock, return timeout error
        Err(anyhow::anyhow!("Timeout acquiring project lock '{}' after {:?}", lock_name, timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_project_lock_acquire_and_release() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Acquire lock
        let lock = ProjectLock::acquire(project_dir, "test").await.unwrap();

        // Verify lock file was created
        let lock_path = project_dir.join(".agpm").join(".locks").join("test.lock");
        assert!(lock_path.exists());

        // Drop the lock
        drop(lock);

        // Lock file should be deleted on drop
        assert!(!lock_path.exists());
    }

    #[tokio::test]
    async fn test_project_lock_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Directories shouldn't exist initially
        let locks_dir = project_dir.join(".agpm").join(".locks");
        assert!(!locks_dir.exists());

        // Acquire lock - should create directories
        let lock = ProjectLock::acquire(project_dir, "test").await.unwrap();

        // Verify directories were created
        assert!(locks_dir.exists());
        assert!(locks_dir.is_dir());

        drop(lock);
    }

    #[tokio::test]
    async fn test_project_lock_exclusive_blocking() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let project_dir1 = project_dir.clone();
        let barrier1 = barrier.clone();

        // Task 1: Acquire lock and hold it
        let handle1 = tokio::spawn(async move {
            let _lock = ProjectLock::acquire(&project_dir1, "exclusive_test").await.unwrap();
            barrier1.wait().await; // Signal that lock is acquired
            tokio::time::sleep(Duration::from_millis(100)).await; // Hold lock
            // Lock released on drop
        });

        let project_dir2 = project_dir.clone();

        // Task 2: Try to acquire same lock (should block)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await; // Wait for first task to acquire lock
            let start = Instant::now();
            let _lock = ProjectLock::acquire(&project_dir2, "exclusive_test").await.unwrap();
            let elapsed = start.elapsed();

            // Should have blocked for at least 50ms (less than 100ms due to timing)
            assert!(elapsed >= Duration::from_millis(50));
        });

        handle1.await.unwrap();
        handle2.await.unwrap();
    }

    #[tokio::test]
    async fn test_project_lock_different_names_dont_block() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let project_dir1 = project_dir.clone();
        let barrier1 = barrier.clone();

        // Task 1: Lock "lock1"
        let handle1 = tokio::spawn(async move {
            let _lock = ProjectLock::acquire(&project_dir1, "lock1").await.unwrap();
            barrier1.wait().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let project_dir2 = project_dir.clone();

        // Task 2: Lock "lock2" (different name, shouldn't block)
        let handle2 = tokio::spawn(async move {
            barrier.wait().await;
            let start = Instant::now();
            let _lock = ProjectLock::acquire(&project_dir2, "lock2").await.unwrap();
            let elapsed = start.elapsed();

            // Should not block (complete quickly)
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
    async fn test_project_lock_acquire_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // First lock succeeds
        let _lock1 = ProjectLock::acquire(project_dir, "test").await.unwrap();

        // Second lock attempt should timeout quickly
        let start = std::time::Instant::now();
        let result =
            ProjectLock::acquire_with_timeout(project_dir, "test", Duration::from_millis(100))
                .await;

        let elapsed = start.elapsed();

        // Verify timeout occurred
        assert!(result.is_err(), "Expected timeout error");

        // Verify error message mentions timeout
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Timeout") || error_msg.contains("timeout"),
            "Error message should mention timeout: {}",
            error_msg
        );

        // Verify timeout happened around the expected time
        assert!(elapsed >= Duration::from_millis(50), "Timeout too quick: {:?}", elapsed);
        assert!(elapsed < Duration::from_millis(500), "Timeout too slow: {:?}", elapsed);
    }
}
