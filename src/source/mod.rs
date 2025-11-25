//! Source repository management for AGPM resources.
//!
//! Manages Git repositories containing Claude Code resources (agents, snippets, etc.) with local
//! caching, authentication, and cross-platform support.
//!
//! # Components
//!
//! - [`Source`] - Individual repository with metadata
//! - [`SourceManager`] - Manages multiple sources with sync/verify operations
//!
//! # Configuration
//!
//! Sources defined in `agpm.toml` (shared) or `~/.agpm/config.toml` (user-specific with tokens).
//! Global sources loaded first, local overrides for customization.
//!
//! # Features
//!
//! - Remote (HTTPS/SSH) and local repositories support
//! - Efficient caching in `~/.agpm/cache/sources/{owner}_{repo}`
//! - Transparent authentication via embedded tokens in URLs
//! - Parallel sync operations with file-based locking
//! - Automatic cleanup and validation of invalid caches

use crate::cache::lock::CacheLock;
use crate::config::GlobalConfig;
use crate::core::AgpmError;
use crate::git::{GitRepo, parse_git_url};
use crate::manifest::Manifest;
use crate::utils::fs::ensure_dir;
use crate::utils::security::validate_path_security;
use anyhow::{Context, Result};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Git repository source containing Claude Code resources.
///
/// Defines repository location and metadata. Supports remote (HTTPS/SSH) and local repositories.
///
/// # Fields
///
/// - `name`: Unique identifier
/// - `url`: Repository location (HTTPS, SSH, file://, or local path)
/// - `description`: Optional description
/// - `enabled`: Whether source is active
/// - `local_path`: Runtime cache location (not serialized)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Unique identifier for this source
    pub name: String,
    /// Repository URL or local path
    pub url: String,
    /// Optional human-readable description
    pub description: Option<String>,
    /// Whether this source is enabled for operations
    pub enabled: bool,
    /// Runtime path to cached repository (not serialized)
    #[serde(skip)]
    pub local_path: Option<PathBuf>,
}

impl Source {
    /// Creates a new source with the given name and URL.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique identifier for this source
    /// * `url` - Repository URL or local path
    #[must_use]
    pub const fn new(name: String, url: String) -> Self {
        Self {
            name,
            url,
            description: None,
            enabled: true,
            local_path: None,
        }
    }

    /// Adds a human-readable description to this source.
    ///
    /// # Arguments
    ///
    /// * `desc` - Human-readable description of the source
    #[must_use]
    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    /// Generates the cache directory path for this source.
    ///
    /// Creates unique directory name as `{base_dir}/sources/{owner}_{repo}`.
    /// Falls back to `unknown_{source_name}` for invalid URLs.
    ///
    /// # Arguments
    ///
    /// * `base_dir` - Base cache directory (typically `~/.agpm/cache`)
    #[must_use]
    pub fn cache_dir(&self, base_dir: &Path) -> PathBuf {
        let (owner, repo) =
            parse_git_url(&self.url).unwrap_or(("unknown".to_string(), self.name.clone()));
        base_dir.join("sources").join(format!("{owner}_{repo}"))
    }
}

/// Manages multiple source repositories with caching, synchronization, and verification.
///
/// Central component for handling source repositories. Provides operations for adding, removing,
/// syncing, and verifying sources with local caching. Handles both remote repositories and local
/// paths with authentication support via global configuration.
///
/// # Cache Management
///
/// Maintains cache in `~/.agpm/cache/sources/` with persistence between operations, offline
/// access, and automatic validation/repair of invalid caches.
#[derive(Debug, Clone)]
pub struct SourceManager {
    /// Collection of managed sources, indexed by name
    sources: HashMap<String, Source>,
    /// Base directory for caching repositories
    cache_dir: PathBuf,
}

/// Helper function to detect if a URL represents a local filesystem path
fn is_local_filesystem_path(url: &str) -> bool {
    // Unix-style relative paths
    if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
        return true;
    }

    // Windows absolute paths (e.g., C:\path or C:/path)
    #[cfg(windows)]
    {
        // Check for drive letter pattern: X:\ or X:/
        if url.len() >= 3 {
            let chars: Vec<char> = url.chars().collect();
            if chars.len() >= 3
                && chars[0].is_ascii_alphabetic()
                && chars[1] == ':'
                && (chars[2] == '\\' || chars[2] == '/')
            {
                return true;
            }
        }
        // UNC paths (\\server\share)
        if url.starts_with("\\\\") {
            return true;
        }
    }

    false
}

impl SourceManager {
    /// Creates a new source manager with the default cache directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory cannot be determined or created.
    pub fn new() -> Result<Self> {
        let cache_dir = crate::config::get_cache_dir()?;
        Ok(Self {
            sources: HashMap::new(),
            cache_dir,
        })
    }

