//! Global configuration management for AGPM.
//!
//! This module handles the global user configuration file (`~/.agpm/config.toml`) which stores
//! user-wide settings including authentication tokens for private repositories. The global
//! configuration provides a secure way to manage credentials without exposing them in
//! version-controlled project files.
//!
//! # Security Model
//!
//! The global configuration is designed with security as a primary concern:
//!
//! - **Credential Isolation**: Authentication tokens are stored only in the global config
//! - **Never Committed**: Global config is never part of version control
//! - **User-Specific**: Each user maintains their own global configuration
//! - **Platform-Secure**: Uses platform-appropriate secure storage locations
//!
//! # Configuration File Location
//!
//! The global configuration file is stored in platform-specific locations:
//!
//! - **Unix/macOS**: `~/.agpm/config.toml`
//! - **Windows**: `%LOCALAPPDATA%\agpm\config.toml`
//!
//! The location can be overridden using the `AGPM_CONFIG_PATH` environment variable.
//!
//! # File Format
//!
//! The global configuration uses TOML format:
//!
//! ```toml
//! # Global sources with authentication (never commit this file!)
//! [sources]
//! # GitHub with personal access token
//! private = "https://oauth2:ghp_xxxxxxxxxxxx@github.com/company/private-agpm.git"
//!
//! # GitLab with deploy token
//! enterprise = "https://gitlab-ci-token:token123@gitlab.company.com/ai/resources.git"
//!
//! # SSH-based authentication
//! internal = "git@internal.company.com:team/agpm-resources.git"
//!
//! # Basic authentication (not recommended)
//! legacy = "https://username:password@old-server.com/repo.git"
//! ```
//!
//! # Authentication Methods
//!
//! Supported authentication methods for Git repositories:
//!
//! ## GitHub Personal Access Token (Recommended)
//! ```text
//! https://oauth2:ghp_xxxxxxxxxxxx@github.com/owner/repo.git
//! ```
//!
//! ## GitLab Deploy Token
//! ```text
//! https://gitlab-ci-token:token@gitlab.com/group/repo.git
//! ```
//!
//! ## SSH Keys
//! ```text
//! git@github.com:owner/repo.git
//! ```
//!
//! ## Basic Authentication (Not Recommended)
//! ```text
//! https://username:password@server.com/repo.git
//! ```
//!
//! # Source Resolution Priority
//!
//! When resolving sources, AGPM follows this priority order:
//!
//! 1. **Global sources** from `~/.agpm/config.toml` (loaded first)
//! 2. **Project sources** from `agpm.toml` (can override global sources)
//!
//! This allows teams to share public sources in `agpm.toml` while keeping
//! authentication tokens private in individual global configurations.
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```rust,no_run
//! use agpm_cli::config::GlobalConfig;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Load existing configuration or create default
//! let mut config = GlobalConfig::load().await?;
//!
//! // Add authenticated source
//! config.add_source(
//!     "private".to_string(),
//!     "https://oauth2:token@github.com/company/repo.git".to_string()
//! );
//!
//! // Save changes
//! config.save().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Using Configuration Manager
//!
//! ```rust,no_run
//! use agpm_cli::config::GlobalConfigManager;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut manager = GlobalConfigManager::new()?;
//!
//! // Get configuration with caching
//! let config = manager.get().await?;
//! println!("Found {} global sources", config.sources.len());
//!
//! // Modify configuration
//! let config = manager.get_mut().await?;
//! config.add_source("new".to_string(), "https://example.com/repo.git".to_string());
//!
//! // Save changes
//! manager.save().await?;
//! # Ok(())
//! # }
//! ```

use crate::upgrade::config::UpgradeConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Default maximum file size for operations that read/embed files.
///
/// Default: 1 MB (1,048,576 bytes)
///
/// This limit prevents memory exhaustion when reading files. Currently used by:
/// - Template content filter for embedding project files
///
/// Future uses may include:
/// - General file reading operations
/// - Resource validation
/// - Content processing
const fn default_max_content_file_size() -> u64 {
    1024 * 1024 // 1 MB
}

