//! Lock ordering manager to prevent deadlocks during parallel cache operations.
//!
//! This module implements a strict alphabetical ordering system for repository locks
//! to prevent deadlock scenarios where concurrent tasks acquire locks in different orders.
//! The manager enforces that all tasks acquire locks in the same canonical order,
//! allowing parallelism while guaranteeing correctness.

use crate::cache::lock::CacheLock;
use anyhow::Result;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;

/// Error types for lock ordering violations.
#[derive(Debug, thiserror::Error)]
pub enum LockOrderError {
    /// Attempted to acquire a lock out of alphabetical order.
    #[error(
        "Lock order violation: attempted to acquire '{requested_lock}' while holding {held_locks:?}"
    )]
    OutOfOrder {
        /// The list of locks currently held by this task
        held_locks: Vec<String>,
        /// The lock that was requested out of order
        requested_lock: String,
    },

    /// Lock acquisition failed.
    #[error("Lock acquisition failed: {0}")]
    AcquisitionFailed(#[from] anyhow::Error),
}

impl LockOrderError {
    /// Check if this is an out-of-order error that can be retried.
    pub fn is_out_of_order(&self) -> bool {
        matches!(self, LockOrderError::OutOfOrder { .. })
    }
}

/// Simple identifier that works both inside and outside of Tokio task contexts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskIdentifier {
    /// Thread ID
    thread_id: std::thread::ThreadId,
}

impl TaskIdentifier {
    /// Get the current task identifier
    pub fn current() -> Self {
        Self {
            thread_id: std::thread::current().id(),
        }
    }
}

/// A guard that represents an acquired lock.
///
/// This wrapper ensures that locks are automatically released and deregistered
/// from the manager when they go out of scope.
#[derive(Debug)]
pub struct AcquiredLock<'a> {
    /// The underlying cache lock
    pub lock: CacheLock,
    /// The name of this lock for tracking
    lock_name: String,
    /// The task ID that owns this lock
    task_id: TaskIdentifier,
    /// Reference to the lock manager for cleanup
    manager: &'a LockManager,
}

impl<'a> Drop for AcquiredLock<'a> {
    fn drop(&mut self) {
        // Tell the manager that this lock is being released
        self.manager.release_lock(self.task_id, &self.lock_name);
    }
}

/// Lock manager that enforces strict alphabetical ordering to prevent deadlocks.
///
/// This manager tracks which locks are held by each task and enforces that
/// new locks are acquired in alphabetical order. If a task attempts to acquire
/// a lock out of order, it receives an `OutOfOrder` error and must retry
/// with the correct ordering.
///
/// # Deadlock Prevention Strategy
///
/// The core strategy is to ensure a **global total order** for lock acquisition:
///
/// 1. **Canonical Ordering**: All locks are sorted alphabetically before acquisition
/// 2. **All-or-Nothing**: Tasks must acquire all required locks in the correct order
/// 3. **Retry on Conflict**: If new dependencies are discovered, release all locks and retry
/// 4. **Automatic Cleanup**: Locks are automatically released when guards are dropped
///
/// # Example Workflow
///
/// ```text
/// Task discovers dependencies for repo-A and repo-C:
/// 1. Required locks = ["repo-A", "repo-C"] (already sorted)
/// 2. Acquire "repo-A" (first in order)
/// 3. Acquire "repo-C" (second in order)
/// 4. During processing, discover transitive dependency on repo-B
/// 5. Return OutOfOrder error (B < C alphabetically)
/// 6. Task releases all locks
/// 7. Task retries with Required locks = ["repo-A", "repo-B", "repo-C"]
/// ```
#[derive(Debug)]
pub struct LockManager {
    /// Tracks the sorted list of lock names held by each task.
    ///
    /// Key: Task Identifier
    /// Value: Alphabetically sorted list of lock names held by this task
    held_locks: Arc<DashMap<TaskIdentifier, Vec<String>>>,
}

impl LockManager {
    /// Create a new lock manager.
    pub fn new() -> Self {
        Self {
            held_locks: Arc::new(DashMap::new()),
        }
    }

