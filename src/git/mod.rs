//! Git operations wrapper for AGPM.
//!
//! This module provides an async wrapper around the system `git` command. Uses system Git
//! (not libgit2) for maximum compatibility with authentication, configurations, and platforms.
//!
//! # Core Features
//!
//! - **Async operations**: Non-blocking I/O using Tokio
//! - **Worktree support**: Parallel package installation via Git worktrees
//! - **Authentication**: HTTPS tokens, SSH keys, credential helpers
//! - **Cross-platform**: Windows, macOS, Linux support
//! - **Progress reporting**: User feedback during long operations
//! - **Tag caching**: Per-instance caching for performance (v0.4.11+)
//!
//! # Security
//!
//! - Command injection prevention via proper argument passing
//! - Credentials never logged or exposed in errors
//! - HTTPS verification enabled by default

pub mod command_builder;
#[cfg(test)]
mod tests;

use crate::core::AgpmError;
use crate::git::command_builder::GitCommand;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A Git repository handle providing async operations via CLI commands.
///
#[derive(Debug, Clone)]
pub struct GitRepo {
    /// The local filesystem path to the Git repository.
    ///
    /// This path should point to the root directory of a Git repository
    /// (the directory containing `.git/` subdirectory).
    path: PathBuf,

    /// Cached list of tags for performance optimization.
    ///
    /// Tags are cached after the first `list_tags()` call to avoid repeated
    /// `git tag -l` operations within a single command execution. This is
    /// particularly important for version constraint resolution where the same
    /// tag list may be queried hundreds of times.
    ///
    /// Uses Arc to enable sharing the cache across cloned instances, which is
    /// critical for parallel dependency resolution where multiple tasks access
    /// the same repository.
    tag_cache: std::sync::Arc<OnceLock<Vec<String>>>,
}

