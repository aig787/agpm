//! Global constants used throughout the AGPM codebase.
//!
//! This module contains timeout durations, retry parameters, and other
//! numeric constants that are used across multiple modules. Defining
//! them centrally improves maintainability and makes magic numbers
//! more discoverable.

use std::time::Duration;

/// Default timeout for cache lock acquisition.
/// In test mode (AGPM_TEST_MODE=true), uses 8 seconds to trigger before test timeouts.
/// In production, uses 30 seconds.
pub fn default_lock_timeout() -> Duration {
    if std::env::var("AGPM_TEST_MODE").is_ok() {
        Duration::from_secs(8) // Short enough to trigger before 15s test timeout
    } else {
        Duration::from_secs(30)
    }
}

/// Legacy constant for backwards compatibility - prefer `default_lock_timeout()` function.
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for pending operations.
/// In test mode, uses 5 seconds. In production, uses 10 seconds.
pub fn pending_state_timeout() -> Duration {
    if std::env::var("AGPM_TEST_MODE").is_ok() {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(10)
    }
}

/// Legacy constant for backwards compatibility - prefer `pending_state_timeout()` function.
pub const PENDING_STATE_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum backoff delay for exponential backoff (500ms).
///
/// Exponential backoff delays are capped at this value to prevent
/// excessive wait times during retry operations.
pub const MAX_BACKOFF_DELAY_MS: u64 = 500;

/// Starting delay for exponential backoff (10ms).
///
/// This is the initial delay used in exponential backoff calculations,
/// which doubles on each retry attempt.
pub const STARTING_BACKOFF_DELAY_MS: u64 = 10;

/// Timeout for Git fetch operations (60 seconds).
///
/// This timeout prevents hung network connections from blocking
/// worktree creation indefinitely.
pub const GIT_FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for Git clone operations (120 seconds).
///
/// Clone operations may take longer than fetch, especially
/// for large repositories.
pub const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for Git worktree creation (60 seconds).
///
/// Creating a worktree involves checking out files which
/// can take time for large repositories.
pub const GIT_WORKTREE_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for batch operations using `join_all`.
///
/// This prevents indefinite blocking when batch futures hang.
/// In test mode, uses 30 seconds (allows multiple retries within 60s test timeout).
/// In production, uses 5 minutes.
pub fn batch_operation_timeout() -> Duration {
    if std::env::var("AGPM_TEST_MODE").is_ok() {
        Duration::from_secs(30) // Short enough to detect hangs within 60s test timeout
    } else {
        Duration::from_secs(300) // 5 minutes for large dependency graphs
    }
}
