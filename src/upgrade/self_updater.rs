use anyhow::{Context, Result};
use self_update::backends::github::Update;
use self_update::cargo_crate_version;
use tracing::{debug, info, warn};

/// Core self-update manager for CCPM binary upgrades.
///
/// `SelfUpdater` handles the entire process of checking for and installing CCPM updates
/// from GitHub releases. It provides a safe, reliable way to upgrade the running binary
/// with proper error handling and version management.
///
/// # Features
///
/// - **GitHub Integration**: Fetches releases from the official CCPM repository
/// - **Version Comparison**: Uses semantic versioning for intelligent update detection
/// - **Force Updates**: Allows forcing updates even when already on latest version
/// - **Target Versions**: Supports upgrading to specific versions or latest
/// - **Progress Tracking**: Shows download progress during updates
///
/// # Safety
///
/// The updater itself only handles the download and binary replacement. For full
/// safety, it should be used in conjunction with [`BackupManager`](crate::upgrade::backup::BackupManager)
/// to create backups before updates.
///
/// # Examples
///
/// ## Check for Updates
/// ```rust,no_run
/// use ccpm::upgrade::SelfUpdater;
///
/// # async fn example() -> anyhow::Result<()> {
/// let updater = SelfUpdater::new();
///
/// if let Some(latest_version) = updater.check_for_update().await? {
///     println!("Update available: {} -> {}",
///              updater.current_version(), latest_version);
/// } else {
///     println!("Already on latest version: {}", updater.current_version());
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Update to Latest Version
/// ```rust,no_run
/// use ccpm::upgrade::SelfUpdater;
///
/// # async fn example() -> anyhow::Result<()> {
/// let updater = SelfUpdater::new();
///
/// match updater.update_to_latest().await? {
///     true => println!("Successfully updated to latest version"),
///     false => println!("Already on latest version"),
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Force Update
/// ```rust,no_run
/// use ccpm::upgrade::SelfUpdater;
///
/// # async fn example() -> anyhow::Result<()> {
/// let updater = SelfUpdater::new().force(true);
///
/// // This will update even if already on the latest version
/// updater.update_to_latest().await?;
/// # Ok(())
/// # }
/// ```
///
/// # Repository Configuration
///
/// By default, updates are fetched from `aig787/ccpm` on GitHub. This is configured
/// in the [`Default`] implementation and targets the official CCPM repository.
///
/// # Error Handling
///
/// All methods return `Result<T, anyhow::Error>` for comprehensive error handling:
/// - Network errors during GitHub API calls
/// - Version parsing errors for invalid semver
/// - File system errors during binary replacement
/// - Permission errors on locked or protected files
pub struct SelfUpdater {
    /// GitHub repository owner (e.g., "aig787").
    repo_owner: String,
    /// GitHub repository name (e.g., "ccpm").
    repo_name: String,
    /// Binary name to update (e.g., "ccpm").
    bin_name: String,
    /// Current version of the running binary.
    current_version: String,
    /// Whether to force updates even when already on latest version.
    force: bool,
}

impl Default for SelfUpdater {
    /// Create a new `SelfUpdater` with default configuration.
    ///
    /// # Default Configuration
    ///
    /// - **Repository**: `aig787/ccpm` (official CCPM repository)
    /// - **Binary Name**: `ccpm`
    /// - **Current Version**: Detected from build-time crate version
    /// - **Force Mode**: Disabled (respects version comparisons)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// let updater = SelfUpdater::default();
    /// println!("Current version: {}", updater.current_version());
    /// ```
    fn default() -> Self {
        Self {
            repo_owner: "aig787".to_string(),
            repo_name: "ccpm".to_string(),
            bin_name: "ccpm".to_string(),
            current_version: cargo_crate_version!().to_string(),
            force: false,
        }
    }
}

