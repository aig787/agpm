//! Self-update functionality for CCPM.
//!
//! This module provides comprehensive self-update capabilities for the CCPM binary,
//! allowing users to upgrade to newer versions directly from the command line.
//! The implementation follows safety-first principles with automatic backups,
//! rollback capabilities, and robust error handling.
//!
//! # Architecture Overview
//!
//! The upgrade system consists of four main components working together:
//!
//! ## Core Components
//!
//! - **[`SelfUpdater`]**: The main updater that handles GitHub release fetching and binary replacement
//! - **[`BackupManager`]**: Creates and manages backups of the current binary before upgrades
//! - **[`VersionChecker`]**: Provides version comparison and caching for update checks
//! - **[`UpgradeConfig`]**: Configuration options for controlling upgrade behavior
//!
//! ## Update Process Flow
//!
//! ```text
//! 1. Version Check
//!    ├── Check cache for recent version info
//!    └── Fetch latest release from GitHub if needed
//!
//! 2. Backup Creation (unless --no-backup)
//!    ├── Copy current binary to .backup file
//!    └── Preserve file permissions on Unix systems
//!
//! 3. Binary Update
//!    ├── Download new binary from GitHub releases
//!    ├── Verify download integrity
//!    └── Replace current binary atomically
//!
//! 4. Post-Update
//!    ├── Clear version cache
//!    ├── Clean up backup on success
//!    └── Restore from backup on failure
//! ```
//!
//! # Safety Mechanisms
//!
//! The upgrade system implements multiple safety mechanisms:
//!
//! ## Automatic Backups
//! - Creates backups before any binary modification
//! - Preserves file permissions and metadata
//! - Automatic restoration on upgrade failure
//! - Manual rollback capability via `--rollback` flag
//!
//! ## Robust Error Handling
//! - Validates downloads before replacement
//! - Atomic file operations where possible
//! - Graceful degradation on permission issues
//! - Detailed error messages for troubleshooting
//!
//! ## Cross-Platform Compatibility
//! - Handles Windows file locking issues with retry logic
//! - Preserves Unix executable permissions
//! - Works with various file system layouts
//! - Supports different installation methods
//!
//! # Security Considerations
//!
//! The upgrade system includes several security measures:
//!
//! - **Verified Downloads**: All downloads are verified against expected checksums
//! - **GitHub Integration**: Only downloads from official GitHub releases
//! - **Permission Preservation**: Maintains original file permissions and ownership
//! - **Atomic Operations**: Minimizes windows of vulnerability during updates
//!
//! # Usage Patterns
//!
//! ## Basic Update Check
//! ```bash
//! ccpm upgrade --check          # Check for updates without installing
//! ccpm upgrade --status         # Show current and latest versions
//! ```
//!
//! ## Safe Upgrade
//! ```bash
//! ccpm upgrade                  # Upgrade to latest with automatic backup
//! ccpm upgrade v0.4.0          # Upgrade to specific version
//! ```
//!
//! ## Advanced Options
//! ```bash
//! ccpm upgrade --force          # Force upgrade even if on latest
//! ccpm upgrade --no-backup      # Skip backup creation (not recommended)
//! ccpm upgrade --rollback       # Restore from backup
//! ```
//!
//! # Module Structure
//!
//! Each submodule has a specific responsibility:
//!
//! - [`self_updater`]: Core update logic and GitHub integration
//! - [`backup`]: Backup creation, restoration, and management
//! - [`version_check`]: Version comparison and caching
//! - [`config`]: Configuration structures and defaults
//!
//! # Error Handling
//!
//! All functions return `Result<T, E>` for proper error propagation:
//!
//! ```rust,no_run
//! use ccpm::upgrade::SelfUpdater;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let updater = SelfUpdater::new();
//! match updater.update_to_latest().await {
//!     Ok(true) => println!("Updated successfully"),
//!     Ok(false) => println!("Already on latest version"),
//!     Err(e) => eprintln!("Update failed: {}", e),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Implementation Notes
//!
//! - Uses the `self_update` crate for GitHub integration
//! - Implements async/await for non-blocking operations
//! - Supports semver version parsing and comparison
//! - Includes comprehensive logging for debugging
//! - Designed for minimal external dependencies

pub mod backup;
pub mod config;
pub mod self_updater;
pub mod verification;
pub mod version_check;

#[cfg(test)]
mod tests;

pub use self_updater::SelfUpdater;
pub use verification::ChecksumVerifier;
pub use version_check::VersionChecker;
