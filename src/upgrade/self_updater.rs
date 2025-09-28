use anyhow::{Context, Result, bail};
use semver::Version;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Validate repository identifiers to prevent URL injection attacks.
///
/// Repository owner and name must only contain alphanumeric characters,
/// hyphens, underscores, and dots. This prevents malicious injection of
/// special characters that could be used to construct malicious URLs.
///
/// # Arguments
///
/// * `identifier` - The repository owner or name to validate
///
/// # Returns
///
/// `true` if the identifier is safe to use in URL construction, `false` otherwise.
///
/// # Examples
///
/// ```rust,no_run
/// // This function is used internally by SelfUpdater::with_repo()
/// // for repository identifier validation
/// use ccpm::upgrade::SelfUpdater;
///
/// // Valid repository identifiers
/// let updater = SelfUpdater::with_repo("aig787", "ccpm");
/// assert!(updater.is_ok());
///
/// let updater = SelfUpdater::with_repo("my-repo", "my_project");
/// assert!(updater.is_ok());
///
/// // Invalid repository identifiers would fail
/// let updater = SelfUpdater::with_repo("../evil", "repo");
/// assert!(updater.is_err());
/// ```
fn validate_repo_identifier(identifier: &str) -> bool {
    if identifier.is_empty() || identifier.len() > 100 {
        return false;
    }

    // Only allow alphanumeric, hyphens, underscores, and dots
    identifier.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        // Additional safety: prevent starting/ending with dots or hyphens
        && !identifier.starts_with('.')
        && !identifier.starts_with('-')
        && !identifier.ends_with('.')
        && !identifier.ends_with('-')
        // Prevent consecutive dots or dot-slash combinations
        && !identifier.contains("..")
        && !identifier.contains("./")
        && !identifier.contains("\\")
}

/// Validate and sanitize a file path to prevent path traversal attacks.
///
/// This function performs comprehensive path traversal protection by:
/// 1. Checking for obvious traversal patterns
/// 2. Resolving the path to its canonical form
/// 3. Verifying the canonical path is within the expected base directory
///
/// # Arguments
///
/// * `path` - The path to validate
/// * `base_dir` - The base directory that the path must be within
///
/// # Returns
///
/// `Ok(PathBuf)` with the validated canonical path, or an error if the path is unsafe.
fn validate_and_sanitize_path(path: &Path, base_dir: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();

    // Basic checks for obvious traversal attempts
    if path_str.contains("..")
        || path_str.starts_with('/')
        || path_str.starts_with('\\')
        || path_str.contains('\0')
    {
        bail!("Path contains unsafe traversal patterns: {}", path_str);
    }

    // Get canonical base directory
    let canonical_base = base_dir.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize base directory: {}",
            base_dir.display()
        )
    })?;

    // Create the full path and canonicalize it
    let full_path = base_dir.join(path);
    let canonical_path = match full_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // If canonicalize fails, the path might not exist yet
            // Try to canonicalize the parent and append the filename
            if let Some(parent) = full_path.parent() {
                if let Some(filename) = full_path.file_name() {
                    match parent.canonicalize() {
                        Ok(canonical_parent) => canonical_parent.join(filename),
                        Err(_) => {
                            // If parent doesn't exist either, validate manually
                            return validate_path_components(&full_path, &canonical_base);
                        }
                    }
                } else {
                    bail!("Invalid path structure: {}", full_path.display());
                }
            } else {
                bail!("Invalid path: {}", full_path.display());
            }
        }
    };

    // Ensure the canonical path is within the base directory
    if !canonical_path.starts_with(&canonical_base) {
        bail!(
            "Path traversal detected: {} is outside base directory {}",
            canonical_path.display(),
            canonical_base.display()
        );
    }

    Ok(canonical_path)
}

