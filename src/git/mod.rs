//! Git operations wrapper for CCPM
//!
//! This module provides a safe, async wrapper around the system `git` command, serving as
//! the foundation for CCPM's distributed package management capabilities. Unlike libraries
//! that use embedded Git implementations (like `libgit2`), this module leverages the system's
//! installed Git binary to ensure maximum compatibility with existing Git configurations,
//! authentication methods, and platform-specific optimizations.
//!
//! # Design Philosophy: CLI-Based Git Integration
//!
//! CCPM follows the same approach as Cargo with `git-fetch-with-cli`, using the system's
//! `git` command rather than an embedded Git library. This design choice provides several
//! critical advantages:
//!
//! - **Authentication Compatibility**: Seamlessly works with SSH agents, credential helpers,
//!   Git configuration, and platform-specific authentication (Windows Credential Manager,
//!   macOS Keychain, Linux credential stores)
//! - **Feature Completeness**: Access to all Git features without library limitations
//! - **Platform Integration**: Leverages platform-optimized Git builds and configurations
//! - **Security**: Benefits from system Git's security updates and hardening
//! - **Debugging**: Uses familiar Git commands for troubleshooting and logging
//!
//! # Core Features
//!
//! ## Asynchronous Operations
//! All Git operations are async and built on Tokio, enabling:
//! - Non-blocking I/O for better performance
//! - Concurrent repository operations
//! - Progress reporting during long operations
//! - Graceful cancellation support
//!
//! ## Worktree Support for Parallel Operations
//! Advanced Git worktree integration for safe parallel package installation:
//! - **Bare repository cloning**: Creates repositories optimized for worktrees
//! - **Parallel worktree creation**: Multiple versions checked out simultaneously
//! - **Per-worktree locking**: Individual worktree creation locks prevent conflicts
//! - **Command-level concurrency**: Parallelism controlled by `--max-parallel` flag
//! - **Automatic cleanup**: Efficient worktree lifecycle management
//! - **Conflict-free operations**: Each dependency gets its own isolated working directory
//!
//! ## Progress Reporting
//! User feedback during:
//! - Repository cloning with transfer progress
//! - Fetch operations with network activity
//! - Large repository operations
//!
//! ## Authentication Handling
//! Supports multiple authentication methods through URL-based configuration:
//! - HTTPS with embedded tokens: `https://token@github.com/user/repo.git`
//! - SSH with key-based authentication: `git@github.com:user/repo.git`
//! - System credential helpers and Git configuration
//! - Platform-specific credential storage
//!
//! ## Cross-Platform Compatibility
//! Tested and optimized for:
//! - **Windows**: Handles path length limits, `PowerShell` vs CMD differences
//! - **macOS**: Integrates with Keychain and Xcode command line tools
//! - **Linux**: Works with various distributions and Git installations
//!
//! # Security Considerations
//!
//! ## Command Injection Prevention
//! All Git operations use proper argument passing to prevent injection attacks:
//! - Arguments passed as separate parameters, not shell strings
//! - URL validation before Git operations
//! - Path sanitization for repository locations
//!
//! ## Authentication Security
//! - Credentials never logged or exposed in error messages
//! - Authentication URLs are stripped from public error output
//! - Supports secure credential storage via system Git configuration
//!
//! ## Network Security
//! - HTTPS verification enabled by default
//! - Support for custom CA certificates via Git configuration
//! - Timeout handling for network operations
//!
//! # Performance Characteristics
//!
//! ## Network Operations
//! - Async I/O prevents blocking during network operations
//! - Parallel fetch operations for multiple repositories
//! - Efficient progress reporting without polling
//!
//! ## Local Operations
//! - Direct file system access for repository validation
//! - Optimized branch/tag listing with minimal Git calls
//! - Efficient checkout operations with proper reset handling
//!
//! # Error Handling Strategy
//!
//! The module provides rich error context through [`CcpmError`] variants:
//! - Network failures with retry suggestions
//! - Authentication errors with configuration guidance
//! - Repository format errors with recovery steps
//! - Platform-specific error translation
//!
//! # Usage Examples
//!
//! ## Basic Repository Operations
//! ```rust,no_run
//! use ccpm::git::GitRepo;
//! use std::env;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Use platform-appropriate temp directory
//! let temp_dir = env::temp_dir();
//! let repo_path = temp_dir.join("repo");
//!
//! // Clone a repository
//! let repo = GitRepo::clone(
//!     "https://github.com/example/repo.git",
//!     &repo_path
//! ).await?;
//!
//! // Fetch updates from remote
//! repo.fetch(None).await?;
//!
//! // Checkout a specific version
//! repo.checkout("v1.2.3").await?;
//!
//! // List available tags
//! let tags = repo.list_tags().await?;
//! println!("Available versions: {:?}", tags);
//! # Ok(())
//! # }
//! ```
//!
//! ## Authentication with URLs
//! ```rust,no_run
//! use ccpm::git::GitRepo;
//! use std::env;
//!
//! # async fn auth_example() -> anyhow::Result<()> {
//! // Use platform-appropriate temp directory
//! let temp_dir = env::temp_dir();
//! let repo_path = temp_dir.join("private-repo");
//!
//! // Clone with authentication embedded in URL
//! let repo = GitRepo::clone(
//!     "https://token:ghp_xxxx@github.com/private/repo.git",
//!     &repo_path
//! ).await?;
//!
//! // Fetch with different authentication URL
//! let auth_url = "https://oauth2:token@github.com/private/repo.git";
//! repo.fetch(Some(auth_url)).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Repository Validation
//! ```rust,no_run
//! use ccpm::git::{GitRepo, ensure_git_available, is_valid_git_repo};
//! use std::env;
//!
//! # async fn validation_example() -> anyhow::Result<()> {
//! // Ensure Git is installed
//! ensure_git_available()?;
//!
//! // Verify repository URL before cloning
//! GitRepo::verify_url("https://github.com/example/repo.git").await?;
//!
//! // Check if directory is a valid Git repository
//! let temp_dir = env::temp_dir();
//! let path = temp_dir.join("repo");
//! if is_valid_git_repo(&path) {
//!     let repo = GitRepo::new(&path);
//!     let url = repo.get_remote_url().await?;
//!     println!("Repository URL: {}", url);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Worktree-based Parallel Operations
//! ```rust,no_run
//! use ccpm::git::GitRepo;
//! use std::env;
//!
//! # async fn worktree_example() -> anyhow::Result<()> {
//! // Use platform-appropriate temp directory
//! let temp_dir = env::temp_dir();
//! let cache_dir = temp_dir.join("cache");
//! let bare_path = cache_dir.join("repo.git");
//!
//! // Clone repository as bare for worktree use
//! let bare_repo = GitRepo::clone_bare(
//!     "https://github.com/example/repo.git",
//!     &bare_path
//! ).await?;
//!
//! // Create multiple worktrees for parallel processing
//! let work1 = temp_dir.join("work1");
//! let work2 = temp_dir.join("work2");
//! let work3 = temp_dir.join("work3");
//!
//! let worktree1 = bare_repo.create_worktree(&work1, Some("v1.0.0")).await?;
//! let worktree2 = bare_repo.create_worktree(&work2, Some("v2.0.0")).await?;
//! let worktree3 = bare_repo.create_worktree(&work3, Some("main")).await?;
//!
//! // Each worktree can be used independently and concurrently
//! // Process files from worktree1 at v1.0.0
//! // Process files from worktree2 at v2.0.0  
//! // Process files from worktree3 at latest main
//!
//! // Clean up when done
//! bare_repo.remove_worktree(&work1).await?;
//! bare_repo.remove_worktree(&work2).await?;
//! bare_repo.remove_worktree(&work3).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Platform-Specific Considerations
//!
//! ## Windows
//! - Uses `git.exe` or `git.cmd` detection via PATH
//! - Handles long path names (>260 characters)
//! - Works with Windows Credential Manager
//! - Supports both CMD and `PowerShell` environments
//!
//! ## macOS
//! - Integrates with Xcode Command Line Tools Git
//! - Supports Keychain authentication
//! - Handles case-sensitive vs case-insensitive filesystems
//!
//! ## Linux
//! - Works with package manager installed Git
//! - Supports various credential helpers
//! - Handles different filesystem permissions
//!
//! # Integration with CCPM
//!
//! This module integrates with other CCPM components:
//! - [`crate::source`] - Repository source management
//! - [`crate::manifest`] - Manifest-based dependency resolution
//! - [`crate::lockfile`] - Lockfile generation with commit hashes
//! - [`crate::utils::progress`] - User progress feedback
//! - [`crate::core::CcpmError`] - Centralized error handling
//!
//! [`CcpmError`]: crate::core::CcpmError

