use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info};

use crate::config::GlobalConfig;
use crate::upgrade::SelfUpdater;

/// Cached version information with notification tracking.
///
/// This structure stores version check results along with timestamps
/// and notification state to provide intelligent update prompting.
///
/// # Fields
///
/// * `latest_version` - The latest version string from GitHub releases
/// * `current_version` - The version that was running when cached
/// * `checked_at` - UTC timestamp when this information was fetched
/// * `update_available` - Whether an update was available at check time
/// * `notified` - Whether the user has been notified about this update
/// * `notification_count` - Number of times user has been notified
///
/// # Serialization
///
/// This struct is serialized to JSON for persistent caching between AGPM runs.
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionCheckCache {
    /// The latest version string from GitHub releases (e.g., "0.4.0").
    pub latest_version: String,
    /// The version that was running when this cache was created.
    pub current_version: String,
    /// UTC timestamp when this version information was fetched.
    pub checked_at: DateTime<Utc>,
    /// Whether an update was available at the time of check.
    pub update_available: bool,
    /// Whether the user has been notified about this specific update.
    pub notified: bool,
    /// Number of times the user has been notified about this update.
    #[serde(default)]
    pub notification_count: u32,
}

impl VersionCheckCache {
    /// Create a new cache entry from version information.
    pub fn new(current_version: String, latest_version: String) -> Self {
        let update_available = {
            let current = semver::Version::parse(&current_version).ok();
            let latest = semver::Version::parse(&latest_version).ok();

            match (current, latest) {
                (Some(c), Some(l)) => l > c,
                _ => false,
            }
        };

        Self {
            latest_version: latest_version.clone(),
            current_version,
            checked_at: Utc::now(),
            update_available,
            notified: false,
            notification_count: 0,
        }
    }

    /// Check if the cache is still valid based on the given interval.
    pub fn is_valid(&self, interval_seconds: u64) -> bool {
        let age = Utc::now() - self.checked_at;
        age.num_seconds() < interval_seconds as i64
    }

    /// Mark this update as notified and increment the count.
    pub fn mark_notified(&mut self) {
        self.notified = true;
        self.notification_count += 1;
    }

    /// Check if we should notify about this update.
    ///
    /// Implements a backoff strategy to avoid notification fatigue:
    /// - First notification: immediate
    /// - Subsequent notifications: with increasing intervals
    pub fn should_notify(&self) -> bool {
        if !self.update_available {
            return false;
        }

        if !self.notified {
            return true;
        }

        // Implement exponential backoff for re-notifications
        // Don't re-notify more than once per day after initial notification
        let hours_since_check = (Utc::now() - self.checked_at).num_hours();
        let backoff_hours = 24 * (1 << self.notification_count.min(3)); // 24h, 48h, 96h, 192h max

        hours_since_check >= backoff_hours as i64
    }
}

/// Version checking and caching system with automatic update notifications.
///
/// `VersionChecker` provides intelligent caching of version information and
/// automatic update checking based on user configuration. It manages notification
/// state to avoid alert fatigue while ensuring users are aware of updates.
///
/// # Features
///
/// - **Automatic Checking**: Checks for updates based on configured intervals
/// - **Smart Caching**: Reduces GitHub API calls with intelligent cache management
/// - **Notification Tracking**: Avoids repeated notifications for the same update
/// - **Configurable Behavior**: Respects user preferences for update checking
///
/// # Caching Strategy
///
/// The version checker implements a sophisticated caching strategy:
/// - Stores version information with timestamps
/// - Tracks notification state to avoid alert fatigue
/// - Uses configurable intervals for cache expiration
/// - Implements exponential backoff for re-notifications
pub struct VersionChecker {
    /// Path to the version cache file.
    cache_path: PathBuf,
    /// The self-updater instance for version checking.
    updater: SelfUpdater,
    /// Global configuration with upgrade settings.
    config: GlobalConfig,
}

