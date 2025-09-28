use crate::config::GlobalConfig;
use crate::upgrade::{SelfUpdater, backup::BackupManager, version_check::VersionChecker};
use anyhow::{Context, Result, bail};
use clap::Parser;
use colored::Colorize;
use std::env;
use tracing::debug;

/// Command-line arguments for the CCPM upgrade command.
///
/// This structure defines all the options and flags available for upgrading
/// CCPM to newer versions. The upgrade command provides multiple modes of
/// operation from simple version checking to full upgrades with rollback support.
///
/// # Command Modes
///
/// The upgrade command operates in several distinct modes:
///
/// ## Update Modes
/// - **Check Only** (`--check`): Check for updates without installing
/// - **Status Display** (`--status`): Show current and latest version information
/// - **Upgrade to Latest**: Default behavior when no version specified
/// - **Upgrade to Specific Version**: When version argument is provided
///
/// ## Safety Modes
/// - **Normal Upgrade**: Creates backup and upgrades with safety checks
/// - **Force Upgrade** (`--force`): Bypass version checks and force installation
/// - **No Backup** (`--no-backup`): Skip backup creation (not recommended)
/// - **Rollback** (`--rollback`): Restore from previous backup
///
/// # Examples
///
/// ## Basic Usage
/// ```bash
/// # Check for available updates
/// ccpm upgrade --check
///
/// # Show version status
/// ccpm upgrade --status
///
/// # Upgrade to latest version
/// ccpm upgrade
/// ```
///
/// ## Version-Specific Upgrades
/// ```bash
/// # Upgrade to specific version
/// ccpm upgrade 0.4.0
/// ccpm upgrade v0.4.0
///
/// # Force upgrade even if already on target version
/// ccpm upgrade 0.4.0 --force
/// ```
///
/// ## Safety and Recovery
/// ```bash
/// # Upgrade without creating backup (risky)
/// ccpm upgrade --no-backup
///
/// # Rollback to previous version
/// ccpm upgrade --rollback
/// ```
///
/// # Safety Features
///
/// - **Automatic Backups**: Creates backup of current binary before upgrade
/// - **Rollback Support**: Can restore previous version if upgrade fails
/// - **Version Validation**: Validates version strings and availability
/// - **Network Error Handling**: Graceful handling of connectivity issues
/// - **Permission Checks**: Validates write access before attempting upgrade
#[derive(Parser, Debug)]
pub struct UpgradeArgs {
    /// Target version to upgrade to (e.g., "0.4.0" or "v0.4.0").
    ///
    /// When specified, CCPM will attempt to upgrade to this specific version
    /// instead of the latest available version. The version string can be
    /// provided with or without the 'v' prefix.
    ///
    /// # Version Formats
    ///
    /// - `"0.4.0"` - Semantic version number
    /// - `"v0.4.0"` - Version with 'v' prefix (GitHub tag format)
    /// - `"0.4.0-beta.1"` - Pre-release versions
    /// - `"0.4.0-rc.1"` - Release candidate versions
    ///
    /// # Behavior
    ///
    /// - If not specified, upgrades to the latest available version
    /// - Version must exist as a GitHub release with binary assets
    /// - Can be older than current version when used with `--force`
    /// - Invalid version strings will cause the command to fail
    ///
    /// # Examples
    ///
    /// ```bash
    /// ccpm upgrade 0.4.0        # Upgrade to specific stable version
    /// ccpm upgrade v0.5.0-beta  # Upgrade to beta version
    /// ccpm upgrade 0.3.0 --force # Downgrade to older version
    /// ```
    #[arg(value_name = "VERSION")]
    pub version: Option<String>,

    /// Check for updates without installing.
    ///
    /// When enabled, performs a version check against GitHub releases but
    /// does not download or install anything. This is useful for automation,
    /// CI/CD pipelines, or when you want to know about updates without
    /// immediately upgrading.
    ///
    /// # Behavior
    ///
    /// - Fetches latest release information from GitHub
    /// - Compares with current version using semantic versioning
    /// - Displays update availability and version information
    /// - Exits with status 0 regardless of update availability
    /// - Caches version information for future use
    ///
    /// # Output Examples
    ///
    /// ```text
    /// # When update is available
    /// Update available: 0.3.14 -> 0.4.0
    /// Run `ccpm upgrade` to install the latest version
    ///
    /// # When up to date
    /// You are on the latest version (0.4.0)
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **CI/CD Integration**: Check for updates in automated pipelines
    /// - **Notification Scripts**: Alert when updates become available
    /// - **Manual Workflow**: Check before deciding whether to upgrade
    /// - **Development**: Verify release publication without upgrading
    #[arg(long)]
    pub check: bool,