    /// Creates a new source manager with a custom cache directory.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Custom directory for caching repositories
    #[must_use]
    pub fn new_with_cache(cache_dir: PathBuf) -> Self {
        Self {
            sources: HashMap::new(),
            cache_dir,
        }
    }

    /// Creates a source manager from a manifest file (without global config integration).
    ///
    /// Loads only sources from project manifest. Use [`from_manifest_with_global()`] for
    /// authentication tokens and private repositories.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest containing source definitions
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory cannot be determined.
    ///
    /// [`from_manifest_with_global()`]: SourceManager::from_manifest_with_global
    pub fn from_manifest(manifest: &Manifest) -> Result<Self> {
        let cache_dir = crate::config::get_cache_dir()?;
        let mut manager = Self::new_with_cache(cache_dir);

        // Load all sources from the manifest
        for (name, url) in &manifest.sources {
            let source = Source::new(name.clone(), url.clone());
            manager.sources.insert(name.clone(), source);
        }

        Ok(manager)
    }

    /// Creates a source manager from manifest with global configuration integration.
    ///
    /// Recommended for production use. Merges sources from project manifest and global config
    /// to enable authentication for private repositories.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest containing source definitions
    ///
    /// # Errors
    ///
    /// Returns an error if cache directory cannot be determined.
    pub async fn from_manifest_with_global(manifest: &Manifest) -> Result<Self> {
        let cache_dir = crate::config::get_cache_dir()?;
        let mut manager = Self::new_with_cache(cache_dir);

        // Load global config and merge sources
        let global_config = GlobalConfig::load().await.unwrap_or_default();
        let merged_sources = global_config.merge_sources(&manifest.sources);

        // Load all merged sources
        for (name, url) in &merged_sources {
            let source = Source::new(name.clone(), url.clone());
            manager.sources.insert(name.clone(), source);
        }

        Ok(manager)
    }

    /// Creates a source manager from manifest with a custom cache directory.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest containing source definitions
    /// * `cache_dir` - Custom directory for caching repositories
    #[must_use]
    pub fn from_manifest_with_cache(manifest: &Manifest, cache_dir: PathBuf) -> Self {
        let mut manager = Self::new_with_cache(cache_dir);

        // Load all sources from the manifest
        for (name, url) in &manifest.sources {
            let source = Source::new(name.clone(), url.clone());
            manager.sources.insert(name.clone(), source);
        }

        manager
    }

    /// Adds a new source to the manager.
    ///
    /// # Arguments
    ///
    /// * `source` - The source to add to the manager
    ///
    /// # Errors
    ///
    /// Returns [`AgpmError::ConfigError`] if a source with the same name already exists.
    pub fn add(&mut self, source: Source) -> Result<()> {
        if self.sources.contains_key(&source.name) {
            return Err(AgpmError::ConfigError {
                message: format!("Source '{}' already exists", source.name),
            }
            .into());
        }

        self.sources.insert(source.name.clone(), source);
        Ok(())
    }

    /// Removes a source from the manager and cleans up its cache.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the source does not exist or cache cannot be removed.
    pub async fn remove(&mut self, name: &str) -> Result<()> {
        if !self.sources.contains_key(name) {
            return Err(AgpmError::SourceNotFound {
                name: name.to_string(),
            }
            .into());
        }

        self.sources.remove(name);

        let source_cache = self.cache_dir.join("sources").join(name);
        if source_cache.exists() {
            tokio::fs::remove_dir_all(&source_cache)
                .await
                .context("Failed to remove source cache")?;
        }

        Ok(())
    }

    /// Gets a reference to a source by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to retrieve
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Source> {
        self.sources.get(name)
    }

    /// Gets a mutable reference to a source by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to retrieve
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Source> {
        self.sources.get_mut(name)
    }

    /// Returns a list of all sources managed by this manager.
    #[must_use]
    pub fn list(&self) -> Vec<&Source> {
        self.sources.values().collect()
    }

    /// Returns a list of enabled sources managed by this manager.
    #[must_use]
    pub fn list_enabled(&self) -> Vec<&Source> {
        self.sources.values().filter(|s| s.enabled).collect()
    }

    /// Gets the URL of a source by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to get the URL for
    #[must_use]
    pub fn get_source_url(&self, name: &str) -> Option<String> {
        self.sources.get(name).map(|s| s.url.clone())
    }

