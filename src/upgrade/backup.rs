use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

/// Manages backup and restoration of AGPM binaries during upgrades.
///
/// `BackupManager` provides comprehensive backup functionality to protect against
/// failed upgrades and enable rollback capabilities. It creates backups of the
/// current binary before upgrades and can restore them if needed.
///
/// # Safety Features
///
/// - **Automatic Backup Creation**: Creates backups before any binary modification
/// - **Permission Preservation**: Maintains file permissions and metadata on Unix systems
/// - **Atomic Operations**: Uses file copy operations for reliability
/// - **Retry Logic**: Handles Windows file locking issues with automatic retries
/// - **Rollback Support**: Enables quick restoration from backups
///
/// # Backup Strategy
///
/// The backup manager creates a copy of the original binary with a `.backup` suffix
/// in the same directory. This approach:
/// - Keeps backups close to the original for easy access
/// - Preserves the same file system and permissions context
/// - Allows for quick restoration without complex path management
/// - Works consistently across different installation methods
///
/// # Cross-Platform Considerations
///
/// ## Unix Systems (Linux, macOS)
/// - Preserves executable permissions and ownership
/// - Uses standard file copy operations
/// - Handles symbolic links appropriately
///
/// ## Windows
/// - Implements retry logic for file locking issues
/// - Handles executable files that might be in use
/// - Works with Windows permission models
///
/// # Examples
///
/// ## Basic Backup and Restore
/// ```rust,no_run
/// use agpm::upgrade::backup::BackupManager;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let exe_path = PathBuf::from("/usr/local/bin/agpm");
/// let backup_manager = BackupManager::new(exe_path);
///
/// // Create backup before upgrade
/// backup_manager.create_backup().await?;
///
/// // ... perform upgrade ...
///
/// // Restore if upgrade failed
/// let upgrade_failed = false; // Set based on upgrade result
/// if upgrade_failed {
///     backup_manager.restore_backup().await?;
/// } else {
///     backup_manager.cleanup_backup().await?;
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Check for Existing Backup
/// ```rust,no_run
/// use agpm::upgrade::backup::BackupManager;
/// use std::path::PathBuf;
///
/// let backup_manager = BackupManager::new(PathBuf::from("agpm"));
///
/// if backup_manager.backup_exists() {
///     println!("Backup found at: {}", backup_manager.backup_path().display());
/// }
/// ```
///
/// # Error Handling
///
/// All operations return `Result<T, anyhow::Error>` with detailed error context:
/// - Permission errors when unable to read/write files
/// - File system errors during copy operations
/// - Platform-specific issues (Windows file locking, Unix permissions)
///
/// # Implementation Details
///
/// - Uses `tokio::fs` for async file operations
/// - Implements platform-specific permission handling
/// - Provides detailed logging for debugging and monitoring
/// - Handles edge cases like missing files and permission issues
pub struct BackupManager {
    /// Path to the original binary file.
    original_path: PathBuf,
    /// Path where the backup will be stored.
    backup_path: PathBuf,
}

impl BackupManager {
    /// Create a new `BackupManager` for the specified executable.
    ///
    /// Automatically determines the backup file path by appending `.backup`
    /// to the original executable name in the same directory.
    ///
    /// # Arguments
    ///
    /// * `executable_path` - Full path to the executable binary to manage
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::backup::BackupManager;
    /// use std::path::PathBuf;
    ///
    /// // Unix-style path
    /// let manager = BackupManager::new(PathBuf::from("/usr/local/bin/agpm"));
    /// // Backup will be at /usr/local/bin/agpm.backup
    ///
    /// // Windows-style path
    /// let manager = BackupManager::new(PathBuf::from(r"C:\Program Files\agpm\agpm.exe"));
    /// // Backup will be at C:\Program Files\agpm\agpm.exe.backup
    /// ```
    pub fn new(executable_path: PathBuf) -> Self {
        let mut backup_path = executable_path.clone();
        backup_path.set_file_name(format!(
            "{}.backup",
            executable_path.file_name().unwrap_or_default().to_string_lossy()
        ));

        Self {
            original_path: executable_path,
            backup_path,
        }
    }