    /// Show current version and latest available.
    ///
    /// Displays comprehensive version information including the current CCPM
    /// version and the latest available version from GitHub releases. Uses
    /// cached version information when available to avoid unnecessary API calls.
    ///
    /// # Information Displayed
    ///
    /// - Current version of the running CCPM binary
    /// - Latest version available on GitHub (if reachable)
    /// - Update availability status
    /// - Cache status (when version info was last fetched)
    ///
    /// # Caching Behavior
    ///
    /// - First checks local cache for recent version information
    /// - Falls back to GitHub API if cache is expired or missing
    /// - Updates cache with fresh information when fetched
    /// - Gracefully handles network errors by using cached data
    ///
    /// # Output Examples
    ///
    /// ```text
    /// # When update is available
    /// Current version: 0.3.14
    /// Latest version:  0.4.0 (update available)
    ///
    /// # When up to date
    /// Current version: 0.4.0 (up to date)
    ///
    /// # When network is unavailable
    /// Current version: 0.3.14
    /// (Unable to check for latest version)
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Quick Status Check**: See version info without upgrading
    /// - **Troubleshooting**: Verify current version during support
    /// - **Documentation**: Include version info in bug reports
    /// - **Development**: Check version alignment across environments
    #[arg(short, long)]
    pub status: bool,

    /// Force upgrade even if already on latest version.
    ///
    /// Bypasses version comparison checks and forces the upgrade process
    /// to proceed regardless of the current version. This is useful for
    /// reinstalling corrupted binaries, downgrading, or testing.
    ///
    /// # Behavior Changes
    ///
    /// - Skips "already up to date" checks
    /// - Downloads and installs even if target version <= current version
    /// - Enables downgrading to older versions
    /// - Still performs all safety checks (backup, checksum verification)
    /// - Respects other flags like `--no-backup`
    ///
    /// # Use Cases
    ///
    /// - **Reinstallation**: Fix corrupted or modified binaries
    /// - **Downgrading**: Install older version for compatibility
    /// - **Testing**: Verify upgrade mechanism functionality
    /// - **Recovery**: Restore known-good version after problems
    /// - **Development**: Install specific versions for testing
    ///
    /// # Safety Considerations
    ///
    /// Force mode still maintains safety features:
    /// - Creates backups unless `--no-backup` is specified
    /// - Verifies download checksums for integrity
    /// - Validates that target version exists and has binary assets
    /// - Provides rollback capability if installation fails
    ///
    /// # Examples
    ///
    /// ```bash
    /// # Reinstall current version
    /// ccpm upgrade --force
    ///
    /// # Downgrade to older version
    /// ccpm upgrade 0.3.0 --force
    ///
    /// # Force upgrade to specific version
    /// ccpm upgrade 0.4.0 --force
    /// ```
    #[arg(short, long)]
    pub force: bool,

    /// Rollback to previous version from backup.
    ///
    /// Restores the CCPM binary from the backup created during the most recent
    /// upgrade. This provides a quick recovery mechanism if the current version
    /// has issues or if you need to revert to the previous version.
    ///
    /// # Rollback Process
    ///
    /// 1. Validates that a backup file exists
    /// 2. Replaces current binary with backup copy
    /// 3. Preserves file permissions and metadata
    /// 4. Implements retry logic for Windows file locking
    /// 5. Provides success/failure feedback
    ///
    /// # Requirements
    ///
    /// - A backup must exist from a previous upgrade
    /// - Backup file must be readable and valid
    /// - Write permissions to the CCPM binary location
    /// - Current binary must not be locked by running processes
    ///
    /// # Error Conditions
    ///
    /// - No backup file found (never upgraded with backup enabled)
    /// - Backup file is corrupted or unreadable
    /// - Insufficient permissions to replace current binary
    /// - File locking prevents replacement (Windows)
    ///
    /// # Platform Considerations
    ///
    /// - **Windows**: Implements retry logic for file locking issues
    /// - **Unix**: Preserves executable permissions and ownership
    /// - **All Platforms**: Validates backup integrity before restoration
    ///
    /// # Examples
    ///
    /// ```bash
    /// # Simple rollback
    /// ccpm upgrade --rollback
    ///
    /// # Check if backup exists first
    /// ls ~/.local/bin/ccpm.backup  # Unix example
    /// ccpm upgrade --rollback
    /// ```
    ///
    /// # Post-Rollback
    ///
    /// After successful rollback:
    /// - Previous version functionality is restored
    /// - Version cache is not automatically cleared
    /// - Future upgrades will work normally
    #[arg(long)]
    pub rollback: bool,