/// Validate path components when canonicalization is not possible.
fn validate_path_components(path: &Path, base_dir: &Path) -> Result<PathBuf> {
    let mut validated_path = base_dir.to_path_buf();

    for component in path.components() {
        match component {
            std::path::Component::Normal(name) => {
                let name_str = name.to_string_lossy();
                if name_str.contains('\0') || name_str == "." || name_str == ".." {
                    bail!("Invalid path component: {}", name_str);
                }
                validated_path.push(name);
            }
            std::path::Component::CurDir => {
                // Skip current directory references
                continue;
            }
            std::path::Component::ParentDir => {
                bail!("Parent directory traversal not allowed");
            }
            _ => {
                bail!("Absolute path components not allowed in extraction");
            }
        }
    }

    Ok(validated_path)
}

/// Security policy for checksum verification during updates.
///
/// This enum allows configuring how strictly the updater enforces checksum
/// verification, balancing security with usability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumPolicy {
    /// Require checksum verification - fail if checksum is unavailable or invalid.
    Required,
    /// Warn if checksum verification fails but continue with update.
    WarnOnFailure,
    /// Skip checksum verification entirely (not recommended for production).
    Skip,
}

impl Default for ChecksumPolicy {
    fn default() -> Self {
        // Default to warning mode for backward compatibility
        Self::WarnOnFailure
    }
}

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
/// - **Security**: URL validation, path traversal protection, configurable checksum verification
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
/// ## Force Update with Required Checksums
/// ```rust,no_run
/// use ccpm::upgrade::{SelfUpdater, ChecksumPolicy};
///
/// # async fn example() -> anyhow::Result<()> {
/// let updater = SelfUpdater::new()
///     .force(true)
///     .checksum_policy(ChecksumPolicy::Required);
///
/// // This will update even if already on the latest version
/// // and require checksum verification
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
/// - Security validation failures
pub struct SelfUpdater {
    /// GitHub repository owner (e.g., "aig787").
    repo_owner: String,
    /// GitHub repository name (e.g., "ccpm").
    repo_name: String,
    /// Current version of the running binary.
    current_version: String,
    /// Whether to force updates even when already on latest version.
    force: bool,
    /// Policy for checksum verification during downloads.
    checksum_policy: ChecksumPolicy,
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
        // These are hardcoded safe values for the official repository
        let repo_owner = "aig787".to_string();
        let repo_name = "ccpm".to_string();

        // Validate even hardcoded values for extra safety
        debug_assert!(
            validate_repo_identifier(&repo_owner),
            "Default repo_owner must be valid"
        );
        debug_assert!(
            validate_repo_identifier(&repo_name),
            "Default repo_name must be valid"
        );