impl GitRepo {
    /// Creates a new `GitRepo` instance for an existing local repository.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to the Git repository root directory
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            tag_cache: std::sync::Arc::new(OnceLock::new()),
        }
    }

    /// Clones a Git repository from a remote URL to a local path.
    ///
    /// # Arguments
    ///
    /// * `url` - The remote repository URL (HTTPS, SSH, or file://)
    /// * `target` - The local directory where the repository will be cloned
    /// * `progress` - Optional progress bar for user feedback
    ///
    /// # Errors
    ///
    /// - The URL is invalid or unreachable
    /// - Authentication fails
    /// - The target directory already exists and is not empty
    /// - Network connectivity issues
    /// - Insufficient disk space
    pub async fn clone(url: &str, target: impl AsRef<Path>) -> Result<Self> {
        let target_path = target.as_ref();

        // Use command builder for consistent clone operations
        let mut cmd = GitCommand::clone(url, target_path);

        // For file:// URLs, clone with all branches to ensure commit availability
        if url.starts_with("file://") {
            cmd = GitCommand::clone_local(url, target_path);
        }

        // Execute will handle error context properly
        cmd.execute().await?;

        Ok(Self::new(target_path))
    }

    /// Fetches updates from the remote repository without modifying the working tree.
    ///
    /// # Arguments
    ///
    /// * `auth_url` - Optional URL with authentication for private repositories
    /// * `progress` - Optional progress bar for network operation feedback
    ///
    /// # Errors
    ///
    /// - Network connectivity fails
    /// - Authentication is rejected
    /// - The remote repository is unavailable
    /// - The local repository is in an invalid state
    pub async fn fetch(&self, auth_url: Option<&str>) -> Result<()> {
        // Note: file:// URLs are local repositories, but we still need to fetch
        // from them to get updates from the source repository

        // Use git fetch with authentication from global config URL if provided
        if let Some(url) = auth_url {
            // Temporarily update the remote URL with auth for this fetch
            GitCommand::set_remote_url(url).current_dir(&self.path).execute_success().await?;
        }

        // Now fetch with the potentially updated URL
        GitCommand::fetch().current_dir(&self.path).execute_success().await?;

        Ok(())
    }

    /// Checks out a specific Git reference (branch, tag, or commit hash).
    ///
    /// # Arguments
    ///
    /// * `ref_name` - The Git reference to checkout (branch, tag, or commit)
    ///
    /// # Errors
    ///
    /// - The reference doesn't exist in the repository
    /// - The repository is in an invalid state
    /// - File system permissions prevent checkout
    /// - The working directory is locked by another process
    pub async fn checkout(&self, ref_name: &str) -> Result<()> {
        // Reset to clean state before checkout
        let reset_result = GitCommand::reset_hard().current_dir(&self.path).execute().await;

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
        let check_remote =
            GitCommand::verify_ref(&remote_ref).current_dir(&self.path).execute().await;

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
        GitCommand::checkout(ref_name).current_dir(&self.path).execute_success().await.map_err(
            |e| {
                // If it's already a GitCheckoutFailed error, return as-is
                // Otherwise wrap it
                if let Some(agpm_err) = e.downcast_ref::<AgpmError>()
                    && matches!(agpm_err, AgpmError::GitCheckoutFailed { .. })
                {
                    return e;
                }
                AgpmError::GitCheckoutFailed {
                    reference: ref_name.to_string(),
                    reason: e.to_string(),
                }
                .into()
            },
        )
    }

    /// Lists all tags in the repository, sorted by Git's default ordering.
    ///
    /// # Return Value
    ///
    /// # Errors
    ///
    /// - The repository path doesn't exist
    /// - The directory is not a valid Git repository
    /// - Git command execution fails
    /// - File system permissions prevent access
    /// - Lock conflicts persist after retry attempts
    pub async fn list_tags(&self) -> Result<Vec<String>> {
        if let Some(cached_tags) = self.tag_cache.get() {
            return Ok(cached_tags.clone());
        }

        if !self.path.exists() {
            return Err(anyhow::anyhow!("Repository path does not exist: {:?}", self.path));
        }
        if !self.path.join(".git").exists() && !self.path.join("HEAD").exists() {
            return Err(anyhow::anyhow!("Not a git repository: {:?}", self.path));
        }

        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(150);
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            let result = GitCommand::list_tags().current_dir(&self.path).execute_stdout().await;

            match result {
                Ok(stdout) => {
                    let tags: Vec<String> = stdout
                        .lines()
                        .filter(|line| !line.is_empty())
                        .map(std::string::ToString::to_string)
                        .collect();
                    let _ = self.tag_cache.set(tags.clone());
                    return Ok(tags);
                }
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("lock") {
                        last_error = Some(e);
                        tokio::time::sleep(RETRY_DELAY * (attempt + 1)).await; // Exponential backoff
                        continue;
                    }
                    // For non-lock errors, fail immediately
                    return Err(e).context(format!("Failed to list git tags in {:?}", self.path));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Exhausted retries for list_tags")))
            .context(format!(
                "Failed to list git tags in {:?} after {} retries",
                self.path, MAX_RETRIES
            ))
    }

    /// Retrieves the URL of the remote 'origin' repository.
    ///
    /// # Return Value
    ///
    /// - HTTPS: `https://github.com/user/repo.git`
    /// - SSH: `git@github.com:user/repo.git`
    /// - File: `file:///path/to/repo.git`
    ///
    /// # Errors
    ///
    /// - No 'origin' remote is configured
    /// - The repository is not a valid Git repository
    /// - Git command execution fails
    /// - File system access is denied
    pub async fn get_remote_url(&self) -> Result<String> {
        GitCommand::remote_url().current_dir(&self.path).execute_stdout().await
    }

    /// Checks if the directory contains a valid Git repository.\n    ///
    ///
    #[must_use]
    pub fn is_git_repo(&self) -> bool {
        is_git_repository(&self.path)
    }

    /// Returns the filesystem path to the Git repository.
    ///
    /// # Return Value
    ///
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Verifies that a Git repository URL is accessible without performing a full clone.
    ///
    /// # Arguments
    ///
    /// * `url` - The repository URL to verify
    ///
    /// # Errors
    ///
    /// - **Network issues**: DNS resolution, connectivity, timeouts
    /// - **Authentication failures**: Invalid credentials, expired tokens
    /// - **Repository issues**: Repository doesn't exist, access denied
    /// - **Local path issues**: File doesn't exist (for `file://` URLs)
    /// - **URL format issues**: Malformed or unsupported URL schemes
    pub async fn verify_url(url: &str) -> Result<()> {
        // For file:// URLs, just check if the path exists
        if url.starts_with("file://") {
            let path = url.strip_prefix("file://").unwrap();
            return if std::path::Path::new(path).exists() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Local path does not exist: {path}"))
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
            let mut check_cmd =
                GitCommand::new().args(["show-ref", "--head"]).current_dir(&self.path);

            if let Some(ctx) = context {
                check_cmd = check_cmd.with_context(ctx);
            }

            check_cmd
                .execute_success()
                .await
                .map_err(|e| anyhow::anyhow!("Bare repository has no refs available: {e}"))?;
        }

        Ok(())
    }

    /// Clone a repository as a bare repository (no working directory).
    ///
    /// # Arguments
    ///
    /// * `url` - The remote repository URL
    /// * `target` - The local directory where the bare repository will be stored
    /// * `progress` - Optional progress bar for user feedback
    /// # Returns
    ///
    pub async fn clone_bare(url: &str, target: impl AsRef<Path>) -> Result<Self> {
        Self::clone_bare_with_context(url, target, None).await
    }

    /// Clone a repository as a bare repository with logging context.
    ///
    /// # Arguments
    ///
    /// * `url` - The remote repository URL
    /// * `target` - The local directory where the bare repository will be stored
    /// * `progress` - Optional progress bar for user feedback
    /// * `context` - Optional context for logging (e.g., dependency name)
    /// # Returns
    ///
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

        // Configure the fetch refspec to ensure all branches are fetched as remote tracking branches
        // This is crucial for file:// URLs and ensures we can resolve origin/branch after fetching
        let _ = GitCommand::new()
            .args(["config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"])
            .current_dir(repo.path())
            .execute_success()
            .await;

        // Ensure bare repo has refs available for worktree creation
        // This fetch is necessary after clone to set up remote tracking branches
        // Note: The cache layer tracks this fetch so worktree creation won't re-fetch
        repo.ensure_bare_repo_has_refs_with_context(context).await.ok();

        Ok(repo)
    }

    /// Create a new worktree from this repository.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - The path where the worktree will be created
    /// * `reference` - Optional Git reference (branch/tag/commit) to checkout
    /// # Returns
    ///
    pub async fn create_worktree(
        &self,
        worktree_path: impl AsRef<Path>,
        reference: Option<&str>,
    ) -> Result<Self> {
        self.create_worktree_with_context(worktree_path, reference, None).await
    }

    /// Create a new worktree from this repository with logging context.
    ///
    /// # Arguments
    ///
    /// * `worktree_path` - The path where the worktree will be created
    /// * `reference` - Optional Git reference (branch/tag/commit) to checkout
    /// * `context` - Optional context for logging (e.g., dependency name)
    /// # Returns
    ///
    pub async fn create_worktree_with_context(
        &self,
        worktree_path: impl AsRef<Path>,
        reference: Option<&str>,
        context: Option<&str>,
    ) -> Result<Self> {
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
                Ok(()) => {
                    // Initialize and update submodules in the new worktree
                    let worktree_repo = Self::new(worktree_path);

                    // Initialize submodules
                    let mut init_cmd =
                        GitCommand::new().args(["submodule", "init"]).current_dir(worktree_path);

                    if let Some(ctx) = context {
                        init_cmd = init_cmd.with_context(ctx);
                    }

                    if let Err(e) = init_cmd.execute_success().await {
                        let error_str = e.to_string();
                        // Only ignore errors indicating no submodules are present
                        if !error_str.contains("No submodule mapping found")
                            && !error_str.contains("no submodule")
                        {
                            // For other errors, return them
                            return Err(e).context("Failed to initialize submodules");
                        }
                    }

                    // Update submodules
                    let mut update_cmd = GitCommand::new()
                        .args(["submodule", "update", "--recursive"])
                        .current_dir(worktree_path);

                    if let Some(ctx) = context {
                        update_cmd = update_cmd.with_context(ctx);
                    }

                    if let Err(e) = update_cmd.execute_success().await {
                        let error_str = e.to_string();
                        // Ignore errors related to no submodules
                        if !error_str.contains("No submodule mapping found")
                            && !error_str.contains("no submodule")
                        {
                            return Err(e).context("Failed to update submodules");
                        }
                    }

                    return Ok(worktree_repo);
                }
                Err(e) => {
                    let error_str = e.to_string();

                    // Check if this is a concurrent access issue
                    // The "commondir" error occurs when Git scans existing worktrees during
                    // concurrent creation - another thread's worktree entry may be partially
                    // written, causing "failed to read worktrees/<name>/commondir: Undefined error: 0"
                    if error_str.contains("already exists")
                        || error_str.contains("is already checked out")
                        || error_str.contains("fatal: could not create directory")
                        || (error_str.contains("failed to read") && error_str.contains("commondir"))
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
                    // This can happen in Docker containers, CI environments, or after unclean
                    // shutdowns where git's worktree metadata gets out of sync with filesystem.
                    // Recovery strategy:
                    // 1. Remove invalid worktree directory if it exists without .git file
                    // 2. Run `git worktree prune` to clean stale registrations
                    // 3. Retry with `git worktree add --force`
                    //
                    // NOTE: We only run prune in this error recovery path (not speculatively)
                    // to minimize race conditions with concurrent worktree operations.
                    if error_str.contains("missing but already registered worktree") {
                        // Git reports "missing but already registered" when the worktree
                        // state is inconsistent. This can happen when:
                        // - The .git file exists but is broken/empty
                        // - The worktree was partially created
                        // - Docker/CI environments had filesystem state corruption
                        //
                        // Since git explicitly tells us this worktree is INVALID, we can
                        // safely remove it. Git wouldn't report this error for a valid
                        // worktree that other processes might be using.
                        if worktree_path.exists() {
                            let _ = tokio::fs::remove_dir_all(worktree_path).await;
                        }

                        // Prune stale worktree registrations. This is safe in the recovery path
                        // since we already failed once. In Docker/CI environments, --force alone
                        // may not be sufficient to override stale registrations.
                        let mut prune_cmd =
                            GitCommand::new().args(["worktree", "prune"]).current_dir(&self.path);
                        if let Some(ctx) = context {
                            prune_cmd = prune_cmd.with_context(ctx);
                        }
                        let _ = prune_cmd.execute_success().await;

                        // Ensure parent directory exists before force add.
                        // This handles the case where the temp directory was partially cleaned up,
                        // leaving Git's worktree metadata pointing to a non-existent path.
                        if let Some(parent) = worktree_path.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }

                        // Use `git worktree add --force` after pruning stale entries
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
                            Ok(()) => {
                                // Initialize and update submodules in the new worktree
                                let worktree_repo = Self::new(worktree_path);

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
                            Ok(()) => {
                                // Initialize and update submodules in the new worktree
                                let worktree_repo = Self::new(worktree_path);

                                // Initialize submodules
                                let mut init_cmd = GitCommand::new()
                                    .args(["submodule", "init"])
                                    .current_dir(worktree_path);

                                if let Some(ctx) = context {
                                    init_cmd = init_cmd.with_context(ctx);
                                }

                                if let Err(e) = init_cmd.execute_success().await {
                                    let error_str = e.to_string();
                                    // Only ignore errors indicating no submodules are present
                                    if !error_str.contains("No submodule mapping found")
                                        && !error_str.contains("no submodule")
                                    {
                                        // For other errors, return them
                                        return Err(e).context("Failed to initialize submodules");
                                    }
                                }

                                // Update submodules
                                let mut update_cmd = GitCommand::new()
                                    .args(["submodule", "update", "--recursive"])
                                    .current_dir(worktree_path);

                                if let Some(ctx) = context {
                                    update_cmd = update_cmd.with_context(ctx);
                                }

                                if let Err(e) = update_cmd.execute_success().await {
                                    let error_str = e.to_string();
                                    // Ignore errors related to no submodules
                                    if !error_str.contains("No submodule mapping found")
                                        && !error_str.contains("no submodule")
                                    {
                                        return Err(e).context("Failed to update submodules");
                                    }
                                }

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
                    if let Some(ref_name) = reference
                        && (error_str.contains("pathspec")
                            || error_str.contains("not found")
                            || error_str.contains("ambiguous")
                            || error_str.contains("invalid")
                            || error_str.contains("unknown revision"))
                    {
                        return Err(anyhow::anyhow!(
                            "Invalid version or reference '{ref_name}': Failed to checkout reference - the specified version/tag/branch does not exist in the repository"
                        ));
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
    /// # Arguments
    ///
    /// * `worktree_path` - The path to the worktree to remove
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
    pub async fn list_worktrees(&self) -> Result<Vec<PathBuf>> {
        let output = GitCommand::worktree_list().current_dir(&self.path).execute_stdout().await?;

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
            } else if line.is_empty()
                && current_worktree.is_some()
                && let Some(path) = current_worktree.take()
            {
                worktrees.push(path);
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
    /// # Returns
    ///
    /// # Errors
    ///
    /// - The repository is not valid
    /// - HEAD is not pointing to a valid commit
    /// - Git command fails
    pub async fn get_current_commit(&self) -> Result<String> {
        GitCommand::current_commit()
            .current_dir(&self.path)
            .execute_stdout()
            .await
            .context("Failed to get current commit")
    }

    /// Batch resolve multiple refs to SHAs in a single git process.
    ///
    /// Uses `git rev-parse <ref1> <ref2> ...` to resolve all refs at once, reducing
    /// process spawn overhead from O(n) to O(1). This is significantly faster
    /// for Windows where process spawning has high overhead.
    ///
    /// # Arguments
    ///
    /// * `refs` - Slice of ref specifications to resolve
    ///
    /// # Returns
    ///
    /// HashMap mapping each input ref to its resolved SHA (or None if not found)
    ///
    /// # Performance
    ///
    /// - Single process for all refs vs one per ref
    /// - Reduces 100 refs from ~5-10 seconds to ~0.5 seconds on Windows
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use agpm_cli::git::GitRepo;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let repo = GitRepo::new("/path/to/repo");
    /// let refs = vec!["v1.0.0", "main", "abc1234"];
    /// let results = repo.resolve_refs_batch(&refs).await?;
    ///
    /// for (ref_name, sha) in results {
    ///     if let Some(sha) = sha {
    ///         println!("{} -> {}", ref_name, sha);
    ///     } else {
    ///         println!("{} not found", ref_name);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resolve_refs_batch(
        &self,
        refs: &[&str],
    ) -> Result<std::collections::HashMap<String, Option<String>>> {
        use std::collections::HashMap;

        if refs.is_empty() {
            return Ok(HashMap::new());
        }

        // Partition refs: already-SHAs vs need-resolution
        let (already_shas, to_resolve): (Vec<&str>, Vec<&str>) =
            refs.iter().partition(|r| r.len() == 40 && r.chars().all(|c| c.is_ascii_hexdigit()));

        let mut results: HashMap<String, Option<String>> = HashMap::new();

        // Add already-resolved SHAs directly
        for sha in already_shas {
            results.insert(sha.to_string(), Some(sha.to_string()));
        }

        if to_resolve.is_empty() {
            return Ok(results);
        }

        // Build arguments for git rev-parse: ["rev-parse", "ref1", "ref2", ...]
        // This resolves all refs in a single git process
        let mut args = vec!["rev-parse"];
        args.extend(to_resolve.iter().copied());

        // Execute batch resolution
        let output = GitCommand::new().args(args).current_dir(&self.path).execute().await;

        match output {
            Ok(cmd_output) => {
                // Parse output (one SHA per line, in order)
                let shas: Vec<&str> = cmd_output.stdout.lines().collect();

                for (i, ref_name) in to_resolve.iter().enumerate() {
                    let sha = shas.get(i).and_then(|s| {
                        let trimmed = s.trim();
                        // Only accept valid SHA output (40 hex chars)
                        if trimmed.len() == 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                            Some(trimmed.to_string())
                        } else {
                            None
                        }
                    });
                    results.insert(ref_name.to_string(), sha);
                }
            }
            Err(e) => {
                // If batch fails (e.g., one ref is invalid), fall back to individual resolution
                tracing::debug!(
                    target: "git",
                    "Batch rev-parse failed, falling back to individual resolution: {}",
                    e
                );

                for ref_name in to_resolve {
                    let sha = GitCommand::rev_parse(ref_name)
                        .current_dir(&self.path)
                        .execute_stdout()
                        .await
                        .ok();
                    results.insert(ref_name.to_string(), sha);
                }
            }
        }

        Ok(results)
    }

    /// Resolves a Git reference (tag, branch, commit) to its full SHA-1 hash.
    ///
    /// # Arguments
    ///
    /// * `ref_spec` - The Git reference to resolve (tag, branch, short/full SHA, or None for HEAD)
    /// # Returns
    ///
    /// # Errors
    ///
    /// - The reference doesn't exist in the repository
    /// - The repository is invalid or corrupted
    /// - Git command execution fails
    pub async fn resolve_to_sha(&self, ref_spec: Option<&str>) -> Result<String> {
        let reference = ref_spec.unwrap_or("HEAD");

        // Optimization: if it's already a full SHA, return it directly
        if reference.len() == 40 && reference.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(reference.to_string());
        }

        // Determine the reference to resolve based on type (tag vs branch)
        let ref_to_resolve = if !reference.contains('/') && reference != "HEAD" {
            // Check if this is a tag (uses cached tag list for performance)
            let is_tag = self
                .list_tags()
                .await
                .map(|tags| tags.contains(&reference.to_string()))
                .unwrap_or(false);

            if is_tag {
                // It's a tag - use it directly
                reference.to_string()
            } else {
                // Assume it's a branch name - try to resolve origin/branch first to get the latest from remote
                // This ensures we get the most recent commit after a fetch
                let origin_ref = format!("origin/{reference}");
                if GitCommand::rev_parse(&origin_ref)
                    .current_dir(&self.path)
                    .execute_stdout()
                    .await
                    .is_ok()
                {
                    origin_ref
                } else {
                    // Fallback to the original reference (might be a local branch)
                    reference.to_string()
                }
            }
        } else {
            reference.to_string()
        };

        // Use rev-parse to get the full SHA
        let sha = GitCommand::rev_parse(&ref_to_resolve)
            .current_dir(&self.path)
            .execute_stdout()
            .await
            .with_context(|| format!("Failed to resolve reference '{reference}' to SHA"))?;

        // Ensure we have a full SHA (sometimes rev-parse can return short SHAs)
        if sha.len() < 40 {
            // Request the full SHA explicitly
            let full_sha = GitCommand::new()
                .args(["rev-parse", "--verify", &format!("{reference}^{{commit}}")])
                .current_dir(&self.path)
                .execute_stdout()
                .await
                .with_context(|| format!("Failed to get full SHA for reference '{reference}'"))?;
            Ok(full_sha)
        } else {
            Ok(sha)
        }
    }

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

    /// Gets the default branch name for the repository.
    ///
    /// # Returns
    ///
    /// # Errors
    ///
    /// - Git commands fail with non-recoverable errors
    /// - Lock conflicts occur (propagated for caller to retry)
    /// - Default branch cannot be determined
    pub async fn get_default_branch(&self) -> Result<String> {
        let result = GitCommand::new()
            .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(&self.path)
            .execute_stdout()
            .await;

        match result {
            Ok(symbolic_ref) => {
                if let Some(branch) = symbolic_ref.strip_prefix("refs/remotes/origin/") {
                    return Ok(branch.to_string());
                }
                // If parsing fails, fall through to the next method.
            }
            Err(e) => {
                let error_str = e.to_string();
                // If the ref is not found, it's not a fatal error, just fall back.
                // Any other error (like a lock file) should be propagated.
                if !error_str.contains("not a symbolic ref") && !error_str.contains("not found") {
                    return Err(e).context("Failed to get default branch via symbolic-ref");
                }
            }
        }

        // Fallback: try to get current branch (for non-bare repos or if symbolic-ref fails)
        self.get_current_branch().await
    }
}

// Module-level helper functions for Git environment management and URL processing

/// Checks if Git is installed and accessible on the system.
///
/// # Return Value
///
/// - `true` if Git is installed and responding to `--version` commands
/// - `false` if Git is not found, not in PATH, or not executable
///
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
/// # Return Value
///
/// - `Ok(())` if Git is properly installed and accessible
/// - `Err(AgpmError::GitNotFound)` if Git is not available
///
pub fn ensure_git_available() -> Result<()> {
    if !is_git_installed() {
        return Err(AgpmError::GitNotFound.into());
    }
    Ok(())
}

/// Checks if a path contains a Git repository (regular or bare).
///
/// # Arguments
///
/// * `path` - The path to check for a Git repository
/// # Returns
///
/// * `true` if the path is a valid Git repository (regular or bare)
/// * `false` if neither repository marker exists
#[must_use]
pub fn is_git_repository(path: &Path) -> bool {
    // Check for regular repository (.git directory) or bare repository (HEAD file)
    path.join(".git").exists() || path.join("HEAD").exists()
}

/// Checks if a directory contains a valid Git repository.
///
/// # Arguments
///
/// * `path` - The directory path to check for Git repository validity
/// # Return Value
///
/// - `true` if the path contains a `.git` subdirectory
/// - `false` if the `.git` subdirectory is missing or the path doesn't exist
///
#[must_use]
pub fn is_valid_git_repo(path: &Path) -> bool {
    is_git_repository(path)
}

/// Ensures a directory contains a valid Git repository or returns a detailed error.
///
/// # Arguments
///
/// * `path` - The directory path to validate as a Git repository
/// # Return Value
///
/// - `Ok(())` if the path contains a valid `.git` directory
/// - `Err(AgpmError::GitRepoInvalid)` if the path is not a Git repository
///
pub fn ensure_valid_git_repo(path: &Path) -> Result<()> {
    if !is_valid_git_repo(path) {
        return Err(AgpmError::GitRepoInvalid {
            path: path.display().to_string(),
        }
        .into());
    }
    Ok(())
}

/// Parses a Git URL into owner and repository name components.
///
/// # Arguments
///
/// * `url` - The Git repository URL to parse
/// # Return Value
///
/// - `owner` is the user, organization, or "local" for local repositories
/// - `repository_name` is the repository name (with `.git` suffix removed)
///
/// # Errors
///
/// - The URL format is not recognized
/// - The URL doesn't contain sufficient path components
/// - The URL structure doesn't match expected patterns
///
pub fn parse_git_url(url: &str) -> Result<(String, String)> {
    use std::path::Path;

    // Handle file:// URLs
    if url.starts_with("file://") {
        let path_str = url.trim_start_matches("file://");
        let path = Path::new(path_str);
        let repo_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.trim_end_matches(".git"))
            .unwrap_or(path_str);
        return Ok(("local".to_string(), repo_name.to_string()));
    }

    // Handle plain local paths (absolute or relative)
    if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
        let path = Path::new(url);
        let repo_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.trim_end_matches(".git"))
            .unwrap_or(url);
        return Ok(("local".to_string(), repo_name.to_string()));
    }

    // Handle SSH URLs like git@github.com:user/repo.git
    if url.contains('@')
        && url.contains(':')
        && !url.starts_with("ssh://")
        && let Some(colon_pos) = url.find(':')
    {
        let path = &url[colon_pos + 1..];
        let path = path.trim_end_matches(".git");
        if let Some(slash_pos) = path.find('/') {
            return Ok((path[..slash_pos].to_string(), path[slash_pos + 1..].to_string()));
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

    Err(anyhow::anyhow!("Could not parse repository owner and name from URL"))
}

/// Strips authentication information from a Git URL for safe display or logging.
///
/// # Arguments
///
/// * `url` - The Git URL that may contain authentication information
/// # Return Value
///
/// - HTTPS URLs: Removes `user:token@` prefix
/// - SSH URLs: Returned unchanged (no embedded auth to strip)
/// - Other formats: Returned unchanged if no auth detected
///
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
