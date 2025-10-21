//! Operation-scoped context for cross-module state management.
//!
//! Provides a context object that flows through CLI operations, enabling
//! features like warning deduplication without global state.
//!
//! # Overview
//!
//! The [`OperationContext`] is created at the start of CLI command execution
//! and passed through the call chain to coordinate behavior across modules.
//! This enables:
//!
//! - Warning deduplication during dependency resolution
//! - Operation-scoped state without global variables
//! - Better test isolation (each test creates its own context)
//!
//! # Example
//!
//! ```rust,no_run
//! use agpm_cli::core::OperationContext;
//! use std::path::Path;
//!
//! let ctx = OperationContext::new();
//!
//! // First warning for a file
//! assert!(ctx.should_warn_file(Path::new("test.md")));
//!
//! // Subsequent warnings deduplicated
//! assert!(!ctx.should_warn_file(Path::new("test.md")));
//! ```

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

/// Context for a single CLI operation (install, update, validate, etc.)
///
/// This context object flows through the operation call chain, providing
/// operation-scoped state like warning deduplication. It uses [`Mutex`]
/// for interior mutability to support async operations that may span
/// multiple threads.
///
/// # Thread Safety
///
/// This struct is thread-safe and can be shared across async `.await` points.
/// Uses [`Mutex`] for interior mutability, which has minimal overhead since
/// contention is unlikely (warnings are infrequent).
///
/// # Lifecycle
///
/// 1. Created at the start of a CLI command (`InstallCommand::execute()`, etc.)
/// 2. Passed down through resolver → extractor → parser call chain
/// 3. Automatically cleaned up when the operation completes
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::core::OperationContext;
/// use std::path::Path;
///
/// // Create context at operation start
/// let ctx = OperationContext::new();
///
/// // Use for warning deduplication
/// if ctx.should_warn_file(Path::new("invalid.md")) {
///     eprintln!("Warning: Invalid file");
/// }
///
/// // Same file won't warn again
/// assert!(!ctx.should_warn_file(Path::new("invalid.md")));
/// ```
#[derive(Debug, Default)]
pub struct OperationContext {
    /// Files that have already emitted warnings during this operation.
    ///
    /// Keys are normalized filenames (not full paths) to ensure consistent
    /// deduplication across different path representations.
    warned_files: Mutex<HashSet<String>>,
}