    /// Skip creating a backup before upgrade.
    ///
    /// Disables the automatic backup creation that normally occurs before
    /// upgrading the CCPM binary. This removes the safety net of being able
    /// to rollback if the upgrade fails or the new version has issues.
    ///
    /// # ⚠️ WARNING
    ///
    /// Using this flag is **not recommended** for most users. Backups provide
    /// crucial recovery capability with minimal overhead. Only disable backups
    /// in specific scenarios where they cannot be created or are unnecessary.
    ///
    /// # When to Consider Using
    ///
    /// - **Disk Space Constraints**: Extremely limited storage where backup
    ///   would cause space issues
    /// - **Permission Issues**: File system permissions prevent backup creation
    /// - **Read-Only Installations**: When binary is in read-only file system
    /// - **Container Environments**: Ephemeral environments where persistence
    ///   is not needed
    /// - **Alternative Backups**: When external backup mechanisms are in place
    ///
    /// # Risks
    ///
    /// Without backups:
    /// - No automatic rollback if upgrade fails
    /// - Cannot use `ccpm upgrade --rollback` command
    /// - Must manually reinstall if new version has issues
    /// - Requires external recovery mechanisms
    ///
    /// # Alternative Recovery
    ///
    /// If backups are disabled, ensure alternative recovery methods:
    /// - Package manager installation (reinstall via `brew`, `apt`, etc.)
    /// - Manual download from GitHub releases
    /// - Container image rollback
    /// - Version control system with binary tracking
    ///
    /// # Examples
    ///
    /// ```bash
    /// # Upgrade without backup (not recommended)
    /// ccpm upgrade --no-backup
    ///
    /// # Force upgrade without backup
    /// ccpm upgrade 0.4.0 --force --no-backup
    ///
    /// # Check-only mode (backups not relevant)
    /// ccpm upgrade --check
    /// ```
    #[arg(long)]
    pub no_backup: bool,
}

/// Execute the upgrade command with the provided arguments.
///
/// This is the main entry point for all upgrade-related operations. It handles
/// the various upgrade modes (check, status, upgrade, rollback) and coordinates
/// the different components (updater, backup manager, version checker) to
/// provide a safe and reliable upgrade experience.
///
/// # Arguments
///
/// * `args` - The parsed command-line arguments containing upgrade options
///
/// # Command Flow
///
/// 1. **Initialization**: Load global config and set up cache directories
/// 2. **Mode Detection**: Determine operation mode based on flags
/// 3. **Component Setup**: Initialize updater, backup manager, and version checker
/// 4. **Operation Execution**: Perform the requested operation
/// 5. **Result Handling**: Provide user feedback and cleanup
///
/// # Operation Modes
///
/// ## Rollback Mode (`--rollback`)
/// - Validates backup existence
/// - Restores previous version from backup
/// - Provides rollback status feedback
///
/// ## Status Mode (`--status`)
/// - Shows current version information
/// - Checks for latest version (cached or fresh)
/// - Displays formatted version comparison
///
/// ## Check Mode (`--check`)
/// - Fetches latest version from GitHub
/// - Compares with current version
/// - Shows update availability
///
/// ## Upgrade Mode (default)
/// - Creates backup (unless `--no-backup`)
/// - Downloads and installs new version
/// - Handles success/failure scenarios
/// - Cleans up or restores as appropriate
///
/// # Returns
///
/// - `Ok(())` - Command completed successfully
/// - `Err(anyhow::Error)` - Command failed with detailed error information
///
/// # Errors
///
/// This function can fail for various reasons:
///
/// ## Network Errors
/// - GitHub API unreachable or rate limited
/// - Download failures for binary assets
/// - Connectivity issues during version checks
///
/// ## File System Errors
/// - Insufficient permissions to write binary or backups
/// - Disk space exhaustion during download or backup
/// - File locking issues (especially on Windows)
///
/// ## Version Errors
/// - Target version doesn't exist on GitHub
/// - Invalid version string format
/// - No binary assets available for target version
///
/// ## Configuration Errors
/// - Unable to load global configuration
/// - Cache directory creation failures
/// - Invalid executable path detection
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::cli::upgrade::{UpgradeArgs, execute};
/// use clap::Parser;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Parse command line arguments
/// let args = UpgradeArgs::parse();
///
/// // Execute the upgrade command
/// execute(args).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Safety Features
///
/// - **Automatic Backups**: Created before modifications unless disabled
/// - **Rollback Support**: Automatic restoration on upgrade failure
/// - **Version Validation**: Ensures target versions exist and are accessible
/// - **Permission Checks**: Validates file system access before attempting changes
/// - **Atomic Operations**: Uses safe file operations to minimize corruption risk
///
/// # User Experience
///
/// The function provides comprehensive user feedback:
/// - Colored output for different message types (success, warning, error)
/// - Progress indicators for long-running operations
/// - Clear error messages with suggested resolution steps
pub async fn execute(args: UpgradeArgs) -> Result<()> {
    let _config = GlobalConfig::load().await?;

    // Get the current executable path
    let current_exe = env::current_exe().context("Failed to get current executable path")?;

    // Handle rollback
    if args.rollback {
        return handle_rollback(&current_exe).await;
    }

    let updater = SelfUpdater::new().force(args.force);
    let version_checker = VersionChecker::new().await?;

    // Handle status check
    if args.status {
        return show_status(&updater, &version_checker).await;
    }

    // Handle check for updates
    if args.check {
        return check_for_updates(&updater, &version_checker).await;
    }

    // Perform the upgrade
    perform_upgrade(
        &updater,
        &version_checker,
        &current_exe,
        args.version.as_deref(),
        args.no_backup,
    )
    .await
}