/// Global configuration structure for AGPM.
///
/// This structure represents the global user configuration file stored at `~/.agpm/config.toml`.
/// It contains user-wide settings including authentication credentials for private Git repositories.
///
/// # Security Considerations
///
/// - **Never commit** this configuration to version control
/// - Store **only** in the user's home directory or secure location
/// - Contains **sensitive data** like authentication tokens
/// - Should have **restricted file permissions** (600 on Unix systems)
///
/// # Structure
///
/// Currently contains only source definitions, but designed to be extensible
/// for future configuration options like:
/// - Default author information
/// - Preferred Git configuration
/// - Cache settings
/// - Proxy configuration
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::config::GlobalConfig;
/// use std::collections::HashMap;
///
/// // Create new configuration
/// let mut config = GlobalConfig::default();
///
/// // Add authenticated source
/// config.add_source(
///     "company".to_string(),
///     "https://oauth2:token@github.com/company/agpm.git".to_string()
/// );
///
/// assert!(config.has_source("company"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    /// Global Git repository sources with optional authentication.
    ///
    /// Maps source names to Git repository URLs. These URLs may contain authentication
    /// credentials and are kept separate from project manifests for security.
    ///
    /// # Authentication URL Formats
    ///
    /// - `https://oauth2:token@github.com/owner/repo.git` - GitHub personal access token
    /// - `https://gitlab-ci-token:token@gitlab.com/group/repo.git` - GitLab deploy token
    /// - `git@github.com:owner/repo.git` - SSH authentication
    /// - `https://user:pass@server.com/repo.git` - Basic auth (not recommended)
    ///
    /// # Security Notes
    ///
    /// - URLs with credentials are **never** logged in plain text
    /// - The `sources` field is skipped during serialization if empty
    /// - Authentication details should use tokens rather than passwords when possible
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub sources: HashMap<String, String>,

    /// Upgrade configuration settings.
    ///
    /// Controls the behavior of the self-upgrade functionality including
    /// update checks, backup preferences, and verification settings.
    #[serde(default, skip_serializing_if = "is_default_upgrade_config")]
    pub upgrade: UpgradeConfig,

    /// Maximum file size in bytes for file operations.
    ///
    /// Default: 1 MB (1,048,576 bytes)
    ///
    /// This limit prevents memory exhaustion when reading or embedding files.
    /// Currently used by template content filter, may be used by other operations in the future.
    ///
    /// # Configuration
    ///
    /// Set in `~/.agpm/config.toml`:
    /// ```toml
    /// max_content_file_size = 2097152  # 2 MB
    /// ```
    #[serde(
        default = "default_max_content_file_size",
        skip_serializing_if = "is_default_max_content_file_size"
    )]
    pub max_content_file_size: u64,
}

fn is_default_max_content_file_size(size: &u64) -> bool {
    *size == default_max_content_file_size()
}

const fn is_default_upgrade_config(config: &UpgradeConfig) -> bool {
    // Skip serializing if it's the default config
    !config.check_on_startup
        && config.check_interval == 86400
        && config.auto_backup
        && config.verify_checksum
}

impl GlobalConfig {
    /// Load global configuration from the default platform-specific location.
    ///
    /// Attempts to load the configuration file from the default path. If the file
    /// doesn't exist, returns a default (empty) configuration instead of an error.
    ///
    /// # Default Locations
    ///
    /// - **Unix/macOS**: `~/.agpm/config.toml`
    /// - **Windows**: `%LOCALAPPDATA%\agpm\config.toml`
    /// - **Override**: Set `AGPM_CONFIG_PATH` environment variable
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = GlobalConfig::load().await?;
    /// println!("Loaded {} global sources", config.sources.len());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The default path cannot be determined
    /// - The file exists but cannot be read
    /// - The file contains invalid TOML syntax
    pub async fn load() -> Result<Self> {
        let path = Self::default_path()?;
        if path.exists() {
            Self::load_from(&path).await
        } else {
            Ok(Self::default())
        }
    }