        Self {
            repo_owner,
            repo_name,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            force: false,
            checksum_policy: ChecksumPolicy::default(),
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

    /// Create a new `SelfUpdater` with custom repository settings.
    ///
    /// This constructor allows specifying a custom GitHub repository for updates,
    /// with security validation to prevent URL injection attacks.
    ///
    /// # Arguments
    ///
    /// * `repo_owner` - GitHub repository owner (e.g., "aig787")
    /// * `repo_name` - GitHub repository name (e.g., "ccpm")
    ///
    /// # Errors
    ///
    /// Returns an error if the repository identifiers contain invalid characters
    /// that could be used for URL injection or other attacks.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::SelfUpdater;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// // Valid repository identifiers
    /// let updater = SelfUpdater::with_repo("aig787", "ccpm")?;
    /// let custom = SelfUpdater::with_repo("my-org", "my_fork")?;
    ///
    /// // This would fail due to invalid characters
    /// // let bad = SelfUpdater::with_repo("../evil", "repo");
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_repo(repo_owner: &str, repo_name: &str) -> Result<Self> {
        if !validate_repo_identifier(repo_owner) {
            bail!("Invalid repository owner: {}", repo_owner);
        }
        if !validate_repo_identifier(repo_name) {
            bail!("Invalid repository name: {}", repo_name);
        }

        Ok(Self {
            repo_owner: repo_owner.to_string(),
            repo_name: repo_name.to_string(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            force: false,
            checksum_policy: ChecksumPolicy::default(),
        })
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

    /// Configure the checksum verification policy for downloads.
    ///
    /// This setting controls how the updater handles checksum verification during
    /// binary downloads, allowing you to balance security with usability.
    ///
    /// # Security Implications
    ///
    /// - **Required**: Maximum security, but updates may fail if checksums are unavailable
    /// - **WarnOnFailure**: Good balance of security and usability (default)
    /// - **Skip**: Least secure, not recommended for production use
    ///
    /// # Arguments
    ///
    /// * `policy` - The checksum verification policy to use
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::upgrade::{SelfUpdater, ChecksumPolicy};
    ///
    /// // Require checksum verification (most secure)
    /// let secure_updater = SelfUpdater::new()
    ///     .checksum_policy(ChecksumPolicy::Required);
    ///
    /// // Skip checksum verification (least secure)
    /// let fast_updater = SelfUpdater::new()
    ///     .checksum_policy(ChecksumPolicy::Skip);
    /// ```
    pub fn checksum_policy(mut self, policy: ChecksumPolicy) -> Self {
        self.checksum_policy = policy;
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

    /// Construct a validated GitHub API URL.
    ///
    /// This method provides secure URL construction by validating repository
    /// identifiers before use, preventing URL injection attacks.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The API endpoint path (e.g., "releases/latest")
    ///
    /// # Returns
    ///
    /// A validated GitHub API URL.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if repository identifiers are invalid. This should
    /// never happen in practice due to validation in constructors.
    fn build_github_api_url(&self, endpoint: &str) -> String {
        // Re-validate repository identifiers for extra safety
        debug_assert!(
            validate_repo_identifier(&self.repo_owner),
            "Repository owner should be validated: {}",
            self.repo_owner
        );
        debug_assert!(
            validate_repo_identifier(&self.repo_name),
            "Repository name should be validated: {}",
            self.repo_name
        );

        format!(
            "https://api.github.com/repos/{}/{}/{}",
            self.repo_owner, self.repo_name, endpoint
        )
    }

    /// Construct a validated GitHub releases download URL.
    ///
    /// This method provides secure URL construction for downloading release assets.
    ///
    /// # Arguments
    ///
    /// * `version` - The release version
    /// * `filename` - The asset filename
    ///
    /// # Returns
    ///
    /// A validated GitHub releases download URL.
    fn build_github_download_url(&self, version: &str, filename: &str) -> String {
        // Re-validate repository identifiers for extra safety
        debug_assert!(
            validate_repo_identifier(&self.repo_owner),
            "Repository owner should be validated: {}",
            self.repo_owner
        );
        debug_assert!(
            validate_repo_identifier(&self.repo_name),
            "Repository name should be validated: {}",
            self.repo_name
        );

        format!(
            "https://github.com/{}/{}/releases/download/v{}/{}",
            self.repo_owner, self.repo_name, version, filename
        )
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

        let url = self.build_github_api_url("releases/latest");

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("User-Agent", "ccpm")
            .send()
            .await
            .context("Failed to fetch release information")?;

        if !response.status().is_success() {
            if response.status() == 404 {
                warn!("No releases found");
                return Ok(None);
            }
            bail!("GitHub API error: {}", response.status());
        }

        let release: serde_json::Value = response.json().await?;
        let latest_version = release["tag_name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Release missing tag_name"))?
            .trim_start_matches('v');

        debug!("Latest version: {}", latest_version);

        let current =
            Version::parse(&self.current_version).context("Failed to parse current version")?;
        let latest = Version::parse(latest_version).context("Failed to parse latest version")?;

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

        // Custom implementation to handle tar.xz archives
        // First, determine the target version
        let target_version = if let Some(v) = target_version {
            v.trim_start_matches('v').to_string()
        } else {
            // Get latest version from GitHub API
            let url = self.build_github_api_url("releases/latest");

            let client = reqwest::Client::new();
            let response = client
                .get(&url)
                .header("User-Agent", "ccpm")
                .send()
                .await
                .context("Failed to fetch release information")?;

            if !response.status().is_success() {
                bail!("Failed to get latest release: HTTP {}", response.status());
            }

            let release: serde_json::Value = response.json().await?;
            release["tag_name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Release missing tag_name"))?
                .trim_start_matches('v')
                .to_string()
        };

        // Check if we need to update
        let current = Version::parse(&self.current_version)?;
        let target = Version::parse(&target_version)?;

        if !self.force && current >= target {
            info!("Already on version {} (target: {})", current, target);
            return Ok(false);
        }

        // Download the appropriate archive for this platform
        let archive_url = self.get_archive_url(&target_version)?;
        info!("Downloading from {}", archive_url);

        // Download to temp file
        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join("ccpm-archive");

        self.download_file(&archive_url, &archive_path).await?;

        // Extract the binary from the archive
        let extracted_binary = self.extract_binary(&archive_path, temp_dir.path()).await?;

        // Replace the current binary
        self.replace_binary(&extracted_binary).await?;

        info!("Successfully updated to version {}", target_version);
        Ok(true)
    }

    /// Get the download URL for the archive based on platform
    fn get_archive_url(&self, version: &str) -> Result<String> {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("windows", "x86_64") => "x86_64-pc-windows-msvc",
            ("windows", "aarch64") => "aarch64-pc-windows-msvc",
            (os, arch) => bail!("Unsupported platform: {}-{}", os, arch),
        };

        let extension = if std::env::consts::OS == "windows" {
            "zip"
        } else {
            "tar.xz"
        };

        let filename = format!("ccpm-{}.{}", platform, extension);
        Ok(self.build_github_download_url(version, &filename))
    }

    /// Download a file from URL to destination with optional checksum verification
    async fn download_file(&self, url: &str, dest: &std::path::Path) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        // Configure client with timeout
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout
            .build()?;

        let mut retries = 3;
        let mut delay = std::time::Duration::from_secs(1);

        loop {
            match client.get(url).header("User-Agent", "ccpm").send().await {
                Ok(response) => {
                    if !response.status().is_success() {
                        if retries > 0 && response.status().is_server_error() {
                            warn!(
                                "Server error {}, retrying in {:?}...",
                                response.status(),
                                delay
                            );
                            tokio::time::sleep(delay).await;
                            delay *= 2; // Exponential backoff
                            retries -= 1;
                            continue;
                        }
                        bail!("Failed to download: HTTP {}", response.status());
                    }

                    // Check content length for size limits (max 100MB)
                    if let Some(content_length) = response.content_length()
                        && content_length > 100 * 1024 * 1024
                    {
                        bail!("Archive too large: {} bytes (max 100MB)", content_length);
                    }

                    let bytes = response.bytes().await?;

                    // Write using async I/O
                    let mut file = tokio::fs::File::create(dest).await?;
                    file.write_all(&bytes).await?;
                    file.sync_all().await?;

                    // Verify checksum based on policy
                    match self.checksum_policy {
                        ChecksumPolicy::Required => {
                            if let Some(checksum_url) = self.get_checksum_url(url) {
                                self.verify_checksum(&checksum_url, dest, &bytes).await?;
                            } else {
                                bail!(
                                    "Checksum verification required but no checksum available for URL: {}",
                                    url
                                );
                            }
                        }
                        ChecksumPolicy::WarnOnFailure => {
                            if let Some(checksum_url) = self.get_checksum_url(url) {
                                if let Err(e) =
                                    self.verify_checksum(&checksum_url, dest, &bytes).await
                                {
                                    warn!("Checksum verification failed, but continuing: {}", e);
                                }
                            } else {
                                warn!("No checksum available for verification: {}", url);
                            }
                        }
                        ChecksumPolicy::Skip => {
                            debug!("Skipping checksum verification as configured");
                        }
                    }

                    return Ok(());
                }
                Err(e) if retries > 0 => {
                    warn!("Download failed: {}, retrying in {:?}...", e, delay);
                    tokio::time::sleep(delay).await;
                    delay *= 2; // Exponential backoff
                    retries -= 1;
                }
                Err(e) => bail!("Failed to download after retries: {}", e),
            }
        }
    }