impl VersionChecker {
    /// Create a new `VersionChecker` with configuration from global settings.
    ///
    /// Loads the global configuration and sets up the version checker with
    /// appropriate cache paths and update settings.
    ///
    /// # Returns
    ///
    /// - `Ok(VersionChecker)` - Successfully created with loaded configuration
    /// - `Err(error)` - Failed to load configuration or determine cache path
    ///
    /// # Cache Location
    ///
    /// The cache file is stored at:
    /// - Unix/macOS: `~/.agpm/.version_cache`
    /// - Windows: `%LOCALAPPDATA%\agpm\.version_cache`
    pub async fn new() -> Result<Self> {
        let config = GlobalConfig::load().await?;
        let updater = SelfUpdater::new();

        // Determine cache path based on configuration directory
        let cache_path = if let Ok(path) = std::env::var("AGPM_CONFIG_PATH") {
            PathBuf::from(path)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".agpm"))
                .join(".version_cache")
        } else {
            dirs::home_dir()
                .context("Could not determine home directory")?
                .join(".agpm")
                .join(".version_cache")
        };

        Ok(Self {
            cache_path,
            updater,
            config,
        })
    }

    /// Create a new `VersionChecker` with custom cache directory.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory where the version cache file will be stored
    pub fn with_cache_dir(mut self, cache_dir: PathBuf) -> Self {
        self.cache_path = cache_dir.join(".version_cache");
        self
    }

    /// Check for updates automatically based on configuration.
    ///
    /// This is the main entry point for automatic update checking. It:
    /// 1. Checks if automatic updates are enabled in configuration
    /// 2. Loads and validates the cache
    /// 3. Performs a new check if cache is expired
    /// 4. Returns version info if user should be notified
    ///
    /// # Returns
    ///
    /// - `Ok(Some(version))` - Update available and user should be notified
    /// - `Ok(None)` - No update or notification not needed
    /// - `Err(error)` - Error during check (non-fatal, logged)
    pub async fn check_for_updates_if_needed(&self) -> Result<Option<String>> {
        // Check if automatic checking is disabled
        if !self.config.upgrade.check_on_startup && self.config.upgrade.check_interval == 0 {
            debug!("Automatic update checking is disabled");
            return Ok(None);
        }

        // Load existing cache
        let mut cache = self.load_cache().await?;

        // Determine if we need a new check
        let should_check = match &cache {
            None => true,
            Some(c) => !c.is_valid(self.config.upgrade.check_interval),
        };

        if should_check {
            debug!("Performing automatic update check");

            // Perform the check
            match self.updater.check_for_update().await {
                Ok(Some(latest_version)) => {
                    // Create new cache entry
                    let mut new_cache = VersionCheckCache::new(
                        self.updater.current_version().to_string(),
                        latest_version.clone(),
                    );

                    // Check if we should notify
                    let should_notify = match &cache {
                        None => true,
                        Some(old) => {
                            // New update available or not notified about current one
                            old.latest_version != latest_version || !old.notified
                        }
                    };

                    if should_notify {
                        new_cache.mark_notified();
                        self.save_cache(&new_cache).await?;

                        info!(
                            "Update available: {} -> {}",
                            self.updater.current_version(),
                            latest_version
                        );
                        return Ok(Some(latest_version));
                    } else {
                        // Update cache without notification
                        self.save_cache(&new_cache).await?;
                    }
                }
                Ok(None) => {
                    // No update available, update cache
                    let new_cache = VersionCheckCache::new(
                        self.updater.current_version().to_string(),
                        self.updater.current_version().to_string(),
                    );
                    self.save_cache(&new_cache).await?;
                    debug!("No update available, cache updated");
                }
                Err(e) => {
                    // Don't fail the command if update check fails
                    debug!("Update check failed: {}", e);
                }
            }
        } else if let Some(ref mut c) = cache {
            // Cache is still valid, check if we should re-notify
            if c.should_notify() {
                c.mark_notified();
                self.save_cache(c).await?;

                info!(
                    "Update available (reminder): {} -> {}",
                    c.current_version, c.latest_version
                );
                return Ok(Some(c.latest_version.clone()));
            }
        }

        Ok(None)
    }

    /// Perform an explicit update check, bypassing the cache.
    ///
    /// This method always queries GitHub for the latest version,
    /// regardless of cache state. Used for manual update checks.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(version))` - New version available
    /// - `Ok(None)` - Already on latest version
    /// - `Err(error)` - Check failed
    pub async fn check_now(&self) -> Result<Option<String>> {
        debug!("Performing explicit update check");

        let result = self.updater.check_for_update().await?;

        // Update cache with the result
        let cache = VersionCheckCache::new(
            self.updater.current_version().to_string(),
            result
                .as_ref()
                .unwrap_or(&self.updater.current_version().to_string())
                .to_string(),
        );
        self.save_cache(&cache).await?;

        Ok(result)
    }

    /// Load the version cache from disk.
    async fn load_cache(&self) -> Result<Option<VersionCheckCache>> {
        if !self.cache_path.exists() {
            debug!("No version cache found");
            return Ok(None);
        }

        let content = fs::read_to_string(&self.cache_path)
            .await
            .context("Failed to read version cache")?;

        let cache: VersionCheckCache =
            serde_json::from_str(&content).context("Failed to parse version cache")?;

        Ok(Some(cache))
    }

    /// Save the version cache to disk.
    async fn save_cache(&self, cache: &VersionCheckCache) -> Result<()> {
        let content =
            serde_json::to_string_pretty(cache).context("Failed to serialize version cache")?;

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
    /// Removes cached version information, forcing subsequent version
    /// checks to fetch fresh data from GitHub.
    pub async fn clear_cache(&self) -> Result<()> {
        if self.cache_path.exists() {
            fs::remove_file(&self.cache_path)
                .await
                .context("Failed to remove version cache")?;
            debug!("Cleared version cache");
        }
        Ok(())
    }

    /// Display a user-friendly update notification.
    ///
    /// Shows an attractive notification banner informing the user
    /// about the available update with instructions on how to upgrade.
    ///
    /// # Arguments
    ///
    /// * `latest_version` - The new version available for upgrade
    pub fn display_update_notification(latest_version: &str) {
        use colored::*;

        let current_version = env!("CARGO_PKG_VERSION");

        eprintln!();
        eprintln!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_cyan()
        );
        eprintln!("{} A new version of AGPM is available!", "📦".bright_cyan());
        eprintln!();
        eprintln!("  Current version: {}", current_version.yellow());
        eprintln!("  Latest version:  {}", latest_version.green().bold());
        eprintln!();
        eprintln!("  Run {} to upgrade", "agpm upgrade".cyan().bold());
        eprintln!();
        eprintln!("  To disable automatic update checks, run:");
        eprintln!("  {}", "agpm config set upgrade.check_interval 0".dimmed());
        eprintln!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_cyan()
        );
        eprintln!();
    }

    /// Format version information for status display.
    ///
    /// Creates a human-readable string showing the current version and,
    /// if available, the latest version with update availability.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cache_validity() {
        let cache = VersionCheckCache::new("1.0.0".to_string(), "1.1.0".to_string());

        // Should be valid for 1 hour
        assert!(cache.is_valid(3600));

        // Should not be valid for 0 seconds
        assert!(!cache.is_valid(0));
    }

    #[tokio::test]
    async fn test_notification_logic() {
        let mut cache = VersionCheckCache::new("1.0.0".to_string(), "1.1.0".to_string());

        // First time should notify
        assert!(cache.should_notify());

        // After marking as notified, shouldn't notify immediately
        cache.mark_notified();
        assert!(!cache.should_notify());

        // Notification count should increase
        assert_eq!(cache.notification_count, 1);
    }

    #[tokio::test]
    async fn test_cache_save_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        unsafe {
            std::env::set_var("AGPM_CONFIG_PATH", temp_dir.path().join("config.toml"));
        }

        // Can't test the full VersionChecker without mocking, but can test cache directly
        let cache = VersionCheckCache::new("1.0.0".to_string(), "1.1.0".to_string());

        let cache_path = temp_dir.path().join(".version_cache");
        let content = serde_json::to_string_pretty(&cache)?;
        tokio::fs::write(&cache_path, content).await?;

        let loaded_content = tokio::fs::read_to_string(&cache_path).await?;
        let loaded: VersionCheckCache = serde_json::from_str(&loaded_content)?;

        assert_eq!(loaded.current_version, "1.0.0");
        assert_eq!(loaded.latest_version, "1.1.0");
        assert!(loaded.update_available);

        unsafe {
            std::env::remove_var("AGPM_CONFIG_PATH");
        }
        Ok(())
    }
}
