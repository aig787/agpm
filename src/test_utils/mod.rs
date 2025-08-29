//! Test utilities for CCPM
//!
//! This module provides utilities for writing tests, including helpers for
//! managing test environments, temporary directories, and test isolation.
//!
//! # Test Isolation
//!
//! The utilities in this module help ensure tests don't interfere with each other:
//! - Working directory guards to restore cwd after tests
//! - Temporary directory management
//! - Environment variable isolation
//! - Test fixtures for manifests, lockfiles, and markdown files
//! - Complete test environments with mock git repositories
//!
//! # Example
//!
//! ```rust
//! #[cfg(test)]
//! mod tests {
//!     use ccpm::test_utils::{WorkingDirGuard, TestEnvironment};
//!     
//!     #[test]
//!     fn test_with_environment() {
//!         let _guard = WorkingDirGuard::new().unwrap();
//!         let env = TestEnvironment::with_basic_manifest().unwrap();
//!         
//!         // Use the test environment
//!         assert!(env.file_exists("ccpm.toml"));
//!         
//!         // Working directory will be restored when guard is dropped
//!     }
//! }
//! ```

pub mod builder;
pub mod environment;
pub mod fixtures;

pub use builder::{TestEnvironment as SimpleTestEnvironment, TestEnvironmentBuilder};
pub use environment::TestEnvironment;
pub use fixtures::{GitRepoFixture, LockfileFixture, ManifestFixture, MarkdownFixture};

use once_cell::sync::Lazy;
use std::sync::{Mutex, Once};
use tracing_subscriber::EnvFilter;

/// Global mutex to prevent tests that change the current directory from running in parallel.
/// Tests that need to change the current directory should acquire this lock.
pub static WORKING_DIR_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Global flag to ensure logging is only initialized once in tests
static INIT_LOGGING: Once = Once::new();

/// Initialize logging for tests.
///
/// This function initializes the tracing subscriber for tests, but only once
/// regardless of how many times it's called. It respects the RUST_LOG environment
/// variable if set.
///
/// # Example
///
/// ```rust
/// fn my_test() {
///     ccpm::test_utils::init_test_logging();
///     // Your test code here - logging will work
/// }
/// ```
///
/// To enable logging in tests, run:
/// ```bash
/// RUST_LOG=debug cargo test
/// ```
pub fn init_test_logging() {
    INIT_LOGGING.call_once(|| {
        // Only initialize if RUST_LOG is set
        if std::env::var("RUST_LOG").is_ok() {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .with_test_writer() // Important: uses test-compatible writer
                .with_target(false)
                .with_thread_ids(false)
                .try_init();
        }
    });
}

/// A guard that automatically restores the current working directory when dropped.
/// This ensures test isolation when tests need to change the working directory.
/// This guard also holds a lock on the `WORKING_DIR_MUTEX` to prevent parallel execution.
pub struct WorkingDirGuard {
    original_dir: std::path::PathBuf,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl WorkingDirGuard {
    /// Create a new guard that saves the current working directory.
    /// The directory will be restored when this guard is dropped.
    /// This also acquires a lock to prevent parallel test execution.
    pub fn new() -> std::io::Result<Self> {
        // Initialize test logging if RUST_LOG is set
        init_test_logging();

        // Handle poisoned mutex by recovering the lock
        // This is safe for tests because we always restore the working directory
        let lock = match WORKING_DIR_MUTEX.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Recover from poisoned mutex - this is safe because we always
                // restore the working directory in Drop
                poisoned.into_inner()
            }
        };
        let original_dir = std::env::current_dir()?;
        Ok(WorkingDirGuard {
            original_dir,
            _lock: lock,
        })
    }

    /// Change to a new directory while keeping the guard.
    /// The original directory will still be restored when the guard is dropped.
    pub fn change_to<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        std::env::set_current_dir(path)
    }
}

impl Drop for WorkingDirGuard {
    fn drop(&mut self) {
        // Restore the original working directory
        // We ignore errors here because there's not much we can do in a Drop impl
        let _ = std::env::set_current_dir(&self.original_dir);
    }
}