    /// Load global configuration from an optional path.
    ///
    /// If a path is provided, loads from that path. Otherwise, loads from the
    /// default location (`~/.agpm/config.toml` or platform equivalent).
    ///
    /// # Parameters
    ///
    /// - `path`: Optional path to the configuration file
    ///
    /// # Returns
    ///
    /// Returns the loaded configuration or a default configuration if the file
    /// doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file exists but cannot be read
    /// - The file contains invalid TOML syntax
    pub async fn load_with_optional(path: Option<PathBuf>) -> Result<Self> {
        let path = path.unwrap_or_else(|| {
            Self::default_path().unwrap_or_else(|_| PathBuf::from("~/.agpm/config.toml"))
        });
        if path.exists() {
            Self::load_from(&path).await
        } else {
            Ok(Self::default())
        }
    }

    /// Load global configuration from a specific file path.
    ///
    /// This method is primarily used for testing or when a custom configuration
    /// location is needed.
    ///
    /// # Parameters
    ///
    /// - `path`: Path to the configuration file to load
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    /// use std::path::Path;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = GlobalConfig::load_from(Path::new("/custom/config.toml")).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read (permissions, not found, etc.)
    /// - The file contains invalid TOML syntax
    /// - The TOML structure doesn't match the expected schema
    pub async fn load_from(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read global config from {}", path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("Failed to parse global config from {}", path.display()))
    }

    /// Save global configuration to the default platform-specific location.
    ///
    /// Saves the current configuration to the default path, creating parent
    /// directories as needed. The file is written atomically to prevent
    /// corruption during the write process.
    ///
    /// # File Permissions
    ///
    /// The configuration file should be created with restricted permissions
    /// since it may contain sensitive authentication tokens.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut config = GlobalConfig::load().await?;
    /// config.add_source(
    ///     "new".to_string(),
    ///     "https://github.com/owner/repo.git".to_string()
    /// );
    /// config.save().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The default path cannot be determined
    /// - Parent directories cannot be created
    /// - The file cannot be written (permissions, disk space, etc.)
    /// - Serialization to TOML fails
    pub async fn save(&self) -> Result<()> {
        let path = Self::default_path()?;
        self.save_to(&path).await
    }

    /// Save global configuration to a specific file path.
    ///
    /// Creates parent directories as needed and writes the configuration
    /// as pretty-formatted TOML.
    ///
    /// # Parameters
    ///
    /// - `path`: Path where the configuration file should be saved
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    /// use std::path::Path;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = GlobalConfig::default();
    /// config.save_to(Path::new("/tmp/test-config.toml")).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Parent directories cannot be created
    /// - The file cannot be written
    /// - Serialization to TOML fails
    pub async fn save_to(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize global config")?;

        fs::write(path, content)
            .await
            .with_context(|| format!("Failed to write global config to {}", path.display()))?;

        // Set restrictive permissions on Unix systems to protect credentials
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            use tokio::fs as async_fs;

            let mut perms = async_fs::metadata(path)
                .await
                .with_context(|| format!("Failed to read permissions for {}", path.display()))?
                .permissions();
            perms.set_mode(0o600); // Owner read/write only, no group/other access
            async_fs::set_permissions(path, perms).await.with_context(|| {
                format!("Failed to set secure permissions on {}", path.display())
            })?;
        }