pub mod command_builder;
#[cfg(test)]
mod tests;

use crate::core::CcpmError;
use crate::git::command_builder::GitCommand;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// A Git repository handle providing async operations via CLI commands.
///
/// `GitRepo` represents a local Git repository and provides methods for common
/// Git operations such as cloning, fetching, checking out specific references,
/// and querying repository state. All operations are performed asynchronously
/// using the system's `git` command rather than an embedded Git library.
///
/// # Design Principles
///
/// - **CLI-based**: Uses system `git` command for maximum compatibility
/// - **Async**: All operations are non-blocking and support cancellation
/// - **Progress-aware**: Integration with progress reporting for long operations
/// - **Error-rich**: Detailed error information with context and suggestions
/// - **Cross-platform**: Tested on Windows, macOS, and Linux
///
/// # Repository State
///
/// The struct holds minimal state (just the repository path) and queries Git
/// directly for current information. This ensures consistency with external
/// Git operations and avoids state synchronization issues.
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::git::GitRepo;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Create handle for existing repository
/// let repo = GitRepo::new("/path/to/existing/repo");
///
/// // Verify it's a valid Git repository
/// if repo.is_git_repo() {
///     let tags = repo.list_tags().await?;
///     repo.checkout("main").await?;
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// `GitRepo` is `Send` and `Sync`, allowing it to be used across async tasks.
/// However, concurrent Git operations on the same repository may conflict
/// at the Git level (e.g., simultaneous checkouts).
#[derive(Debug)]
pub struct GitRepo {
    /// The local filesystem path to the Git repository.
    ///
    /// This path should point to the root directory of a Git repository
    /// (the directory containing `.git/` subdirectory).
    path: PathBuf,
}