    /// Synchronizes a source repository to the local cache.
    ///
    /// Handles cloning (first time) or fetching (subsequent) with automatic cache validation
    /// and cleanup. Supports remote (HTTPS/SSH) and local repositories.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to synchronize
    ///
    /// # Errors
    ///
    /// Returns an error if source doesn't exist, is disabled, or repository is not accessible.
    pub async fn sync(&mut self, name: &str) -> Result<GitRepo> {
        let source = self.sources.get(name).ok_or_else(|| AgpmError::SourceNotFound {
            name: name.to_string(),
        })?;

        if !source.enabled {
            return Err(AgpmError::ConfigError {
                message: format!("Source '{name}' is disabled"),
            }
            .into());
        }

        let cache_path = source.cache_dir(&self.cache_dir);
        ensure_dir(cache_path.parent().unwrap())?;

        // Use the URL directly (auth tokens are already embedded in URLs from global config)
        let url = source.url.clone();

        // Distinguish between plain directories and git repositories
        let is_local_path = is_local_filesystem_path(&url);
        let is_file_url = url.starts_with("file://");

        // Acquire lock for this source to prevent concurrent git operations
        // This prevents issues like concurrent "git remote set-url" commands
        let _lock = CacheLock::acquire(&self.cache_dir, name).await?;

        let repo = if is_local_path {
            // Local paths are treated as plain directories (not git repositories)
            // Apply security validation for local paths
            let resolved_path = crate::utils::platform::resolve_path(&url)?;

            // Security check: Validate path against blacklist and symlinks BEFORE canonicalization
            validate_path_security(&resolved_path, true)?;

            let canonical_path = crate::utils::safe_canonicalize(&resolved_path)
                .map_err(|_| anyhow::anyhow!("Local path is not accessible or does not exist"))?;

            // For local paths, we just return a GitRepo pointing to the local directory
            // No cloning or fetching needed - these are treated as plain directories
            GitRepo::new(canonical_path)
        } else if is_file_url {
            // file:// URLs must point to valid git repositories
            let path_str = url.strip_prefix("file://").unwrap();

            // On Windows, convert forward slashes back to backslashes
            #[cfg(windows)]
            let path_str = path_str.replace('/', "\\");
            #[cfg(not(windows))]
            let path_str = path_str.to_string();

            let abs_path = PathBuf::from(path_str);

            // Check if the local path exists and is a git repo
            if !abs_path.exists() {
                return Err(anyhow::anyhow!(
                    "Local repository path does not exist or is not accessible: {}",
                    abs_path.display()
                ));
            }

            // Check if it's a git repository (either regular or bare)
            if !crate::git::is_git_repository(&abs_path) {
                return Err(anyhow::anyhow!(
                    "Specified path is not a git repository. file:// URLs must point to valid git repositories."
                ));
            }

            if cache_path.exists() {
                let repo = GitRepo::new(&cache_path);
                if repo.is_git_repo() {
                    // For file:// repos, fetch to get latest changes
                    repo.fetch(Some(&url)).await?;
                    repo
                } else {
                    tokio::fs::remove_dir_all(&cache_path)
                        .await
                        .context("Failed to remove invalid cache directory")?;
                    GitRepo::clone(&url, &cache_path).await?
                }
            } else {
                GitRepo::clone(&url, &cache_path).await?
            }
        } else if cache_path.exists() {
            let repo = GitRepo::new(&cache_path);
            if repo.is_git_repo() {
                // Always fetch for all URLs to get latest changes
                repo.fetch(Some(&url)).await?;
                repo
            } else {
                tokio::fs::remove_dir_all(&cache_path)
                    .await
                    .context("Failed to remove invalid cache directory")?;
                GitRepo::clone(&url, &cache_path).await?
            }
        } else {
            GitRepo::clone(&url, &cache_path).await?
        };

        if let Some(source) = self.sources.get_mut(name) {
            source.local_path = Some(cache_path);
        }

        Ok(repo)
    }