        Ok(())
    }

    /// Get the default file path for global configuration.
    ///
    /// Returns the platform-appropriate path for storing global configuration.
    /// This location is chosen to be secure and follow platform conventions.
    ///
    /// # Path Resolution
    ///
    /// - **Windows**: `%LOCALAPPDATA%\agpm\config.toml`
    /// - **Unix/macOS**: `~/.agpm/config.toml`
    ///
    /// Note: Environment variable overrides are deprecated. Use the `load_with_optional()`
    /// method with an explicit path instead for better thread safety.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let path = GlobalConfig::default_path()?;
    /// println!("Global config path: {}", path.display());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The home directory cannot be determined
    /// - The local data directory cannot be determined (Windows)
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = if cfg!(target_os = "windows") {
            dirs::data_local_dir()
                .ok_or_else(|| anyhow::anyhow!("Unable to determine local data directory"))?
                .join("agpm")
        } else {
            dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?
                .join(".agpm")
        };

        Ok(config_dir.join("config.toml"))
    }

    /// Merge global sources with project manifest sources.
    ///
    /// Combines the global configuration sources with sources from a project manifest,
    /// with project sources taking precedence over global sources. This allows users
    /// to maintain private authentication in global config while projects can override
    /// with public sources.
    ///
    /// # Merge Strategy
    ///
    /// 1. Start with all global sources (may include authentication)
    /// 2. Add/override with local sources from project manifest
    /// 3. Local sources win in case of name conflicts
    ///
    /// # Parameters
    ///
    /// - `local_sources`: Sources from project manifest (`agpm.toml`)
    ///
    /// # Returns
    ///
    /// Combined source map with local sources taking precedence.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    /// use std::collections::HashMap;
    ///
    /// let mut global = GlobalConfig::default();
    /// global.add_source(
    ///     "private".to_string(),
    ///     "https://token@private.com/repo.git".to_string()
    /// );
    ///
    /// let mut local = HashMap::new();
    /// local.insert(
    ///     "public".to_string(),
    ///     "https://github.com/public/repo.git".to_string()
    /// );
    ///
    /// let merged = global.merge_sources(&local);
    /// assert_eq!(merged.len(), 2);
    /// assert!(merged.contains_key("private"));
    /// assert!(merged.contains_key("public"));
    /// ```
    ///
    /// # Security Note
    ///
    /// The merged result may contain authentication credentials from global sources.
    /// Handle with care and never log or expose in version control.
    #[must_use]
    pub fn merge_sources(
        &self,
        local_sources: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut merged = self.sources.clone();

        // Local sources override global ones
        for (name, url) in local_sources {
            merged.insert(name.clone(), url.clone());
        }

        merged
    }

    /// Add or update a source in the global configuration.
    ///
    /// Adds a new source or updates an existing one with the given name and URL.
    /// The URL may contain authentication credentials.
    ///
    /// # Parameters
    ///
    /// - `name`: Unique name for the source (used in manifests)
    /// - `url`: Git repository URL, optionally with authentication
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// let mut config = GlobalConfig::default();
    ///
    /// // Add source with authentication
    /// config.add_source(
    ///     "private".to_string(),
    ///     "https://oauth2:token@github.com/company/repo.git".to_string()
    /// );
    ///
    /// // Update existing source
    /// config.add_source(
    ///     "private".to_string(),
    ///     "git@github.com:company/repo.git".to_string()
    /// );
    ///
    /// assert!(config.has_source("private"));
    /// ```
    ///
    /// # Security Note
    ///
    /// URLs containing credentials should use tokens rather than passwords when possible.
    pub fn add_source(&mut self, name: String, url: String) {
        self.sources.insert(name, url);
    }

    /// Remove a source from the global configuration.
    ///
    /// Removes the source with the given name if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: Name of the source to remove
    ///
    /// # Returns
    ///
    /// - `true` if the source was found and removed
    /// - `false` if the source didn't exist
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// let mut config = GlobalConfig::default();
    /// config.add_source("test".to_string(), "https://example.com/repo.git".to_string());
    ///
    /// assert!(config.remove_source("test"));
    /// assert!(!config.remove_source("test")); // Already removed
    /// ```
    pub fn remove_source(&mut self, name: &str) -> bool {
        self.sources.remove(name).is_some()
    }

    /// Check if a source exists in the global configuration.
    ///
    /// # Parameters
    ///
    /// - `name`: Name of the source to check
    ///
    /// # Returns
    ///
    /// - `true` if the source exists
    /// - `false` if the source doesn't exist
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// let mut config = GlobalConfig::default();
    /// assert!(!config.has_source("test"));
    ///
    /// config.add_source("test".to_string(), "https://example.com/repo.git".to_string());
    /// assert!(config.has_source("test"));
    /// ```
    #[must_use]
    pub fn has_source(&self, name: &str) -> bool {
        self.sources.contains_key(name)
    }

    /// Get a source URL by name.
    ///
    /// Returns a reference to the URL for the specified source name.
    ///
    /// # Parameters
    ///
    /// - `name`: Name of the source to retrieve
    ///
    /// # Returns
    ///
    /// - `Some(&String)` with the URL if the source exists
    /// - `None` if the source doesn't exist
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// let mut config = GlobalConfig::default();
    /// config.add_source(
    ///     "test".to_string(),
    ///     "https://example.com/repo.git".to_string()
    /// );
    ///
    /// assert_eq!(
    ///     config.get_source("test"),
    ///     Some(&"https://example.com/repo.git".to_string())
    /// );
    /// assert_eq!(config.get_source("missing"), None);
    /// ```
    ///
    /// # Security Note
    ///
    /// The returned URL may contain authentication credentials. Handle with care.
    #[must_use]
    pub fn get_source(&self, name: &str) -> Option<&String> {
        self.sources.get(name)
    }

    /// Create a global configuration with example content.
    ///
    /// Creates a new configuration populated with example sources to demonstrate
    /// the expected format. Useful for initial setup or documentation.
    ///
    /// # Returns
    ///
    /// A new [`GlobalConfig`] with example private source configuration.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfig;
    ///
    /// let config = GlobalConfig::init_example();
    /// assert!(config.has_source("private"));
    ///
    /// // The example uses a placeholder token
    /// let url = config.get_source("private").unwrap();
    /// assert!(url.contains("YOUR_TOKEN"));
    /// ```
    ///
    /// # Note
    ///
    /// The example configuration contains placeholder values that must be
    /// replaced with actual authentication credentials before use.
    #[must_use]
    pub fn init_example() -> Self {
        let mut sources = HashMap::new();
        sources.insert(
            "private".to_string(),
            "https://oauth2:YOUR_TOKEN@github.com/yourcompany/private-agpm.git".to_string(),
        );

        Self {
            sources,
            upgrade: UpgradeConfig::default(),
            max_content_file_size: default_max_content_file_size(),
        }
    }
}