impl SelfUpdater {
    /// Create a new `SelfUpdater` with default settings.
    ///
    /// This is equivalent to [`Default::default()`] but provides a more
    /// conventional constructor-style interface.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// let updater = SelfUpdater::new();
    /// assert_eq!(updater.current_version(), env!("CARGO_PKG_VERSION"));
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure whether to force updates regardless of version comparison.
    ///
    /// When force mode is enabled, the updater will attempt to download and
    /// install updates even if the current version is already the latest or
    /// newer than the target version.
    ///
    /// # Use Cases
    ///
    /// - **Reinstalling**: Fix corrupted binary installations
    /// - **Downgrading**: Install older versions for compatibility
    /// - **Testing**: Verify update mechanism functionality
    /// - **Recovery**: Restore from problematic versions
    ///
    /// # Arguments
    ///
    /// * `force` - `true` to enable force mode, `false` to respect version comparisons
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// // Normal update (respects versions)
    /// let updater = SelfUpdater::new();
    ///
    /// // Force update (ignores version comparison)
    /// let force_updater = SelfUpdater::new().force(true);
    /// ```
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Get the current version of the running CCPM binary.
    ///
    /// This version is determined at compile time from the crate's `Cargo.toml`
    /// and represents the version of the currently executing binary.
    ///
    /// # Returns
    ///
    /// A string slice containing the semantic version (e.g., "0.3.14").
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// let updater = SelfUpdater::new();
    /// println!("Current CCPM version: {}", updater.current_version());
    /// ```
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Check if a newer version is available on GitHub.
    ///
    /// Queries the GitHub API to fetch the latest release information and
    /// compares it with the current version using semantic versioning rules.
    /// This method does not download or install anything.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(version))` - A newer version is available
    /// - `Ok(None)` - Already on the latest version or no releases found
    /// - `Err(error)` - Network error, API failure, or version parsing error
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - Network connectivity issues prevent GitHub API access
    /// - GitHub API rate limiting is exceeded
    /// - Release version tags are not valid semantic versions
    /// - Repository is not found or access is denied
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let updater = SelfUpdater::new();
    ///
    /// match updater.check_for_update().await? {
    ///     Some(latest) => {
    ///         println!("Update available: {} -> {}",
    ///                  updater.current_version(), latest);
    ///     }
    ///     None => {
    ///         println!("Already on latest version: {}",
    ///                  updater.current_version());
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn check_for_update(&self) -> Result<Option<String>> {
        debug!(
            "Checking for updates from {}/{}",
            self.repo_owner, self.repo_name
        );

        let releases = self_update::backends::github::ReleaseList::configure()
            .repo_owner(&self.repo_owner)
            .repo_name(&self.repo_name)
            .build()?
            .fetch()?;

        if let Some(latest) = releases.first() {
            let latest_version = &latest.version;
            debug!("Latest version: {}", latest_version);

            let current = semver::Version::parse(&self.current_version)
                .context("Failed to parse current version")?;
            let latest =
                semver::Version::parse(latest_version).context("Failed to parse latest version")?;

            if latest > current {
                info!(
                    "Update available: {} -> {}",
                    self.current_version, latest_version
                );
                Ok(Some(latest_version.to_string()))
            } else {
                debug!("Already on latest version");
                Ok(None)
            }
        } else {
            warn!("No releases found");
            Ok(None)
        }
    }

    /// Update the CCPM binary to a specific version or latest.
    ///
    /// Downloads and installs the specified version from GitHub releases,
    /// replacing the current binary. This is the core update method used by
    /// both version-specific and latest update operations.
    ///
    /// # Arguments
    ///
    /// * `target_version` - Specific version to install (e.g., "0.4.0"), or `None` for latest
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Successfully updated to the target version
    /// - `Ok(false)` - Already on target version (no update needed)
    /// - `Err(error)` - Update failed due to download, permission, or file system error
    ///
    /// # Force Mode Behavior
    ///
    /// When force mode is enabled via [`force()`](Self::force):
    /// - Bypasses version comparison checks
    /// - Downloads and installs even if already on target version
    /// - Useful for reinstalling or recovery scenarios
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - Network issues prevent downloading the release
    /// - Insufficient permissions to replace the binary
    /// - Target version doesn't exist or has no binary assets
    /// - File system errors during binary replacement
    /// - The downloaded binary is corrupted or invalid
    ///
    /// # Platform Considerations
    ///
    /// - **Windows**: May require retries due to file locking
    /// - **Unix**: Preserves executable permissions
    /// - **macOS**: Works with both Intel and Apple Silicon binaries
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let updater = SelfUpdater::new();
    ///
    /// // Update to latest version
    /// if updater.update(None).await? {
    ///     println!("Successfully updated!");
    /// } else {
    ///     println!("Already on latest version");
    /// }
    ///
    /// // Update to specific version
    /// if updater.update(Some("0.4.0")).await? {
    ///     println!("Successfully updated to v0.4.0!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update(&self, target_version: Option<&str>) -> Result<bool> {
        info!("Starting self-update process");

