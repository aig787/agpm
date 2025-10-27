//! Test utilities for AGPM
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
//! ```rust,no_run
//! #[cfg(test)]
//! mod tests {
//!     
//!     #[test]
//!     fn test_with_environment() {
//!         let env = TestEnvironment::with_basic_manifest().unwrap();
//!         
//!         // Use the test environment
//!         assert!(env.file_exists("agpm.toml"));
//!     }
//! }
//! ```

pub mod builder;
pub mod environment;
pub mod fixtures;
pub mod git_helper;

pub use builder::{TestEnvironment as SimpleTestEnvironment, TestEnvironmentBuilder};
pub use environment::TestEnvironment;
pub use fixtures::{GitRepoFixture, LockfileFixture, ManifestFixture, MarkdownFixture};
pub use git_helper::TestGit;

use std::sync::Once;
use tracing::Level;
use tracing_subscriber::EnvFilter;

/// Global flag to ensure logging is only initialized once in tests
static INIT_LOGGING: Once = Once::new();

/// Initialize logging for tests.
///
/// This function initializes the tracing subscriber for tests, but only once
/// regardless of how many times it's called. It respects the `RUST_LOG` environment
/// variable if set, or uses the provided log level.
///
/// # Arguments
///
/// * `level` - Optional log level to use. If None, uses `RUST_LOG` environment variable
///
/// # Example
///
/// ```rust,no_run
/// use tracing::Level;
///
/// fn my_test() {
///     // Use environment variable
///     agpm_cli::test_utils::init_test_logging(None);
///
///     // Or set level programmatically
///     agpm_cli::test_utils::init_test_logging(Some(Level::DEBUG));
///
///     // Your test code here - logging will work
/// }
/// ```
///
/// To enable logging in tests via environment variable:
/// ```bash
/// RUST_LOG=debug cargo test
/// ```
pub fn init_test_logging(level: Option<Level>) {
    INIT_LOGGING.call_once(|| {
        // Determine the filter to use
        let filter = if let Some(level) = level {
            // Use the provided level
            EnvFilter::new(level.to_string())
        } else if std::env::var("RUST_LOG").is_ok() {
            // Use environment variable
            EnvFilter::from_default_env()
        } else {
            // No logging if neither is provided
            return;
        };

        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer() // Important: uses test-compatible writer
            .with_target(true) // Show module targets like "git"
            .with_thread_ids(false)
            .with_ansi(true) // Enable ANSI color codes for better readability
            .try_init();
    });
}

/// Compute the SHA-256 hash of variant_inputs JSON value for tests.
///
/// This helper function computes the correct `variant_inputs_hash` value
/// that should be used when creating `LockedResource` instances in tests.
/// It delegates to the centralized hash computation function.
///
/// # Arguments
///
/// * `variant_inputs` - The variant_inputs JSON value (typically empty object for tests)
///
/// # Returns
///
/// A string in the format "sha256:hexdigest"
///
/// # Example
///
/// ```rust,no_run
/// use agpm_cli::test_utils::compute_variant_inputs_hash;
/// use serde_json::json;
///
/// let hash = compute_variant_inputs_hash(&json!({}));
/// assert_eq!(hash, "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a");
/// ```
pub fn compute_variant_inputs_hash(variant_inputs: &serde_json::Value) -> String {
    crate::utils::compute_variant_inputs_hash(variant_inputs).unwrap_or_else(|_| {
        panic!("Failed to compute variant_inputs_hash in test");
    })
}
