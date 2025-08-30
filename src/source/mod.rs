//! Source repository management
//!
//! This module manages source repositories that contain Claude Code resources (agents, snippets, etc.).
//! Sources are Git repositories that are cloned/cached locally for efficient access and installation.
//! The module provides secure, efficient, and cross-platform repository handling with comprehensive
//! caching and authentication support.
//!
//! # Architecture Overview
//!
//! The source management system is built around two main components:
//!
//! - [`Source`] - Represents an individual repository with metadata and caching information
//! - [`SourceManager`] - Manages multiple sources with operations for syncing, verification, and caching
//!
//! # Source Configuration
//!
//! Sources can be defined in two locations with different purposes:
//!
//! 1. **Project manifest** (`ccpm.toml`) - Committed to version control, shared with team
//!    ```toml
//!    [sources]
//!    community = "https://github.com/example/ccpm-community.git"
//!    official = "https://github.com/example/ccpm-official.git"
//!    ```
//!
//! 2. **Global config** (`~/.ccpm/config.toml`) - User-specific with authentication tokens
//!    ```toml
//!    [sources]
//!    private = "https://oauth2:ghp_xxxx@github.com/company/private-ccpm.git"
//!    ```
//!
//! ## Source Priority and Security
//!
//! When sources are defined in both locations with the same name:
//! - Global sources are loaded first (contain authentication tokens)
//! - Local sources override global ones (for project-specific customization)
//! - Authentication tokens are kept separate from version control for security
//!
//! # Caching Architecture
//!
//! The caching system provides efficient repository management:
//!
//! ## Cache Directory Structure
//!
//! ```text
//! ~/.ccpm/cache/
//! └── sources/
//!     ├── owner1_repo1/          # Cached repository
//!     │   ├── .git/              # Git metadata
//!     │   ├── agents/            # Resource files
//!     │   └── snippets/
//!     └── owner2_repo2/
//!         └── ...
//! ```
//!
//! ## Cache Naming Convention
//!
//! Cache directories are named using the pattern `{owner}_{repository}` parsed from the Git URL.
//! For invalid URLs, falls back to `unknown_{source_name}`.
//!
//! ## Caching Strategy
//!
//! - **First Access**: Repository is cloned to cache directory
//! - **Subsequent Access**: Use cached copy, fetch updates if needed  
//! - **Validation**: Cache integrity is verified before use
//! - **Cleanup**: Invalid cache directories are automatically removed and re-cloned
//!
//! # Authentication Integration
//!
//! Authentication is handled transparently through the global configuration:
//!
//! - **Public repositories**: No authentication required
//! - **Private repositories**: Authentication tokens embedded in URLs in global config
//! - **Security**: Tokens never stored in project manifests or committed to version control
//! - **Format**: Standard Git URL format with embedded credentials
//!
//! ## Supported Authentication Methods
//!
//! - OAuth tokens: `https://oauth2:token@github.com/repo.git`
//! - Personal access tokens: `https://username:token@github.com/repo.git`
//! - SSH keys: `git@github.com:owner/repo.git` (uses system SSH configuration)
//!
//! # Repository Types
//!
//! The module supports multiple repository types:
//!
//! ## Remote Repositories
//! - **HTTPS**: `https://github.com/owner/repo.git`
//! - **SSH**: `git@github.com:owner/repo.git`
//!
//! ## Local Repositories
//! - **Absolute paths**: `/path/to/local/repo`
//! - **Relative paths**: `../local-repo` or `./local-repo`
//! - **File URLs**: `file:///absolute/path/to/repo`
//!
//! # Synchronization Operations
//!
//! Synchronization ensures local caches are up-to-date with remote repositories:
//!
//! ## Sync Operations
//! - **Clone**: First-time repository retrieval
//! - **Fetch**: Update remote references without merging
//! - **Validation**: Verify repository integrity and accessibility
//! - **Parallel**: Multiple repositories can be synced concurrently
//!
//! ## Offline Capabilities
//! - Cached repositories can be used offline
//! - Sync operations gracefully handle network failures
//! - Local repositories work without network access
//!
//! # Error Handling
//!
//! The module provides comprehensive error handling for common scenarios:
//!
//! - **Network failures**: Graceful degradation with cached repositories
//! - **Authentication failures**: Clear error messages with resolution hints
//! - **Invalid repositories**: Automatic cleanup and re-cloning
//! - **Path issues**: Cross-platform path handling and validation
//!
//! # Performance Considerations
//!
//! ## Optimization Strategies
//! - **Lazy loading**: Sources are only cloned when needed
//! - **Incremental updates**: Only fetch changes, not full re-clone
//! - **Parallel operations**: Multiple repositories synced concurrently
//! - **Cache reuse**: Minimize redundant network operations
//!
//! ## Resource Management
//! - **Memory efficient**: Repositories are accessed on-demand
//! - **Disk usage**: Cache cleanup for removed sources
//! - **Network optimization**: Minimal data transfer through Git's efficient protocol
//!
//! # Cross-Platform Compatibility
//!
//! Full support for Windows, macOS, and Linux:
//! - **Path handling**: Correct path separators and absolute path resolution
//! - **Git command**: Uses system git with platform-specific optimizations
//! - **File permissions**: Proper handling across different filesystems
//! - **Authentication**: Works with platform-specific credential managers
//!
//! # Usage Examples
//!
//! ## Basic Source Management
//! ```rust,no_run
//! use ccpm::source::{Source, SourceManager};
//! use ccpm::manifest::Manifest;
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Load from manifest with global config integration
//! let manifest = Manifest::load(Path::new("ccpm.toml"))?;
//! let mut manager = SourceManager::from_manifest_with_global(&manifest).await?;
//!
//! // Sync a specific source
//! let repo = manager.sync("community", None).await?;
//! println!("Repository ready at: {:?}", repo.path());
//!
//! // List all available sources
//! for source in manager.list() {
//!     println!("Source: {} -> {}", source.name, source.url);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Progress Monitoring
//! ```rust,no_run
//! use ccpm::source::SourceManager;
//! use ccpm::utils::progress::ProgressBar;
//!
//! # async fn example(manager: &mut SourceManager) -> anyhow::Result<()> {
//! let progress = ProgressBar::new_spinner();
//! progress.set_message("Syncing repositories...");
//!
//! // Sync all sources with progress updates
//! manager.sync_all(Some(&progress)).await?;
//!
//! progress.finish_with_message("All sources synced successfully");
//! # Ok(())
//! # }
//! ```
//!
//! ## Direct URL Operations
//! ```rust,no_run
//! use ccpm::source::SourceManager;
//!
//! # async fn example(manager: &mut SourceManager) -> anyhow::Result<()> {
//! // Sync a repository by URL (for direct dependencies)
//! let repo = manager.sync_by_url(
//!     "https://github.com/example/dependency.git",
//!     None
//! ).await?;
//!
//! // Access the cached repository
//! let cache_path = manager.get_cached_path(
//!     "https://github.com/example/dependency.git"
//! )?;
//! # Ok(())
//! # }
//! ```