        // If force is true, set a very old version to force an update
        let current_version = if self.force {
            "0.0.1"
        } else {
            &self.current_version
        };

        let mut builder = Update::configure();
        builder
            .repo_owner(&self.repo_owner)
            .repo_name(&self.repo_name)
            .bin_name(&self.bin_name)
            .show_download_progress(true)
            .current_version(current_version);

        if let Some(version) = target_version {
            builder.target_version_tag(&format!("v{}", version.trim_start_matches('v')));
        }

        let status = builder
            .build()
            .context("Failed to build updater")?
            .update()
            .context("Failed to update")?;

        if status.updated() {
            info!("Successfully updated to version {}", status.version());
            Ok(true)
        } else if self.force {
            // If force was set but no update happened, it means we're already on the target
            info!("Already on target version {}", self.current_version);
            Ok(false)
        } else {
            info!("Already on latest version {}", self.current_version);
            Ok(false)
        }
    }

    /// Update to the latest available version from GitHub releases.
    ///
    /// This is a convenience method that calls [`update()`](Self::update) with `None`
    /// as the target version, instructing it to find and install the most recent release.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Successfully updated to a newer version
    /// - `Ok(false)` - Already on the latest version (no update performed)
    /// - `Err(error)` - Update failed (see [`update()`](Self::update) for error conditions)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let updater = SelfUpdater::new();
    ///
    /// match updater.update_to_latest().await? {
    ///     true => println!("Successfully updated to latest version!"),
    ///     false => println!("Already on the latest version"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # See Also
    ///
    /// - [`update_to_version()`](Self::update_to_version) - Update to a specific version
    /// - [`check_for_update()`](Self::check_for_update) - Check for updates without installing
    pub async fn update_to_latest(&self) -> Result<bool> {
        self.update(None).await
    }

    /// Update to a specific version from GitHub releases.
    ///
    /// Downloads and installs the specified version, regardless of whether it's
    /// newer or older than the current version. The version string will be
    /// automatically prefixed with 'v' if not already present.
    ///
    /// # Arguments
    ///
    /// * `version` - The target version string (e.g., "0.4.0" or "v0.4.0")
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Successfully updated to the specified version
    /// - `Ok(false)` - Already on the specified version (no update needed)
    /// - `Err(error)` - Update failed (see [`update()`](Self::update) for error conditions)
    ///
    /// # Version Format
    ///
    /// The version parameter accepts multiple formats:
    /// - `"0.4.0"` - Semantic version number
    /// - `"v0.4.0"` - Version with 'v' prefix (GitHub tag format)
    /// - `"0.4.0-beta.1"` - Pre-release versions
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let updater = SelfUpdater::new();
    ///
    /// // Update to specific stable version
    /// updater.update_to_version("0.4.0").await?;
    ///
    /// // Update to pre-release version
    /// updater.update_to_version("v0.5.0-beta.1").await?;
    ///
    /// // Force downgrade to older version
    /// let force_updater = updater.force(true);
    /// force_updater.update_to_version("0.3.0").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # See Also
    ///
    /// - [`update_to_latest()`](Self::update_to_latest) - Update to the newest available version
    /// - [`check_for_update()`](Self::check_for_update) - Check what version is available
    pub async fn update_to_version(&self, version: &str) -> Result<bool> {
        self.update(Some(version)).await
    }
}
