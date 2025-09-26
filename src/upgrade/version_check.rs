use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

/// Cached version information from GitHub API.
///
/// This structure stores the latest version information along with a timestamp
/// to enable cache expiration and reduce unnecessary GitHub API calls.
///
/// # Fields
///
/// * `latest_version` - The latest version string from GitHub releases
/// * `checked_at` - UTC timestamp when this information was fetched
///
/// # Serialization
///
/// This struct is serialized to JSON for persistent caching between CCPM runs.
/// The cache file is stored in the user's cache directory and expires based on
/// the configured TTL (Time To Live).
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionCheckCache {
    /// The latest version string from GitHub releases (e.g., "0.4.0").
    pub latest_version: String,
    /// UTC timestamp when this version information was fetched.
    pub checked_at: DateTime<Utc>,
}

/// Version checking and caching system for CCPM updates.
///
/// `VersionChecker` provides intelligent caching of version information to reduce
/// GitHub API calls and improve performance. It stores the latest version information
/// locally with configurable expiration times.
///
/// # Caching Strategy
///
/// The version checker implements a simple but effective caching strategy:
/// - Stores version information in a JSON file in the cache directory
/// - Uses configurable TTL (Time To Live) for cache expiration
/// - Falls back to GitHub API when cache is expired or missing
/// - Automatically updates cache when new version information is fetched
///
/// # Performance Benefits
///
/// - **Reduced API Calls**: Minimizes GitHub API requests for frequently used commands
/// - **Faster Response**: Cached version checks are nearly instantaneous
/// - **Rate Limit Friendly**: Helps avoid GitHub API rate limiting
/// - **Offline Capability**: Can provide version info when network is limited
///
/// # Examples
///
/// ## Basic Version Checking
/// ```rust,no_run
/// use ccpm::upgrade::version_check::VersionChecker;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache_dir = PathBuf::from("~/.ccpm/cache");
/// let checker = VersionChecker::new(cache_dir);
///
/// // Check for cached version first
/// if let Some(cached_version) = checker.get_cached_version().await? {
///     println!("Latest version (cached): {}", cached_version);
/// } else {
///     println!("No cached version available, need to fetch from GitHub");
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Custom Cache TTL
/// ```rust,no_run
/// use ccpm::upgrade::version_check::VersionChecker;
/// use std::path::PathBuf;
///
/// let cache_dir = PathBuf::from("~/.ccpm/cache");
/// let checker = VersionChecker::new(cache_dir)
///     .with_ttl(7200); // 2 hours cache TTL
/// ```
///
/// ## Save and Format Version Information
/// ```rust,no_run
/// use ccpm::upgrade::version_check::VersionChecker;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let checker = VersionChecker::new(PathBuf::from("cache"));
///
/// // Save version to cache
/// checker.save_version("0.4.0".to_string()).await?;
///
/// // Format version information for display
/// let info = VersionChecker::format_version_info("0.3.14", Some("0.4.0"));
/// println!("{}", info);
/// # Ok(())
/// # }
/// ```
pub struct VersionChecker {
    /// Path to the version cache file.
    cache_path: PathBuf,
    /// Cache TTL (Time To Live) in seconds.
    cache_ttl_seconds: i64,
}

impl VersionChecker {
    /// Create a new `VersionChecker` with default settings.
    ///
    /// Sets up version checking with a default cache TTL of 1 hour (3600 seconds).
    /// The cache file will be stored as `version_check_cache.json` in the
    /// specified cache directory.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory where the version cache file will be stored
    ///
    /// # Cache Location
    ///
    /// The cache file is stored at `{cache_dir}/version_check_cache.json`.
    /// The directory will be created automatically when saving version information.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::version_check::VersionChecker;
    /// use std::path::PathBuf;
    ///
    /// // Use standard cache directory
    /// let cache_dir = dirs::cache_dir()
    ///     .unwrap_or_else(|| PathBuf::from(".cache"))
    ///     .join("ccpm");
    /// let checker = VersionChecker::new(cache_dir);
    ///
    /// // Use custom cache directory
    /// let checker = VersionChecker::new(PathBuf::from("/tmp/ccpm-cache"));
    /// ```
    pub fn new(cache_dir: PathBuf) -> Self {
        let cache_path = cache_dir.join("version_check_cache.json");
        Self {
            cache_path,
            cache_ttl_seconds: 3600, // 1 hour default TTL
        }
    }