/// Configuration manager with caching for global configuration.
///
/// Provides a higher-level interface for working with global configuration
/// that includes caching to avoid repeated file I/O operations. This is
/// particularly useful in command-line applications that may access
/// configuration multiple times.
///
/// # Features
///
/// - **Lazy Loading**: Configuration is loaded only when first accessed
/// - **Caching**: Subsequent accesses use the cached configuration
/// - **Reload Support**: Can reload from disk when needed
/// - **Custom Paths**: Supports custom configuration file paths for testing
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust,no_run
/// use agpm_cli::config::GlobalConfigManager;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut manager = GlobalConfigManager::new()?;
///
/// // First access loads from disk
/// let config = manager.get().await?;
/// println!("Found {} sources", config.sources.len());
///
/// // Subsequent accesses use cache
/// let config2 = manager.get().await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Modifying Configuration
///
/// ```rust,no_run
/// use agpm_cli::config::GlobalConfigManager;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut manager = GlobalConfigManager::new()?;
///
/// // Get mutable reference
/// let config = manager.get_mut().await?;
/// config.add_source(
///     "new".to_string(),
///     "https://github.com/owner/repo.git".to_string()
/// );
///
/// // Save changes to disk
/// manager.save().await?;
/// # Ok(())
/// # }
/// ```
pub struct GlobalConfigManager {
    config: Option<GlobalConfig>,
    path: PathBuf,
}