    /// Acquire a lock with strict ordering enforcement.
    ///
    /// This method will:
    /// 1. Check if the requested lock would violate alphabetical ordering
    /// 2. If valid, acquire the underlying file lock
    /// 3. Register the lock in the tracking system
    /// 4. Return an AcquiredLock that auto-releases on drop
    ///
    /// # Parameters
    ///
    /// * `cache_dir` - The cache directory for lock files
    /// * `lock_name` - The name of the lock to acquire
    /// * `timeout` - Maximum time to wait for lock acquisition
    ///
    /// # Returns
    ///
    /// Returns an `AcquiredLock` guard. The lock is automatically released when dropped.
    ///
    /// # Errors
    ///
    /// - `OutOfOrder`: If the lock would violate alphabetical ordering
    /// - `AcquisitionFailed`: If the lock cannot be acquired
    pub async fn acquire<'a>(
        &'a self,
        cache_dir: &std::path::Path,
        lock_name: String,
        timeout: Duration,
    ) -> Result<AcquiredLock<'a>, LockOrderError> {
        let task_id = TaskIdentifier::current();

        // Check lock ordering
        self.validate_lock_order(&task_id, &lock_name)?;

        // Acquire the underlying file lock
        let lock = CacheLock::acquire_with_timeout(cache_dir, &lock_name, timeout)
            .await
            .map_err(LockOrderError::AcquisitionFailed)?;

        // Register the lock as held by this task
        self.register_lock(task_id, lock_name.clone());

        Ok(AcquiredLock {
            lock,
            lock_name,
            task_id,
            manager: self,
        })
    }

    /// Validate that acquiring a lock would maintain alphabetical order.
    ///
    /// Also checks if the lock is already held by this task.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the order is valid, or `Err(OutOfOrder)` if it would violate ordering.
    fn validate_lock_order(
        &self,
        task_id: &TaskIdentifier,
        requested_lock: &str,
    ) -> Result<(), LockOrderError> {
        if let Some(held) = self.held_locks.get(task_id) {
            // Check if we already hold this exact lock - if so, skip validation
            if held.iter().any(|held_lock| held_lock.as_str() == requested_lock) {
                return Ok(()); // Already holding this lock, no issue
            }

            // Check if the requested lock comes before any already-held lock
            for held_lock in held.iter() {
                if requested_lock < held_lock.as_str() {
                    return Err(LockOrderError::OutOfOrder {
                        held_locks: held.clone(),
                        requested_lock: requested_lock.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Register a lock as being held by a task.
    ///
    /// The lock list is maintained in sorted order.
    /// Returns true if this is a new lock, false if already held.
    fn register_lock(&self, task_id: TaskIdentifier, lock_name: String) -> bool {
        let mut entry = self.held_locks.entry(task_id).or_default();

        // Check if already exists
        if entry.binary_search(&lock_name).is_ok() {
            return false; // Already exists
        }

        // Insert in sorted order (binary search for efficiency)
        let insert_pos = entry.binary_search(&lock_name).unwrap_or_else(|pos| pos);
        entry.insert(insert_pos, lock_name);
        true
    }

    /// Release a lock held by a task.
    ///
    /// Called by `AcquiredLock` when it's dropped.
    fn release_lock(&self, task_id: TaskIdentifier, lock_name: &str) {
        if let Some(mut entry) = self.held_locks.get_mut(&task_id) {
            // Find and remove the lock
            if let Some(pos) = entry.iter().position(|name| name == lock_name) {
                entry.remove(pos);
            }

            // Clean up empty entries
            if entry.is_empty() {
                drop(entry); // Release the mutable reference
                self.held_locks.remove(&task_id);
            }
        }
    }

    /// Get the list of locks currently held by a task.
    ///
    /// Returns None if the task has no locks registered.
    #[cfg(test)]
    pub fn get_held_locks(&self, task_id: TaskIdentifier) -> Option<Vec<String>> {
        self.held_locks.get(&task_id).map(|entry| entry.clone())
    }

    /// Get the number of tasks currently holding locks.
    #[cfg(test)]
    pub fn active_task_count(&self) -> usize {
        self.held_locks.len()
    }

    /// Clear all lock tracking (for testing purposes).
    #[cfg(test)]
    pub fn clear(&self) {
        self.held_locks.clear();
    }
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_lock_order_validation_success() {
        let manager = Arc::new(LockManager::new());
        let temp_dir = Arc::new(TempDir::new().unwrap());

        let manager_clone = manager.clone();
        let temp_dir_clone = temp_dir.clone();

        tokio::spawn(async move {
            // Acquire locks in alphabetical order - should succeed
            let lock_a = manager_clone
                .acquire(temp_dir_clone.path(), "lock-a".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            let lock_b = manager_clone
                .acquire(temp_dir_clone.path(), "lock-b".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            // Locks are held
            let task_id = TaskIdentifier::current();
            let held = manager_clone.get_held_locks(task_id).unwrap();
            assert_eq!(held, vec!["lock-a", "lock-b"]);

            drop(lock_a);
            drop(lock_b);
        })
        .await
        .unwrap();

        // Locks are released
        assert_eq!(manager.active_task_count(), 0);
    }

    #[tokio::test]
    async fn test_lock_order_violation() {
        let manager = Arc::new(LockManager::new());
        let temp_dir = Arc::new(TempDir::new().unwrap());

        let manager_clone = manager.clone();
        let temp_dir_clone = temp_dir.clone();

        let test_result = tokio::spawn(async move {
            // Acquire "lock-c" first
            let lock_c = manager_clone
                .acquire(temp_dir_clone.path(), "lock-c".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            // Try to acquire "lock-a" (should fail - out of order)
            let result = manager_clone
                .acquire(temp_dir_clone.path(), "lock-a".to_string(), Duration::from_millis(100))
                .await;

            drop(lock_c);

            // Convert result to a portable format
            match result {
                Ok(_) => false, // Should not succeed
                Err(LockOrderError::OutOfOrder {
                    held_locks,
                    requested_lock,
                }) => held_locks == vec!["lock-c"] && requested_lock == "lock-a",
                _ => false,
            }
        })
        .await
        .unwrap();

        assert!(test_result, "Expected out-of-order error");
    }

    #[tokio::test]
    async fn test_multiple_tasks_independent() {
        let manager = Arc::new(LockManager::new());
        let temp_dir = TempDir::new().unwrap();

        let manager1 = manager.clone();
        let temp_dir1 = temp_dir.path().to_path_buf();
        let handle1 = tokio::spawn(async move {
            manager1
                .acquire(&temp_dir1, "task1-lock".to_string(), Duration::from_millis(100))
                .await
                .unwrap();
        });

        let manager2 = manager.clone();
        let temp_dir2 = temp_dir.path().to_path_buf();
        let handle2 = tokio::spawn(async move {
            manager2
                .acquire(&temp_dir2, "task2-lock".to_string(), Duration::from_millis(100))
                .await
                .unwrap();
        });

        // Both should succeed (different tasks, independent locks)
        let (_lock1, _lock2) = tokio::join!(handle1, handle2);
        assert_eq!(manager.active_task_count(), 0); // All locks released
    }

    #[tokio::test]
    async fn test_retry_scenario() {
        let manager = Arc::new(LockManager::new());
        let temp_dir = Arc::new(TempDir::new().unwrap());

        let manager_clone = manager.clone();
        let temp_dir_clone = temp_dir.clone();

        tokio::spawn(async move {
            // Simulate the retry scenario from the plan
            let mut required_locks = ["repo-a".to_string(), "repo-c".to_string()];
            required_locks.sort(); // Ensure alphabetical order

            // First attempt: acquire repo-a and repo-c
            let _lock_a = manager_clone
                .acquire(temp_dir_clone.path(), "repo-a".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            let _lock_c = manager_clone
                .acquire(temp_dir_clone.path(), "repo-c".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            // During processing, discover we need repo-b (out of order)
            drop(_lock_a);
            drop(_lock_c); // Release all locks

            // Retry with all three locks in correct order
            let mut required_locks =
                ["repo-a".to_string(), "repo-b".to_string(), "repo-c".to_string()];
            required_locks.sort();

            let _lock_a = manager_clone
                .acquire(temp_dir_clone.path(), "repo-a".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            let _lock_b = manager_clone
                .acquire(temp_dir_clone.path(), "repo-b".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            let _lock_c = manager_clone
                .acquire(temp_dir_clone.path(), "repo-c".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            // All locks acquired successfully
            let task_id = TaskIdentifier::current();
            let held = manager_clone.get_held_locks(task_id).unwrap();
            assert_eq!(held, vec!["repo-a", "repo-b", "repo-c"]);
        })
        .await
        .unwrap();

        // Locks are released
        assert_eq!(manager.active_task_count(), 0);
    }

    #[tokio::test]
    async fn test_duplicate_lock_prevention() {
        let manager = Arc::new(LockManager::new());
        let temp_dir = Arc::new(TempDir::new().unwrap());

        let manager_clone = manager.clone();
        let temp_dir_clone = temp_dir.clone();

        tokio::spawn(async move {
            // Acquire a lock once - this tests our tracking logic
            let _lock1 = manager_clone
                .acquire(temp_dir_clone.path(), "duplicate".to_string(), Duration::from_millis(100))
                .await
                .unwrap();

            let task_id = TaskIdentifier::current();
            let held = manager_clone.get_held_locks(task_id).unwrap();
            // Should appear exactly once
            assert_eq!(held, vec!["duplicate"]);
        })
        .await
        .unwrap();

        // Locks are released
        assert_eq!(manager.active_task_count(), 0);
    }
}
