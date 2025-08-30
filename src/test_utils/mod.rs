//! Test utilities for CCPM
//!
//! This module provides utilities for writing tests, including helpers for
//! managing test environments, temporary directories, and test isolation.
//!
//! # Test Isolation
//!
//! The utilities in this module help ensure tests don't interfere with each other:
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
//!     
//!     #[test]
//!     fn test_with_environment() {
//!         let env = TestEnvironment::with_basic_manifest().unwrap();
//!         
//!         // Use the test environment
//!         assert!(env.file_exists("ccpm.toml"));
//!     }
//! }
//! ```

pub mod builder;
pub mod environment;
pub mod fixtures;

pub use builder::{TestEnvironment as SimpleTestEnvironment, TestEnvironmentBuilder};
pub use environment::TestEnvironment;
pub use fixtures::{GitRepoFixture, LockfileFixture, ManifestFixture, MarkdownFixture};

use std::sync::Once;
use tracing_subscriber::EnvFilter;

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
