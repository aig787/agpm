use serde::{Deserialize, Serialize};

/// Configuration settings for AGPM self-update behavior.
///
/// `UpgradeConfig` defines how AGPM handles automatic update checking,
/// backup creation, and security verification during upgrades. These settings
/// can be configured globally or per-project to control update behavior.
///
/// # Configuration Categories
///
/// ## Update Timing
/// - **Startup Checks**: Whether to check for updates when AGPM starts
/// - **Check Intervals**: How frequently to perform background update checks
///
/// ## Safety Settings
/// - **Automatic Backups**: Whether to create backups before upgrades
/// - **Checksum Verification**: Whether to verify download integrity
///
/// # Default Behavior
///
/// The default configuration prioritizes safety and user control:
/// - No automatic update checking on startup (avoids startup delays)
/// - 24-hour intervals for update checks (balances freshness with performance)
/// - Always create backups (enables rollback on failures)
/// - Always verify checksums (ensures download integrity)
///
/// # Examples
///
/// ## Using Default Configuration
/// ```rust,no_run
/// use agpm::upgrade::config::UpgradeConfig;
///
/// let config = UpgradeConfig::default();
/// assert_eq!(config.check_on_startup, false);
/// assert_eq!(config.auto_backup, true);
/// ```
///
/// ## Custom Configuration
/// ```rust,no_run
/// use agpm::upgrade::config::UpgradeConfig;
///
/// let config = UpgradeConfig {
///     check_on_startup: true,
///     check_interval: 3600, // 1 hour
///     auto_backup: true,
///     verify_checksum: true,
/// };
/// ```
///
/// # Serialization
///
/// This configuration can be serialized to TOML, JSON, or other formats
/// supported by serde for storage in configuration files.
///
/// ## TOML Example
/// ```toml
/// [upgrade]
/// check_on_startup = false
/// check_interval = 86400
/// auto_backup = true
/// verify_checksum = true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeConfig {
    /// Whether to check for updates when AGPM starts.
    ///
    /// When enabled, AGPM will perform a background check for updates every
    /// time it starts up. This provides the earliest notification of available
    /// updates but may slightly delay startup time.
    ///
    /// # Default: `false`
    ///
    /// Disabled by default to avoid slowing down CLI operations. Users can
    /// manually check for updates using `agpm upgrade --check`.
    ///
    /// # Considerations
    ///
    /// - **Startup Performance**: Adds network delay to every AGPM invocation
    /// - **Network Dependency**: May fail or timeout in poor network conditions
    /// - **Rate Limiting**: Frequent use may hit GitHub API rate limits
    /// - **User Experience**: Can be intrusive for automated scripts
    #[serde(default = "default_check_on_startup")]
    pub check_on_startup: bool,

    /// Interval between automatic update checks in seconds.
    ///
    /// Controls how frequently AGPM performs background checks for new versions.
    /// This setting balances update notification timeliness with network usage
    /// and API rate limit consumption.
    ///
    /// # Default: `86400` (24 hours)
    ///
    /// The default 24-hour interval provides daily update notifications while
    /// being respectful of GitHub's API rate limits.
    ///
    /// # Recommended Values
    ///
    /// - **3600** (1 hour): For development environments or beta testing
    /// - **21600** (6 hours): For active development workflows
    /// - **86400** (1 day): Standard for most users (default)
    /// - **604800** (1 week): For stable environments with infrequent updates
    ///
    /// # Rate Limiting
    ///
    /// GitHub allows 60 unauthenticated API requests per hour per IP address.
    /// Setting intervals below 1 minute may exceed rate limits with heavy usage.
    #[serde(default = "default_check_interval")]
    pub check_interval: u64,

    /// Whether to automatically create backups before upgrades.
    ///
    /// When enabled, AGPM creates a backup copy of the current binary before
    /// attempting any upgrade. This enables rollback if the upgrade fails or
    /// the new version has issues.
    ///
    /// # Default: `true`
    ///
    /// Enabled by default for maximum safety. Backups use minimal disk space
    /// and provide crucial recovery capability.
    ///
    /// # Backup Process
    ///
    /// - Creates a copy with `.backup` suffix in the same directory
    /// - Preserves file permissions and metadata
    /// - Automatically removed after successful upgrades
    /// - Can be restored manually or via `agpm upgrade --rollback`
    ///
    /// # Disabling Backups
    ///
    /// Consider disabling only in environments where:
    /// - Disk space is severely constrained
    /// - File system permissions prevent backup creation
    /// - Alternative backup/recovery mechanisms are in place
    /// - Upgrade failures can be resolved through reinstallation
    #[serde(default = "default_auto_backup")]
    pub auto_backup: bool,

    /// Whether to verify checksums of downloaded binaries.
    ///
    /// When enabled, AGPM verifies the integrity of downloaded binaries by
    /// comparing their checksums against expected values. This provides
    /// protection against corrupted downloads and potential security issues.
    ///
    /// # Default: `true`
    ///
    /// Enabled by default for security and reliability. Checksum verification
    /// adds minimal overhead but provides important integrity guarantees.
    ///
    /// # Security Benefits
    ///
    /// - **Download Integrity**: Detects corrupted or incomplete downloads
    /// - **Tamper Detection**: Identifies potentially modified binaries
    /// - **Supply Chain Security**: Helps ensure binary authenticity
    /// - **Network Reliability**: Catches network-induced corruption
    ///
    /// # Verification Process
    ///
    /// - Downloads expected checksums from GitHub releases
    /// - Computes actual checksum of downloaded binary
    /// - Compares checksums before proceeding with installation
    /// - Aborts upgrade if checksums don't match
    ///
    /// # Disabling Verification
    ///
    /// Consider disabling only in environments where:
    /// - Network reliability is extremely poor
    /// - Checksum information is unavailable from releases
    /// - Alternative integrity verification is in place
    /// - Testing scenarios require bypassing verification
    #[serde(default = "default_verify_checksum")]
    pub verify_checksum: bool,
}