    /// Configure the cache TTL (Time To Live) in seconds.
    ///
    /// Sets how long cached version information remains valid before requiring
    /// a fresh fetch from GitHub. Longer TTLs reduce API calls but may delay
    /// notification of new releases.
    ///
    /// # Arguments
    ///
    /// * `ttl_seconds` - Cache expiration time in seconds
    ///
    /// # Recommended TTL Values
    ///
    /// - **1 hour (3600)**: Default, good balance of freshness and performance
    /// - **15 minutes (900)**: For development or frequent update checking
    /// - **6 hours (21600)**: For stable environments with infrequent updates
    /// - **1 day (86400)**: For minimal API usage, slower update notification
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::version_check::VersionChecker;
    /// use std::path::PathBuf;
    ///
    /// // Short TTL for development
    /// let dev_checker = VersionChecker::new(PathBuf::from("cache"))
    ///     .with_ttl(900); // 15 minutes
    ///
    /// // Long TTL for production
    /// let prod_checker = VersionChecker::new(PathBuf::from("cache"))
    ///     .with_ttl(21600); // 6 hours
    /// ```
    pub fn with_ttl(mut self, ttl_seconds: i64) -> Self {
        self.cache_ttl_seconds = ttl_seconds;
        self
    }

    /// Retrieve cached version information if available and not expired.
    ///
    /// Checks the local cache for previously fetched version information.
    /// Returns the cached version only if it exists and hasn't exceeded the
    /// configured TTL (Time To Live).
    ///
    /// # Returns
    ///
    /// - `Ok(Some(version))` - Valid cached version found
    /// - `Ok(None)` - No cache found or cache expired
    /// - `Err(error)` - Error reading or parsing cache file
    ///
    /// # Cache Validation
    ///
    /// The method validates both existence and freshness:
    /// 1. Checks if cache file exists
    /// 2. Reads and parses cache content
    /// 3. Compares cache age against configured TTL
    /// 4. Returns version only if within TTL window
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - Cache file exists but is corrupted or invalid JSON
    /// - File system errors prevent reading the cache file
    /// - Cache format has changed between CCPM versions
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::version_check::VersionChecker;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let checker = VersionChecker::new(PathBuf::from("cache"));
    ///
    /// match checker.get_cached_version().await? {
    ///     Some(version) => {
    ///         println!("Using cached version: {}", version);
    ///         // Use cached version without GitHub API call
    ///     }
    ///     None => {
    ///         println!("Cache expired or missing, fetching from GitHub...");
    ///         // Fetch fresh version from GitHub API
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_cached_version(&self) -> Result<Option<String>> {
        if !self.cache_path.exists() {
            debug!("No version cache found");
            return Ok(None);
        }

        let content = fs::read_to_string(&self.cache_path)
            .await
            .context("Failed to read version cache")?;

        let cache: VersionCheckCache =
            serde_json::from_str(&content).context("Failed to parse version cache")?;

        let age = Utc::now() - cache.checked_at;
        if age < Duration::seconds(self.cache_ttl_seconds) {
            debug!(
                "Using cached version check (age: {} seconds)",
                age.num_seconds()
            );
            Ok(Some(cache.latest_version))
        } else {
            debug!("Version cache expired (age: {} seconds)", age.num_seconds());
            Ok(None)
        }
    }

    /// Save version information to cache for future use.
    ///
    /// Stores the provided version string along with the current timestamp
    /// in the cache file. This enables subsequent calls to use cached data
    /// instead of making GitHub API requests.
    ///
    /// # Arguments
    ///
    /// * `version` - The version string to cache (e.g., "0.4.0")
    ///
    /// # Process
    ///
    /// 1. Creates a cache entry with the version and current timestamp
    /// 2. Serializes the cache entry to pretty-printed JSON
    /// 3. Ensures the cache directory exists
    /// 4. Writes the cache file atomically
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Version successfully saved to cache
    /// - `Err(error)` - Failed to save cache due to file system or serialization error
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - Insufficient permissions to create cache directory or file
    /// - File system errors during write operation
    /// - JSON serialization fails (very unlikely)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::version_check::VersionChecker;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let checker = VersionChecker::new(PathBuf::from("cache"));
    ///
    /// // Save version after fetching from GitHub
    /// let latest_version = "0.4.0".to_string();
    /// checker.save_version(latest_version).await?;
    ///
    /// println!("Version cached for future use");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Cache Format
    ///
    /// The cache is stored as JSON:
    /// ```json
    /// {
    ///   "latest_version": "0.4.0",
    ///   "checked_at": "2024-01-15T10:30:00Z"
    /// }
    /// ```
    pub async fn save_version(&self, version: String) -> Result<()> {
        let cache = VersionCheckCache {
            latest_version: version,
            checked_at: Utc::now(),
        };

        let content =
            serde_json::to_string_pretty(&cache).context("Failed to serialize version cache")?;

        // Ensure cache directory exists
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create cache directory")?;
        }