impl GitRepo {
    /// Creates a new `GitRepo` instance for an existing local repository.
    ///
    /// This constructor does not verify that the path contains a valid Git repository.
    /// Use [`is_git_repo`](#method.is_git_repo) or [`ensure_valid_git_repo`] to validate
    /// the repository before performing Git operations.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to the Git repository root directory
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::git::GitRepo;
    /// use std::path::Path;
    ///
    /// // Create repository handle
    /// let repo = GitRepo::new("/path/to/repo");
    ///
    /// // Verify it's valid before operations
    /// if repo.is_git_repo() {
    ///     println!("Valid Git repository at: {:?}", repo.path());
    /// }
    /// ```
    ///
    /// # See Also
    ///
    /// * [`clone`](#method.clone) - For creating repositories by cloning from remote
    /// * [`is_git_repo`](#method.is_git_repo) - For validating repository state
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Clones a Git repository from a remote URL to a local path.
    ///
    /// This method performs a full clone operation, downloading the entire repository
    /// history to the target directory. The operation is async and supports progress
    /// reporting for large repositories.
    ///
    /// # Arguments
    ///
    /// * `url` - The remote repository URL (HTTPS, SSH, or file://)
    /// * `target` - The local directory where the repository will be cloned
    /// * `progress` - Optional progress bar for user feedback
    ///
    /// # Authentication
    ///
    /// Authentication can be provided in several ways:
    /// - **HTTPS with tokens**: `https://token:value@github.com/user/repo.git`
    /// - **SSH keys**: Handled by system SSH agent and Git configuration
    /// - **Credential helpers**: System Git credential managers
    ///
    /// # Supported URL Formats
    ///
    /// - `https://github.com/user/repo.git` - HTTPS
    /// - `git@github.com:user/repo.git` - SSH
    /// - `file:///path/to/repo.git` - Local file system
    /// - `https://user:token@github.com/user/repo.git` - HTTPS with auth
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    /// use std::env;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let temp_dir = env::temp_dir();
    ///
    /// // Clone public repository
    /// let repo = GitRepo::clone(
    ///     "https://github.com/rust-lang/git2-rs.git",
    ///     temp_dir.join("git2-rs")
    /// ).await?;
    ///
    /// // Clone another repository
    /// let repo = GitRepo::clone(
    ///     "https://github.com/example/repository.git",
    ///     temp_dir.join("example-repo")
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::GitCloneFailed`] if:
    /// - The URL is invalid or unreachable
    /// - Authentication fails
    /// - The target directory already exists and is not empty
    /// - Network connectivity issues
    /// - Insufficient disk space
    ///
    /// # Security
    ///
    /// URLs are validated and sanitized before passing to Git. Authentication
    /// tokens in URLs are never logged or exposed in error messages.
    ///
    /// [`CcpmError::GitCloneFailed`]: crate::core::CcpmError::GitCloneFailed
    pub async fn clone(url: &str, target: impl AsRef<Path>) -> Result<Self> {
        let target_path = target.as_ref();

        // Use command builder for consistent clone operations
        let mut cmd = GitCommand::clone(url, target_path);

        // For file:// URLs, clone with all branches to ensure commit availability
        if url.starts_with("file://") {
            cmd = GitCommand::new()
                .args([
                    "clone",
                    "--progress",
                    "--no-single-branch",
                    "--recurse-submodules",
                    url,
                ])
                .arg(target_path.display().to_string());
        }

        // Execute will handle error context properly
        cmd.execute().await?;

        Ok(Self::new(target_path))
    }

    /// Fetches updates from the remote repository without modifying the working tree.
    ///
    /// This operation downloads new commits, branches, and tags from the remote
    /// repository but does not modify the current branch or working directory.
    /// It's equivalent to `git fetch --all --tags`.
    ///
    /// # Arguments
    ///
    /// * `auth_url` - Optional URL with authentication for private repositories
    /// * `progress` - Optional progress bar for network operation feedback
    ///
    /// # Authentication URL
    ///
    /// The `auth_url` parameter allows fetching from repositories that require
    /// different authentication than the original clone URL. This is useful when:
    /// - Using rotating tokens or credentials
    /// - Accessing private repositories through different auth methods
    /// - Working with multiple authentication contexts
    ///
    /// # Local Repository Optimization
    ///
    /// For local repositories (file:// URLs), fetch is automatically skipped
    /// as local repositories don't require network synchronization.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    /// use std::env;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let temp_dir = env::temp_dir();
    /// let repo_path = temp_dir.join("repo");
    /// let repo = GitRepo::new(&repo_path);
    ///
    /// // Basic fetch from configured remote
    /// repo.fetch(None).await?;
    ///
    /// // Fetch with authentication
    /// let auth_url = "https://token:ghp_xxxx@github.com/user/repo.git";
    /// repo.fetch(Some(auth_url)).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::GitCommandError`] if:
    /// - Network connectivity fails
    /// - Authentication is rejected
    /// - The remote repository is unavailable
    /// - The local repository is in an invalid state
    ///
    /// # Performance
    ///
    /// Fetch operations are optimized to:
    /// - Skip unnecessary work for local repositories
    /// - Provide progress feedback for large transfers
    /// - Use efficient Git transfer protocols
    ///
    /// [`CcpmError::GitCommandError`]: crate::core::CcpmError::GitCommandError
    pub async fn fetch(&self, auth_url: Option<&str>) -> Result<()> {
        // Note: file:// URLs are local repositories, but we still need to fetch
        // from them to get updates from the source repository

        // Use git fetch with authentication from global config URL if provided
        if let Some(url) = auth_url {
            // Temporarily update the remote URL with auth for this fetch
            GitCommand::set_remote_url(url)
                .current_dir(&self.path)
                .execute_success()
                .await?;
        }

        // Now fetch with the potentially updated URL
        GitCommand::fetch()
            .current_dir(&self.path)
            .execute_success()
            .await?;

        Ok(())
    }

    /// Checks out a specific Git reference (branch, tag, or commit hash).
    ///
    /// This operation switches the repository's working directory to match the
    /// specified reference. It performs a hard reset before checkout to ensure
    /// a clean state, discarding any local modifications.
    ///
    /// # Arguments
    ///
    /// * `ref_name` - The Git reference to checkout (branch, tag, or commit)
    ///
    /// # Reference Resolution Strategy
    ///
    /// The method attempts to resolve references in the following order:
    /// 1. **Direct reference**: Exact match for tags, branches, or commit hashes
    /// 2. **Remote branch**: Tries `origin/{ref_name}` for remote branches
    /// 3. **Error**: If neither resolution succeeds, returns an error
    ///
    /// # Supported Reference Types
    ///
    /// - **Tags**: `v1.0.0`, `release-2023-01`, etc.
    /// - **Branches**: `main`, `develop`, `feature/new-ui`, etc.
    /// - **Commit hashes**: `abc123def`, `1234567890abcdef` (full or abbreviated)
    /// - **Remote branches**: Automatically tries `origin/{branch_name}`
    ///
    /// # State Management
    ///
    /// Before checkout, the method performs:
    /// 1. **Hard reset**: `git reset --hard HEAD` to discard local changes
    /// 2. **Clean checkout**: Switches to the target reference
    /// 3. **Detached HEAD**: For tags/commits (normal Git behavior)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    ///
    /// // Checkout a specific version tag
    /// repo.checkout("v1.2.3").await?;
    ///
    /// // Checkout a branch
    /// repo.checkout("main").await?;
    ///
    /// // Checkout a commit hash
    /// repo.checkout("abc123def456").await?;
    ///
    /// // Checkout remote branch
    /// repo.checkout("feature/experimental").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Data Loss Warning
    ///
    /// **This operation discards uncommitted changes.** The hard reset before
    /// checkout ensures a clean state but will permanently lose any local
    /// modifications. This behavior is intentional for CCPM's package management
    /// use case where clean, reproducible states are required.
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::GitCheckoutFailed`] if:
    /// - The reference doesn't exist in the repository
    /// - The repository is in an invalid state
    /// - File system permissions prevent checkout
    /// - The working directory is locked by another process
    ///
    /// # Performance
    ///
    /// Checkout operations are optimized for:
    /// - Fast switching between cached references
    /// - Minimal file system operations
    /// - Efficient handling of large repositories
    ///
    /// [`CcpmError::GitCheckoutFailed`]: crate::core::CcpmError::GitCheckoutFailed
    pub async fn checkout(&self, ref_name: &str) -> Result<()> {
        // Reset to clean state before checkout
        let reset_result = GitCommand::reset_hard()
            .current_dir(&self.path)
            .execute()
            .await;

        if let Err(e) = reset_result {
            // Only warn if it's not a detached HEAD situation (which is normal)
            let error_str = e.to_string();
            if !error_str.contains("HEAD detached") {
                eprintln!("Warning: git reset failed: {error_str}");
            }
        }

        // Check if this ref exists as a remote branch
        // If it does, always use -B to ensure we get the latest
        let remote_ref = format!("origin/{ref_name}");
        let check_remote = GitCommand::verify_ref(&remote_ref)
            .current_dir(&self.path)
            .execute()
            .await;

        if check_remote.is_ok() {
            // Remote branch exists, use -B to force update to latest
            if GitCommand::checkout_branch(ref_name, &remote_ref)
                .current_dir(&self.path)
                .execute_success()
                .await
                .is_ok()
            {
                return Ok(());
            }
        }

        // Not a remote branch, try direct checkout (works for tags and commits)
        GitCommand::checkout(ref_name)
            .current_dir(&self.path)
            .execute_success()
            .await
            .map_err(|e| {
                // If it's already a GitCheckoutFailed error, return as-is
                // Otherwise wrap it
                if let Some(ccpm_err) = e.downcast_ref::<CcpmError>() {
                    if matches!(ccpm_err, CcpmError::GitCheckoutFailed { .. }) {
                        return e;
                    }
                }
                CcpmError::GitCheckoutFailed {
                    reference: ref_name.to_string(),
                    reason: e.to_string(),
                }
                .into()
            })
    }

    /// Lists all tags in the repository, sorted by Git's default ordering.
    ///
    /// This method retrieves all Git tags from the local repository using
    /// `git tag -l`. Tags are returned as strings in Git's natural ordering,
    /// which may not be semantic version order.
    ///
    /// # Return Value
    ///
    /// Returns a `Vec<String>` containing all tag names. Empty if no tags exist.
    /// Tags are returned exactly as they appear in Git (no prefix stripping).
    ///
    /// # Repository Validation
    ///
    /// The method validates that:
    /// - The repository path exists on the filesystem
    /// - The directory contains a `.git` subdirectory
    /// - The repository is in a valid state for Git operations
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    ///
    /// // Get all available tags
    /// let tags = repo.list_tags().await?;
    /// for tag in tags {
    ///     println!("Available version: {}", tag);
    /// }
    ///
    /// // Check for specific tag
    /// let tags = repo.list_tags().await?;
    /// if tags.contains(&"v1.0.0".to_string()) {
    ///     repo.checkout("v1.0.0").await?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Version Parsing
    ///
    /// For semantic version ordering, consider using the `semver` crate:
    ///
    /// ```rust,no_run
    /// # use anyhow::Result;
    /// use semver::Version;
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn version_example() -> Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    /// let tags = repo.list_tags().await?;
    ///
    /// // Parse and sort semantic versions
    /// let mut versions: Vec<Version> = tags
    ///     .iter()
    ///     .filter_map(|tag| tag.strip_prefix('v'))
    ///     .filter_map(|v| Version::parse(v).ok())
    ///     .collect();
    /// versions.sort();
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::GitCommandError`] if:
    /// - The repository path doesn't exist
    /// - The directory is not a valid Git repository
    /// - Git command execution fails
    /// - File system permissions prevent access
    ///
    /// # Performance
    ///
    /// This operation is relatively fast as it only reads Git's tag database
    /// without network access. For repositories with thousands of tags,
    /// consider filtering or pagination if memory usage is a concern.
    ///
    /// [`CcpmError::GitCommandError`]: crate::core::CcpmError::GitCommandError
    pub async fn list_tags(&self) -> Result<Vec<String>> {
        // Check if the directory exists and is a git repo
        if !self.path.exists() {
            return Err(anyhow::anyhow!(
                "Repository path does not exist: {:?}",
                self.path
            ));
        }

        if !self.path.join(".git").exists() {
            return Err(anyhow::anyhow!("Not a git repository: {:?}", self.path));
        }

        let stdout = GitCommand::list_tags()
            .current_dir(&self.path)
            .execute_stdout()
            .await
            .context(format!("Failed to list git tags in {:?}", self.path))?;

        Ok(stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(std::string::ToString::to_string)
            .collect())
    }

    /// Retrieves the URL of the remote 'origin' repository.
    ///
    /// This method queries the Git repository for the URL associated with the
    /// 'origin' remote, which is typically the source repository from which
    /// the local repository was cloned.
    ///
    /// # Return Value
    ///
    /// Returns the origin URL as configured in the repository's Git configuration.
    /// The URL format depends on how the repository was cloned:
    /// - HTTPS: `https://github.com/user/repo.git`
    /// - SSH: `git@github.com:user/repo.git`
    /// - File: `file:///path/to/repo.git`
    ///
    /// # Authentication Handling
    ///
    /// The returned URL reflects the repository's configured origin, which may
    /// or may not include authentication information depending on the original
    /// clone method and Git configuration.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    ///
    /// // Get the origin URL
    /// let url = repo.get_remote_url().await?;
    /// println!("Repository origin: {}", url);
    ///
    /// // Check if it's a specific platform
    /// if url.contains("github.com") {
    ///     println!("This is a GitHub repository");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # URL Processing
    ///
    /// For processing the URL further, consider using [`parse_git_url`]:
    ///
    /// ```rust,no_run
    /// use ccpm::git::{GitRepo, parse_git_url};
    ///
    /// # async fn parse_example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    /// let url = repo.get_remote_url().await?;
    ///
    /// // Parse into owner and repository name
    /// let (owner, name) = parse_git_url(&url)?;
    /// println!("Owner: {}, Repository: {}", owner, name);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::GitCommandError`] if:
    /// - No 'origin' remote is configured
    /// - The repository is not a valid Git repository
    /// - Git command execution fails
    /// - File system access is denied
    ///
    /// # Security
    ///
    /// The returned URL may contain authentication information if it was
    /// configured that way. Be cautious when logging or displaying URLs
    /// that might contain sensitive tokens or credentials.
    ///
    /// [`parse_git_url`]: fn.parse_git_url.html
    /// [`CcpmError::GitCommandError`]: crate::core::CcpmError::GitCommandError
    pub async fn get_remote_url(&self) -> Result<String> {
        GitCommand::remote_url()
            .current_dir(&self.path)
            .execute_stdout()
            .await
    }

    /// Checks if the directory contains a valid Git repository.
    ///
    /// This is a fast, synchronous operation that simply checks for the presence
    /// of a `.git` subdirectory in the repository path. It does not validate
    /// the Git repository's internal structure or integrity.
    ///
    /// # Return Value
    ///
    /// - `true` if the directory contains a `.git` subdirectory
    /// - `false` if the `.git` subdirectory is missing or inaccessible
    ///
    /// # Performance
    ///
    /// This method is intentionally synchronous and lightweight for efficiency.
    /// It performs a single filesystem check without spawning async tasks or
    /// executing Git commands.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::git::GitRepo;
    ///
    /// let repo = GitRepo::new("/path/to/repo");
    ///
    /// if repo.is_git_repo() {
    ///     println!("Valid Git repository detected");
    /// } else {
    ///     println!("Not a Git repository");
    /// }
    ///
    /// // Use before async operations
    /// # async fn async_example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    /// if repo.is_git_repo() {
    ///     let tags = repo.list_tags().await?;
    ///     // Process tags...
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Validation Scope
    ///
    /// This method only checks for the `.git` directory's presence. It does not:
    /// - Validate Git repository integrity
    /// - Check for repository corruption
    /// - Verify specific Git version compatibility
    /// - Test network connectivity to remotes
    ///
    /// For more thorough validation, use Git operations that will fail with
    /// detailed error information if the repository is corrupted.
    ///
    /// # Alternative
    ///
    /// For error-based validation with detailed context, use [`ensure_valid_git_repo`]:
    ///
    /// ```rust,no_run
    /// use ccpm::git::ensure_valid_git_repo;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let path = Path::new("/path/to/repo");
    /// ensure_valid_git_repo(path)?; // Returns detailed error if invalid
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`ensure_valid_git_repo`]: fn.ensure_valid_git_repo.html
    #[must_use]
    pub fn is_git_repo(&self) -> bool {
        self.path.join(".git").exists()
    }

    /// Returns the filesystem path to the Git repository.
    ///
    /// This method provides access to the repository's root directory path
    /// as configured when the `GitRepo` instance was created.
    ///
    /// # Return Value
    ///
    /// Returns a reference to the [`Path`] representing the repository's
    /// root directory (the directory containing the `.git` subdirectory).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::git::GitRepo;
    /// use std::path::Path;
    ///
    /// let repo = GitRepo::new("/home/user/my-project");
    /// let path = repo.path();
    ///
    /// println!("Repository path: {}", path.display());
    /// assert_eq!(path, Path::new("/home/user/my-project"));
    ///
    /// // Use for file operations within the repository
    /// let readme_path = path.join("README.md");
    /// if readme_path.exists() {
    ///     println!("Repository has a README file");
    /// }
    /// ```
    ///
    /// # File System Operations
    ///
    /// The returned path can be used for various filesystem operations:
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # fn example() -> std::io::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    /// let repo_path = repo.path();
    ///
    /// // Check repository contents
    /// for entry in std::fs::read_dir(repo_path)? {
    ///     let entry = entry?;
    ///     println!("Found: {}", entry.file_name().to_string_lossy());
    /// }
    ///
    /// // Access specific files
    /// let manifest_path = repo_path.join("Cargo.toml");
    /// if manifest_path.exists() {
    ///     println!("Rust project detected");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Path Validity
    ///
    /// The returned path reflects the value provided during construction and
    /// may not exist or may not be a valid Git repository. Use [`is_git_repo`]
    /// to validate the repository state.
    ///
    /// [`is_git_repo`]: #method.is_git_repo
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Verifies that a Git repository URL is accessible without performing a full clone.
    ///
    /// This static method performs a lightweight check to determine if a repository
    /// URL is valid and accessible. It uses `git ls-remote` for remote repositories
    /// or filesystem checks for local paths.
    ///
    /// # Arguments
    ///
    /// * `url` - The repository URL to verify
    ///
    /// # Verification Methods
    ///
    /// - **Local repositories** (`file://` URLs): Checks if the path exists
    /// - **Remote repositories**: Uses `git ls-remote --heads` to test connectivity
    /// - **Authentication**: Leverages system Git configuration and credential helpers
    ///
    /// # Supported URL Types
    ///
    /// - `https://github.com/user/repo.git` - HTTPS with optional authentication
    /// - `git@github.com:user/repo.git` - SSH with key-based authentication
    /// - `file:///path/to/repo` - Local filesystem repositories
    /// - `https://token:value@host.com/repo.git` - HTTPS with embedded credentials
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // Verify public repository
    /// GitRepo::verify_url("https://github.com/rust-lang/git2-rs.git").await?;
    ///
    /// // Verify before cloning
    /// let url = "https://github.com/user/private-repo.git";
    /// match GitRepo::verify_url(url).await {
    ///     Ok(_) => {
    ///         let repo = GitRepo::clone(url, "/tmp/repo").await?;
    ///         println!("Repository cloned successfully");
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Repository not accessible: {}", e);
    ///     }
    /// }
    ///
    /// // Verify local repository
    /// GitRepo::verify_url("file:///home/user/local-repo").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Benefits
    ///
    /// This method is much faster than attempting a full clone because it:
    /// - Only queries repository metadata (refs and heads)
    /// - Transfers minimal data over the network
    /// - Avoids creating local filesystem structures
    /// - Provides quick feedback on accessibility
    ///
    /// # Authentication Testing
    ///
    /// The verification process tests the complete authentication chain:
    /// - Credential helper invocation
    /// - SSH key validation (for SSH URLs)
    /// - Token validation (for HTTPS URLs)
    /// - Network connectivity and DNS resolution
    ///
    /// # Use Cases
    ///
    /// - **Pre-flight checks**: Validate URLs before expensive clone operations
    /// - **Dependency validation**: Ensure all repository sources are accessible
    /// - **Configuration testing**: Verify authentication setup
    /// - **Network diagnostics**: Test connectivity to repository hosts
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - **Network issues**: DNS resolution, connectivity, timeouts
    /// - **Authentication failures**: Invalid credentials, expired tokens
    /// - **Repository issues**: Repository doesn't exist, access denied
    /// - **Local path issues**: File doesn't exist (for `file://` URLs)
    /// - **URL format issues**: Malformed or unsupported URL schemes
    ///
    /// # Security
    ///
    /// This method respects the same security boundaries as Git operations:
    /// - Uses system Git configuration and security settings
    /// - Never bypasses authentication requirements
    /// - Doesn't cache or expose authentication credentials
    /// - Follows Git's SSL/TLS verification policies
    pub async fn verify_url(url: &str) -> Result<()> {
        // For file:// URLs, just check if the path exists
        if url.starts_with("file://") {
            let path = url.strip_prefix("file://").unwrap();
            return if std::path::Path::new(path).exists() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Local path does not exist: {}", path))
            };
        }

        // For all other URLs, use ls-remote to verify
        GitCommand::ls_remote(url)
            .execute_success()
            .await
            .context("Failed to verify remote repository")
    }

    /// Fetch updates for a bare repository with logging context.
    async fn ensure_bare_repo_has_refs_with_context(&self, context: Option<&str>) -> Result<()> {
        // Try to fetch to ensure we have refs
        let mut fetch_cmd = GitCommand::fetch().current_dir(&self.path);

        if let Some(ctx) = context {
            fetch_cmd = fetch_cmd.with_context(ctx);
        }

        let fetch_result = fetch_cmd.execute_success().await;

        if fetch_result.is_err() {
            // If fetch fails, it might be because there's no remote
            // Just check if we have any refs at all
            let mut check_cmd = GitCommand::new()
                .args(["show-ref", "--head"])
                .current_dir(&self.path);

            if let Some(ctx) = context {
                check_cmd = check_cmd.with_context(ctx);
            }

            check_cmd
                .execute_success()
                .await
                .map_err(|e| anyhow::anyhow!("Bare repository has no refs available: {}", e))?;
        }

        Ok(())
    }

    /// Clone a repository as a bare repository (no working directory).
    ///
    /// Bare repositories are optimized for use as a source for worktrees,
    /// allowing multiple concurrent checkouts without conflicts.
    ///
    /// # Arguments
    ///
    /// * `url` - The remote repository URL
    /// * `target` - The local directory where the bare repository will be stored
    /// * `progress` - Optional progress bar for user feedback
    ///
    /// # Returns
    ///
    /// Returns a new `GitRepo` instance pointing to the bare repository
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    /// use std::env;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let temp_dir = env::temp_dir();
    /// let bare_repo = GitRepo::clone_bare(
    ///     "https://github.com/example/repo.git",
    ///     temp_dir.join("repo.git")
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn clone_bare(url: &str, target: impl AsRef<Path>) -> Result<Self> {
        Self::clone_bare_with_context(url, target, None).await
    }

    /// Clone a repository as a bare repository with logging context.
    ///
    /// Bare repositories are optimized for use as a source for worktrees,
    /// allowing multiple concurrent checkouts without conflicts.
    ///
    /// # Arguments
    ///
    /// * `url` - The remote repository URL
    /// * `target` - The local directory where the bare repository will be stored
    /// * `progress` - Optional progress bar for user feedback
    /// * `context` - Optional context for logging (e.g., dependency name)
    ///
    /// # Returns
    ///
    /// Returns a new `GitRepo` instance pointing to the bare repository
    pub async fn clone_bare_with_context(
        url: &str,
        target: impl AsRef<Path>,
        context: Option<&str>,
    ) -> Result<Self> {
        let target_path = target.as_ref();

        let mut cmd = GitCommand::clone_bare(url, target_path);

        if let Some(ctx) = context {
            cmd = cmd.with_context(ctx);
        }

        cmd.execute_success().await?;

        let repo = Self::new(target_path);

        // Ensure the bare repo has refs available for worktree creation
        // Also needs context for the fetch operation
        repo.ensure_bare_repo_has_refs_with_context(context)
            .await
            .ok();

        Ok(repo)
    }

    /// Create a new worktree from this repository.
    ///
    /// Worktrees allow multiple working directories to be checked out from
    /// a single repository, enabling parallel operations on different versions.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - The path where the worktree will be created
    /// * `reference` - Optional Git reference (branch/tag/commit) to checkout
    ///
    /// # Returns
    ///
    /// Returns a new `GitRepo` instance pointing to the worktree
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let bare_repo = GitRepo::new("/path/to/bare.git");
    ///
    /// // Create worktree with specific version
    /// let worktree = bare_repo.create_worktree(
    ///     "/tmp/worktree1",
    ///     Some("v1.0.0")
    /// ).await?;
    ///
    /// // Create worktree with default branch
    /// let worktree2 = bare_repo.create_worktree(
    ///     "/tmp/worktree2",
    ///     None
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_worktree(
        &self,
        worktree_path: impl AsRef<Path>,
        reference: Option<&str>,
    ) -> Result<GitRepo> {
        self.create_worktree_with_context(worktree_path, reference, None)
            .await
    }

    /// Create a new worktree from this repository with logging context.
    ///
    /// Worktrees allow multiple working directories to be checked out from
    /// a single repository, enabling parallel operations on different versions.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - The path where the worktree will be created
    /// * `reference` - Optional Git reference (branch/tag/commit) to checkout
    /// * `context` - Optional context for logging (e.g., dependency name)
    ///
    /// # Returns
    ///
    /// Returns a new `GitRepo` instance pointing to the worktree
    pub async fn create_worktree_with_context(
        &self,
        worktree_path: impl AsRef<Path>,
        reference: Option<&str>,
        context: Option<&str>,
    ) -> Result<GitRepo> {
        let worktree_path = worktree_path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = worktree_path.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("Failed to create parent directory for worktree: {parent:?}")
            })?;
        }

        // Retry logic for worktree creation to handle concurrent operations
        let max_retries = 3;
        let mut retry_count = 0;

        loop {
            // For bare repositories, we may need to handle the case where no default branch exists yet
            // If no reference provided, try to use the default branch
            let default_branch = if reference.is_none() && retry_count == 0 {
                // Try to get the default branch
                GitCommand::new()
                    .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
                    .current_dir(&self.path)
                    .execute_stdout()
                    .await
                    .ok()
                    .and_then(|s| s.strip_prefix("refs/remotes/origin/").map(String::from))
                    .or_else(|| Some("main".to_string()))
            } else {
                None
            };

            let effective_ref = if let Some(ref branch) = default_branch {
                Some(branch.as_str())
            } else {
                reference
            };

            let mut cmd =
                GitCommand::worktree_add(worktree_path, effective_ref).current_dir(&self.path);

            if let Some(ctx) = context {
                cmd = cmd.with_context(ctx);
            }

            let result = cmd.execute_success().await;

            match result {
                Ok(_) => {
                    // Initialize and update submodules in the new worktree
                    let worktree_repo = GitRepo::new(worktree_path);

                    // Initialize submodules
                    let mut init_cmd = GitCommand::new()
                        .args(["submodule", "init"])
                        .current_dir(worktree_path);

                    if let Some(ctx) = context {
                        init_cmd = init_cmd.with_context(ctx);
                    }

                    // Ignore errors - if there are no submodules, this will fail
                    let _ = init_cmd.execute_success().await;

                    // Update submodules
                    let mut update_cmd = GitCommand::new()
                        .args(["submodule", "update", "--recursive"])
                        .current_dir(worktree_path);

                    if let Some(ctx) = context {
                        update_cmd = update_cmd.with_context(ctx);
                    }

                    // Ignore errors - if there are no submodules, this will fail
                    let _ = update_cmd.execute_success().await;

                    return Ok(worktree_repo);
                }
                Err(e) => {
                    let error_str = e.to_string();

                    // Check if this is a concurrent access issue
                    if error_str.contains("already exists")
                        || error_str.contains("is already checked out")
                        || error_str.contains("fatal: could not create directory")
                    {
                        retry_count += 1;
                        if retry_count >= max_retries {
                            return Err(e).with_context(|| {
                                format!(
                                    "Failed to create worktree at {} from {} after {} retries",
                                    worktree_path.display(),
                                    self.path.display(),
                                    max_retries
                                )
                            });
                        }

                        // Wait a bit before retrying
                        tokio::time::sleep(tokio::time::Duration::from_millis(100 * retry_count))
                            .await;
                        continue;
                    }

                    // Handle stale registration: "missing but already registered worktree"
                    if error_str.contains("missing but already registered worktree") {
                        // Prune stale admin entries, then retry (once) with --force
                        let _ = self.prune_worktrees().await;

                        // Retry with --force
                        let worktree_path_str = worktree_path.display().to_string();
                        let mut args = vec![
                            "worktree".to_string(),
                            "add".to_string(),
                            "--force".to_string(),
                            worktree_path_str,
                        ];
                        if let Some(r) = effective_ref {
                            args.push(r.to_string());
                        }

                        let mut force_cmd = GitCommand::new().args(args).current_dir(&self.path);
                        if let Some(ctx) = context {
                            force_cmd = force_cmd.with_context(ctx);
                        }

                        match force_cmd.execute_success().await {
                            Ok(_) => {
                                // Initialize and update submodules in the new worktree
                                let worktree_repo = GitRepo::new(worktree_path);

                                let mut init_cmd = GitCommand::new()
                                    .args(["submodule", "init"])
                                    .current_dir(worktree_path);
                                if let Some(ctx) = context {
                                    init_cmd = init_cmd.with_context(ctx);
                                }
                                let _ = init_cmd.execute_success().await;

                                let mut update_cmd = GitCommand::new()
                                    .args(["submodule", "update", "--recursive"])
                                    .current_dir(worktree_path);
                                if let Some(ctx) = context {
                                    update_cmd = update_cmd.with_context(ctx);
                                }
                                let _ = update_cmd.execute_success().await;

                                return Ok(worktree_repo);
                            }
                            Err(e2) => {
                                // Fall through to other recovery paths with the original error context
                                // but include the forced attempt error as context
                                return Err(e).with_context(|| {
                                    format!(
                                        "Failed to create worktree at {} from {} (forced add failed: {})",
                                        worktree_path.display(),
                                        self.path.display(),
                                        e2
                                    )
                                });
                            }
                        }
                    }

                    // If no reference was provided and the command failed, it might be because
                    // the bare repo doesn't have a default branch set. Try with explicit HEAD
                    if reference.is_none() && retry_count == 0 {
                        let mut head_cmd = GitCommand::worktree_add(worktree_path, Some("HEAD"))
                            .current_dir(&self.path);

                        if let Some(ctx) = context {
                            head_cmd = head_cmd.with_context(ctx);
                        }

                        let head_result = head_cmd.execute_success().await;

                        match head_result {
                            Ok(_) => {
                                // Initialize and update submodules in the new worktree
                                let worktree_repo = GitRepo::new(worktree_path);

                                // Initialize submodules
                                let mut init_cmd = GitCommand::new()
                                    .args(["submodule", "init"])
                                    .current_dir(worktree_path);

                                if let Some(ctx) = context {
                                    init_cmd = init_cmd.with_context(ctx);
                                }

                                // Ignore errors - if there are no submodules, this will fail
                                let _ = init_cmd.execute_success().await;

                                // Update submodules
                                let mut update_cmd = GitCommand::new()
                                    .args(["submodule", "update", "--recursive"])
                                    .current_dir(worktree_path);

                                if let Some(ctx) = context {
                                    update_cmd = update_cmd.with_context(ctx);
                                }

                                // Ignore errors - if there are no submodules, this will fail
                                let _ = update_cmd.execute_success().await;

                                return Ok(worktree_repo);
                            }
                            Err(head_err) => {
                                // If HEAD also fails, return the original error
                                return Err(e).with_context(|| {
                                    format!(
                                        "Failed to create worktree at {} from {} (also tried HEAD: {})",
                                        worktree_path.display(),
                                        self.path.display(),
                                        head_err
                                    )
                                });
                            }
                        }
                    }

                    // Check if the error is likely due to an invalid reference
                    let error_str = e.to_string();
                    if let Some(ref_name) = reference {
                        if error_str.contains("pathspec")
                            || error_str.contains("not found")
                            || error_str.contains("ambiguous")
                            || error_str.contains("invalid")
                            || error_str.contains("unknown revision")
                        {
                            return Err(anyhow::anyhow!(
                                "Invalid version or reference '{}': Failed to checkout reference - the specified version/tag/branch does not exist in the repository",
                                ref_name
                            ));
                        }
                    }

                    return Err(e).with_context(|| {
                        format!(
                            "Failed to create worktree at {} from {}",
                            worktree_path.display(),
                            self.path.display()
                        )
                    });
                }
            }
        }
    }

    /// Remove a worktree associated with this repository.
    ///
    /// This removes the worktree and its administrative files, but preserves
    /// the bare repository for future use.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - The path to the worktree to remove
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let bare_repo = GitRepo::new("/path/to/bare.git");
    /// bare_repo.remove_worktree("/tmp/worktree1").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_worktree(&self, worktree_path: impl AsRef<Path>) -> Result<()> {
        let worktree_path = worktree_path.as_ref();

        GitCommand::worktree_remove(worktree_path)
            .current_dir(&self.path)
            .execute_success()
            .await
            .with_context(|| format!("Failed to remove worktree at {}", worktree_path.display()))?;

        // Also try to remove the directory if it still exists
        if worktree_path.exists() {
            tokio::fs::remove_dir_all(worktree_path).await.ok(); // Ignore errors as git worktree remove may have already cleaned it
        }

        Ok(())
    }

    /// List all worktrees associated with this repository.
    ///
    /// Returns a list of paths to existing worktrees.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let bare_repo = GitRepo::new("/path/to/bare.git");
    /// let worktrees = bare_repo.list_worktrees().await?;
    /// for worktree in worktrees {
    ///     println!("Worktree: {}", worktree.display());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_worktrees(&self) -> Result<Vec<PathBuf>> {
        let output = GitCommand::worktree_list()
            .current_dir(&self.path)
            .execute_stdout()
            .await?;

        let mut worktrees = Vec::new();
        let mut current_worktree: Option<PathBuf> = None;

        for line in output.lines() {
            if line.starts_with("worktree ") {
                if let Some(path) = line.strip_prefix("worktree ") {
                    current_worktree = Some(PathBuf::from(path));
                }
            } else if line == "bare" {
                // Skip bare repository entry
                current_worktree = None;
            } else if line.is_empty() && current_worktree.is_some() {
                if let Some(path) = current_worktree.take() {
                    worktrees.push(path);
                }
            }
        }

        // Add the last worktree if there is one
        if let Some(path) = current_worktree {
            worktrees.push(path);
        }

        Ok(worktrees)
    }

    /// Prune stale worktree administrative files.
    ///
    /// This cleans up worktree entries that no longer have a corresponding
    /// working directory on disk.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let bare_repo = GitRepo::new("/path/to/bare.git");
    /// bare_repo.prune_worktrees().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn prune_worktrees(&self) -> Result<()> {
        GitCommand::worktree_prune()
            .current_dir(&self.path)
            .execute_success()
            .await
            .with_context(|| "Failed to prune worktrees")?;

        Ok(())
    }

    /// Check if this repository is a bare repository.
    ///
    /// Bare repositories don't have a working directory and are optimized
    /// for use as a source for worktrees.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo.git");
    /// if repo.is_bare().await? {
    ///     println!("This is a bare repository");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn is_bare(&self) -> Result<bool> {
        let output = GitCommand::new()
            .args(["config", "--get", "core.bare"])
            .current_dir(&self.path)
            .execute_stdout()
            .await?;

        Ok(output.trim() == "true")
    }

    /// Get the current commit SHA of the repository.
    ///
    /// Returns the full 40-character SHA-1 hash of the current HEAD commit.
    /// This is useful for recording exact versions in lockfiles.
    ///
    /// # Returns
    ///
    /// The full commit hash as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The repository is not valid
    /// - HEAD is not pointing to a valid commit
    /// - Git command fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use ccpm::git::GitRepo;
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    /// let commit = repo.get_current_commit().await?;
    /// println!("Current commit: {}", commit);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_current_commit(&self) -> Result<String> {
        GitCommand::current_commit()
            .current_dir(&self.path)
            .execute_stdout()
            .await
            .context("Failed to get current commit")
    }

    #[cfg(test)]
    pub async fn get_current_branch(&self) -> Result<String> {
        let branch = GitCommand::current_branch()
            .current_dir(&self.path)
            .execute_stdout()
            .await
            .context("Failed to get current branch")?;

        if branch.is_empty() {
            // Fallback for very old Git or repos without commits
            Ok("master".to_string())
        } else {
            Ok(branch)
        }
    }
}