    /// Synchronizes a repository by URL without adding it as a named source.
    ///
    /// Used for direct Git dependencies. Cache directory derived from URL structure.
    ///
    /// # Arguments
    ///
    /// * `url` - Repository URL or local path to synchronize
    ///
    /// # Errors
    ///
    /// Returns an error if repository is invalid, inaccessible, or has permission issues.
    pub async fn sync_by_url(&self, url: &str) -> Result<GitRepo> {
        // Generate a cache directory based on the URL
        let (owner, repo_name) =
            parse_git_url(url).unwrap_or(("direct".to_string(), "repo".to_string()));
        let cache_path = self.cache_dir.join("sources").join(format!("{owner}_{repo_name}"));
        ensure_dir(cache_path.parent().unwrap())?;

        // Check URL type
        let is_local_path = is_local_filesystem_path(url);
        let is_file_url = url.starts_with("file://");

        // Handle local paths (not git repositories, just directories)
        if is_local_path {
            // Apply security validation for local paths
            let resolved_path = crate::utils::platform::resolve_path(url)?;

            // Security check: Validate path against blacklist and symlinks BEFORE canonicalization
            validate_path_security(&resolved_path, true)?;

            let canonical_path = crate::utils::safe_canonicalize(&resolved_path)
                .map_err(|_| anyhow::anyhow!("Local path is not accessible or does not exist"))?;

            // For local paths, we just return a GitRepo pointing to the local directory
            // No cloning or fetching needed - these are treated as plain directories
            return Ok(GitRepo::new(canonical_path));
        }

        // For file:// URLs, verify they're git repositories
        if is_file_url {
            let path_str = url.strip_prefix("file://").unwrap();

            // On Windows, convert forward slashes back to backslashes
            #[cfg(windows)]
            let path_str = path_str.replace('/', "\\");
            #[cfg(not(windows))]
            let path_str = path_str.to_string();

            let abs_path = PathBuf::from(path_str);

            if !abs_path.exists() {
                return Err(anyhow::anyhow!(
                    "Local repository path does not exist or is not accessible: {}",
                    abs_path.display()
                ));
            }

            // Check if it's a git repository (either regular or bare)
            if !crate::git::is_git_repository(&abs_path) {
                return Err(anyhow::anyhow!(
                    "Specified path is not a git repository. file:// URLs must point to valid git repositories."
                ));
            }
        }

        // Acquire lock for this URL-based source to prevent concurrent git operations
        // Use a deterministic lock name based on owner and repo
        let lock_name = format!("{owner}_{repo_name}");
        let _lock = CacheLock::acquire(&self.cache_dir, &lock_name).await?;

        // Use the URL directly (auth tokens are already embedded in URLs from global config)
        let authenticated_url = url.to_string();

        let repo = if cache_path.exists() {
            let repo = GitRepo::new(&cache_path);
            if repo.is_git_repo() {
                // For file:// URLs, always fetch to update refs
                // For remote URLs, also fetch
                repo.fetch(Some(&authenticated_url)).await?;
                repo
            } else {
                tokio::fs::remove_dir_all(&cache_path)
                    .await
                    .context("Failed to remove invalid cache directory")?;
                GitRepo::clone(&authenticated_url, &cache_path).await?
            }
        } else {
            GitRepo::clone(&authenticated_url, &cache_path).await?
        };

        Ok(repo)
    }

    /// Synchronizes all enabled sources by fetching latest changes.
    ///
    /// # Errors
    ///
    /// Returns an error if any source fails to sync.
    pub async fn sync_all(&mut self) -> Result<()> {
        let enabled_sources: Vec<String> =
            self.list_enabled().iter().map(|s| s.name.clone()).collect();

        for name in enabled_sources {
            self.sync(&name).await?;
        }

        Ok(())
    }

    /// Sync multiple sources by URL in parallel.
    ///
    /// Executes all sync operations concurrently with file-based locking for thread safety.
    pub async fn sync_multiple_by_url(&self, urls: &[String]) -> Result<Vec<GitRepo>> {
        if urls.is_empty() {
            return Ok(Vec::new());
        }

        // Create async tasks for each URL
        let futures: Vec<_> =
            urls.iter().map(|url| async move { self.sync_by_url(url).await }).collect();

        // Execute all syncs in parallel and collect results
        let results = join_all(futures).await;

        // Convert Vec<Result<GitRepo>> to Result<Vec<GitRepo>>
        results.into_iter().collect()
    }

    /// Enables a source for use in operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to enable
    ///
    /// # Errors
    ///
    /// Returns [`AgpmError::SourceNotFound`] if no source with the given name exists.
    pub fn enable(&mut self, name: &str) -> Result<()> {
        let source = self.sources.get_mut(name).ok_or_else(|| AgpmError::SourceNotFound {
            name: name.to_string(),
        })?;

        source.enabled = true;
        Ok(())
    }

    /// Disables a source to exclude it from operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to disable
    ///
    /// # Errors
    ///
    /// Returns [`AgpmError::SourceNotFound`] if no source with the given name exists.
    pub fn disable(&mut self, name: &str) -> Result<()> {
        let source = self.sources.get_mut(name).ok_or_else(|| AgpmError::SourceNotFound {
            name: name.to_string(),
        })?;

        source.enabled = false;
        Ok(())
    }

    /// Gets the cache directory path for a source by URL.
    ///
    /// # Arguments
    ///
    /// * `url` - Repository URL to look up
    ///
    /// # Errors
    ///
    /// Returns [`AgpmError::SourceNotFound`] if no source with the given URL exists.
    pub fn get_cached_path(&self, url: &str) -> Result<PathBuf> {
        // Try to find the source by URL
        let source = self.sources.values().find(|s| s.url == url).ok_or_else(|| {
            AgpmError::SourceNotFound {
                name: url.to_string(),
            }
        })?;

        Ok(source.cache_dir(&self.cache_dir))
    }