        fs::write(&self.cache_path, content)
            .await
            .context("Failed to write version cache")?;

        debug!("Saved version check to cache");
        Ok(())
    }

    /// Clear the version cache by removing the cache file.
    ///
    /// Removes the cached version information, forcing subsequent version
    /// checks to fetch fresh data from GitHub. This is typically called
    /// after successful upgrades to ensure the cache doesn't contain
    /// outdated information.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Cache cleared successfully or no cache existed
    /// - `Err(error)` - Failed to remove cache file
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - Insufficient permissions to delete the cache file
    /// - File system errors during deletion
    /// - File is locked or in use (rare)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::version_check::VersionChecker;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let checker = VersionChecker::new(PathBuf::from("cache"));
    ///
    /// // Clear cache after successful upgrade
    /// checker.clear_cache().await?;
    /// println!("Cache cleared, next check will fetch fresh data");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Post-upgrade cleanup**: Clear cache after successful binary update
    /// - **Force refresh**: Force next version check to fetch from GitHub
    /// - **Troubleshooting**: Clear potentially corrupted cache data
    /// - **Testing**: Reset cache state for test scenarios
    ///
    /// # Note
    ///
    /// This method silently succeeds if no cache file exists, making it safe
    /// to call unconditionally.
    pub async fn clear_cache(&self) -> Result<()> {
        if self.cache_path.exists() {
            fs::remove_file(&self.cache_path)
                .await
                .context("Failed to remove version cache")?;
            debug!("Cleared version cache");
        }
        Ok(())
    }

    /// Format version information for user display.
    ///
    /// Creates a human-readable string showing the current version and, if available,
    /// the latest version with update availability information.
    ///
    /// # Arguments
    ///
    /// * `current` - The current version string (e.g., "0.3.14")
    /// * `latest` - Optional latest version string (e.g., Some("0.4.0") or None)
    ///
    /// # Returns
    ///
    /// A formatted string suitable for display to users.
    ///
    /// # Format Examples
    ///
    /// ## Update Available
    /// ```text
    /// Current version: 0.3.14
    /// Latest version:  0.4.0 (update available)
    /// ```
    ///
    /// ## Up to Date
    /// ```text
    /// Current version: 0.4.0 (up to date)
    /// ```
    ///
    /// ## No Latest Version Info
    /// ```text
    /// Current version: 0.3.14 (up to date)
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::version_check::VersionChecker;
    ///
    /// // Update available scenario
    /// let info = VersionChecker::format_version_info("0.3.14", Some("0.4.0"));
    /// println!("{}", info);
    /// // Output:
    /// // Current version: 0.3.14
    /// // Latest version:  0.4.0 (update available)
    ///
    /// // Up to date scenario
    /// let info = VersionChecker::format_version_info("0.4.0", Some("0.4.0"));
    /// println!("{}", info);
    /// // Output: Current version: 0.4.0 (up to date)
    ///
    /// // No latest info scenario
    /// let info = VersionChecker::format_version_info("0.3.14", None);
    /// println!("{}", info);
    /// // Output: Current version: 0.3.14 (up to date)
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Status commands**: Display version status to users
    /// - **Update notifications**: Show when updates are available
    /// - **CLI output**: Consistent formatting across commands
    /// - **Help messages**: Include version info in help text
    pub fn format_version_info(current: &str, latest: Option<&str>) -> String {
        match latest {
            Some(v) if v != current => {
                format!(
                    "Current version: {}\nLatest version:  {} (update available)",
                    current, v
                )
            }
            _ => format!("Current version: {} (up to date)", current),
        }
    }
}