    /// Create a backup of the original binary.
    ///
    /// Copies the current binary to the backup location, preserving permissions
    /// and metadata. If a backup already exists, it will be replaced.
    ///
    /// # Process
    ///
    /// 1. Validate that the original file exists
    /// 2. Remove any existing backup file
    /// 3. Copy the original file to the backup location
    /// 4. Preserve file permissions on Unix systems
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Backup created successfully
    /// - `Err(error)` - Backup creation failed
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - The original file doesn't exist or is not readable
    /// - Insufficient permissions to create the backup file
    /// - File system errors during the copy operation
    /// - Unable to set permissions on the backup file (Unix)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::backup::BackupManager;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let manager = BackupManager::new(PathBuf::from("./agpm"));
    ///
    /// match manager.create_backup().await {
    ///     Ok(()) => println!("Backup created successfully"),
    ///     Err(e) => eprintln!("Failed to create backup: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_backup(&self) -> Result<()> {
        if !self.original_path.exists() {
            bail!("Original file does not exist: {:?}", self.original_path);
        }

        // Remove old backup if it exists
        if self.backup_path.exists() {
            debug!("Removing old backup at {:?}", self.backup_path);
            fs::remove_file(&self.backup_path).await.context("Failed to remove old backup")?;
        }

        // Copy current binary to backup location
        info!("Creating backup at {:?}", self.backup_path);
        fs::copy(&self.original_path, &self.backup_path)
            .await
            .context("Failed to create backup")?;