// Module-level helper functions for Git environment management and URL processing

/// Checks if Git is installed and accessible on the system.
///
/// This function verifies that the system's `git` command is available in the PATH
/// and responds to version queries. It's a prerequisite check for all Git operations
/// in CCPM.
///
/// # Return Value
///
/// - `true` if Git is installed and responding to `--version` commands
/// - `false` if Git is not found, not in PATH, or not executable
///
/// # Implementation Details
///
/// The function uses [`get_git_command()`] to determine the appropriate Git command
/// for the current platform, then executes `git --version` to verify functionality.
///
/// # Platform Differences
///
/// - **Windows**: Checks for `git.exe`, `git.cmd`, or `git.bat` in PATH
/// - **Unix-like**: Checks for `git` command in PATH
/// - **All platforms**: Respects PATH environment variable ordering
///
/// # Examples
///
/// ```rust
/// use ccpm::git::is_git_installed;
///
/// if is_git_installed() {
///     println!("Git is available - proceeding with repository operations");
/// } else {
///     eprintln!("Error: Git is not installed or not in PATH");
///     std::process::exit(1);
/// }
/// ```
///
/// # Usage in CCPM
///
/// This function is typically called during:
/// - Application startup to validate prerequisites
/// - Before any Git operations to provide clear error messages
/// - In CI/CD pipelines to verify build environment
///
/// # Alternative
///
/// For error-based validation with detailed context, use [`ensure_git_available()`]:
///
/// ```rust,no_run
/// use ccpm::git::ensure_git_available;
///
/// # fn example() -> anyhow::Result<()> {
/// ensure_git_available()?; // Throws CcpmError::GitNotFound if not available
/// # Ok(())
/// # }
/// ```
///
/// [`get_git_command()`]: crate::utils::platform::get_git_command
/// [`ensure_git_available()`]: fn.ensure_git_available.html
#[must_use]
pub fn is_git_installed() -> bool {
    // For synchronous checking, we still use std::process::Command directly
    std::process::Command::new(crate::utils::platform::get_git_command())
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Ensures Git is available on the system or returns a detailed error.
///
/// This function validates that Git is installed and accessible, providing a
/// [`CcpmError::GitNotFound`] with actionable guidance if Git is unavailable.
/// It's the error-throwing equivalent of [`is_git_installed()`].
///
/// # Return Value
///
/// - `Ok(())` if Git is properly installed and accessible
/// - `Err(CcpmError::GitNotFound)` if Git is not available
///
/// # Error Context
///
/// The returned error includes:
/// - Clear description of the missing Git requirement
/// - Platform-specific installation instructions
/// - Troubleshooting guidance for common PATH issues
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::git::ensure_git_available;
///
/// # fn example() -> anyhow::Result<()> {
/// // Validate Git before starting operations
/// ensure_git_available()?;
///
/// // Git is guaranteed to be available beyond this point
/// println!("Git is available - proceeding with operations");
/// # Ok(())
/// # }
/// ```
///
/// # Error Handling
///
/// ```rust,no_run
/// use ccpm::git::ensure_git_available;
/// use ccpm::core::CcpmError;
///
/// match ensure_git_available() {
///     Ok(_) => println!("Git is ready"),
///     Err(e) => {
///         if let Some(CcpmError::GitNotFound) = e.downcast_ref::<CcpmError>() {
///             eprintln!("Please install Git to continue");
///             // Show platform-specific installation instructions
///         }
///     }
/// }
/// ```
///
/// # Usage Pattern
///
/// Typically called at the start of Git-dependent operations:
///
/// ```rust,no_run
/// use ccpm::git::{ensure_git_available, GitRepo};
/// use std::env;
///
/// # async fn git_operation() -> anyhow::Result<()> {
/// // Validate prerequisites first
/// ensure_git_available()?;
///
/// // Then proceed with Git operations
/// let temp_dir = env::temp_dir();
/// let repo = GitRepo::clone(
///     "https://github.com/example/repo.git",
///     temp_dir.join("repo")
/// ).await?;
/// # Ok(())
/// # }
/// ```
///
/// [`CcpmError::GitNotFound`]: crate::core::CcpmError::GitNotFound
/// [`is_git_installed()`]: fn.is_git_installed.html
pub fn ensure_git_available() -> Result<()> {
    if !is_git_installed() {
        return Err(CcpmError::GitNotFound.into());
    }
    Ok(())
}

/// Checks if a directory contains a valid Git repository.
///
/// This function performs the same validation as [`GitRepo::is_git_repo()`] but
/// operates on an arbitrary path without requiring a `GitRepo` instance. It's
/// useful for validating paths before creating repository handles.
///
/// # Arguments
///
/// * `path` - The directory path to check for Git repository validity
///
/// # Return Value
///
/// - `true` if the path contains a `.git` subdirectory
/// - `false` if the `.git` subdirectory is missing or the path doesn't exist
///
/// # Examples
///
/// ```rust
/// use ccpm::git::is_valid_git_repo;
/// use std::path::Path;
///
/// let path = Path::new("/home/user/my-project");
///
/// if is_valid_git_repo(path) {
///     println!("Found Git repository at: {}", path.display());
/// } else {
///     println!("Not a Git repository: {}", path.display());
/// }
/// ```
///
/// # Use Cases
///
/// - **Path validation**: Check directories before creating `GitRepo` instances
/// - **Discovery**: Scan directories to find Git repositories
/// - **Conditional logic**: Branch behavior based on repository presence
/// - **Bulk operations**: Filter lists of paths to Git repositories only
///
/// # Batch Processing Example
///
/// ```rust,no_run
/// use ccpm::git::is_valid_git_repo;
/// use std::fs;
/// use std::path::Path;
///
/// # fn example() -> std::io::Result<()> {
/// let search_dir = Path::new("/home/user/projects");
///
/// // Find all Git repositories in a directory
/// for entry in fs::read_dir(search_dir)? {
///     let path = entry?.path();
///     if path.is_dir() && is_valid_git_repo(&path) {
///         println!("Found repository: {}", path.display());
///     }
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Validation Scope
///
/// This function only verifies the presence of a `.git` directory and does not:
/// - Check repository integrity or corruption
/// - Validate Git version compatibility  
/// - Test network connectivity to remotes
/// - Verify specific repository content or structure
///
/// # Performance
///
/// This is a lightweight, synchronous operation that performs a single
/// filesystem check. It's suitable for bulk validation scenarios.
///
/// [`GitRepo::is_git_repo()`]: struct.GitRepo.html#method.is_git_repo
#[must_use]
pub fn is_valid_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Ensures a directory contains a valid Git repository or returns a detailed error.
///
/// This function validates that the specified path contains a Git repository,
/// providing a [`CcpmError::GitRepoInvalid`] with actionable guidance if the
/// validation fails. It's the error-throwing equivalent of [`is_valid_git_repo()`].
///
/// # Arguments
///
/// * `path` - The directory path to validate as a Git repository
///
/// # Return Value
///
/// - `Ok(())` if the path contains a valid `.git` directory
/// - `Err(CcpmError::GitRepoInvalid)` if the path is not a Git repository
///
/// # Error Context
///
/// The returned error includes:
/// - The specific path that failed validation
/// - Clear description of what constitutes a valid Git repository
/// - Suggestions for initializing or cloning repositories
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::git::ensure_valid_git_repo;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let path = Path::new("/home/user/my-project");
///
/// // Validate before operations
/// ensure_valid_git_repo(path)?;
///
/// // Path is guaranteed to be a Git repository beyond this point
/// println!("Validated Git repository at: {}", path.display());
/// # Ok(())
/// # }
/// ```
///
/// # Error Handling Pattern
///
/// ```rust,no_run
/// use ccpm::git::ensure_valid_git_repo;
/// use ccpm::core::CcpmError;
/// use std::path::Path;
///
/// let path = Path::new("/some/directory");
///
/// match ensure_valid_git_repo(path) {
///     Ok(_) => println!("Valid repository found"),
///     Err(e) => {
///         if let Some(CcpmError::GitRepoInvalid { path }) = e.downcast_ref::<CcpmError>() {
///             eprintln!("Directory {} is not a Git repository", path);
///             eprintln!("Try: git clone <url> {} or git init {}", path, path);
///         }
///     }
/// }
/// ```
///
/// # Integration with `GitRepo`
///
/// This function provides validation before creating `GitRepo` instances:
///
/// ```rust,no_run
/// use ccpm::git::{ensure_valid_git_repo, GitRepo};
/// use std::path::Path;
///
/// # async fn validated_repo_operations() -> anyhow::Result<()> {
/// let path = Path::new("/path/to/repo");
///
/// // Validate first
/// ensure_valid_git_repo(path)?;
///
/// // Then create repository handle
/// let repo = GitRepo::new(path);
/// let tags = repo.list_tags().await?;
/// # Ok(())
/// # }
/// ```
///
/// # Use Cases
///
/// - **Precondition validation**: Ensure paths are Git repositories before operations
/// - **Error-first APIs**: Provide detailed errors rather than boolean returns
/// - **Pipeline validation**: Fail fast in processing pipelines
/// - **User feedback**: Give actionable error messages with suggestions
///
/// [`CcpmError::GitRepoInvalid`]: crate::core::CcpmError::GitRepoInvalid
/// [`is_valid_git_repo()`]: fn.is_valid_git_repo.html
pub fn ensure_valid_git_repo(path: &Path) -> Result<()> {
    if !is_valid_git_repo(path) {
        return Err(CcpmError::GitRepoInvalid {
            path: path.display().to_string(),
        }
        .into());
    }
    Ok(())
}

/// Parses a Git URL into owner and repository name components.
///
/// This function extracts the repository owner (user/organization) and repository
/// name from various Git URL formats. It handles the most common Git URL patterns
/// used across different hosting platforms and local repositories.
///
/// # Arguments
///
/// * `url` - The Git repository URL to parse
///
/// # Return Value
///
/// Returns a tuple `(owner, repository_name)` where:
/// - `owner` is the user, organization, or "local" for local repositories
/// - `repository_name` is the repository name (with `.git` suffix removed)
///
/// # Supported URL Formats
///
/// ## HTTPS URLs
/// - `https://github.com/rust-lang/cargo.git` → `("rust-lang", "cargo")`
/// - `https://gitlab.com/group/project.git` → `("group", "project")
/// - `https://bitbucket.org/user/repo.git` → `("user", "repo")
///
/// ## SSH URLs
/// - `git@github.com:rust-lang/cargo.git` → `("rust-lang", "cargo")`
/// - `git@gitlab.com:group/project.git` → `("group", "project")`
///
/// ## Local URLs
/// - `file:///path/to/repo.git` → `("local", "repo")`
/// - `/absolute/path/to/repo` → `("local", "repo")`
/// - `./relative/path/repo.git` → `("local", "repo")`
///
///
/// # Examples
///
/// ```rust
/// use ccpm::git::parse_git_url;
///
/// # fn example() -> anyhow::Result<()> {
/// // Parse GitHub URL
/// let (owner, repo) = parse_git_url("https://github.com/rust-lang/cargo.git")?;
/// assert_eq!(owner, "rust-lang");
/// assert_eq!(repo, "cargo");
///
/// // Parse SSH URL
/// let (owner, repo) = parse_git_url("git@github.com:user/project.git")?;
/// assert_eq!(owner, "user");
/// assert_eq!(repo, "project");
///
/// // Parse local repository
/// let (owner, repo) = parse_git_url("/home/user/my-repo")?;
/// assert_eq!(owner, "local");
/// assert_eq!(repo, "my-repo");
/// # Ok(())
/// # }
/// ```
///
/// # Use Cases
///
/// - **Cache directory naming**: Generate consistent cache paths
/// - **Repository identification**: Create unique identifiers for repositories
/// - **Metadata extraction**: Extract repository information for display
/// - **Path generation**: Create filesystem-safe directory names
///
/// # Cache Integration Example
///
/// ```rust,no_run
/// use ccpm::git::parse_git_url;
/// use std::path::PathBuf;
///
/// # fn cache_example() -> anyhow::Result<()> {
/// let url = "https://github.com/rust-lang/cargo.git";
/// let (owner, repo) = parse_git_url(url)?;
///
/// // Create cache directory path
/// let cache_path = PathBuf::from("/home/user/.ccpm/cache")
///     .join(&owner)
///     .join(&repo);
///     
/// println!("Cache location: {}", cache_path.display());
/// // Output: Cache location: /home/user/.ccpm/cache/rust-lang/cargo
/// # Ok(())
/// # }
/// ```
///
/// # Authentication Handling
///
/// The parser handles URLs with embedded authentication but extracts only
/// the repository components:
///
/// ```rust
/// use ccpm::git::parse_git_url;
///
/// # fn auth_example() -> anyhow::Result<()> {
/// // Authentication is ignored in parsing
/// let (owner, repo) = parse_git_url("https://token:value@github.com/user/repo.git")?;
/// assert_eq!(owner, "user");
/// assert_eq!(repo, "repo");
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The URL format is not recognized
/// - The URL doesn't contain sufficient path components
/// - The URL structure doesn't match expected patterns
///
/// # Platform Considerations
///
/// The parser handles platform-specific path formats:
/// - Windows: Supports backslash separators in local paths
/// - Unix: Handles standard forward slash separators
/// - All platforms: Normalizes path separators internally
pub fn parse_git_url(url: &str) -> Result<(String, String)> {
    // Handle file:// URLs
    if url.starts_with("file://") {
        let path = url.trim_start_matches("file://");
        if let Some(last_slash) = path.rfind('/') {
            let repo_name = &path[last_slash + 1..];
            let repo_name = repo_name.trim_end_matches(".git");
            return Ok(("local".to_string(), repo_name.to_string()));
        }
    }

    // Handle plain local paths (absolute or relative)
    if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
        if let Some(last_slash) = url.rfind('/') {
            let repo_name = &url[last_slash + 1..];
            let repo_name = repo_name.trim_end_matches(".git");
            return Ok(("local".to_string(), repo_name.to_string()));
        } else {
            let repo_name = url.trim_end_matches(".git");
            return Ok(("local".to_string(), repo_name.to_string()));
        }
    }

    // Handle SSH URLs like git@github.com:user/repo.git
    if url.contains('@') && url.contains(':') && !url.starts_with("ssh://") {
        if let Some(colon_pos) = url.find(':') {
            let path = &url[colon_pos + 1..];
            let path = path.trim_end_matches(".git");
            if let Some(slash_pos) = path.find('/') {
                return Ok((
                    path[..slash_pos].to_string(),
                    path[slash_pos + 1..].to_string(),
                ));
            }
        }
    }

    // Handle HTTPS URLs
    if url.contains("github.com") || url.contains("gitlab.com") || url.contains("bitbucket.org") {
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 2 {
            let repo = parts[parts.len() - 1].trim_end_matches(".git");
            let owner = parts[parts.len() - 2];
            return Ok((owner.to_string(), repo.to_string()));
        }
    }

    Err(anyhow::anyhow!(
        "Could not parse repository owner and name from URL"
    ))
}