impl GlobalConfigManager {
    /// Create a new configuration manager using the default global config path.
    ///
    /// The manager will use the platform-appropriate default location for
    /// the global configuration file.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfigManager;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manager = GlobalConfigManager::new()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the default configuration path cannot be determined.
    pub fn new() -> Result<Self> {
        Ok(Self {
            config: None,
            path: GlobalConfig::default_path()?,
        })
    }

    /// Create a configuration manager with a custom file path.
    ///
    /// This method is primarily useful for testing or when you need to
    /// use a non-standard location for the configuration file.
    ///
    /// # Parameters
    ///
    /// - `path`: Custom path for the configuration file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfigManager;
    /// use std::path::PathBuf;
    ///
    /// let manager = GlobalConfigManager::with_path(PathBuf::from("/tmp/test.toml"));
    /// ```
    #[must_use]
    pub const fn with_path(path: PathBuf) -> Self {
        Self {
            config: None,
            path,
        }
    }

    /// Get a reference to the global configuration, loading it if necessary.
    ///
    /// If the configuration hasn't been loaded yet, this method will load it
    /// from disk. Subsequent calls will return the cached configuration.
    ///
    /// # Returns
    ///
    /// A reference to the cached [`GlobalConfig`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfigManager;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = GlobalConfigManager::new()?;
    /// let config = manager.get().await?;
    /// println!("Global config has {} sources", config.sources.len());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration file exists but cannot be read
    /// - The configuration file contains invalid TOML syntax
    pub async fn get(&mut self) -> Result<&GlobalConfig> {
        if self.config.is_none() {
            self.config = Some(if self.path.exists() {
                GlobalConfig::load_from(&self.path).await?
            } else {
                GlobalConfig::default()
            });
        }

        Ok(self.config.as_ref().unwrap())
    }

    /// Get a mutable reference to the global configuration, loading it if necessary.
    ///
    /// Similar to [`get`](Self::get), but returns a mutable reference allowing
    /// modification of the configuration.
    ///
    /// # Returns
    ///
    /// A mutable reference to the cached [`GlobalConfig`].
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfigManager;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = GlobalConfigManager::new()?;
    /// let config = manager.get_mut().await?;
    ///
    /// config.add_source(
    ///     "new".to_string(),
    ///     "https://github.com/owner/repo.git".to_string()
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration file exists but cannot be read
    /// - The configuration file contains invalid TOML syntax
    pub async fn get_mut(&mut self) -> Result<&mut GlobalConfig> {
        if self.config.is_none() {
            self.config = Some(if self.path.exists() {
                GlobalConfig::load_from(&self.path).await?
            } else {
                GlobalConfig::default()
            });
        }

        Ok(self.config.as_mut().unwrap())
    }

    /// Save the current cached configuration to disk.
    ///
    /// Writes the current configuration state to the file system. If no
    /// configuration has been loaded, this method does nothing.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfigManager;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = GlobalConfigManager::new()?;
    ///
    /// // Modify configuration
    /// let config = manager.get_mut().await?;
    /// config.add_source("test".to_string(), "https://test.com/repo.git".to_string());
    ///
    /// // Save changes
    /// manager.save().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be written (permissions, disk space, etc.)
    /// - Parent directories cannot be created
    pub async fn save(&self) -> Result<()> {
        if let Some(config) = &self.config {
            config.save_to(&self.path).await?;
        }
        Ok(())
    }

