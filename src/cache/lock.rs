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
    /// This method creates and acquires an exclusive file lock for the specified
    /// source name. The operation is blocking - it will wait until any existing
    /// locks are released before proceeding.
    ///
    /// # Lock File Management
    ///
    /// The method performs several setup operations:
    /// 1. **Locks directory creation**: Creates `.locks/` directory if needed
    /// 2. **Lock file creation**: Creates `{source_name}.lock` file
    /// 3. **Exclusive locking**: Acquires exclusive access via OS file locking
    /// 4. **Handle retention**: Keeps file handle open to maintain lock
    ///
    /// # Blocking Behavior
    ///
    /// If another process already holds a lock for the same source:
    /// - **Blocking wait**: Method blocks until other lock is released
    /// - **Fair queuing**: Locks are typically acquired in FIFO order
    /// - **No timeout**: Method will wait indefinitely (use with caution)
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
    /// - `~/.ccpm/cache/.locks/community.lock`
    /// - `~/.ccpm/cache/.locks/work-tools.lock`
    /// - `~/.ccpm/cache/.locks/my-project.lock`
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
    /// - **Windows**: Uses Win32 LockFile API via [`fs4`]
    /// - **Unix**: Uses POSIX fcntl() locking via [`fs4`]
    /// - **NFS/Network**: Behavior depends on file system support
    /// - **Docker**: Works within containers with proper volume mounts
    ///
    /// # Examples
    ///
    /// Simple lock acquisition:
    ///
    /// ```rust,no_run
    /// use ccpm::cache::lock::CacheLock;
    /// use std::path::PathBuf;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache_dir = PathBuf::from("/home/user/.ccpm/cache");
    ///
    /// // This will block if another process has the lock
    /// let lock = CacheLock::acquire(&cache_dir, "my-source")?;
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
    /// use ccpm::cache::lock::CacheLock;
    /// use std::path::PathBuf;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let cache_dir = PathBuf::from("/tmp/cache");
    ///
    /// match CacheLock::acquire(&cache_dir, "problematic-source") {
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
    pub fn acquire(cache_dir: &Path, source_name: &str) -> Result<Self> {
        // Create lock file path: ~/.ccpm/cache/.locks/source_name.lock
        let locks_dir = cache_dir.join(".locks");
        std::fs::create_dir_all(&locks_dir).with_context(|| {
            format!("Failed to create locks directory: {}", locks_dir.display())
        })?;

        let lock_path = locks_dir.join(format!("{}.lock", source_name));

        // Open or create the lock file
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file: {}", lock_path.display()))?;

        // Try to acquire exclusive lock (blocking)
        file.lock_exclusive()
            .with_context(|| format!("Failed to acquire lock for: {}", source_name))?;

        Ok(CacheLock {
            _file: file,
            path: lock_path,
        })
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        // Lock is automatically released when file is closed (on Drop)
        // But we can explicitly unlock for clarity
        if let Err(e) = self._file.unlock() {
            eprintln!("Warning: Failed to unlock {}: {}", self.path.display(), e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_lock_acquire_and_release() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // Acquire lock
        let lock = CacheLock::acquire(cache_dir, "test_source").unwrap();

        // Verify lock file was created
        let lock_path = cache_dir.join(".locks").join("test_source.lock");
        assert!(lock_path.exists());

        // Drop the lock
        drop(lock);

        // Lock file should still exist (we don't delete it)
        assert!(lock_path.exists());
    }

    #[test]
    fn test_cache_lock_creates_locks_directory() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();

        // Locks directory shouldn't exist initially
        let locks_dir = cache_dir.join(".locks");
        assert!(!locks_dir.exists());

        // Acquire lock - should create directory
        let _lock = CacheLock::acquire(cache_dir, "test").unwrap();

        // Verify locks directory was created
        assert!(locks_dir.exists());
        assert!(locks_dir.is_dir());
    }

    #[test]
    fn test_cache_lock_exclusive_blocking() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{Duration, Instant};

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let cache_dir1 = cache_dir.clone();
        let barrier1 = barrier.clone();

        // Thread 1: Acquire lock and hold it
        let handle1 = thread::spawn(move || {
            let _lock = CacheLock::acquire(&cache_dir1, "exclusive_test").unwrap();
            barrier1.wait(); // Signal that lock is acquired
            thread::sleep(Duration::from_millis(100)); // Hold lock
                                                       // Lock released on drop
        });

        let cache_dir2 = cache_dir.clone();

        // Thread 2: Try to acquire same lock (should block)
        let handle2 = thread::spawn(move || {
            barrier.wait(); // Wait for first thread to acquire lock
            let start = Instant::now();
            let _lock = CacheLock::acquire(&cache_dir2, "exclusive_test").unwrap();
            let elapsed = start.elapsed();

            // Should have blocked for at least 50ms (less than 100ms due to timing)
            assert!(elapsed >= Duration::from_millis(50));
        });

        handle1.join().unwrap();
        handle2.join().unwrap();
    }

    #[test]
    fn test_cache_lock_different_sources_dont_block() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{Duration, Instant};

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = Arc::new(temp_dir.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(2));

        let cache_dir1 = cache_dir.clone();
        let barrier1 = barrier.clone();

        // Thread 1: Lock source1
        let handle1 = thread::spawn(move || {
            let _lock = CacheLock::acquire(&cache_dir1, "source1").unwrap();
            barrier1.wait();
            thread::sleep(Duration::from_millis(100));
        });

        let cache_dir2 = cache_dir.clone();

        // Thread 2: Lock source2 (different source, shouldn't block)
        let handle2 = thread::spawn(move || {
            barrier.wait();
            let start = Instant::now();
            let _lock = CacheLock::acquire(&cache_dir2, "source2").unwrap();
            let elapsed = start.elapsed();

            // Should not block (complete quickly)
            assert!(elapsed < Duration::from_millis(50));
        });

        handle1.join().unwrap();
        handle2.join().unwrap();
    }

    #[test]
    fn test_cache_lock_path_with_special_characters() {
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
            let lock = CacheLock::acquire(cache_dir, name).unwrap();
            let expected_path = cache_dir.join(".locks").join(format!("{}.lock", name));
            assert!(expected_path.exists());
            drop(lock);
        }
    }
}