    /// Get checksum URL for a given download URL
    fn get_checksum_url(&self, url: &str) -> Option<String> {
        // GitHub releases have .sha256 files
        if url.contains("github.com") && !url.ends_with(".sha256") {
            Some(format!("{}.sha256", url))
        } else {
            None
        }
    }

    /// Verify SHA256 checksum of downloaded file
    async fn verify_checksum(
        &self,
        checksum_url: &str,
        file_path: &std::path::Path,
        content: &[u8],
    ) -> Result<()> {
        use sha2::{Digest, Sha256};

        // Download checksum file
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let response = client
            .get(checksum_url)
            .header("User-Agent", "ccpm")
            .send()
            .await
            .context("Failed to download checksum file")?;

        if !response.status().is_success() {
            bail!("Failed to download checksum: HTTP {}", response.status());
        }

        let checksum_text = response
            .text()
            .await
            .context("Failed to read checksum file content")?;

        // Parse checksum (format: "<hash>  <filename>" or just "<hash>")
        let expected_checksum = checksum_text
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Invalid checksum format: empty file"))?;

        // Validate checksum format (should be 64 hex characters for SHA256)
        if expected_checksum.len() != 64
            || !expected_checksum.chars().all(|c| c.is_ascii_hexdigit())
        {
            bail!("Invalid SHA256 checksum format: {}", expected_checksum);
        }

        // Calculate actual checksum
        let mut hasher = Sha256::new();
        hasher.update(content);
        let actual_checksum = format!("{:x}", hasher.finalize());

        if expected_checksum.to_lowercase() != actual_checksum {
            // Delete the potentially corrupted file
            let _ = tokio::fs::remove_file(file_path).await;
            bail!(
                "Checksum verification failed! Expected: {}, Got: {}. File may be corrupted or tampered with.",
                expected_checksum,
                actual_checksum
            );
        }

        info!(
            "Checksum verified successfully (SHA256: {})",
            &actual_checksum[..16]
        );
        Ok(())
    }

