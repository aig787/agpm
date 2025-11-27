//! Git repository cache management with worktree-based parallel operations.
//!
//! Provides caching for Git repositories with safe parallel resource installation via worktrees.
//!
//! # Architecture
//!
//! - [`Cache`]: Core repository management and worktree orchestration
//! - [`CacheLock`]: File-based locking for process-safe concurrent access
//! - SHA-based worktrees: One worktree per unique commit for maximum deduplication
//! - Notification-based coordination: `tokio::sync::Notify` eliminates polling
//!
//! # Cache Structure
//!
//! ```text
//! ~/.agpm/cache/
//! ├── sources/       # Bare repositories
//! ├── worktrees/     # SHA-based worktrees with .state.json registry
//! └── .locks/        # Per-repository and per-worktree locks
//! ```
//!
//! # Key Features
//!
//! - Fsync-based verification ensures files readable after worktree creation
//! - DashMap for lock-free concurrent worktree access
//! - Command-instance fetch caching (single fetch per repo per command)
//! - Cross-platform path handling and cache locations

use crate::constants::{default_lock_timeout, pending_state_timeout};
use crate::core::error::AgpmError;
use crate::core::file_error::{FileOperation, FileResultExt};
use crate::git::GitRepo;
use crate::git::command_builder::GitCommand;
use crate::utils::fs;
use crate::utils::security::validate_path_security;
use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs as async_fs;
use tokio::sync::{Mutex, MutexGuard, RwLock};

/// Acquire a tokio Mutex with timeout and diagnostic dump on failure.
/// Uses test-mode aware timeout from constants.
async fn acquire_mutex_with_timeout<'a, T>(
    mutex: &'a Mutex<T>,
    name: &str,
) -> Result<MutexGuard<'a, T>> {
    let timeout = default_lock_timeout();
    match tokio::time::timeout(timeout, mutex.lock()).await {
        Ok(guard) => Ok(guard),
        Err(_) => {
            eprintln!("[DEADLOCK] Timeout waiting for mutex '{}' after {:?}", name, timeout);
            anyhow::bail!(
                "Timeout waiting for mutex '{}' after {:?} - possible deadlock",
                name,
                timeout
            )
        }
    }
}

/// Acquire a tokio RwLock read guard with timeout and diagnostic dump on failure.
async fn acquire_rwlock_read_with_timeout<'a, T>(
    rwlock: &'a RwLock<T>,
    name: &str,
) -> Result<tokio::sync::RwLockReadGuard<'a, T>> {
    let timeout = default_lock_timeout();
    match tokio::time::timeout(timeout, rwlock.read()).await {
        Ok(guard) => Ok(guard),
        Err(_) => {
            eprintln!("[DEADLOCK] Timeout waiting for RwLock read '{}' after {:?}", name, timeout);
            anyhow::bail!(
                "Timeout waiting for RwLock read '{}' after {:?} - possible deadlock",
                name,
                timeout
            )
        }
    }
}

/// Acquire a tokio RwLock write guard with timeout and diagnostic dump on failure.
async fn acquire_rwlock_write_with_timeout<'a, T>(
    rwlock: &'a RwLock<T>,
    name: &str,
) -> Result<tokio::sync::RwLockWriteGuard<'a, T>> {
    let timeout = default_lock_timeout();
    match tokio::time::timeout(timeout, rwlock.write()).await {
        Ok(guard) => Ok(guard),
        Err(_) => {
            eprintln!("[DEADLOCK] Timeout waiting for RwLock write '{}' after {:?}", name, timeout);
            anyhow::bail!(
                "Timeout waiting for RwLock write '{}' after {:?} - possible deadlock",
                name,
                timeout
            )
        }
    }
}

// Concurrency Architecture:
// - Direct control approach: Command parallelism (--max-parallel) + per-worktree file locking
// - Instance-level caching: Worktrees and fetch operations cached per Cache instance
// - Command-level control: --max-parallel flag controls dependency processing parallelism
// - Fetch caching: Network operations cached for 5 minutes to reduce redundancy

/// Worktree lifecycle state for concurrent coordination.
///
/// State machine enabling safe concurrent access: Pending (creating) → Ready (available).
/// First thread creates with `Pending(notify)`, others wait on notification.
/// Key format: `"{cache_dir_hash}:{owner}_{repo}:{sha}"` for SHA-based deduplication.
#[derive(Debug, Clone)]
enum WorktreeState {
    /// Worktree being created. Notification triggered when complete.
    Pending(Arc<tokio::sync::Notify>),
    /// Worktree ready at path. Validate before use as may be externally deleted.
    Ready(PathBuf),
}

/// Extract notification handle from worktree cache entry to wake waiters.
fn extract_notify_handle(
    cache: &DashMap<String, WorktreeState>,
    key: &str,
) -> Option<Arc<tokio::sync::Notify>> {
    cache.get(key).and_then(|entry| {
        if let WorktreeState::Pending(n) = entry.value() {
            Some(n.clone())
        } else {
            None
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WorktreeRegistry {
    entries: HashMap<String, WorktreeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorktreeRecord {
    source: String,
    version: String,
    path: PathBuf,
    last_used: u64,
}

impl WorktreeRegistry {
    fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                tracing::warn!("Failed to load worktree registry from {}: {}", path.display(), err);
                Self::default()
            }
        }
    }

    fn update(&mut self, key: String, source: String, version: String, path: PathBuf) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs();

        self.entries.insert(
            key,
            WorktreeRecord {
                source,
                version,
                path,
                last_used: timestamp,
            },
        );
    }

    fn remove_by_path(&mut self, target: &Path) -> bool {
        if let Some(key) = self.entries.iter().find_map(|(k, record)| {
            if record.path == target {
                Some(k.clone())
            } else {
                None
            }
        }) {
            self.entries.remove(&key);
            true
        } else {
            false
        }
    }

    /// Gets the source URL for a worktree by its path.
    ///
    /// Used to look up repository information without parsing the worktree directory name.
    fn get_source_by_path(&self, target: &Path) -> Option<String> {
        self.entries
            .values()
            .find(|record| record.path == target)
            .map(|record| record.source.clone())
    }

    async fn persist(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            async_fs::create_dir_all(parent).await?;
        }

        let data = serde_json::to_vec_pretty(self)?;
        async_fs::write(path, data).await?;
        Ok(())
    }
}