impl OperationContext {
    /// Create a new operation context.
    ///
    /// Call this at the start of each CLI operation that needs state tracking.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use agpm_cli::core::OperationContext;
    ///
    /// let ctx = OperationContext::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Normalize a path to a deduplication key.
    ///
    /// Uses just the filename (not full path) for consistency across different
    /// path representations (relative paths, worktrees, symlinks, etc.).
    ///
    /// Falls back to the full path string if filename extraction fails.
    fn normalize_key(path: &Path) -> String {
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Use just the filename for consistency across path representations
            filename.to_string()
        } else {
            // Fallback to full path if filename extraction fails
            path.to_string_lossy().to_string()
        }
    }

    /// Check if we should warn about a file and mark it as warned.
    ///
    /// Returns `true` if this is the first warning for this file in this operation,
    /// `false` if we've already warned about it (deduplicated).
    ///
    /// # Deduplication Strategy
    ///
    /// Uses filename-based keys (not full paths) to handle different path
    /// representations consistently:
    /// - `/foo/bar/test.md` and `./bar/test.md` both key on `"test.md"`
    /// - This works across worktrees, relative paths, and symlinks
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file being processed
    ///
    /// # Returns
    ///
    /// * `true` - First warning for this file, caller should display the warning
    /// * `false` - Already warned about this file, caller should skip the warning
    ///
    /// # Examples
    ///
    /// ```rust
    /// use agpm_cli::core::OperationContext;
    /// use std::path::Path;
    ///
    /// let ctx = OperationContext::new();
    /// let path = Path::new("agents/invalid.md");
    ///
    /// // First call returns true
    /// assert!(ctx.should_warn_file(path));
    ///
    /// // Second call returns false (deduplicated)
    /// assert!(!ctx.should_warn_file(path));
    /// ```
    pub fn should_warn_file(&self, path: &Path) -> bool {
        let normalized_key = Self::normalize_key(path);
        let mut warned = self.warned_files.lock().unwrap();
        // insert() returns true if key was newly inserted, false if already present
        warned.insert(normalized_key)
    }

    /// Check if a file has been warned about without modifying state.
    ///
    /// This is primarily useful for testing to verify deduplication behavior.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check
    ///
    /// # Returns
    ///
    /// * `true` - File has been warned about
    /// * `false` - File has not been warned about
    ///
    /// # Examples
    ///
    /// ```rust
    /// use agpm_cli::core::OperationContext;
    /// use std::path::Path;
    ///
    /// let ctx = OperationContext::new();
    /// let path = Path::new("test.md");
    ///
    /// assert!(!ctx.has_warned(path));
    /// ctx.should_warn_file(path);
    /// assert!(ctx.has_warned(path));
    /// ```
    #[cfg(test)]
    pub fn has_warned(&self, path: &Path) -> bool {
        let normalized_key = Self::normalize_key(path);
        self.warned_files.lock().unwrap().contains(&normalized_key)
    }

    /// Get the count of unique files that have been warned about.
    ///
    /// Useful for diagnostics and testing.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use agpm_cli::core::OperationContext;
    /// use std::path::Path;
    ///
    /// let ctx = OperationContext::new();
    ///
    /// assert_eq!(ctx.warning_count(), 0);
    ///
    /// ctx.should_warn_file(Path::new("file1.md"));
    /// ctx.should_warn_file(Path::new("file2.md"));
    ///
    /// assert_eq!(ctx.warning_count(), 2);
    /// ```
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.warned_files.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_new_context_is_empty() {
        let ctx = OperationContext::new();
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn test_should_warn_first_time() {
        let ctx = OperationContext::new();
        let path = PathBuf::from("test.md");

        assert!(ctx.should_warn_file(&path));
        assert!(!ctx.should_warn_file(&path));
    }

    #[test]
    fn test_same_filename_different_paths() {
        let ctx = OperationContext::new();
        let path1 = PathBuf::from("/foo/bar/test.md");
        let path2 = PathBuf::from("/baz/qux/test.md");

        // First path warns
        assert!(ctx.should_warn_file(&path1));

        // Second path with same filename is deduplicated
        assert!(!ctx.should_warn_file(&path2));
    }

    #[test]
    fn test_different_filenames() {
        let ctx = OperationContext::new();
        let path1 = PathBuf::from("file1.md");
        let path2 = PathBuf::from("file2.md");

        assert!(ctx.should_warn_file(&path1));
        assert!(ctx.should_warn_file(&path2));
        assert_eq!(ctx.warning_count(), 2);
    }

    #[test]
    fn test_has_warned() {
        let ctx = OperationContext::new();
        let path = PathBuf::from("test.md");

        assert!(!ctx.has_warned(&path));
        ctx.should_warn_file(&path);
        assert!(ctx.has_warned(&path));
    }

    #[test]
    fn test_warning_count() {
        let ctx = OperationContext::new();

        assert_eq!(ctx.warning_count(), 0);

        ctx.should_warn_file(&PathBuf::from("file1.md"));
        assert_eq!(ctx.warning_count(), 1);

        ctx.should_warn_file(&PathBuf::from("file2.md"));
        assert_eq!(ctx.warning_count(), 2);

        // Duplicate doesn't increase count
        ctx.should_warn_file(&PathBuf::from("file1.md"));
        assert_eq!(ctx.warning_count(), 2);
    }

    #[test]
    fn test_multiple_contexts_are_isolated() {
        let ctx1 = OperationContext::new();
        let ctx2 = OperationContext::new();
        let path = PathBuf::from("test.md");

        // Each context tracks independently
        assert!(ctx1.should_warn_file(&path));
        assert!(ctx2.should_warn_file(&path));

        // Within each context, deduplication works
        assert!(!ctx1.should_warn_file(&path));
        assert!(!ctx2.should_warn_file(&path));
    }

    #[test]
    fn test_path_without_filename() {
        let ctx = OperationContext::new();
        // Path that has no filename component (edge case)
        let path = PathBuf::from("/");

        // Should still work (uses full path as fallback)
        assert!(ctx.should_warn_file(&path));
        assert!(!ctx.should_warn_file(&path));
    }
}
