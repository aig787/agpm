//! Global constants used throughout the AGPM codebase.
//!
//! This module contains timeout durations, retry parameters, and other
//! numeric constants that are used across multiple modules. Defining
//! them centrally improves maintainability and makes magic numbers
//! more discoverable.

use std::time::Duration;

/// Default timeout for cache lock acquisition (30 seconds).
///
/// This is the standard timeout for acquiring exclusive locks
/// on cache files to prevent indefinite blocking.
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for pending operations (10 seconds).
///
/// Used for operations that may be in a pending state,
/// such as worktree creation or Git operations.
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