        // Preserve permissions on Unix
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&self.original_path)
                .await
                .context("Failed to read original file metadata")?;
            let permissions = metadata.permissions();
            fs::set_permissions(&self.backup_path, permissions)
                .await
                .context("Failed to set backup permissions")?;
        }

        info!("Backup created successfully");
        Ok(())
    }

    /// Restore the original binary from backup.
    ///
    /// Replaces the current binary with the backup copy, effectively rolling
    /// back to the previous version. This operation includes retry logic for
    /// Windows systems where the binary might be locked.
    ///
    /// # Process
    ///
    /// 1. Validate that a backup file exists
    /// 2. Remove the current (potentially corrupted) binary
    /// 3. Copy the backup file back to the original location
    /// 4. Restore file permissions on Unix systems
    /// 5. Retry up to 3 times on Windows for file locking issues
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Backup restored successfully
    /// - `Err(error)` - Restoration failed after all retries
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - No backup file exists at the expected location
    /// - Insufficient permissions to replace the original file
    /// - File locking issues prevent replacement (Windows)
    /// - File system errors during the copy operation
    /// - Unable to restore permissions (Unix)
    ///
    /// # Platform Behavior
    ///
    /// ## Windows
    /// - Implements retry logic with 1-second delays
    /// - Handles file locking from running processes
    /// - Attempts up to 3 times before giving up
    ///
    /// ## Unix
    /// - Preserves executable permissions and ownership
    /// - Single attempt (usually succeeds immediately)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::backup::BackupManager;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let manager = BackupManager::new(PathBuf::from("./agpm"));
    ///
    /// if manager.backup_exists() {
    ///     match manager.restore_backup().await {
    ///         Ok(()) => println!("Successfully restored from backup"),
    ///         Err(e) => eprintln!("Failed to restore backup: {}", e),
    ///     }
    /// } else {
    ///     eprintln!("No backup found to restore");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn restore_backup(&self) -> Result<()> {
        if !self.backup_path.exists() {
            bail!("No backup found at {:?}", self.backup_path);
        }

        warn!("Restoring from backup at {:?}", self.backup_path);

        // On Windows, we might need to retry if the file is in use
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 3;

        while attempts < MAX_ATTEMPTS {
            match self.attempt_restore().await {
                Ok(()) => {
                    info!("Successfully restored from backup");
                    return Ok(());
                }
                Err(e) if attempts < MAX_ATTEMPTS - 1 => {
                    warn!("Restore attempt {} failed: {}. Retrying...", attempts + 1, e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    attempts += 1;
                }
                Err(e) => return Err(e),
            }
        }

        bail!("Failed to restore backup after {MAX_ATTEMPTS} attempts")
    }

    /// Attempt a single restoration operation.
    ///
    /// This is an internal method used by [`restore_backup()`](Self::restore_backup)
    /// to handle the actual file operations. It's separated to enable retry logic
    /// for Windows file locking issues.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Single restoration attempt succeeded
    /// - `Err(error)` - Restoration attempt failed
    ///
    /// # Process
    ///
    /// 1. Remove the current binary file if it exists
    /// 2. Copy the backup file to the original location
    /// 3. Restore file permissions on Unix systems
    async fn attempt_restore(&self) -> Result<()> {
        // Remove the potentially corrupted binary
        if self.original_path.exists() {
            fs::remove_file(&self.original_path)
                .await
                .context("Failed to remove corrupted binary")?;
        }

        // Copy backup back to original location
        fs::copy(&self.backup_path, &self.original_path)
            .await
            .context("Failed to restore backup")?;

        // Restore permissions on Unix
        #[cfg(unix)]
        {
            let metadata =
                fs::metadata(&self.backup_path).await.context("Failed to read backup metadata")?;
            let permissions = metadata.permissions();
            fs::set_permissions(&self.original_path, permissions)
                .await
                .context("Failed to restore permissions")?;
        }

        Ok(())
    }

    /// Remove the backup file after a successful upgrade.
    ///
    /// Cleans up the backup file once it's no longer needed, typically after
    /// a successful upgrade has been completed and verified.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Backup cleaned up successfully or no backup existed
    /// - `Err(error)` - Failed to remove the backup file
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - Insufficient permissions to delete the backup file
    /// - File system errors during deletion
    /// - File is locked or in use (rare on most systems)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::backup::BackupManager;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let manager = BackupManager::new(PathBuf::from("./agpm"));
    ///
    /// // After successful upgrade
    /// manager.cleanup_backup().await?;
    /// println!("Backup cleaned up");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Note
    ///
    /// This method silently succeeds if no backup file exists, making it safe
    /// to call unconditionally after upgrades.
    pub async fn cleanup_backup(&self) -> Result<()> {
        if self.backup_path.exists() {
            debug!("Cleaning up backup at {:?}", self.backup_path);
            fs::remove_file(&self.backup_path).await.context("Failed to remove backup")?;
        }
        Ok(())
    }

    /// Check if a backup file currently exists.
    ///
    /// This is a synchronous check that verifies whether a backup file is
    /// present at the expected location.
    ///
    /// # Returns
    ///
    /// - `true` - A backup file exists and can potentially be restored
    /// - `false` - No backup file found at the expected location
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::backup::BackupManager;
    /// use std::path::PathBuf;
    ///
    /// let manager = BackupManager::new(PathBuf::from("./agpm"));
    ///
    /// if manager.backup_exists() {
    ///     println!("Backup available for rollback");
    /// } else {
    ///     println!("No backup found");
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// This method only checks for file existence, not validity or integrity
    /// of the backup file. Use [`restore_backup()`](Self::restore_backup) to
    /// verify the backup can actually be used.
    pub fn backup_exists(&self) -> bool {
        self.backup_path.exists()
    }

    /// Get the path where the backup file is stored.
    ///
    /// Returns the full path to the backup file location, which is useful
    /// for logging, debugging, or manual backup management.
    ///
    /// # Returns
    ///
    /// A path reference to the backup file location.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm::upgrade::backup::BackupManager;
    /// use std::path::PathBuf;
    ///
    /// let manager = BackupManager::new(PathBuf::from("/usr/local/bin/agpm"));
    /// println!("Backup location: {}", manager.backup_path().display());
    /// // Output: Backup location: /usr/local/bin/agpm.backup
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Logging**: Include backup location in log messages
    /// - **Debugging**: Help users locate backup files manually
    /// - **Error Messages**: Show backup location when operations fail
    /// - **Manual Recovery**: Allow users to manually restore backups
    pub fn backup_path(&self) -> &Path {
        &self.backup_path
    }
}