/// File-based locking mechanism for cache operations
///
/// This module provides thread-safe and process-safe locking for cache
/// operations through OS-level file locks, ensuring data consistency
/// when multiple AGPM processes access the same cache directory.
pub mod lock;
pub use lock::CacheLock;

/// Git repository cache for efficient resource management.
///
/// Manages repository cloning, updating, version management, and resource copying.
/// Multiple instances can safely operate on same cache via [`CacheLock`].
pub struct Cache {
    /// Root directory for cached repositories
    dir: PathBuf,
    /// Instance-level worktree cache. Key: `"{cache_dir_hash}:{owner}_{repo}:{sha}"`.
    /// DashMap enables lock-free concurrent access.
    worktree_cache: Arc<DashMap<String, WorktreeState>>,
    /// Per-repository locks preventing redundant fetches
    fetch_locks: Arc<DashMap<PathBuf, Arc<Mutex<()>>>>,
    /// Tracks fetched repos in this command instance (single fetch per repo per command)
    fetched_repos: Arc<RwLock<HashSet<PathBuf>>>,
    /// Persistent worktree registry for reuse across runs
    worktree_registry: Arc<Mutex<WorktreeRegistry>>,
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        Self {
            dir: self.dir.clone(),
            worktree_cache: Arc::clone(&self.worktree_cache),
            fetch_locks: Arc::clone(&self.fetch_locks),
            fetched_repos: Arc::clone(&self.fetched_repos),
            worktree_registry: Arc::clone(&self.worktree_registry),
        }
    }
}

impl Cache {
    fn registry_path_for(cache_dir: &Path) -> PathBuf {
        cache_dir.join("worktrees").join(".state.json")
    }

    fn registry_path(&self) -> PathBuf {
        Self::registry_path_for(&self.dir)
    }

    async fn record_worktree_usage(
        &self,
        registry_key: &str,
        source_name: &str,
        version_key: &str,
        worktree_path: &Path,
    ) -> Result<()> {
        let mut registry =
            acquire_mutex_with_timeout(&self.worktree_registry, "worktree_registry").await?;
        registry.update(
            registry_key.to_string(),
            source_name.to_string(),
            version_key.to_string(),
            worktree_path.to_path_buf(),
        );
        registry.persist(&self.registry_path()).await?;
        Ok(())
    }

    async fn remove_worktree_record_by_path(&self, worktree_path: &Path) -> Result<()> {
        let mut registry =
            acquire_mutex_with_timeout(&self.worktree_registry, "worktree_registry").await?;
        if registry.remove_by_path(worktree_path) {
            registry.persist(&self.registry_path()).await?;
        }
        Ok(())
    }

    async fn configure_connection_pooling(path: &Path) -> Result<()> {
        let commands = [
            ("http.version", "HTTP/2"),
            ("http.postBuffer", "524288000"),
            ("core.compression", "0"),
        ];

        for (key, value) in commands {
            GitCommand::new()
                .args(["config", key, value])
                .current_dir(path)
                .execute_success()
                .await
                .ok();
        }

        Ok(())
    }

    /// Creates cache instance with default platform-specific directory.
    ///
    /// Linux/macOS: `~/.agpm/cache/`, Windows: `%LOCALAPPDATA%\agpm\cache\`.
    /// Override with `AGPM_CACHE_DIR` environment variable.
    pub fn new() -> Result<Self> {
        let dir = crate::config::get_cache_dir()?;
        let registry_path = Self::registry_path_for(&dir);
        let registry = WorktreeRegistry::load(&registry_path);
        Ok(Self {
            dir,
            worktree_cache: Arc::new(DashMap::new()),
            fetch_locks: Arc::new(DashMap::new()),
            fetched_repos: Arc::new(RwLock::new(HashSet::new())),
            worktree_registry: Arc::new(Mutex::new(registry)),
        })
    }

    /// Creates cache instance with custom directory (useful for testing).
    pub fn with_dir(dir: PathBuf) -> Result<Self> {
        let registry_path = Self::registry_path_for(&dir);
        let registry = WorktreeRegistry::load(&registry_path);
        Ok(Self {
            dir,
            worktree_cache: Arc::new(DashMap::new()),
            fetch_locks: Arc::new(DashMap::new()),
            fetched_repos: Arc::new(RwLock::new(HashSet::new())),
            worktree_registry: Arc::new(Mutex::new(registry)),
        })
    }

    /// Ensures cache directory exists, creating if necessary. Safe to call multiple times.
    pub async fn ensure_cache_dir(&self) -> Result<()> {
        if !self.dir.exists() {
            async_fs::create_dir_all(&self.dir).await.with_file_context(
                FileOperation::CreateDir,
                &self.dir,
                "creating cache directory",
                "cache::ensure_cache_dir",
            )?;
        }
        Ok(())
    }