async fn handle_rollback(current_exe: &std::path::Path) -> Result<()> {
    println!("{}", "Rolling back to previous version...".yellow());

    let backup_manager = BackupManager::new(current_exe.to_path_buf());

    if !backup_manager.backup_exists() {
        bail!("No backup found. Cannot rollback.");
    }

    backup_manager
        .restore_backup()
        .await
        .context("Failed to restore from backup")?;

    println!("{}", "Successfully rolled back to previous version".green());

    Ok(())
}

async fn show_status(updater: &SelfUpdater, version_checker: &VersionChecker) -> Result<()> {
    let current_version = updater.current_version();

    // Use the new check_now method which handles caching internally
    let latest_version = match version_checker.check_now().await {
        Ok(version) => version,
        Err(e) => {
            debug!("Failed to check for updates: {}", e);
            None
        }
    };

    let info = VersionChecker::format_version_info(current_version, latest_version.as_deref());
    println!("{}", info);

    Ok(())
}

async fn check_for_updates(updater: &SelfUpdater, version_checker: &VersionChecker) -> Result<()> {
    println!("{}", "Checking for updates...".cyan());

    // Use check_now which bypasses the cache and saves the result
    match version_checker.check_now().await {
        Ok(Some(latest_version)) => {
            println!(
                "{}",
                format!(
                    "Update available: {} -> {}",
                    updater.current_version(),
                    latest_version
                )
                .green()
            );
            println!("Run `ccpm upgrade` to install the latest version");
        }
        Ok(None) => {
            println!(
                "{}",
                format!(
                    "You are on the latest version ({})",
                    updater.current_version()
                )
                .green()
            );
        }
        Err(e) => {
            bail!("Failed to check for updates: {}", e);
        }
    }

    Ok(())
}

async fn perform_upgrade(
    updater: &SelfUpdater,
    version_checker: &VersionChecker,
    current_exe: &std::path::Path,
    target_version: Option<&str>,
    no_backup: bool,
) -> Result<()> {
    // Create backup unless explicitly skipped
    let backup_manager = if !no_backup {
        println!("{}", "Creating backup...".cyan());
        let manager = BackupManager::new(current_exe.to_path_buf());
        manager
            .create_backup()
            .await
            .context("Failed to create backup")?;
        Some(manager)
    } else {
        None
    };

    // Perform the upgrade
    let upgrade_msg = if let Some(version) = target_version {
        format!("Upgrading to version {}...", version).cyan()
    } else {
        "Upgrading to latest version...".cyan()
    };
    println!("{}", upgrade_msg);

    let result = if let Some(version) = target_version {
        updater.update_to_version(version).await
    } else {
        updater.update_to_latest().await
    };

    match result {
        Ok(true) => {
            // Clear version cache after successful update
            version_checker.clear_cache().await?;

            println!("{}", "Upgrade completed successfully!".green());

            // Clean up backup after successful upgrade
            if let Some(manager) = backup_manager
                && let Err(e) = manager.cleanup_backup().await
            {
                debug!("Failed to cleanup backup: {}", e);
            }
        }
        Ok(false) => {
            println!(
                "{}",
                format!(
                    "Already on the latest version ({})",
                    updater.current_version()
                )
                .green()
            );
        }
        Err(e) => {
            // Attempt to restore from backup on failure
            if let Some(manager) = backup_manager {
                println!(
                    "{}",
                    "Upgrade failed. Attempting to restore backup...".red()
                );
                if let Err(restore_err) = manager.restore_backup().await {
                    eprintln!(
                        "{}",
                        format!("Failed to restore backup: {}", restore_err).red()
                    );
                    eprintln!("Backup is located at: {}", manager.backup_path().display());
                } else {
                    println!("{}", "Successfully restored from backup".green());
                }
            }
            bail!("Upgrade failed: {}", e);
        }
    }

    Ok(())
}