impl Default for UpgradeConfig {
    /// Create an `UpgradeConfig` with safe, conservative defaults.
    ///
    /// The default configuration prioritizes safety, reliability, and user control
    /// over aggressive update checking. This approach:
    ///
    /// - Avoids surprising users with automatic behavior
    /// - Minimizes impact on CLI performance
    /// - Provides maximum safety during upgrades
    /// - Respects GitHub API rate limits
    ///
    /// # Default Values
    ///
    /// - `check_on_startup`: `false` - No startup delays
    /// - `check_interval`: `86400` (24 hours) - Daily update checks
    /// - `auto_backup`: `true` - Always create backups for safety
    /// - `verify_checksum`: `true` - Always verify download integrity
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::config::UpgradeConfig;
    ///
    /// let config = UpgradeConfig::default();
    /// assert_eq!(config.check_on_startup, false);
    /// assert_eq!(config.check_interval, 86400);
    /// assert_eq!(config.auto_backup, true);
    /// assert_eq!(config.verify_checksum, true);
    /// ```
    fn default() -> Self {
        Self {
            check_on_startup: default_check_on_startup(),
            check_interval: default_check_interval(),
            auto_backup: default_auto_backup(),
            verify_checksum: default_verify_checksum(),
        }
    }
}

/// Default value for startup update checking.
///
/// Returns `false` to avoid adding network latency to every AGPM invocation.
/// Users can explicitly enable this or use manual update checking.
fn default_check_on_startup() -> bool {
    false // Default to not checking on startup to avoid slowing down the CLI
}

/// Default value for update check interval.
///
/// Returns `86400` (24 hours) to provide daily update notifications while
/// being respectful of GitHub API rate limits and user attention.
fn default_check_interval() -> u64 {
    86400 // 24 hours in seconds
}

/// Default value for automatic backup creation.
///
/// Returns `true` to maximize safety during upgrades. Backups enable quick
/// recovery from failed upgrades and add minimal overhead.
fn default_auto_backup() -> bool {
    true // Always create backups for safety
}

/// Default value for checksum verification.
///
/// Returns `true` to ensure download integrity and provide security against
/// corrupted or tampered binaries. Verification adds minimal overhead.
fn default_verify_checksum() -> bool {
    true // Always verify checksums for security
}

impl UpgradeConfig {
    /// Create a new `UpgradeConfig` with default settings.
    ///
    /// This is equivalent to [`Default::default()`] but provides a more
    /// conventional constructor-style interface for creating configurations.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::config::UpgradeConfig;
    ///
    /// // These are equivalent
    /// let config1 = UpgradeConfig::new();
    /// let config2 = UpgradeConfig::default();
    ///
    /// assert_eq!(config1.check_on_startup, config2.check_on_startup);
    /// assert_eq!(config1.auto_backup, config2.auto_backup);
    /// ```
    ///
    /// # See Also
    ///
    /// - [`Default::default()`] - Alternative way to create default configuration
    pub fn new() -> Self {
        Self::default()
    }
}