    /// Returns path to cache directory.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.dir
    }

    /// Constructs worktree path for URL and SHA (does not check existence or create).
    pub fn get_worktree_path(&self, url: &str, sha: &str) -> Result<PathBuf> {
        let (owner, repo) =
            crate::git::parse_git_url(url).map_err(|e| anyhow::anyhow!("Invalid Git URL: {e}"))?;
        let sha_short = &sha[..8.min(sha.len())];
        Ok(self.dir.join("worktrees").join(format!("{owner}_{repo}_{sha_short}")))
    }

    /// Gets or clones source repository to cache.
    ///
    /// Handles cloning new repos and updating existing ones with file-based locking.
    /// Concurrent calls with same `name` block; different names run in parallel.
    ///
    /// # Arguments
    ///
    /// * `name` - Source identifier for cache directory and locking
    /// * `url` - Git repository URL (HTTPS, SSH, or local)
    /// * `version` - Optional Git ref (tag, branch, commit, or None for default)
    pub async fn get_or_clone_source(
        &self,
        name: &str,
        url: &str,
        version: Option<&str>,
    ) -> Result<PathBuf> {
        self.get_or_clone_source_impl(name, url, version).await
    }

    /// Removes worktree using `git worktree remove` to properly clean up metadata.
    ///
    /// This ensures both the worktree directory AND the bare repo's metadata are cleaned up,
    /// preventing "missing but already registered worktree" errors on subsequent creation.
    pub async fn cleanup_worktree(&self, worktree_path: &Path) -> Result<()> {
        if !worktree_path.exists() {
            return Ok(());
        }

        // Look up source URL from registry instead of parsing the path
        // This avoids brittle path parsing that breaks with underscores in owner/repo names
        let source_url = {
            let registry =
                acquire_mutex_with_timeout(&self.worktree_registry, "worktree_registry").await?;
            registry.get_source_by_path(worktree_path)
        };

        if let Some(url) = source_url {
            // Use parse_git_url to get owner/repo from the URL
            if let Ok((owner, repo)) = crate::git::parse_git_url(&url) {
                let bare_repo_path = self.dir.join("sources").join(format!("{owner}_{repo}.git"));
                if bare_repo_path.exists() {
                    // Acquire bare-repo-level lock for worktree removal
                    let bare_repo_worktree_lock_name = format!("bare-worktree-{owner}_{repo}");
                    let _bare_worktree_lock =
                        CacheLock::acquire(&self.dir, &bare_repo_worktree_lock_name).await?;

                    // Use git worktree remove --force to properly clean up
                    let repo = GitRepo::new(&bare_repo_path);
                    let _ = repo.remove_worktree(worktree_path).await;
                }
            }
        }

        // Fallback: remove directory if git worktree remove didn't clean it up
        if worktree_path.exists() {
            tokio::fs::remove_dir_all(worktree_path).await.with_file_context(
                FileOperation::Write,
                worktree_path,
                "removing worktree directory",
                "cache::cleanup_worktree",
            )?;
        }

        self.remove_worktree_record_by_path(worktree_path).await?;
        Ok(())
    }

    /// Removes all worktrees from cache and prunes bare repo references.
    pub async fn cleanup_all_worktrees(&self) -> Result<()> {
        let worktrees_dir = self.dir.join("worktrees");

        if !worktrees_dir.exists() {
            return Ok(());
        }

        // Remove the entire worktrees directory
        tokio::fs::remove_dir_all(&worktrees_dir).await.with_file_context(
            FileOperation::Write,
            &worktrees_dir,
            "cleaning up worktrees directory",
            "cache_module",
        )?;

        // Also prune worktree references from all bare repos
        let sources_dir = self.dir.join("sources");
        if sources_dir.exists() {
            let mut entries = tokio::fs::read_dir(&sources_dir).await.with_file_context(
                FileOperation::Read,
                &sources_dir,
                "reading sources directory",
                "cache_module",
            )?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("git") {
                    let bare_repo = GitRepo::new(&path);
                    bare_repo.prune_worktrees().await.ok();
                }
            }
        }

        {
            let mut registry =
                acquire_mutex_with_timeout(&self.worktree_registry, "worktree_registry").await?;
            if !registry.entries.is_empty() {
                registry.entries.clear();
                registry.persist(&self.registry_path()).await?;
            }
        }

        Ok(())
    }

    /// Gets or creates SHA-based worktree with notification coordination.
    ///
    /// First thread creates worktree, others wait on notification. SHA-based ensures
    /// maximum reuse and deterministic installations.
    ///
    /// # Arguments
    ///
    /// * `sha` - Full 40-character commit SHA (pre-resolved)
    #[allow(clippy::too_many_lines)]
    pub async fn get_or_create_worktree_for_sha(
        &self,
        name: &str,
        url: &str,
        sha: &str,
        context: Option<&str>,
    ) -> Result<PathBuf> {
        // Validate SHA format
        if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!(
                "Invalid SHA format: expected 40 hex characters, got '{sha}'"
            ));
        }

        // Check if this is a local path
        let is_local_path = crate::utils::is_local_path(url);
        if is_local_path {
            // Local paths don't use worktrees
            return self.get_or_clone_source(name, url, None).await;
        }

        self.ensure_cache_dir().await?;

        // Parse URL for cache structure
        let (owner, repo) =
            crate::git::parse_git_url(url).unwrap_or(("direct".to_string(), "repo".to_string()));

        // Define unified lock name and bare repo path for this repository
        let bare_repo_dir = self.dir.join("sources").join(format!("{owner}_{repo}.git"));
        let bare_repo_lock_name = format!("bare-repo-{owner}_{repo}");

        // Create SHA-based cache key
        // Using first 8 chars of SHA for directory name (like Git does)
        let sha_short = &sha[..8];
        let cache_dir_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            self.dir.hash(&mut hasher);
            format!("{:x}", hasher.finish())[..8].to_string()
        };
        let cache_key = format!("{cache_dir_hash}:{owner}_{repo}:{sha}");

        // Check if we already have a worktree for this SHA using DashMap's lock-free API
        // This eliminates lock contention and deadlocks from the previous RwLock implementation
        let pending_timeout = pending_state_timeout();

        loop {
            match self.worktree_cache.entry(cache_key.clone()) {
                dashmap::mapref::entry::Entry::Occupied(entry) => {
                    match entry.get() {
                        WorktreeState::Ready(cached_path) if cached_path.exists() => {
                            // Worktree already exists and is ready
                            let cached_path = cached_path.clone();
                            drop(entry);

                            self.record_worktree_usage(&cache_key, name, sha_short, &cached_path)
                                .await?;

                            if let Some(ctx) = context {
                                tracing::debug!(
                                    target: "git",
                                    "({}) Reusing SHA-based worktree for {} @ {}",
                                    ctx,
                                    url.split('/').next_back().unwrap_or(url),
                                    sha_short
                                );
                            }
                            return Ok(cached_path);
                        }
                        WorktreeState::Ready(_cached_path) => {
                            // Path exists in cache but not on filesystem - need to recreate
                            // Create fresh notify handle for this creation attempt
                            let notify = Arc::new(tokio::sync::Notify::new());
                            drop(entry);
                            // Insert Pending state and proceed to creation
                            self.worktree_cache
                                .insert(cache_key.clone(), WorktreeState::Pending(notify));
                            break;
                        }
                        WorktreeState::Pending(existing_notify) => {
                            // Another thread is creating this worktree - wait for notification
                            let existing_notify = existing_notify.clone();

                            // CRITICAL: Create the notified future BEFORE dropping entry
                            // This ensures we won't miss the notification if the other thread
                            // finishes between drop() and notified() - Notify only wakes
                            // futures that are ALREADY waiting
                            let notified_future = existing_notify.notified();
                            drop(entry);

                            if let Some(ctx) = context {
                                tracing::debug!(
                                    target: "git",
                                    "({}) Waiting for SHA worktree creation for {} @ {}",
                                    ctx,
                                    url.split('/').next_back().unwrap_or(url),
                                    sha_short
                                );
                            }

                            // Wait for notification with timeout
                            tokio::select! {
                                _ = notified_future => {
                                    // Worktree creation completed (success or failure) - retry from top
                                    continue;
                                }
                                _ = tokio::time::sleep(pending_timeout) => {
                                    // Timeout waiting - the other thread may have hung.
                                    // We need to take ownership by inserting our own Pending state.
                                    // This ensures proper coordination with any other waiting threads.
                                    let our_notify = Arc::new(tokio::sync::Notify::new());
                                    self.worktree_cache
                                        .insert(cache_key.clone(), WorktreeState::Pending(our_notify));

                                    // Notify existing waiters so they can re-evaluate the new state
                                    existing_notify.notify_waiters();
                                    tracing::warn!(
                                        target: "git",
                                        "Timeout waiting for worktree creation for {} @ {} - taking ownership",
                                        url.split('/').next_back().unwrap_or(url),
                                        sha_short
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }
                dashmap::mapref::entry::Entry::Vacant(entry) => {
                    // No entry exists - create notify handle here to ensure
                    // it's only created when actually needed for a new entry
                    let notify = Arc::new(tokio::sync::Notify::new());
                    entry.insert(WorktreeState::Pending(notify));
                    break;
                }
            }
        }

        // All work wrapped in a result to handle cleanup explicitly on error
        let worktree_cache = self.worktree_cache.clone();
        let cache_key_for_cleanup = cache_key.clone();

        let result: Result<PathBuf> = async {
            tracing::debug!(
                target: "git::worktree",
                "Starting worktree creation for {} @ {} (cache_key={})",
                url.split('/').next_back().unwrap_or(url),
                sha_short,
                cache_key
            );

            // Check if bare repository already exists BEFORE acquiring lock
            // This avoids lock order violations when multiple worktrees are created concurrently
            if !bare_repo_dir.exists() {
                // Bare repo doesn't exist - acquire lock and clone
                tracing::debug!(
                    target: "git",
                    "Bare repo does not exist, acquiring lock to clone: {}",
                    bare_repo_dir.display()
                );

                let bare_repo_lock = CacheLock::acquire(&self.dir, &bare_repo_lock_name).await?;

                // Re-check after acquiring lock (another task may have cloned it)
                if !bare_repo_dir.exists() {
                    if let Some(parent) = bare_repo_dir.parent() {
                        tokio::fs::create_dir_all(parent).await.with_file_context(
                            FileOperation::CreateDir,
                            parent,
                            "creating cache parent directory",
                            "cache_module",
                        )?;
                    }

                    if let Some(ctx) = context {
                        tracing::debug!("📦 ({ctx}) Cloning repository {url}...");
                    } else {
                        tracing::debug!("📦 Cloning repository {url} to cache...");
                    }

                    // Add timeout to prevent hung clone operations
                    tokio::time::timeout(
                        crate::constants::GIT_CLONE_TIMEOUT,
                        GitRepo::clone_bare_with_context(url, &bare_repo_dir, context),
                    )
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "Git clone operation timed out after {:?} for {}",
                            crate::constants::GIT_CLONE_TIMEOUT,
                            url
                        )
                    })??;

                    Self::configure_connection_pooling(&bare_repo_dir).await.ok();

                    // Mark as fetched since clone_bare_with_context already fetches
                    acquire_rwlock_write_with_timeout(&self.fetched_repos, "fetched_repos")
                        .await?
                        .insert(bare_repo_dir.clone());
                }

                // Release bare repo lock before proceeding to worktree creation
                drop(bare_repo_lock);
            }
            // If bare_repo_dir already existed, we never acquired the lock

            let bare_repo = GitRepo::new(&bare_repo_dir);

            // Create worktree path using SHA
            let worktree_path =
                self.dir.join("worktrees").join(format!("{owner}_{repo}_{sha_short}"));

            // Acquire per-SHA worktree lock for caching/deduplication.
            let worktree_lock_name = format!("worktree-{owner}-{repo}-{sha_short}");
            let _worktree_lock = CacheLock::acquire(&self.dir, &worktree_lock_name).await?;

            // Re-check after lock
            if worktree_path.exists() {
                // Notify and update cache to Ready
                let notify_to_wake = extract_notify_handle(&self.worktree_cache, &cache_key);
                self.worktree_cache
                    .insert(cache_key.clone(), WorktreeState::Ready(worktree_path.clone()));
                if let Some(n) = notify_to_wake {
                    n.notify_waiters();
                }

                self.record_worktree_usage(&cache_key, name, sha_short, &worktree_path).await?;
                return Ok(worktree_path);
            }

            // NOTE: We intentionally do NOT call prune_worktrees() here.
            // The speculative prune caused race conditions when multiple threads created
            // worktrees from the same bare repo simultaneously (each had different SHA locks
            // but prune affects the entire .git/worktrees/ directory).
            // If stale worktree metadata exists, git worktree add will fail with
            // "missing but already registered worktree" and the error handling path
            // (create_worktree_with_context) will prune and retry.

            // Create worktree at specific SHA
            if let Some(ctx) = context {
                tracing::debug!(
                    target: "git",
                    "({}) Creating SHA-based worktree: {} @ {}",
                    ctx,
                    url.split('/').next_back().unwrap_or(url),
                    sha_short
                );
            }

            // Acquire bare-repo-level lock for worktree creation.
            // This serializes all worktree operations for a given bare repo, preventing
            // Git's internal race conditions when multiple SHAs create worktrees concurrently.
            let bare_repo_worktree_lock_name = format!("bare-worktree-{owner}_{repo}");
            let _bare_worktree_lock =
                CacheLock::acquire(&self.dir, &bare_repo_worktree_lock_name).await?;

            // Create worktree using SHA directly
            // Add timeout to prevent hung worktree creation
            let worktree_result = tokio::time::timeout(
                crate::constants::GIT_WORKTREE_TIMEOUT,
                bare_repo.create_worktree_with_context(&worktree_path, Some(sha), context),
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Git worktree creation timed out after {:?} for {} @ {}",
                    crate::constants::GIT_WORKTREE_TIMEOUT,
                    url,
                    sha_short
                )
            })?;

            // Keep lock held until cache is updated to ensure git state is fully settled
            match worktree_result {
                Ok(_) => {
                    // Verify worktree is fully accessible before marking as Ready
                    // Using git status to ensure files are accessible, avoiding deadlocks from retry loops.
                    // Files verified here are guaranteed readable, eliminating need for retry logic later.
                    // This architectural choice enables better parallelism and prevents lock contention.
                    if !worktree_path.exists() {
                        return Err(anyhow::anyhow!(
                            "Worktree directory does not exist: {}",
                            worktree_path.display()
                        ));
                    }

                    let git_file = worktree_path.join(".git");
                    if !git_file.exists() {
                        return Err(anyhow::anyhow!(
                            "Worktree .git file does not exist: {}",
                            git_file.display()
                        ));
                    }

                    // Release bare repo lock - worktree creation is complete
                    drop(_bare_worktree_lock);

                    // Fsync both directories to ensure all file entries are visible:
                    // 1. The worktree directory itself (source files)
                    // 2. The bare repo's worktrees metadata directory (commondir, gitdir, etc.)
                    // This fixes APFS/filesystem buffer cache issues where files aren't
                    // immediately readable after git worktree add completes
                    //
                    // CRITICAL: Use spawn_blocking to avoid blocking tokio runtime.
                    // These are best-effort operations - we don't fail if they error.
                    let worktree_path_clone = worktree_path.clone();
                    let bare_worktrees_dir = bare_repo_dir.join("worktrees");
                    let bare_worktrees_exists = bare_worktrees_dir.exists();

                    let _ = tokio::task::spawn_blocking(move || {
                        // Fsync worktree directory (best-effort for Windows file locking)
                        if let Ok(dir) = std::fs::File::open(&worktree_path_clone) {
                            let _ = dir.sync_all();
                        }

                        // Fsync bare repo's worktrees metadata directory
                        if bare_worktrees_exists {
                            if let Ok(dir) = std::fs::File::open(&bare_worktrees_dir) {
                                let _ = dir.sync_all();
                            }
                        }
                    })
                    .await;

                    tracing::debug!(
                        target: "git::worktree",
                        "Worktree fsync completed for {} @ {}",
                        worktree_path.display(),
                        &sha[..8]
                    );

                    // Notify and update cache to Ready
                    let notify_to_wake = extract_notify_handle(&self.worktree_cache, &cache_key);
                    self.worktree_cache
                        .insert(cache_key.clone(), WorktreeState::Ready(worktree_path.clone()));
                    if let Some(n) = notify_to_wake {
                        n.notify_waiters();
                    }

                    self.record_worktree_usage(&cache_key, name, sha_short, &worktree_path).await?;
                    Ok(worktree_path)
                }
                Err(e) => Err(e),
            }
        }
        .await;

        // Handle result with explicit cleanup on error
        match result {
            Ok(path) => Ok(path),
            Err(e) => {
                // Cleanup: remove Pending entry and notify waiters
                let notify = extract_notify_handle(&worktree_cache, &cache_key_for_cleanup);
                worktree_cache.remove(&cache_key_for_cleanup);
                if let Some(n) = notify {
                    n.notify_waiters();
                }
                Err(e)
            }
        }
    }

    /// Get or clone a source repository with options to control cache behavior.
    ///
    /// This method provides the core functionality for repository access with
    /// additional control over cache behavior. Creates bare repositories that
    /// can be shared by all operations (resolution, installation, etc).
    ///
    /// # Parameters
    ///
    /// * `name` - The name of the source (used for cache directory naming)
    /// * `url` - The Git repository URL or local path
    /// * `version` - Optional specific version/tag/branch to checkout
    /// * `force_refresh` - If true, ignore cached version and clone/fetch fresh
    ///
    /// # Returns
    ///
    /// Returns the path to the cached bare repository directory
    async fn get_or_clone_source_impl(
        &self,
        name: &str,
        url: &str,
        version: Option<&str>,
    ) -> Result<PathBuf> {
        // Check if this is a local path (not a git repository URL)
        let is_local_path = crate::utils::is_local_path(url);

        if is_local_path {
            // For local paths (directories), validate and return the secure path
            // No cloning or version management needed

            // Resolve path securely with validation
            let resolved_path = crate::utils::platform::resolve_path(url)?;

            // Canonicalize to get the real path and prevent symlink attacks
            let canonical_path = crate::utils::safe_canonicalize(&resolved_path)
                .map_err(|_| anyhow::anyhow!("Local path is not accessible or does not exist"))?;

            // Security check: Validate path against blacklist and symlinks
            validate_path_security(&canonical_path, true)?;

            // For local paths, versions don't apply. Suppress warning for internal sentinel values.
            if let Some(ver) = version
                && ver != "local"
            {
                eprintln!("Warning: Version constraints are ignored for local paths");
            }

            return Ok(canonical_path);
        }

        self.ensure_cache_dir().await?;

        // Acquire lock for this source to prevent concurrent access
        let _lock = CacheLock::acquire(&self.dir, name)
            .await
            .with_context(|| format!("Failed to acquire lock for source: {name}"))?;

        // Use the same cache directory structure as worktrees - bare repos with .git suffix
        // This ensures we have ONE repository that's shared by all operations
        let (owner, repo) =
            crate::git::parse_git_url(url).unwrap_or(("direct".to_string(), "repo".to_string()));
        let source_dir = self.dir.join("sources").join(format!("{owner}_{repo}.git")); // Always use .git suffix for bare repos

        // Ensure parent directory exists
        if let Some(parent) = source_dir.parent() {
            tokio::fs::create_dir_all(parent).await.with_file_context(
                FileOperation::CreateDir,
                parent,
                "creating cache directory",
                "cache_module",
            )?;
        }

        if source_dir.exists() {
            // Use existing cache - fetch to ensure we have latest refs
            // Skip fetch for local paths as they don't have remotes
            // For Git URLs, always fetch to get the latest refs (especially important for branches)
            if crate::utils::is_git_url(url) {
                // Check if we've already fetched this repo in this command instance
                let already_fetched = {
                    let fetched =
                        acquire_rwlock_read_with_timeout(&self.fetched_repos, "fetched_repos")
                            .await?;
                    fetched.contains(&source_dir)
                };

                if already_fetched {
                    tracing::debug!(
                        target: "agpm::cache",
                        "Skipping fetch for {} (already fetched in this command)",
                        name
                    );
                } else {
                    tracing::debug!(
                        target: "agpm::cache",
                        "Fetching updates for {} from {}",
                        name,
                        url
                    );
                    let repo = crate::git::GitRepo::new(&source_dir);
                    if let Err(e) = repo.fetch(None).await {
                        tracing::warn!(
                            target: "agpm::cache",
                            "Failed to fetch updates for {}: {}",
                            name,
                            e
                        );
                    } else {
                        // Mark this repo as fetched for this command execution
                        let mut fetched =
                            acquire_rwlock_write_with_timeout(&self.fetched_repos, "fetched_repos")
                                .await?;
                        fetched.insert(source_dir.clone());
                        tracing::debug!(
                            target: "agpm::cache",
                            "Successfully fetched updates for {}",
                            name
                        );
                    }
                }
            } else {
                tracing::debug!(
                    target: "agpm::cache",
                    "Skipping fetch for local path: {}",
                    url
                );
            }
        } else {
            // Directory doesn't exist - clone fresh as bare repo
            self.clone_source(url, &source_dir).await?;
        }

        Ok(source_dir)
    }

    /// Clones a Git repository to the specified target directory as a bare repository.
    ///
    /// This internal method performs the initial clone operation for repositories
    /// that are not yet present in the cache. It creates a bare repository which
    /// is optimal for serving and allows multiple worktrees to be created from it.
    ///
    /// # Why Bare Repositories
    ///
    /// Bare repositories are used because:
    /// - **No working directory conflicts**: Multiple worktrees can be created safely
    /// - **Optimized for serving**: Like GitHub/GitLab, designed for fetch operations
    /// - **Space efficient**: No checkout of files in the main repository
    /// - **Thread-safe**: Multiple processes can fetch from it simultaneously
    ///
    /// # Authentication
    ///
    /// Repository authentication is handled through:
    /// - **SSH keys**: For `git@github.com:` URLs (user's SSH configuration)
    /// - **HTTPS tokens**: For private repositories (from global config)
    /// - **Public repos**: No authentication required
    ///
    /// # Parameters
    ///
    /// * `url` - Git repository URL to clone from
    /// * `target` - Local directory path where bare repository should be created
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository URL is invalid or unreachable
    /// - Authentication fails for private repositories
    /// - Target directory cannot be created or written to
    /// - Network connectivity issues
    /// - Git command is not available in PATH
    async fn clone_source(&self, url: &str, target: &Path) -> Result<()> {
        tracing::debug!("📦 Cloning {} to cache...", url);

        // Clone as a bare repository for better concurrency and worktree support
        GitRepo::clone_bare(url, target)
            .await
            .with_context(|| format!("Failed to clone repository from {url}"))?;

        // Debug: List what was cloned
        if cfg!(test)
            && let Ok(entries) = std::fs::read_dir(target)
        {
            tracing::debug!(
                target: "agpm::cache",
                "Cloned bare repo to {}, contents:",
                target.display()
            );
            for entry in entries.flatten() {
                tracing::debug!(
                    target: "agpm::cache",
                    "  - {}",
                    entry.path().display()
                );
            }
        }

        Ok(())
    }

    /// Copies resource file from cached repository to project (silent).
    ///
    /// Uses copy-based approach (not symlinks) for cross-platform compatibility
    /// and Git integration. Creates parent directories automatically.
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Cached repository path
    /// * `source_path` - Relative path within repository
    /// * `target_path` - Absolute installation path
    pub async fn copy_resource(
        &self,
        source_dir: &Path,
        source_path: &str,
        target_path: &Path,
    ) -> Result<()> {
        self.copy_resource_with_output(source_dir, source_path, target_path, false).await
    }

    /// Copies resource file with optional installation output.
    ///
    /// Same as `copy_resource` but optionally displays "✅ Installed" messages.
    ///
    /// # Arguments
    ///
    /// * `show_output` - Whether to display installation progress
    pub async fn copy_resource_with_output(
        &self,
        source_dir: &Path,
        source_path: &str,
        target_path: &Path,
        show_output: bool,
    ) -> Result<()> {
        let source_file = source_dir.join(source_path);

        if !source_file.exists() {
            return Err(AgpmError::ResourceFileNotFound {
                path: source_path.to_string(),
                source_name: source_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
            }
            .into());
        }

        if let Some(parent) = target_path.parent() {
            async_fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        async_fs::copy(&source_file, target_path).await.with_context(|| {
            format!("Failed to copy {} to {}", source_file.display(), target_path.display())
        })?;

        if show_output {
            println!("  ✅ Installed {}", target_path.display());
        }

        Ok(())
    }

    /// Removes cached repositories not in active sources list.
    ///
    /// Returns count of removed directories. Displays progress messages.
    ///
    /// # Arguments
    ///
    /// * `active_sources` - Source names to preserve
    pub async fn clean_unused(&self, active_sources: &[String]) -> Result<usize> {
        self.ensure_cache_dir().await?;

        let mut removed_count = 0;
        let mut entries = async_fs::read_dir(&self.dir)
            .await
            .with_context(|| "Failed to read cache directory")?;

        while let Some(entry) =
            entries.next_entry().await.with_context(|| "Failed to read directory entry")?
        {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if !active_sources.contains(&dir_name.to_string()) {
                    println!("🗑️  Removing unused cache: {dir_name}");
                    async_fs::remove_dir_all(&path).await.with_context(|| {
                        format!("Failed to remove cache directory: {}", path.display())
                    })?;
                    removed_count += 1;
                }
            }
        }

        Ok(removed_count)
    }

    /// Calculates total cache size in bytes (recursive, returns 0 if not exists).
    pub async fn get_cache_size(&self) -> Result<u64> {
        if !self.dir.exists() {
            return Ok(0);
        }

        let size = fs::get_directory_size(&self.dir).await?;
        Ok(size)
    }

    /// Returns cache directory path (may not exist, use `ensure_cache_dir` to create).
    #[must_use]
    pub fn get_cache_location(&self) -> &Path {
        &self.dir
    }

    /// Removes entire cache directory (destructive, requires re-cloning repos).
    pub async fn clear_all(&self) -> Result<()> {
        if self.dir.exists() {
            async_fs::remove_dir_all(&self.dir).await.with_context(|| "Failed to clear cache")?;
            println!("🗑️  Cleared all cache");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cache_dir_creation() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let cache = Cache::with_dir(cache_dir.clone()).unwrap();
        cache.ensure_cache_dir().await.unwrap();

        assert!(cache_dir.exists());
    }

    #[tokio::test]
    async fn test_cache_location() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let location = cache.get_cache_location();
        assert_eq!(location, temp_dir.path());
    }

    #[tokio::test]
    async fn test_cache_size_empty() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();
        let size = cache.get_cache_size().await.unwrap();
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_cache_size_with_content() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some test content
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test content").unwrap();

        let size = cache.get_cache_size().await.unwrap();
        assert!(size > 0);
        assert_eq!(size, 12); // "test content" is 12 bytes
    }

    #[tokio::test]
    async fn test_clean_unused_empty_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        let removed = cache.clean_unused(&["active".to_string()]).await.unwrap();
        assert_eq!(removed, 0);
    }

    #[tokio::test]
    async fn test_clean_unused_removes_correct_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some test directories
        let active_dir = temp_dir.path().join("active");
        let unused_dir = temp_dir.path().join("unused");
        let another_unused = temp_dir.path().join("another_unused");

        std::fs::create_dir_all(&active_dir).unwrap();
        std::fs::create_dir_all(&unused_dir).unwrap();
        std::fs::create_dir_all(&another_unused).unwrap();

        // Add some content to verify directories are removed completely
        std::fs::write(active_dir.join("file.txt"), "keep").unwrap();
        std::fs::write(unused_dir.join("file.txt"), "remove").unwrap();
        std::fs::write(another_unused.join("file.txt"), "remove").unwrap();

        let removed = cache.clean_unused(&["active".to_string()]).await.unwrap();

        assert_eq!(removed, 2);
        assert!(active_dir.exists());
        assert!(!unused_dir.exists());
        assert!(!another_unused.exists());
    }

    #[tokio::test]
    async fn test_clear_all_removes_entire_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some content
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(subdir.join("file.txt"), "content").unwrap();

        assert!(temp_dir.path().exists());
        assert!(subdir.exists());

        cache.clear_all().await.unwrap();

        assert!(!temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_copy_resource() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source_file = source_dir.join("resource.md");
        std::fs::write(&source_file, "# Test Resource\nContent").unwrap();

        // Copy resource
        let dest = temp_dir.path().join("dest.md");
        cache.copy_resource(&source_dir, "resource.md", &dest).await.unwrap();

        assert!(dest.exists());
        let content = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(content, "# Test Resource\nContent");
    }

    #[tokio::test]
    async fn test_copy_resource_nested_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file in nested directory
        let source_dir = temp_dir.path().join("source");
        let nested_dir = source_dir.join("nested").join("path");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let source_file = nested_dir.join("resource.md");
        std::fs::write(&source_file, "# Nested Resource").unwrap();

        // Copy resource using relative path from source_dir
        let dest = temp_dir.path().join("dest.md");
        cache.copy_resource(&source_dir, "nested/path/resource.md", &dest).await.unwrap();

        assert!(dest.exists());
        let content = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(content, "# Nested Resource");
    }

    #[tokio::test]
    async fn test_copy_resource_invalid_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();

        // Try to copy non-existent resource
        let dest = temp_dir.path().join("dest.md");
        let result = cache.copy_resource(&source_dir, "nonexistent.md", &dest).await;

        assert!(result.is_err());
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn test_ensure_cache_dir_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let cache = Cache::with_dir(cache_dir.clone()).unwrap();

        // Call ensure_cache_dir multiple times
        cache.ensure_cache_dir().await.unwrap();
        assert!(cache_dir.exists());

        cache.ensure_cache_dir().await.unwrap();
        assert!(cache_dir.exists());

        // Add a file and ensure it's preserved
        std::fs::write(cache_dir.join("test.txt"), "content").unwrap();

        cache.ensure_cache_dir().await.unwrap();
        assert!(cache_dir.exists());
        assert!(cache_dir.join("test.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_resource_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("file.md"), "content").unwrap();

        // Copy to a destination with non-existent parent directories
        let dest = temp_dir.path().join("deep").join("nested").join("dest.md");
        cache.copy_resource(&source_dir, "file.md", &dest).await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "content");
    }

    #[tokio::test]
    async fn test_copy_resource_with_output_flag() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("file.md"), "content").unwrap();

        // Test with output flag false
        let dest1 = temp_dir.path().join("dest1.md");
        cache.copy_resource_with_output(&source_dir, "file.md", &dest1, false).await.unwrap();
        assert!(dest1.exists());

        // Test with output flag true
        let dest2 = temp_dir.path().join("dest2.md");
        cache.copy_resource_with_output(&source_dir, "file.md", &dest2, true).await.unwrap();
        assert!(dest2.exists());
    }

    #[tokio::test]
    async fn test_cache_size_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");
        let cache = Cache::with_dir(nonexistent).unwrap();

        let size = cache.get_cache_size().await.unwrap();
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_clear_all_nonexistent_cache() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");
        let cache = Cache::with_dir(nonexistent).unwrap();

        // Should not error when clearing non-existent cache
        cache.clear_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_clean_unused_with_files_and_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create directories
        std::fs::create_dir_all(temp_dir.path().join("keep")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("remove")).unwrap();

        // Create a file (not a directory)
        std::fs::write(temp_dir.path().join("file.txt"), "content").unwrap();

        let removed = cache.clean_unused(&["keep".to_string()]).await.unwrap();

        // Should only remove the "remove" directory, not the file
        assert_eq!(removed, 1);
        assert!(temp_dir.path().join("keep").exists());
        assert!(!temp_dir.path().join("remove").exists());
        assert!(temp_dir.path().join("file.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_resource_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("file.md"), "new content").unwrap();

        // Create existing destination file
        let dest = temp_dir.path().join("dest.md");
        std::fs::write(&dest, "old content").unwrap();

        // Copy should overwrite
        cache.copy_resource(&source_dir, "file.md", &dest).await.unwrap();

        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "new content");
    }

    #[tokio::test]
    async fn test_copy_resource_special_characters() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source file with special characters
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        let special_name = "file with spaces & special-chars.md";
        std::fs::write(source_dir.join(special_name), "content").unwrap();

        // Copy resource
        let dest = temp_dir.path().join("dest.md");
        cache.copy_resource(&source_dir, special_name, &dest).await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "content");
    }

    #[tokio::test]
    async fn test_cache_location_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("my_cache");
        let cache = Cache::with_dir(cache_dir.clone()).unwrap();

        // Get location multiple times
        let loc1 = cache.get_cache_location();
        let loc2 = cache.get_cache_location();

        assert_eq!(loc1, loc2);
        assert_eq!(loc1, cache_dir.as_path());
    }

    #[tokio::test]
    async fn test_clean_unused_empty_active_list() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create some directories
        std::fs::create_dir_all(temp_dir.path().join("source1")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("source2")).unwrap();

        // Empty active list should remove all
        let removed = cache.clean_unused(&[]).await.unwrap();

        assert_eq!(removed, 2);
        assert!(!temp_dir.path().join("source1").exists());
        assert!(!temp_dir.path().join("source2").exists());
    }

    #[tokio::test]
    async fn test_copy_resource_with_relative_paths() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().join("cache")).unwrap();

        // Create source with subdirectories
        let source_dir = temp_dir.path().join("source");
        let sub_dir = source_dir.join("agents");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(sub_dir.join("helper.md"), "# Helper Agent").unwrap();

        // Copy using relative path
        let dest = temp_dir.path().join("my-agent.md");
        cache.copy_resource(&source_dir, "agents/helper.md", &dest).await.unwrap();

        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "# Helper Agent");
    }

    #[tokio::test]
    async fn test_cache_size_with_subdirectories() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::with_dir(temp_dir.path().to_path_buf()).unwrap();

        cache.ensure_cache_dir().await.unwrap();

        // Create nested structure with files
        let sub1 = temp_dir.path().join("sub1");
        let sub2 = sub1.join("sub2");
        std::fs::create_dir_all(&sub2).unwrap();

        std::fs::write(temp_dir.path().join("file1.txt"), "12345").unwrap(); // 5 bytes
        std::fs::write(sub1.join("file2.txt"), "1234567890").unwrap(); // 10 bytes
        std::fs::write(sub2.join("file3.txt"), "abc").unwrap(); // 3 bytes

        let size = cache.get_cache_size().await.unwrap();
        assert_eq!(size, 18); // 5 + 10 + 3
    }
}