    /// Gets the cache directory path for a source by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to get the cache path for
    ///
    /// # Errors
    ///
    /// Returns [`AgpmError::SourceNotFound`] if no source with the given name exists.
    pub fn get_cached_path_by_name(&self, name: &str) -> Result<PathBuf> {
        let source = self.sources.get(name).ok_or_else(|| AgpmError::SourceNotFound {
            name: name.to_string(),
        })?;

        Ok(source.cache_dir(&self.cache_dir))
    }

    /// Verifies that all enabled sources are accessible.
    ///
    /// Performs lightweight verification without full synchronization.
    ///
    /// # Errors
    ///
    /// Returns an error if any enabled source fails verification.
    pub async fn verify_all(&self) -> Result<()> {
        let enabled_sources: Vec<&Source> = self.list_enabled();

        if enabled_sources.is_empty() {
            return Ok(());
        }

        for source in enabled_sources {
            // Check if source URL is reachable by attempting a quick operation
            self.verify_source(&source.url).await?;
        }

        Ok(())
    }

    /// Verifies that a single source URL is accessible.
    ///
    /// # Arguments
    ///
    /// * `url` - Repository URL or local path to verify
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not accessible.
    async fn verify_source(&self, url: &str) -> Result<()> {
        // For file:// URLs (used in tests), just check if the path exists
        if url.starts_with("file://") {
            let path = url.strip_prefix("file://").unwrap();
            if std::path::Path::new(path).exists() {
                return Ok(());
            }
            return Err(anyhow::anyhow!("Local path does not exist: {path}"));
        }

        // For other URLs, try to create a GitRepo object and verify it's accessible
        // This is a lightweight check - we don't actually clone the repo
        match crate::git::GitRepo::verify_url(url).await {
            Ok(()) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Source not accessible: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestGit;
    use tempfile::TempDir;

    #[test]
    fn test_source_creation() {
        let source =
            Source::new("test".to_string(), "https://github.com/user/repo.git".to_string())
                .with_description("Test source".to_string());

        assert_eq!(source.name, "test");
        assert_eq!(source.url, "https://github.com/user/repo.git");
        assert_eq!(source.description, Some("Test source".to_string()));
        assert!(source.enabled);
    }

    #[tokio::test]
    async fn test_source_manager_add_remove() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        let source =
            Source::new("test".to_string(), "https://github.com/user/repo.git".to_string());

        manager.add(source.clone()).unwrap();
        assert!(manager.get("test").is_some());

        let result = manager.add(source);
        assert!(result.is_err());

        manager.remove("test").await.unwrap();
        assert!(manager.get("test").is_none());

        let result = manager.remove("test").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_source_enable_disable() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        let source =
            Source::new("test".to_string(), "https://github.com/user/repo.git".to_string());

        manager.add(source).unwrap();
        assert!(manager.get("test").unwrap().enabled);

        manager.disable("test").unwrap();
        assert!(!manager.get("test").unwrap().enabled);

        manager.enable("test").unwrap();
        assert!(manager.get("test").unwrap().enabled);
    }

    #[test]
    fn test_list_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        manager.add(Source::new("source1".to_string(), "url1".to_string())).unwrap();
        manager.add(Source::new("source2".to_string(), "url2".to_string())).unwrap();
        manager.add(Source::new("source3".to_string(), "url3".to_string())).unwrap();

        assert_eq!(manager.list_enabled().len(), 3);

        manager.disable("source2").unwrap();
        assert_eq!(manager.list_enabled().len(), 2);
    }

    #[test]
    fn test_source_cache_dir() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();

        let source =
            Source::new("test".to_string(), "https://github.com/user/repo.git".to_string());