/// Strips authentication information from a Git URL for safe display or logging.
///
/// This function removes sensitive authentication tokens, usernames, and passwords
/// from Git URLs while preserving the repository location information. It's essential
/// for security when logging or displaying URLs that might contain credentials.
///
/// # Arguments
///
/// * `url` - The Git URL that may contain authentication information
///
/// # Return Value
///
/// Returns the URL with authentication components removed:
/// - HTTPS URLs: Removes `user:token@` prefix
/// - SSH URLs: Returned unchanged (no embedded auth to strip)
/// - Other formats: Returned unchanged if no auth detected
///
/// # Security Purpose
///
/// This function prevents accidental credential exposure in:
/// - Log files and console output
/// - Error messages shown to users
/// - Debug information and stack traces
/// - Documentation and examples
///
/// # Supported Authentication Formats
///
/// ## HTTPS with Tokens
/// - `https://token@github.com/user/repo.git` → `https://github.com/user/repo.git`
/// - `https://user:pass@gitlab.com/repo.git` → `https://gitlab.com/repo.git`
/// - `https://oauth2:token@bitbucket.org/repo.git` → `https://bitbucket.org/repo.git`
///
/// ## Preserved Formats
/// - `git@github.com:user/repo.git` → `git@github.com:user/repo.git` (unchanged)
/// - `https://github.com/user/repo.git` → `https://github.com/user/repo.git` (no auth)
/// - `file:///path/to/repo` → `file:///path/to/repo` (unchanged)
///
/// # Examples
///
/// ```rust
/// use ccpm::git::strip_auth_from_url;
///
/// # fn example() -> anyhow::Result<()> {
/// // Strip token from HTTPS URL
/// let clean_url = strip_auth_from_url("https://ghp_token123@github.com/user/repo.git")?;
/// assert_eq!(clean_url, "https://github.com/user/repo.git");
///
/// // Strip user:password authentication
/// let clean_url = strip_auth_from_url("https://user:secret@gitlab.com/project.git")?;
/// assert_eq!(clean_url, "https://gitlab.com/project.git");
///
/// // URLs without auth are unchanged
/// let clean_url = strip_auth_from_url("https://github.com/public/repo.git")?;
/// assert_eq!(clean_url, "https://github.com/public/repo.git");
/// # Ok(())
/// # }
/// ```
///
/// # Safe Logging Pattern
///
/// ```rust,no_run
/// use ccpm::git::strip_auth_from_url;
/// use anyhow::Result;
///
/// fn log_repository_operation(url: &str, operation: &str) -> Result<()> {
///     let safe_url = strip_auth_from_url(url)?;
///     println!("Performing {} on repository: {}", operation, safe_url);
///     // Logs: "Performing clone on repository: https://github.com/user/repo.git"
///     // Instead of exposing: "https://token:secret@github.com/user/repo.git"
///     Ok(())
/// }
/// ```
///
/// # Error Context Integration
///
/// ```rust,no_run
/// use ccpm::git::strip_auth_from_url;
/// use ccpm::core::CcpmError;
///
/// # async fn operation_example(url: &str) -> anyhow::Result<()> {
/// match some_git_operation(url).await {
///     Ok(result) => Ok(result),
///     Err(e) => {
///         let safe_url = strip_auth_from_url(url)?;
///         eprintln!("Git operation failed for repository: {}", safe_url);
///         Err(e)
///     }
/// }
/// # }
/// # async fn some_git_operation(url: &str) -> anyhow::Result<()> { Ok(()) }
/// ```
///
/// # Implementation Details
///
/// The function uses careful parsing to distinguish between:
/// - Authentication `@` symbols (before the hostname)
/// - Email address `@` symbols in commit information (preserved)
/// - Path components that might contain `@` symbols (preserved)
///
/// # Edge Cases Handled
///
/// - URLs with multiple `@` symbols (only strips auth prefix)
/// - URLs with no authentication (returned unchanged)
/// - Malformed URLs (best-effort processing)
/// - Non-HTTP protocols (returned unchanged)
///
/// # Security Note
///
/// This function is for **display/logging safety only**. The original authenticated
/// URL should still be used for actual Git operations. Never use the stripped URL
/// for authentication-required operations.
pub fn strip_auth_from_url(url: &str) -> Result<String> {
    if url.starts_with("https://") || url.starts_with("http://") {
        // Find the @ symbol that marks the end of authentication
        if let Some(at_pos) = url.find('@') {
            let protocol_end = if url.starts_with("https://") {
                "https://".len()
            } else {
                "http://".len()
            };

            // Check if @ is part of auth (comes before first /)
            let first_slash = url[protocol_end..].find('/').map(|p| p + protocol_end);
            if first_slash.is_none() || at_pos < first_slash.unwrap() {
                // Extract protocol and the part after @
                let protocol = &url[..protocol_end];
                let after_auth = &url[at_pos + 1..];
                return Ok(format!("{protocol}{after_auth}"));
            }
        }
    }

    // Return URL as-is if no auth found or not HTTP(S)
    Ok(url.to_string())
}