    /// Reload the configuration from disk, discarding cached data.
    ///
    /// Forces a reload of the configuration from the file system, discarding
    /// any cached data. Useful when the configuration file may have been
    /// modified externally.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::config::GlobalConfigManager;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let mut manager = GlobalConfigManager::new()?;
    ///
    /// // Load initial configuration
    /// let config1 = manager.get().await?;
    /// let count1 = config1.sources.len();
    ///
    /// // Configuration file modified externally...
    ///
    /// // Reload to pick up external changes
    /// manager.reload().await?;
    /// let config2 = manager.get().await?;
    /// let count2 = config2.sources.len();
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration file exists but cannot be read
    /// - The configuration file contains invalid TOML syntax
    pub async fn reload(&mut self) -> Result<()> {
        self.config = Some(if self.path.exists() {
            GlobalConfig::load_from(&self.path).await?
        } else {
            GlobalConfig::default()
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_global_config_default() {
        let config = GlobalConfig::default();
        assert!(config.sources.is_empty());
    }

    #[tokio::test]
    async fn test_global_config_save_load() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        let mut config = GlobalConfig::default();
        config.add_source("test".to_string(), "https://example.com/repo.git".to_string());

        config.save_to(&config_path).await.unwrap();

        let loaded = GlobalConfig::load_from(&config_path).await.unwrap();
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.get_source("test"), Some(&"https://example.com/repo.git".to_string()));
    }

    #[tokio::test]
    async fn test_merge_sources() {
        let mut global = GlobalConfig::default();
        global.add_source("private".to_string(), "https://token@private.com/repo.git".to_string());
        global.add_source("shared".to_string(), "https://shared.com/repo.git".to_string());

        let mut local = HashMap::new();
        local.insert("shared".to_string(), "https://override.com/repo.git".to_string());
        local.insert("public".to_string(), "https://public.com/repo.git".to_string());

        let merged = global.merge_sources(&local);

        // Global source preserved
        assert_eq!(merged.get("private"), Some(&"https://token@private.com/repo.git".to_string()));

        // Local override wins
        assert_eq!(merged.get("shared"), Some(&"https://override.com/repo.git".to_string()));

        // Local-only source included
        assert_eq!(merged.get("public"), Some(&"https://public.com/repo.git".to_string()));
    }

    #[tokio::test]
    async fn test_source_operations() {
        let mut config = GlobalConfig::default();

        // Add source
        config.add_source("test".to_string(), "https://test.com/repo.git".to_string());
        assert!(config.has_source("test"));
        assert_eq!(config.get_source("test"), Some(&"https://test.com/repo.git".to_string()));

        // Update source
        config.add_source("test".to_string(), "https://updated.com/repo.git".to_string());
        assert_eq!(config.get_source("test"), Some(&"https://updated.com/repo.git".to_string()));

        // Remove source
        assert!(config.remove_source("test"));
        assert!(!config.has_source("test"));
        assert!(!config.remove_source("test")); // Already removed
    }

    #[tokio::test]
    async fn test_init_example() {
        let config = GlobalConfig::init_example();

        assert!(config.has_source("private"));
        assert_eq!(
            config.get_source("private"),
            Some(&"https://oauth2:YOUR_TOKEN@github.com/yourcompany/private-agpm.git".to_string())
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_config_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test-config.toml");

        // Create and save config
        let config = GlobalConfig::default();
        config.save_to(&config_path).await.unwrap();

        // Check permissions
        let metadata = tokio::fs::metadata(&config_path).await.unwrap();
        let permissions = metadata.permissions();
        let mode = permissions.mode() & 0o777; // Get only permission bits

        assert_eq!(mode, 0o600, "Config file should have 600 permissions");
    }

    #[tokio::test]
    async fn test_config_manager() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        let mut manager = GlobalConfigManager::with_path(config_path.clone());

        // Get config (should create default)
        let config = manager.get_mut().await.unwrap();
        config.add_source("test".to_string(), "https://test.com/repo.git".to_string());

        // Save
        manager.save().await.unwrap();

        // Create new manager and verify it loads
        let mut manager2 = GlobalConfigManager::with_path(config_path);
        let config2 = manager2.get().await.unwrap();
        assert!(config2.has_source("test"));
    }
}