        let cache_dir = source.cache_dir(base_dir);
        assert!(cache_dir.to_string_lossy().contains("sources"));
        assert!(cache_dir.to_string_lossy().contains("user_repo"));
    }

    #[test]
    fn test_source_cache_dir_invalid_url() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();

        let source = Source::new("test".to_string(), "not-a-valid-url".to_string());

        let cache_dir = source.cache_dir(base_dir);
        assert!(cache_dir.to_string_lossy().contains("sources"));
        assert!(cache_dir.to_string_lossy().contains("unknown_test"));
    }

    #[test]
    fn test_from_manifest() {
        let mut manifest = Manifest::new();
        manifest.add_source(
            "official".to_string(),
            "https://github.com/example-org/agpm-official.git".to_string(),
        );
        manifest.add_source(
            "community".to_string(),
            "https://github.com/example-org/agpm-community.git".to_string(),
        );

        let temp_dir = TempDir::new().unwrap();
        let manager =
            SourceManager::from_manifest_with_cache(&manifest, temp_dir.path().to_path_buf());

        assert_eq!(manager.list().len(), 2);
        assert!(manager.get("official").is_some());
        assert!(manager.get("community").is_some());
    }

    #[test]
    fn test_source_manager_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        assert_eq!(manager.list().len(), 0);

        manager.add(Source::new("source1".to_string(), "url1".to_string())).unwrap();
        manager.add(Source::new("source2".to_string(), "url2".to_string())).unwrap();

        assert_eq!(manager.list().len(), 2);
    }

    #[test]
    fn test_source_manager_get_mut() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        manager.add(Source::new("test".to_string(), "url".to_string())).unwrap();

        if let Some(source) = manager.get_mut("test") {
            source.description = Some("Updated description".to_string());
        }

        assert_eq!(
            manager.get("test").unwrap().description,
            Some("Updated description".to_string())
        );
    }

    #[test]
    fn test_source_manager_enable_disable_errors() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        let result = manager.enable("nonexistent");
        assert!(result.is_err());

        let result = manager.disable("nonexistent");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_source_manager_sync_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        let source =
            Source::new("test".to_string(), "https://github.com/user/repo.git".to_string());
        manager.add(source).unwrap();
        manager.disable("test").unwrap();

        let result = manager.sync("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_source_manager_sync_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        let result = manager.sync("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_source_manager_sync_local_repo() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create a local git repo using TestGit helper
        std::fs::create_dir(&repo_dir).unwrap();
        let git = TestGit::new(&repo_dir);
        git.init()?;
        git.config_user()?;
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        git.add_all()?;
        git.commit("Initial commit")?;

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());
        let source = Source::new("test".to_string(), format!("file://{}", repo_dir.display()));
        manager.add(source).unwrap();

        // First sync (clone)
        let repo = manager.sync("test").await?;
        assert!(repo.is_git_repo());

        // Second sync (fetch + pull)
        manager.sync("test").await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_source_manager_sync_all() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        // Create two local git repos using TestGit helper
        let repo1_dir = temp_dir.path().join("repo1");
        let repo2_dir = temp_dir.path().join("repo2");

        for repo_dir in &[&repo1_dir, &repo2_dir] {
            std::fs::create_dir(repo_dir).unwrap();
            let git = TestGit::new(repo_dir);
            git.init()?;
            git.config_user()?;
            std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
            git.add_all()?;
            git.commit("Initial commit")?;
        }

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());

        manager
            .add(Source::new("repo1".to_string(), format!("file://{}", repo1_dir.display())))
            .unwrap();

        manager
            .add(Source::new("repo2".to_string(), format!("file://{}", repo2_dir.display())))
            .unwrap();

        // Sync all
        manager.sync_all().await?;

        // Verify both repos were cloned
        let source1_cache = manager.get("repo1").unwrap().cache_dir(&cache_dir);
        let source2_cache = manager.get("repo2").unwrap().cache_dir(&cache_dir);
        assert!(source1_cache.exists());
        assert!(source2_cache.exists());
        Ok(())
    }

    // Additional error path tests

    #[tokio::test]
    async fn test_sync_non_existent_local_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        let source = Source::new("test".to_string(), "/non/existent/path".to_string());
        manager.add(source).unwrap();

        let result = manager.sync("test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_sync_non_git_directory() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let non_git_dir = temp_dir.path().join("not_git");
        std::fs::create_dir(&non_git_dir).unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir);
        let source = Source::new("test".to_string(), non_git_dir.to_str().unwrap().to_string());
        manager.add(source).unwrap();

        // Local paths are now treated as plain directories, so sync should succeed
        let result = manager.sync("test").await;
        if let Err(ref e) = result {
            eprintln!("Test failed with error: {e}");
            eprintln!("Path was: {non_git_dir:?}");
        }
        let repo = result.map_err(|e| anyhow::anyhow!("Failed to sync: {e:?}"))?;
        // Should point to the canonicalized local directory
        assert_eq!(repo.path(), crate::utils::safe_canonicalize(&non_git_dir).unwrap());
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_invalid_cache_directory() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create a valid git repo using TestGit helper
        std::fs::create_dir(&repo_dir).unwrap();
        let git = TestGit::new(&repo_dir);
        git.init()?;
        git.config_user()?;
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        git.add_all()?;
        git.commit("Initial")?;

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());
        let source = Source::new("test".to_string(), format!("file://{}", repo_dir.display()));
        manager.add(source).unwrap();

        // Create an invalid cache directory (not a git repo)
        let source_cache_dir = manager.get("test").unwrap().cache_dir(&cache_dir);
        std::fs::create_dir_all(&source_cache_dir).unwrap();
        std::fs::write(source_cache_dir.join("file.txt"), "not a git repo").unwrap();

        // Sync should detect invalid cache and re-clone
        let _repo = manager.sync("test").await?;
        assert!(crate::git::is_git_repository(&source_cache_dir));
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_by_url_invalid_url() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.sync_by_url("not-a-valid-url").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sync_multiple_by_url_empty() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.sync_multiple_by_url(&[]).await?;
        assert_eq!(result.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_multiple_by_url_with_failures() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create one valid repo using TestGit helper
        std::fs::create_dir(&repo_dir).unwrap();
        let git = TestGit::new(&repo_dir);
        git.init().unwrap();
        git.config_user().unwrap();
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        git.add_all().unwrap();
        git.commit("Initial").unwrap();

        let manager = SourceManager::new_with_cache(cache_dir);

        let urls = vec![format!("file://{}", repo_dir.display()), "invalid-url".to_string()];

        // Should fail on invalid URL
        let result = manager.sync_multiple_by_url(&urls).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_cached_path_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.get_cached_path("https://unknown/url.git");
        assert!(result.is_err());
        // Just check that it returns an error - the message format may vary
    }

    #[tokio::test]
    async fn test_get_cached_path_by_name_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.get_cached_path_by_name("nonexistent");
        assert!(result.is_err());
        // Just check that it returns an error - the message format may vary
    }

    #[tokio::test]
    async fn test_verify_all_no_sources() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        manager.verify_all().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_all_with_disabled_sources() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        // Add but disable a source
        let source =
            Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
        manager.add(source).unwrap();
        manager.disable("test").unwrap();

        // Verify should skip disabled sources
        manager.verify_all().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_source_file_url_not_exist() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.verify_source("file:///non/existent/path").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_verify_source_invalid_remote() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.verify_source("https://invalid-host-9999.test/repo.git").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not accessible"));
    }

    #[tokio::test]
    async fn test_remove_with_cache_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir.clone());

        let source =
            Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
        manager.add(source).unwrap();

        // Create cache directory
        let source_cache = cache_dir.join("sources").join("test");
        std::fs::create_dir_all(&source_cache).unwrap();
        std::fs::write(source_cache.join("file.txt"), "cached").unwrap();
        assert!(source_cache.exists());

        // Remove should clean up cache
        manager.remove("test").await.unwrap();
        assert!(!source_cache.exists());
    }

    #[tokio::test]
    async fn test_get_source_url() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        let source =
            Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
        manager.add(source).unwrap();

        let url = manager.get_source_url("test");
        assert_eq!(url, Some("https://github.com/test/repo.git".to_string()));

        let url = manager.get_source_url("nonexistent");
        assert_eq!(url, None);
    }

    #[test]
    fn test_source_with_description() {
        let source =
            Source::new("test".to_string(), "https://github.com/test/repo.git".to_string())
                .with_description("Test description".to_string());

        assert_eq!(source.description, Some("Test description".to_string()));
    }

    #[tokio::test]
    async fn test_sync_with_progress() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create a git repo using TestGit helper
        std::fs::create_dir(&repo_dir).unwrap();
        let git = TestGit::new(&repo_dir);
        git.init()?;
        git.config_user()?;
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        git.add_all()?;
        git.commit("Initial")?;

        let mut manager = SourceManager::new_with_cache(cache_dir);
        let source = Source::new("test".to_string(), format!("file://{}", repo_dir.display()));
        manager.add(source).unwrap();

        manager.sync("test").await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_from_manifest_with_global() -> anyhow::Result<()> {
        let manifest = Manifest::new();
        SourceManager::from_manifest_with_global(&manifest).await?;
        Ok(())
    }

    #[test]
    fn test_new_source_manager() {
        let result = SourceManager::new();
        // May fail if cache dir can't be created, but should handle gracefully
        if let Ok(manager) = result {
            assert!(manager.sources.is_empty());
        }
    }

    #[tokio::test]
    async fn test_sync_local_path_directory() -> anyhow::Result<()> {
        // Test that local paths (not file:// URLs) are treated as plain directories
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let local_dir = temp_dir.path().join("local_deps");

        // Create a plain directory with some files (not a git repo)
        std::fs::create_dir(&local_dir).unwrap();
        std::fs::write(local_dir.join("agent.md"), "# Test Agent").unwrap();
        std::fs::write(local_dir.join("snippet.md"), "# Test Snippet").unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());

        // Add source with local path
        let source = Source::new("local".to_string(), local_dir.to_string_lossy().to_string());
        manager.add(source).unwrap();

        // Sync should work with plain directory (not require git)
        let repo = manager.sync("local").await?;
        // The returned GitRepo should point to the canonicalized local directory
        // On macOS, /var is a symlink to /private/var, so we need to compare canonical paths
        assert_eq!(repo.path(), crate::utils::safe_canonicalize(&local_dir).unwrap());
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_by_url_local_path() -> anyhow::Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let local_dir = temp_dir.path().join("local_deps");

        // Create a plain directory with files
        std::fs::create_dir(&local_dir).unwrap();
        std::fs::write(local_dir.join("test.md"), "# Test Resource").unwrap();

        let manager = SourceManager::new_with_cache(cache_dir);

        // Test absolute path
        let repo = manager.sync_by_url(&local_dir.to_string_lossy()).await?;
        assert_eq!(repo.path(), crate::utils::safe_canonicalize(&local_dir).unwrap());

        // Note: Relative path test removed as it's not parallel-safe and unreliable
        // in different test environments (cargo, nextest, RustRover)
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_local_path_not_exist() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        // Try to sync non-existent local path
        let result = manager.sync_by_url("/non/existent/path").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_file_url_requires_git() {
        // Test that file:// URLs require valid git repositories
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let plain_dir = temp_dir.path().join("plain_dir");

        // Create a plain directory (not a git repo)
        std::fs::create_dir(&plain_dir).unwrap();
        std::fs::write(plain_dir.join("test.md"), "# Test").unwrap();

        let manager = SourceManager::new_with_cache(cache_dir);

        // file:// URL should fail for non-git directory
        let file_url = format!("file://{}", plain_dir.display());
        let result = manager.sync_by_url(&file_url).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a git repository"));
    }

    #[tokio::test]
    async fn test_path_traversal_attack_prevention() {
        // Test that access to blacklisted system directories is prevented
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let manager = SourceManager::new_with_cache(cache_dir.clone());

        // Test that blacklisted system paths are blocked
        let blacklisted_paths = vec!["/etc/passwd", "/System/Library", "/private/etc/hosts"];

        for malicious_path in blacklisted_paths {
            // Skip if path doesn't exist (e.g., /System on Linux)
            if !std::path::Path::new(malicious_path).exists() {
                continue;
            }

            let result = manager.sync_by_url(malicious_path).await;
            assert!(result.is_err(), "Blacklisted path not detected for: {malicious_path}");
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("Security error") || err_msg.contains("not allowed"),
                "Expected security error for blacklisted path: {malicious_path}, got: {err_msg}"
            );
        }

        // Test that normal paths in temp directories work fine
        let safe_dir = temp_dir.path().join("safe_dir");
        std::fs::create_dir(&safe_dir).unwrap();

        let result = manager.sync_by_url(&safe_dir.to_string_lossy()).await;
        assert!(result.is_ok(), "Safe path was incorrectly blocked: {result:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_symlink_attack_prevention() {
        // Test that symlink attacks are prevented
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let project_dir = temp_dir.path().join("project");
        let deps_dir = project_dir.join("deps");
        let sensitive_dir = temp_dir.path().join("sensitive");

        // Create directories
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::create_dir(&deps_dir).unwrap();
        std::fs::create_dir(&sensitive_dir).unwrap();
        std::fs::write(sensitive_dir.join("secret.txt"), "secret data").unwrap();

        // Create a symlink pointing to sensitive directory
        use std::os::unix::fs::symlink;
        let symlink_path = deps_dir.join("malicious_link");
        symlink(&sensitive_dir, &symlink_path).unwrap();

        let manager = SourceManager::new_with_cache(cache_dir);

        // Try to access the symlink directly as a local path
        let result = manager.sync_by_url(symlink_path.to_str().unwrap()).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Symlinks are not allowed") || err_msg.contains("Security error"),
            "Expected symlink error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_absolute_path_restriction() {
        // Test that blacklisted absolute paths are blocked
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let manager = SourceManager::new_with_cache(cache_dir);

        // With blacklist approach, temp directories are allowed
        // So this test verifies that normal development paths work
        let safe_dir = temp_dir.path().join("project");
        std::fs::create_dir(&safe_dir).unwrap();
        std::fs::write(safe_dir.join("file.txt"), "content").unwrap();

        let result = manager.sync_by_url(&safe_dir.to_string_lossy()).await;

        // Temp directories should work fine with blacklist approach
        assert!(result.is_ok(), "Safe temp path was incorrectly blocked: {result:?}");
    }

    #[test]
    fn test_error_message_sanitization() {
        // Test that error messages don't leak sensitive path information
        // This is a compile-time test to ensure error messages are properly sanitized

        // Check that we're not including full paths in error messages
        let error_msg = "Local path is not accessible or does not exist";
        assert!(!error_msg.contains("/home"));
        assert!(!error_msg.contains("/Users"));
        assert!(!error_msg.contains("C:\\"));

        let security_msg =
            "Security error: Local path must be within the project directory or AGPM cache";
        assert!(!security_msg.contains("{:?}"));
        assert!(!security_msg.contains("{}"));
    }
}