    /// Extract the binary from the downloaded archive with security checks
    async fn extract_binary(
        &self,
        archive_path: &std::path::Path,
        temp_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf> {
        let binary_name = if std::env::consts::OS == "windows" {
            "ccpm.exe"
        } else {
            "ccpm"
        };

        if archive_path.to_string_lossy().ends_with(".zip") {
            // Handle zip archives for Windows
            let archive_data = tokio::fs::read(archive_path).await?;
            let cursor = std::io::Cursor::new(archive_data);
            let mut archive = zip::ZipArchive::new(cursor)?;

            // Check for zip bombs - total uncompressed size
            let total_size: u64 = (0..archive.len())
                .map(|i| archive.by_index(i).map(|f| f.size()).unwrap_or(0))
                .sum();

            if total_size > 500 * 1024 * 1024 {
                // 500MB limit
                bail!("Archive uncompressed size too large: {} bytes", total_size);
            }

            for i in 0..archive.len() {
                let file = archive.by_index(i)?;
                let file_name = file.name();

                if file_name.ends_with(&binary_name) {
                    // Use comprehensive path validation
                    let file_path = Path::new(file_name);
                    if let Err(e) = validate_and_sanitize_path(file_path, temp_dir) {
                        warn!("Skipping malicious path {}: {}", file_name, e);
                        continue;
                    }

                    // Additional check: ensure the file is in a reasonable location
                    let path_components: Vec<&str> = file_name.split(&['/', '\\'][..]).collect();
                    if path_components.len() > 3 {
                        warn!("Binary nested too deep in archive: {}", file_name);
                        continue;
                    }

                    // Use the validated path, but always extract to the binary name in temp_dir for consistency
                    let output_path = temp_dir.join(binary_name);

                    // Read and write with size limit
                    use std::io::Read;
                    let mut content = Vec::new();
                    let size = file
                        .take(100 * 1024 * 1024) // 100MB limit
                        .read_to_end(&mut content)?;

                    if size >= 100 * 1024 * 1024 {
                        bail!("Binary file too large in archive");
                    }

                    // Write using async I/O
                    tokio::fs::write(&output_path, content).await?;
                    return Ok(output_path);
                }
            }
            bail!("Binary not found in archive");
        } else {
            // Handle tar.xz archives for Unix
            // Use system tar command as it's more reliable for xz
            let output = tokio::process::Command::new("tar")
                .args([
                    "-xf",
                    &archive_path.to_string_lossy(),
                    "-C",
                    &temp_dir.to_string_lossy(),
                ])
                .output()
                .await?;

            if !output.status.success() {
                bail!(
                    "Failed to extract archive: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            // Look for the binary in extracted directory structure
            // The archive contains a directory with the binary inside
            let mut entries = tokio::fs::read_dir(temp_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                // Use comprehensive path validation
                let relative_path = match path.strip_prefix(temp_dir) {
                    Ok(rel) => rel,
                    Err(_) => {
                        warn!("Skipping path outside temp directory: {:?}", path);
                        continue;
                    }
                };

                match validate_and_sanitize_path(relative_path, temp_dir) {
                    Ok(validated_path) => {
                        // Ensure the validated path matches the original path
                        if validated_path != path {
                            warn!("Path validation mismatch, skipping: {:?}", path);
                            continue;
                        }
                    }
                    Err(e) => {
                        warn!("Skipping invalid path {:?}: {}", path, e);
                        continue;
                    }
                }

                if path.is_dir() {
                    // Check inside the directory for the binary
                    let binary_path = path.join(binary_name);

                    // Validate the binary path as well
                    if let Ok(metadata) = tokio::fs::metadata(&binary_path).await {
                        let relative_binary_path = match binary_path.strip_prefix(temp_dir) {
                            Ok(rel) => rel,
                            Err(_) => continue,
                        };

                        match validate_and_sanitize_path(relative_binary_path, temp_dir) {
                            Ok(_) => {
                                if metadata.is_file() && metadata.len() < 100 * 1024 * 1024 {
                                    return Ok(binary_path);
                                }
                            }
                            Err(e) => {
                                warn!("Invalid binary path {:?}: {}", binary_path, e);
                                continue;
                            }
                        }
                    }
                }
                // Also check if the file is directly in temp_dir
                if path.file_name() == Some(std::ffi::OsStr::new(binary_name))
                    && let Ok(metadata) = tokio::fs::metadata(&path).await
                    && metadata.is_file()
                    && metadata.len() < 100 * 1024 * 1024
                {
                    return Ok(path);
                }
            }

            bail!("Binary not found after extraction");
        }
    }

    /// Replace the current binary with the new one
    async fn replace_binary(&self, new_binary: &std::path::Path) -> Result<()> {
        let current_exe = std::env::current_exe()?;

        // Make sure the new binary is executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&new_binary).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&new_binary, perms).await?;
        }

        // On Windows, we may need to retry due to file locking
        let mut retries = 3;
        while retries > 0 {
            match tokio::fs::rename(&new_binary, &current_exe).await {
                Ok(_) => return Ok(()),
                Err(e) if retries > 1 => {
                    warn!("Failed to replace binary, retrying: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    retries -= 1;
                }
                Err(e) => bail!("Failed to replace binary: {}", e),
            }
        }

        Ok(())
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
