//! Global constants used throughout the AGPM codebase.
//!
//! This module contains timeout durations, retry parameters, and other
//! numeric constants that are used across multiple modules. Defining
//! them centrally improves maintainability and makes magic numbers
//! more discoverable.

use std::time::Duration;

/// Default timeout for cache lock acquisition (120 seconds).
///
/// This timeout must be long enough to accommodate multiple sequential worktree
/// operations that share the same lock (e.g., `bare-worktree-{owner}_{repo}`).
/// On slow CI environments or when conflict resolution creates many worktrees,
/// the lock may be held for extended periods. Set to 2× GIT_WORKTREE_TIMEOUT
/// to allow for at least 2 sequential worktree creations.
pub fn default_lock_timeout() -> Duration {
    Duration::from_secs(120)
}

/// Legacy constant for backwards compatibility - prefer `default_lock_timeout()` function.
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for pending operations (10 seconds).
pub fn pending_state_timeout() -> Duration {
    Duration::from_secs(10)
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

/// Timeout for batch operations using `join_all` (5 minutes).
///
/// This prevents indefinite blocking when batch futures hang.
pub fn batch_operation_timeout() -> Duration {
    Duration::from_secs(300)
}

/// Minimum number of parallel operations regardless of CPU count.
///
/// This ensures reasonable parallelism even on single-core machines.
/// The value of 10 provides good throughput for I/O-bound Git operations.
pub const MIN_PARALLELISM: usize = 10;

/// Multiplier applied to CPU core count for default parallelism.
///
/// Higher values increase throughput but may strain resources or hit rate limits.
/// The value of 2 balances throughput with system stability.
pub const PARALLELISM_CORE_MULTIPLIER: usize = 2;

/// Default CPU core count when detection fails.
///
/// Used as a fallback when `std::thread::available_parallelism()` returns an error.
pub const FALLBACK_CORE_COUNT: usize = 4;