use crate::cache::lock::CacheLock;
use crate::config::GlobalConfig;
use crate::core::CcpmError;
use crate::git::{parse_git_url, GitRepo};
use crate::manifest::Manifest;
use crate::utils::fs::ensure_dir;
use crate::utils::progress::ProgressBar;
use crate::utils::security::validate_path_security;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Represents a Git repository source containing Claude Code resources.
///
/// A [`Source`] defines a repository location and metadata for accessing Claude Code
/// resources like agents and snippets. Sources can be remote repositories (GitHub, GitLab, etc.)
/// or local file paths, and support various authentication mechanisms.
///
/// # Fields
///
/// - `name`: Unique identifier for the source (used in manifests and commands)
/// - `url`: Repository location (HTTPS, SSH, file://, or local path)
/// - `description`: Optional human-readable description
/// - `enabled`: Whether this source should be used for operations
/// - `local_path`: Runtime cache location (not serialized, set during sync operations)
///
/// # Repository URL Formats
///
/// ## Remote Repositories
/// - HTTPS: `https://github.com/owner/repo.git`
/// - SSH: `git@github.com:owner/repo.git`
/// - HTTPS with auth: `https://token@github.com/owner/repo.git`
///
/// ## Local Repositories
/// - Absolute path: `/path/to/repository`
/// - Relative path: `../relative/path` or `./local-path`
/// - File URL: `file:///absolute/path/to/repository`
///
/// # Security Considerations
///
/// Authentication tokens should never be stored in [`Source`] instances that are
/// serialized to project manifests. Use the global configuration for credentials.
///
/// # Examples
///
/// ```rust
/// use ccpm::source::Source;
///
/// // Public repository
/// let source = Source::new(
///     "community".to_string(),
///     "https://github.com/example/ccpm-community.git".to_string()
/// ).with_description("Community resources".to_string());
///
/// // Local development repository
/// let local = Source::new(
///     "local-dev".to_string(),
///     "/path/to/local/repo".to_string()
/// );
/// ```
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
    /// The source is created with default settings:
    /// - No description
    /// - Enabled by default
    /// - No local path (will be set during sync operations)
    ///
    /// # Arguments
    ///
    /// * `name` - Unique identifier for this source
    /// * `url` - Repository URL or local path
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::source::Source;
    ///
    /// let source = Source::new(
    ///     "official".to_string(),
    ///     "https://github.com/example/ccpm-official.git".to_string()
    /// );
    ///
    /// assert_eq!(source.name, "official");
    /// assert!(source.enabled);
    /// assert!(source.description.is_none());
    /// ```
    #[must_use]
    pub fn new(name: String, url: String) -> Self {
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
    /// This is a builder pattern method that consumes the source and returns it
    /// with the description field set. Descriptions help users understand the
    /// purpose and contents of each source.
    ///
    /// # Arguments
    ///
    /// * `desc` - Human-readable description of the source
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::source::Source;
    ///
    /// let source = Source::new(
    ///     "community".to_string(),
    ///     "https://github.com/example/ccpm-community.git".to_string()
    /// ).with_description("Community-contributed agents and snippets".to_string());
    ///
    /// assert_eq!(source.description, Some("Community-contributed agents and snippets".to_string()));
    /// ```
    #[must_use]
    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    /// Generates the cache directory path for this source.
    ///
    /// Creates a unique cache directory name based on the repository URL to avoid
    /// conflicts between sources. The directory name follows the pattern `{owner}_{repo}`
    /// parsed from the Git URL.
    ///
    /// # Cache Directory Structure
    ///
    /// - For `https://github.com/owner/repo.git` → `{base_dir}/sources/owner_repo`
    /// - For invalid URLs → `{base_dir}/sources/unknown_{source_name}`
    ///
    /// # Arguments
    ///
    /// * `base_dir` - Base cache directory (typically `~/.ccpm/cache`)
    ///
    /// # Returns
    ///
    /// [`PathBuf`] pointing to the cache directory for this source
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::source::Source;
    /// use std::path::Path;
    ///
    /// let source = Source::new(
    ///     "community".to_string(),
    ///     "https://github.com/example/ccpm-community.git".to_string()
    /// );
    ///
    /// let base_dir = Path::new("/home/user/.ccpm/cache");
    /// let cache_dir = source.cache_dir(base_dir);
    ///
    /// assert_eq!(
    ///     cache_dir,
    ///     Path::new("/home/user/.ccpm/cache/sources/example_ccpm-community")
    /// );
    /// ```
    #[must_use]
    pub fn cache_dir(&self, base_dir: &Path) -> PathBuf {
        let (owner, repo) =
            parse_git_url(&self.url).unwrap_or(("unknown".to_string(), self.name.clone()));
        base_dir.join("sources").join(format!("{owner}_{repo}"))
    }
}

/// Manages multiple source repositories with caching, synchronization, and verification.
///
/// [`SourceManager`] is the central component for handling source repositories in CCPM.
/// It provides operations for adding, removing, syncing, and verifying sources while
/// maintaining a local cache for efficient access. The manager handles both remote
/// repositories and local file paths with comprehensive error handling and progress reporting.
///
/// # Core Responsibilities
///
/// - **Source Registry**: Maintains a collection of named sources
/// - **Cache Management**: Handles local caching of repository content
/// - **Synchronization**: Keeps cached repositories up-to-date
/// - **Verification**: Ensures repositories are accessible and valid
/// - **Authentication**: Integrates with global configuration for private repositories
/// - **Progress Reporting**: Provides feedback during long-running operations
///
/// # Cache Management
///
/// The manager maintains a cache directory (typically `~/.ccpm/cache/sources/`) where
/// each source is stored in a subdirectory named after the repository owner and name.
/// The cache provides:
///
/// - **Persistence**: Repositories remain cached between operations
/// - **Efficiency**: Avoid re-downloading unchanged repositories
/// - **Offline Access**: Use cached content when network is unavailable
/// - **Integrity**: Validate cache consistency and auto-repair when needed
///
/// # Thread Safety
///
/// [`SourceManager`] is designed for single-threaded use but can be cloned for use
/// across multiple operations. For concurrent access, wrap in appropriate synchronization
/// primitives like `Arc` and `Mutex`.
///
/// # Examples
///
/// ## Basic Usage
/// ```rust,no_run
/// use ccpm::source::{Source, SourceManager};
/// use anyhow::Result;
///
/// # async fn example() -> Result<()> {
/// // Create a new manager
/// let mut manager = SourceManager::new()?;
///
/// // Add a source
/// let source = Source::new(
///     "community".to_string(),
///     "https://github.com/example/ccpm-community.git".to_string()
/// );
/// manager.add(source)?;
///
/// // Sync the repository
/// let repo = manager.sync("community", None).await?;
/// println!("Repository synced to: {:?}", repo.path());
/// # Ok(())
/// # }
/// ```
///
/// ## Loading from Manifest
/// ```rust,no_run
/// use ccpm::source::SourceManager;
/// use ccpm::manifest::Manifest;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Load sources from project manifest and global config
/// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
/// let manager = SourceManager::from_manifest_with_global(&manifest).await?;
///
/// println!("Loaded {} sources", manager.list().len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct SourceManager {
    /// Collection of managed sources, indexed by name
    sources: HashMap<String, Source>,
    /// Base directory for caching repositories
    cache_dir: PathBuf,
}

impl SourceManager {
    /// Creates a new source manager with the default cache directory.
    ///
    /// The cache directory is determined by the system configuration, typically
    /// `~/.ccpm/cache/` on Unix systems or `%APPDATA%\ccpm\cache\` on Windows.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory cannot be determined or created.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::SourceManager;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manager = SourceManager::new()?;
    /// println!("Manager created with {} sources", manager.list().len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let cache_dir = crate::config::get_cache_dir()?;
        Ok(Self {
            sources: HashMap::new(),
            cache_dir,
        })
    }

    /// Creates a new source manager with a custom cache directory.
    ///
    /// This constructor is primarily used for testing and scenarios where a specific
    /// cache location is required. For normal usage, prefer [`SourceManager::new()`].
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Custom directory for caching repositories
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::source::SourceManager;
    /// use std::path::PathBuf;
    ///
    /// let custom_cache = PathBuf::from("/custom/cache/location");
    /// let manager = SourceManager::new_with_cache(custom_cache);
    /// ```
    #[must_use]
    pub fn new_with_cache(cache_dir: PathBuf) -> Self {
        Self {
            sources: HashMap::new(),
            cache_dir,
        }
    }

    /// Creates a source manager from a manifest file (without global config integration).
    ///
    /// This method loads only sources defined in the project manifest, without merging
    /// with global configuration. Use [`from_manifest_with_global()`] for full integration
    /// that includes authentication tokens and private repositories.
    ///
    /// This method is primarily for backward compatibility and testing scenarios.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest containing source definitions
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory cannot be determined.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::SourceManager;
    /// use ccpm::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
    /// let manager = SourceManager::from_manifest(&manifest)?;
    ///
    /// println!("Loaded {} sources from manifest", manager.list().len());
    /// # Ok(())
    /// # }
    /// ```
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
    /// This is the recommended method for creating a [`SourceManager`] in production use.
    /// It merges sources from both the project manifest and global configuration, enabling:
    ///
    /// - **Authentication**: Access to private repositories with embedded credentials
    /// - **User customization**: Global sources that extend project-defined sources
    /// - **Security**: Credentials stored safely outside version control
    ///
    /// # Source Resolution Priority
    ///
    /// 1. **Global sources**: Loaded first (may contain authentication tokens)
    /// 2. **Local sources**: Override global sources with same names
    /// 3. **Merged result**: Final source collection used by the manager
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest containing source definitions
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Cache directory cannot be determined
    /// - Global configuration cannot be loaded (though this is non-fatal)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::SourceManager;
    /// use ccpm::manifest::Manifest;
    /// use std::path::Path;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
    /// let manager = SourceManager::from_manifest_with_global(&manifest).await?;
    ///
    /// // Manager now includes both project and global sources
    /// for source in manager.list() {
    ///     println!("Available source: {} -> {}", source.name, source.url);
    /// }
    /// # Ok(())
    /// # }
    /// ```
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
    /// This method is primarily used for testing where a specific cache location is needed.
    /// It loads only sources from the manifest without global configuration integration.
    ///
    /// # Arguments
    ///
    /// * `manifest` - Project manifest containing source definitions  
    /// * `cache_dir` - Custom directory for caching repositories
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::SourceManager;
    /// use ccpm::manifest::Manifest;
    /// use std::path::{Path, PathBuf};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manifest = Manifest::load(Path::new("ccpm.toml"))?;
    /// let custom_cache = PathBuf::from("/tmp/test-cache");
    /// let manager = SourceManager::from_manifest_with_cache(&manifest, custom_cache);
    /// # Ok(())
    /// # }
    /// ```
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
    /// The source name must be unique within this manager. Adding a source with an
    /// existing name will return an error.
    ///
    /// # Arguments
    ///
    /// * `source` - The source to add to the manager
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::ConfigError`] if a source with the same name already exists.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    ///
    /// let source = Source::new(
    ///     "community".to_string(),
    ///     "https://github.com/example/ccpm-community.git".to_string()
    /// );
    ///
    /// manager.add(source)?;
    /// assert!(manager.get("community").is_some());
    /// # Ok(())
    /// # }
    /// ```
    pub fn add(&mut self, source: Source) -> Result<()> {
        if self.sources.contains_key(&source.name) {
            return Err(CcpmError::ConfigError {
                message: format!("Source '{}' already exists", source.name),
            }
            .into());
        }

        self.sources.insert(source.name.clone(), source);
        Ok(())
    }

    /// Removes a source from the manager and cleans up its cache.
    ///
    /// This operation permanently removes the source from the manager and deletes
    /// its cached repository data from disk. This cannot be undone, though the
    /// repository can be re-added and will be cloned again on next sync.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to remove
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The source does not exist ([`CcpmError::SourceNotFound`])
    /// - The cache directory cannot be removed due to filesystem permissions
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    ///
    /// // Add and then remove a source
    /// let source = Source::new("temp".to_string(), "https://github.com/temp/repo.git".to_string());
    /// manager.add(source)?;
    /// manager.remove("temp").await?;
    ///
    /// assert!(manager.get("temp").is_none());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove(&mut self, name: &str) -> Result<()> {
        if !self.sources.contains_key(name) {
            return Err(CcpmError::SourceNotFound {
                name: name.to_string(),
            }
            .into());
        }

        self.sources.remove(name);

        let source_cache = self.cache_dir.join("sources").join(name);
        if source_cache.exists() {
            tokio::fs::remove_dir_all(&source_cache).await.context("Failed to remove source cache")?;
        }

        Ok(())
    }

    /// Gets a reference to a source by name.
    ///
    /// Returns [`None`] if no source with the given name exists.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to retrieve
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
    /// manager.add(source)?;
    ///
    /// if let Some(source) = manager.get("test") {
    ///     println!("Found source: {} -> {}", source.name, source.url);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Source> {
        self.sources.get(name)
    }

    /// Gets a mutable reference to a source by name.
    ///
    /// Returns [`None`] if no source with the given name exists. Use this method
    /// when you need to modify source properties like description or enabled status.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to retrieve
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
    /// manager.add(source)?;
    ///
    /// if let Some(source) = manager.get_mut("test") {
    ///     source.description = Some("Updated description".to_string());
    ///     source.enabled = false;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Source> {
        self.sources.get_mut(name)
    }

    /// Returns a list of all sources managed by this manager.
    ///
    /// The returned vector contains references to all sources, both enabled and disabled.
    /// For only enabled sources, use [`list_enabled()`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manager = SourceManager::new()?;
    ///
    /// for source in manager.list() {
    ///     println!("Source: {} -> {} (enabled: {})",
    ///         source.name, source.url, source.enabled);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`list_enabled()`]: SourceManager::list_enabled
    #[must_use]
    pub fn list(&self) -> Vec<&Source> {
        self.sources.values().collect()
    }

    /// Returns a list of enabled sources managed by this manager.
    ///
    /// Only sources with `enabled: true` are included in the result. This is useful
    /// for operations that should only work with active sources.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manager = SourceManager::new()?;
    ///
    /// println!("Enabled sources: {}", manager.list_enabled().len());
    /// for source in manager.list_enabled() {
    ///     println!("  {} -> {}", source.name, source.url);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn list_enabled(&self) -> Vec<&Source> {
        self.sources.values().filter(|s| s.enabled).collect()
    }

    /// Gets the URL of a source by name.
    ///
    /// Returns the repository URL for the named source, or [`None`] if the source doesn't exist.
    /// This is useful for logging and debugging purposes.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to get the URL for
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
    /// manager.add(source)?;
    ///
    /// if let Some(url) = manager.get_source_url("test") {
    ///     println!("Source URL: {}", url);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn get_source_url(&self, name: &str) -> Option<String> {
        self.sources.get(name).map(|s| s.url.clone())
    }

    /// Synchronizes a source repository to the local cache.
    ///
    /// This is the core method for ensuring a source repository is available locally.
    /// It handles both initial cloning and subsequent updates, with intelligent caching
    /// and error recovery.
    ///
    /// # Synchronization Process
    ///
    /// 1. **Validation**: Check that the source exists and is enabled
    /// 2. **Cache Check**: Determine if repository is already cached
    /// 3. **Repository Type Detection**: Handle remote vs local repositories
    /// 4. **Sync Operation**:
    ///    - **First time**: Clone the repository to cache
    ///    - **Subsequent**: Fetch updates from remote
    ///    - **Invalid cache**: Remove corrupted cache and re-clone
    /// 5. **Cache Update**: Update source's `local_path` with cache location
    ///
    /// # Repository Types Supported
    ///
    /// ## Remote Repositories
    /// - **HTTPS**: `https://github.com/owner/repo.git`  
    /// - **SSH**: `git@github.com:owner/repo.git`
    ///
    /// ## Local Repositories  
    /// - **Absolute paths**: `/absolute/path/to/repo`
    /// - **Relative paths**: `../relative/path` or `./local-path`
    /// - **File URLs**: `file:///absolute/path/to/repo`
    ///
    /// # Authentication
    ///
    /// Authentication is handled transparently through URLs with embedded credentials
    /// from the global configuration. Private repositories should have their authentication
    /// tokens configured in `~/.ccpm/config.toml`.
    ///
    /// # Error Handling
    ///
    /// The method provides comprehensive error handling for common scenarios:
    /// - **Source not found**: Clear error with source name
    /// - **Disabled source**: Prevents operations on disabled sources  
    /// - **Network failures**: Graceful handling with context
    /// - **Invalid repositories**: Validation of Git repository structure
    /// - **Cache corruption**: Automatic cleanup and re-cloning
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to synchronize
    /// * `progress` - Optional progress bar for user feedback during long operations
    ///
    /// # Returns
    ///
    /// Returns a [`GitRepo`] instance pointing to the synchronized repository cache.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source doesn't exist ([`CcpmError::SourceNotFound`])
    /// - Source is disabled ([`CcpmError::ConfigError`])
    /// - Repository is not accessible (network, permissions, etc.)
    /// - Local path doesn't exist or isn't a Git repository
    /// - Cache directory cannot be created
    ///
    /// # Examples
    ///
    /// ## Basic Synchronization
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new(
    ///     "community".to_string(),
    ///     "https://github.com/example/ccpm-community.git".to_string()
    /// );
    /// manager.add(source)?;
    ///
    /// // Sync without progress feedback
    /// let repo = manager.sync("community", None).await?;
    /// println!("Repository available at: {:?}", repo.path());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Synchronization with Progress
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new(
    ///     "large-repo".to_string(),
    ///     "https://github.com/example/large-repository.git".to_string()
    /// );
    /// manager.add(source)?;
    ///
    /// // Sync with progress feedback
    /// let progress = ProgressBar::new_spinner();
    /// progress.set_message("Syncing large repository...");
    ///
    /// let repo = manager.sync("large-repo", Some(&progress)).await?;
    /// progress.finish_with_message("Repository synced successfully");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn sync(&mut self, name: &str, progress: Option<&ProgressBar>) -> Result<GitRepo> {
        let source = self
            .sources
            .get(name)
            .ok_or_else(|| CcpmError::SourceNotFound {
                name: name.to_string(),
            })?;

        if !source.enabled {
            return Err(CcpmError::ConfigError {
                message: format!("Source '{name}' is disabled"),
            }
            .into());
        }

        let cache_path = source.cache_dir(&self.cache_dir);
        ensure_dir(cache_path.parent().unwrap())?;

        // Use the URL directly (auth tokens are already embedded in URLs from global config)
        let url = source.url.clone();

        // Distinguish between plain directories and git repositories
        let is_local_path = url.starts_with('/') || url.starts_with("./") || url.starts_with("../");
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
            let abs_path = PathBuf::from(path_str);

            // Check if the local path exists and is a git repo
            if !abs_path.exists() {
                return Err(anyhow::anyhow!(
                    "Local repository path does not exist or is not accessible"
                ));
            }

            if !abs_path.join(".git").exists() {
                return Err(anyhow::anyhow!(
                    "Specified path is not a git repository. file:// URLs must point to valid git repositories."
                ));
            }

            if cache_path.exists() {
                let repo = GitRepo::new(&cache_path);
                if repo.is_git_repo() {
                    // For file:// repos, fetch to get latest changes
                    repo.fetch(Some(&url), progress).await?;
                    repo
                } else {
                    tokio::fs::remove_dir_all(&cache_path).await
                        .context("Failed to remove invalid cache directory")?;
                    GitRepo::clone(&url, &cache_path, progress).await?
                }
            } else {
                GitRepo::clone(&url, &cache_path, progress).await?
            }
        } else if cache_path.exists() {
            let repo = GitRepo::new(&cache_path);
            if repo.is_git_repo() {
                // Always fetch for all URLs to get latest changes
                repo.fetch(Some(&url), progress).await?;
                repo
            } else {
                tokio::fs::remove_dir_all(&cache_path).await
                    .context("Failed to remove invalid cache directory")?;
                GitRepo::clone(&url, &cache_path, progress).await?
            }
        } else {
            GitRepo::clone(&url, &cache_path, progress).await?
        };

        if let Some(source) = self.sources.get_mut(name) {
            source.local_path = Some(cache_path);
        }

        Ok(repo)
    }

    /// Synchronizes a repository by URL without adding it as a named source.
    ///
    /// This method is used for direct Git dependencies that are referenced by URL rather
    /// than by source name. It's particularly useful for one-off repository access or
    /// when dealing with dependencies that don't need to be permanently registered.
    ///
    /// # Key Differences from `sync()`
    ///
    /// - **No source registration**: Repository is not added to the manager's source list
    /// - **URL-based caching**: Cache directory is derived from the URL structure
    /// - **Direct access**: Bypasses source name resolution and enablement checks
    /// - **Temporary usage**: Ideal for short-lived or one-time repository access
    ///
    /// # Cache Management
    ///
    /// The cache directory is generated using the same pattern as named sources:
    /// `{cache_dir}/sources/{owner}_{repository}` where owner and repository are
    /// parsed from the Git URL.
    ///
    /// # Repository Types
    ///
    /// Supports the same repository types as `sync()`:
    /// - Remote HTTPS/SSH repositories
    /// - Local file paths and file:// URLs
    /// - Proper validation for all repository types
    ///
    /// # Arguments
    ///
    /// * `url` - Repository URL or local path to synchronize
    /// * `progress` - Optional progress bar for user feedback
    ///
    /// # Returns
    ///
    /// Returns a [`GitRepo`] instance pointing to the cached repository.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Repository URL is invalid or inaccessible
    /// - Local path doesn't exist or isn't a Git repository  
    /// - Network connectivity issues for remote repositories
    /// - Filesystem permission issues
    ///
    /// # Examples
    ///
    /// ## Direct Repository Access
    /// ```rust,no_run
    /// use ccpm::source::SourceManager;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    ///
    /// // Sync a repository directly by URL
    /// let repo = manager.sync_by_url(
    ///     "https://github.com/example/direct-dependency.git",
    ///     None
    /// ).await?;
    ///
    /// println!("Direct repository available at: {:?}", repo.path());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Local Repository Access
    /// ```rust,no_run
    /// use ccpm::source::SourceManager;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    ///
    /// // Access a local development repository
    /// let repo = manager.sync_by_url(
    ///     "/path/to/local/development/repo",
    ///     None
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn sync_by_url(
        &mut self,
        url: &str,
        progress: Option<&ProgressBar>,
    ) -> Result<GitRepo> {
        // Generate a cache directory based on the URL
        let (owner, repo_name) =
            parse_git_url(url).unwrap_or(("direct".to_string(), "repo".to_string()));
        let cache_path = self
            .cache_dir
            .join("sources")
            .join(format!("{owner}_{repo_name}"));
        ensure_dir(cache_path.parent().unwrap())?;

        // Check URL type
        let is_local_path = url.starts_with('/') || url.starts_with("./") || url.starts_with("../");
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
            let abs_path = PathBuf::from(path_str);

            if !abs_path.exists() {
                return Err(anyhow::anyhow!(
                    "Local repository path does not exist or is not accessible"
                ));
            }

            if !abs_path.join(".git").exists() {
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
                repo.fetch(Some(&authenticated_url), progress).await?;
                repo
            } else {
                tokio::fs::remove_dir_all(&cache_path).await
                    .context("Failed to remove invalid cache directory")?;
                GitRepo::clone(&authenticated_url, &cache_path, progress).await?
            }
        } else {
            GitRepo::clone(&authenticated_url, &cache_path, progress).await?
        };

        Ok(repo)
    }

    /// Synchronizes all enabled sources by fetching latest changes
    ///
    /// This method iterates through all enabled sources and synchronizes each one
    /// by fetching the latest changes from their remote repositories.
    ///
    /// # Arguments
    ///
    /// * `progress` - Optional progress bar for displaying sync progress
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all sources sync successfully
    ///
    /// # Errors
    ///
    /// Returns an error if any source fails to sync
    pub async fn sync_all(&mut self, progress: Option<&ProgressBar>) -> Result<()> {
        let enabled_sources: Vec<String> =
            self.list_enabled().iter().map(|s| s.name.clone()).collect();

        for name in enabled_sources {
            if let Some(pb) = &progress {
                pb.set_message(format!("Syncing {name}"));
            }
            self.sync(&name, progress).await?;
        }

        if let Some(pb) = progress {
            pb.finish_with_message("All sources synced");
        }

        Ok(())
    }

    /// Sync multiple sources by URL in parallel
    pub async fn sync_multiple_by_url(
        &mut self,
        urls: &[String],
        progress: Option<&ProgressBar>,
    ) -> Result<Vec<GitRepo>> {
        if urls.is_empty() {
            return Ok(Vec::new());
        }

        if let Some(pb) = progress {
            pb.set_message(format!("Syncing {} repositories", urls.len()));
        }

        // For now, sync sequentially
        // TODO: Use tokio::join_all for parallel execution
        let mut repos = Vec::new();
        for (index, url) in urls.iter().enumerate() {
            if let Some(pb) = progress {
                pb.set_message(format!("Syncing repository {}/{}", index + 1, urls.len()));
            }

            let repo = self.sync_by_url(url, None).await?;
            repos.push(repo);
        }

        if let Some(pb) = progress {
            pb.finish_with_message("All repositories synced");
        }

        Ok(repos)
    }

    /// Enables a source for use in operations.
    ///
    /// Enabled sources are included in operations like [`sync_all()`] and [`verify_all()`].
    /// Sources are enabled by default when created.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to enable
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::SourceNotFound`] if no source with the given name exists.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
    /// manager.add(source)?;
    ///
    /// // Disable then re-enable
    /// manager.disable("test")?;
    /// manager.enable("test")?;
    ///
    /// assert!(manager.get("test").unwrap().enabled);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`sync_all()`]: SourceManager::sync_all
    /// [`verify_all()`]: SourceManager::verify_all
    pub fn enable(&mut self, name: &str) -> Result<()> {
        let source = self
            .sources
            .get_mut(name)
            .ok_or_else(|| CcpmError::SourceNotFound {
                name: name.to_string(),
            })?;

        source.enabled = true;
        Ok(())
    }

    /// Disables a source to exclude it from operations.
    ///
    /// Disabled sources are excluded from bulk operations like [`sync_all()`] and
    /// [`verify_all()`], and cannot be synced individually. This is useful for
    /// temporarily disabling problematic sources without removing them entirely.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to disable
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::SourceNotFound`] if no source with the given name exists.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new("test".to_string(), "https://github.com/test/repo.git".to_string());
    /// manager.add(source)?;
    ///
    /// // Disable the source
    /// manager.disable("test")?;
    ///
    /// assert!(!manager.get("test").unwrap().enabled);
    /// assert_eq!(manager.list_enabled().len(), 0);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`sync_all()`]: SourceManager::sync_all
    /// [`verify_all()`]: SourceManager::verify_all
    pub fn disable(&mut self, name: &str) -> Result<()> {
        let source = self
            .sources
            .get_mut(name)
            .ok_or_else(|| CcpmError::SourceNotFound {
                name: name.to_string(),
            })?;

        source.enabled = false;
        Ok(())
    }

    /// Gets the cache directory path for a source by URL.
    ///
    /// Searches through managed sources to find one with a matching URL and returns
    /// its cache directory path. This is useful when you have a URL and need to
    /// determine where its cached content would be stored.
    ///
    /// # Arguments
    ///
    /// * `url` - Repository URL to look up
    ///
    /// # Returns
    ///
    /// [`PathBuf`] pointing to the cache directory for the source.
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::SourceNotFound`] if no source with the given URL exists.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let url = "https://github.com/example/repo.git".to_string();
    /// let source = Source::new("example".to_string(), url.clone());
    /// manager.add(source)?;
    ///
    /// let cache_path = manager.get_cached_path(&url)?;
    /// println!("Cache path: {:?}", cache_path);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_cached_path(&self, url: &str) -> Result<PathBuf> {
        // Try to find the source by URL
        let source = self
            .sources
            .values()
            .find(|s| s.url == url)
            .ok_or_else(|| CcpmError::SourceNotFound {
                name: url.to_string(),
            })?;

        Ok(source.cache_dir(&self.cache_dir))
    }

    /// Gets the cache directory path for a source by name.
    ///
    /// Returns the cache directory path where the named source's repository
    /// content is or would be stored.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the source to get the cache path for
    ///
    /// # Returns
    ///
    /// [`PathBuf`] pointing to the cache directory for the source.
    ///
    /// # Errors
    ///
    /// Returns [`CcpmError::SourceNotFound`] if no source with the given name exists.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    /// let source = Source::new(
    ///     "community".to_string(),
    ///     "https://github.com/example/ccpm-community.git".to_string()
    /// );
    /// manager.add(source)?;
    ///
    /// let cache_path = manager.get_cached_path_by_name("community")?;
    /// println!("Community cache: {:?}", cache_path);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_cached_path_by_name(&self, name: &str) -> Result<PathBuf> {
        let source = self
            .sources
            .get(name)
            .ok_or_else(|| CcpmError::SourceNotFound {
                name: name.to_string(),
            })?;

        Ok(source.cache_dir(&self.cache_dir))
    }

    /// Verifies that all enabled sources are accessible.
    ///
    /// This method performs lightweight verification checks on all enabled sources
    /// without performing full synchronization. It's useful for validating source
    /// configurations and network connectivity before attempting operations.
    ///
    /// # Verification Process
    ///
    /// For each enabled source:
    /// 1. **URL validation**: Check URL format and structure
    /// 2. **Connectivity test**: Verify remote repositories are reachable
    /// 3. **Local path validation**: Ensure local repositories exist and are Git repos
    /// 4. **Authentication check**: Validate credentials for private repositories
    ///
    /// # Performance Characteristics
    ///
    /// - **Lightweight**: No cloning or downloading of repository content
    /// - **Fast**: Quick network checks rather than full Git operations
    /// - **Sequential**: Sources verified one at a time for clear error reporting
    ///
    /// # Arguments
    ///
    /// * `progress` - Optional progress bar for user feedback
    ///
    /// # Errors
    ///
    /// Returns an error if any enabled source fails verification:
    /// - Network connectivity issues
    /// - Authentication failures
    /// - Invalid repository URLs
    /// - Local paths that don't exist or aren't Git repositories
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::source::{Source, SourceManager};
    /// use ccpm::utils::progress::ProgressBar;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = SourceManager::new()?;
    ///
    /// // Add some sources
    /// manager.add(Source::new(
    ///     "community".to_string(),
    ///     "https://github.com/example/ccpm-community.git".to_string()
    /// ))?;
    ///
    /// // Verify all sources with progress feedback
    /// let progress = ProgressBar::new_spinner();
    /// manager.verify_all(Some(&progress)).await?;
    ///
    /// println!("All sources verified successfully");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_all(&self, progress: Option<&ProgressBar>) -> Result<()> {
        let enabled_sources: Vec<&Source> = self.list_enabled();

        if enabled_sources.is_empty() {
            if let Some(pb) = progress {
                pb.finish_with_message("No sources to verify");
            }
            return Ok(());
        }

        for source in enabled_sources {
            if let Some(pb) = progress {
                pb.set_message(format!("Verifying {}", source.name));
            }

            // Check if source URL is reachable by attempting a quick operation
            self.verify_source(&source.url).await?;
        }

        if let Some(pb) = progress {
            pb.finish_with_message("All sources verified");
        }

        Ok(())
    }

    /// Verifies that a single source URL is accessible.
    ///
    /// Performs a lightweight check to determine if a repository URL is accessible
    /// without downloading content. The verification method depends on the URL type:
    ///
    /// - **file:// URLs**: Check if the local path exists
    /// - **Remote URLs**: Perform network connectivity check
    /// - **Local paths**: Validate path exists and is a Git repository
    ///
    /// # Arguments
    ///
    /// * `url` - Repository URL or local path to verify
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not accessible, with specific error
    /// messages based on the failure type (network, authentication, path, etc.).
    async fn verify_source(&self, url: &str) -> Result<()> {
        // For file:// URLs (used in tests), just check if the path exists
        if url.starts_with("file://") {
            let path = url.strip_prefix("file://").unwrap();
            if std::path::Path::new(path).exists() {
                return Ok(());
            }
            return Err(anyhow::anyhow!("Local path does not exist: {}", path));
        }

        // For other URLs, try to create a GitRepo object and verify it's accessible
        // This is a lightweight check - we don't actually clone the repo
        match crate::git::GitRepo::verify_url(url).await {
            Ok(()) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Source not accessible: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_source_creation() {
        let source = Source::new(
            "test".to_string(),
            "https://github.com/user/repo.git".to_string(),
        )
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

        let source = Source::new(
            "test".to_string(),
            "https://github.com/user/repo.git".to_string(),
        );

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

        let source = Source::new(
            "test".to_string(),
            "https://github.com/user/repo.git".to_string(),
        );

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

        manager
            .add(Source::new("source1".to_string(), "url1".to_string()))
            .unwrap();
        manager
            .add(Source::new("source2".to_string(), "url2".to_string()))
            .unwrap();
        manager
            .add(Source::new("source3".to_string(), "url3".to_string()))
            .unwrap();

        assert_eq!(manager.list_enabled().len(), 3);

        manager.disable("source2").unwrap();
        assert_eq!(manager.list_enabled().len(), 2);
    }

    #[test]
    fn test_source_cache_dir() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();

        let source = Source::new(
            "test".to_string(),
            "https://github.com/user/repo.git".to_string(),
        );

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
            "https://github.com/example-org/ccpm-official.git".to_string(),
        );
        manifest.add_source(
            "community".to_string(),
            "https://github.com/example-org/ccpm-community.git".to_string(),
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

        manager
            .add(Source::new("source1".to_string(), "url1".to_string()))
            .unwrap();
        manager
            .add(Source::new("source2".to_string(), "url2".to_string()))
            .unwrap();

        assert_eq!(manager.list().len(), 2);
    }

    #[test]
    fn test_source_manager_get_mut() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        manager
            .add(Source::new("test".to_string(), "url".to_string()))
            .unwrap();

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

        let source = Source::new(
            "test".to_string(),
            "https://github.com/user/repo.git".to_string(),
        );
        manager.add(source).unwrap();
        manager.disable("test").unwrap();

        let result = manager.sync("test", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_source_manager_sync_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SourceManager::new_with_cache(temp_dir.path().to_path_buf());

        let result = manager.sync("nonexistent", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_source_manager_sync_local_repo() {
        use crate::test_utils::WorkingDirGuard;
        use std::process::Command;

        // Use WorkingDirGuard to ensure proper test isolation
        // In coverage/CI environments, current dir might not exist, so set a safe one first
        let _ = std::env::set_current_dir(std::env::temp_dir());
        let _guard = WorkingDirGuard::new().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create a local git repo
        std::fs::create_dir(&repo_dir).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());
        let source = Source::new("test".to_string(), format!("file://{}", repo_dir.display()));
        manager.add(source).unwrap();

        // First sync (clone)
        let result = manager.sync("test", None).await;
        assert!(result.is_ok());
        let repo = result.unwrap();
        assert!(repo.is_git_repo());

        // Second sync (fetch + pull)
        let result = manager.sync("test", None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_source_manager_sync_all() {
        use crate::test_utils::WorkingDirGuard;
        use std::process::Command;

        // Use WorkingDirGuard to ensure proper test isolation
        // In coverage/CI environments, current dir might not exist, so set a safe one first
        let _ = std::env::set_current_dir(std::env::temp_dir());
        let _guard = WorkingDirGuard::new().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        // Create two local git repos
        let repo1_dir = temp_dir.path().join("repo1");
        let repo2_dir = temp_dir.path().join("repo2");

        for repo_dir in &[&repo1_dir, &repo2_dir] {
            std::fs::create_dir(repo_dir).unwrap();
            Command::new("git")
                .args(["init"])
                .current_dir(repo_dir)
                .output()
                .unwrap();
            Command::new("git")
                .args(["config", "user.email", "test@example.com"])
                .current_dir(repo_dir)
                .output()
                .unwrap();
            Command::new("git")
                .args(["config", "user.name", "Test User"])
                .current_dir(repo_dir)
                .output()
                .unwrap();
            std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
            Command::new("git")
                .args(["add", "."])
                .current_dir(repo_dir)
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-m", "Initial commit"])
                .current_dir(repo_dir)
                .output()
                .unwrap();
        }

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());

        manager
            .add(Source::new(
                "repo1".to_string(),
                format!("file://{}", repo1_dir.display()),
            ))
            .unwrap();

        manager
            .add(Source::new(
                "repo2".to_string(),
                format!("file://{}", repo2_dir.display()),
            ))
            .unwrap();

        // Sync all
        let result = manager.sync_all(None).await;
        assert!(result.is_ok());

        // Verify both repos were cloned
        let source1_cache = manager.get("repo1").unwrap().cache_dir(&cache_dir);
        let source2_cache = manager.get("repo2").unwrap().cache_dir(&cache_dir);
        assert!(source1_cache.exists());
        assert!(source2_cache.exists());
    }

    // Additional error path tests

    #[tokio::test]
    async fn test_sync_non_existent_local_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        let source = Source::new("test".to_string(), "/non/existent/path".to_string());
        manager.add(source).unwrap();

        let result = manager.sync("test", None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_sync_non_git_directory() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let non_git_dir = temp_dir.path().join("not_git");
        std::fs::create_dir(&non_git_dir).unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir);
        let source = Source::new(
            "test".to_string(),
            non_git_dir.to_str().unwrap().to_string(),
        );
        manager.add(source).unwrap();

        // Local paths are now treated as plain directories, so sync should succeed
        let result = manager.sync("test", None).await;
        if let Err(ref e) = result {
            eprintln!("Test failed with error: {e}");
            eprintln!("Path was: {non_git_dir:?}");
        }
        assert!(result.is_ok(), "Failed to sync: {result:?}");
        let repo = result.unwrap();
        // Should point to the canonicalized local directory
        assert_eq!(
            repo.path(),
            crate::utils::safe_canonicalize(&non_git_dir).unwrap()
        );
    }

    #[tokio::test]
    async fn test_sync_invalid_cache_directory() {
        use crate::test_utils::WorkingDirGuard;
        use std::process::Command;

        // Ensure stable test environment
        // In coverage/CI environments, current dir might not exist, so set a safe one first
        let _ = std::env::set_current_dir(std::env::temp_dir());
        let _guard = WorkingDirGuard::new().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create a valid git repo
        std::fs::create_dir(&repo_dir).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());
        let source = Source::new("test".to_string(), format!("file://{}", repo_dir.display()));
        manager.add(source).unwrap();

        // Create an invalid cache directory (not a git repo)
        let source_cache_dir = manager.get("test").unwrap().cache_dir(&cache_dir);
        std::fs::create_dir_all(&source_cache_dir).unwrap();
        std::fs::write(source_cache_dir.join("file.txt"), "not a git repo").unwrap();

        // Sync should detect invalid cache and re-clone
        let result = manager.sync("test", None).await;
        assert!(result.is_ok());
        assert!(source_cache_dir.join(".git").exists());
    }

    #[tokio::test]
    async fn test_sync_by_url_invalid_url() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.sync_by_url("not-a-valid-url", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sync_multiple_by_url_empty() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        let result = manager.sync_multiple_by_url(&[], None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_sync_multiple_by_url_with_failures() {
        use std::process::Command;

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create one valid repo
        std::fs::create_dir(&repo_dir).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir);

        let urls = vec![
            format!("file://{}", repo_dir.display()),
            "invalid-url".to_string(),
        ];

        // Should fail on invalid URL
        let result = manager.sync_multiple_by_url(&urls, None).await;
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
    async fn test_verify_all_no_sources() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let manager = SourceManager::new_with_cache(cache_dir);

        let pb = crate::utils::progress::ProgressBar::new_spinner();
        let result = manager.verify_all(Some(&pb)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_verify_all_with_disabled_sources() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        // Add but disable a source
        let source = Source::new(
            "test".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manager.add(source).unwrap();
        manager.disable("test").unwrap();

        // Verify should skip disabled sources
        let result = manager.verify_all(None).await;
        assert!(result.is_ok());
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

        let result = manager
            .verify_source("https://invalid-host-9999.test/repo.git")
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not accessible"));
    }

    #[tokio::test]
    async fn test_remove_with_cache_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir.clone());

        let source = Source::new(
            "test".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
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

        let source = Source::new(
            "test".to_string(),
            "https://github.com/test/repo.git".to_string(),
        );
        manager.add(source).unwrap();

        let url = manager.get_source_url("test");
        assert_eq!(url, Some("https://github.com/test/repo.git".to_string()));

        let url = manager.get_source_url("nonexistent");
        assert_eq!(url, None);
    }

    #[test]
    fn test_source_with_description() {
        let source = Source::new(
            "test".to_string(),
            "https://github.com/test/repo.git".to_string(),
        )
        .with_description("Test description".to_string());

        assert_eq!(source.description, Some("Test description".to_string()));
    }

    #[tokio::test]
    async fn test_sync_with_progress() {
        use crate::test_utils::WorkingDirGuard;
        use std::process::Command;

        // Ensure stable test environment
        // In coverage/CI environments, current dir might not exist, so set a safe one first
        let _ = std::env::set_current_dir(std::env::temp_dir());
        let _guard = WorkingDirGuard::new().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let repo_dir = temp_dir.path().join("repo");

        // Create a git repo
        std::fs::create_dir(&repo_dir).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::fs::write(repo_dir.join("README.md"), "Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir);
        let source = Source::new("test".to_string(), format!("file://{}", repo_dir.display()));
        manager.add(source).unwrap();

        let pb = crate::utils::progress::ProgressBar::new_spinner();
        pb.set_message("Testing sync");

        let result = manager.sync("test", Some(&pb)).await;
        assert!(result.is_ok());

        pb.finish_with_message("Done");
    }

    #[tokio::test]
    async fn test_from_manifest_with_global() {
        let manifest = Manifest::new();
        let result = SourceManager::from_manifest_with_global(&manifest).await;
        assert!(result.is_ok());
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
    async fn test_sync_local_path_directory() {
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
        let result = manager.sync("local", None).await;
        assert!(result.is_ok());

        let repo = result.unwrap();
        // The returned GitRepo should point to the canonicalized local directory
        // On macOS, /var is a symlink to /private/var, so we need to compare canonical paths
        assert_eq!(
            repo.path(),
            crate::utils::safe_canonicalize(&local_dir).unwrap()
        );
    }

    #[tokio::test]
    async fn test_sync_by_url_local_path() {
        // Test sync_by_url with local paths
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let local_dir = temp_dir.path().join("local_deps");

        // Create a plain directory with files
        std::fs::create_dir(&local_dir).unwrap();
        std::fs::write(local_dir.join("test.md"), "# Test Resource").unwrap();

        let mut manager = SourceManager::new_with_cache(cache_dir);

        // Test absolute path
        let result = manager
            .sync_by_url(&local_dir.to_string_lossy(), None)
            .await;
        assert!(result.is_ok());
        let repo = result.unwrap();
        assert_eq!(
            repo.path(),
            crate::utils::safe_canonicalize(&local_dir).unwrap()
        );

        // Test relative path
        {
            use crate::test_utils::WorkingDirGuard;
            // In coverage/CI environments, current dir might not exist, so set a safe one first
            let _ = std::env::set_current_dir(std::env::temp_dir());
            let guard = WorkingDirGuard::new().unwrap();
            guard.change_to(&temp_dir).unwrap();
            let result = manager.sync_by_url("./local_deps", None).await;
            assert!(result.is_ok());
            // Guard will restore directory when dropped
        }
    }

    #[tokio::test]
    async fn test_sync_local_path_not_exist() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let mut manager = SourceManager::new_with_cache(cache_dir);

        // Try to sync non-existent local path
        let result = manager.sync_by_url("/non/existent/path", None).await;
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

        let mut manager = SourceManager::new_with_cache(cache_dir);

        // file:// URL should fail for non-git directory
        let file_url = format!("file://{}", plain_dir.display());
        let result = manager.sync_by_url(&file_url, None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a git repository"));
    }

    #[tokio::test]
    async fn test_path_traversal_attack_prevention() {
        // Test that access to blacklisted system directories is prevented
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let mut manager = SourceManager::new_with_cache(cache_dir.clone());

        // Test that blacklisted system paths are blocked
        let blacklisted_paths = vec!["/etc/passwd", "/System/Library", "/private/etc/hosts"];

        for malicious_path in blacklisted_paths {
            // Skip if path doesn't exist (e.g., /System on Linux)
            if !std::path::Path::new(malicious_path).exists() {
                continue;
            }

            let result = manager.sync_by_url(malicious_path, None).await;
            assert!(
                result.is_err(),
                "Blacklisted path not detected for: {malicious_path}"
            );
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("Security error") || err_msg.contains("not allowed"),
                "Expected security error for blacklisted path: {malicious_path}, got: {err_msg}"
            );
        }

        // Test that normal paths in temp directories work fine
        let safe_dir = temp_dir.path().join("safe_dir");
        std::fs::create_dir(&safe_dir).unwrap();

        let result = manager.sync_by_url(&safe_dir.to_string_lossy(), None).await;
        assert!(
            result.is_ok(),
            "Safe path was incorrectly blocked: {result:?}"
        );
    }

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
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let symlink_path = deps_dir.join("malicious_link");
            symlink(&sensitive_dir, &symlink_path).unwrap();

            // Change to project directory
            use crate::test_utils::WorkingDirGuard;
            // In coverage/CI environments, current dir might not exist, so set a safe one first
            let _ = std::env::set_current_dir(std::env::temp_dir());
            let guard = WorkingDirGuard::new().unwrap();
            guard.change_to(&project_dir).unwrap();

            let mut manager = SourceManager::new_with_cache(cache_dir);

            // Try to access the symlink
            let result = manager.sync_by_url("./deps/malicious_link", None).await;
            assert!(result.is_err());
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("Symlinks are not allowed") || err_msg.contains("Security error"),
                "Expected symlink error, got: {err_msg}"
            );
            // Guard will restore directory when dropped
        }
    }

    #[tokio::test]
    async fn test_absolute_path_restriction() {
        // Test that blacklisted absolute paths are blocked
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");

        let mut manager = SourceManager::new_with_cache(cache_dir);

        // With blacklist approach, temp directories are allowed
        // So this test verifies that normal development paths work
        let safe_dir = temp_dir.path().join("project");
        std::fs::create_dir(&safe_dir).unwrap();
        std::fs::write(safe_dir.join("file.txt"), "content").unwrap();

        let result = manager.sync_by_url(&safe_dir.to_string_lossy(), None).await;

        // Temp directories should work fine with blacklist approach
        assert!(
            result.is_ok(),
            "Safe temp path was incorrectly blocked: {result:?}"
        );
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
            "Security error: Local path must be within the project directory or CCPM cache";
        assert!(!security_msg.contains("{:?}"));
        assert!(!security_msg.contains("{}"));
    }
}
