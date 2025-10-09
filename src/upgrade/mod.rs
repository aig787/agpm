//! Self-update functionality for AGPM.
//!
//! This module provides comprehensive self-update capabilities for the AGPM binary,
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
//! - **[`backup::BackupManager`]**: Creates and manages backups of the current binary before upgrades
//! - **[`VersionChecker`]**: Provides version comparison and caching for update checks
//! - **[`config::UpgradeConfig`]**: Configuration options for controlling upgrade behavior
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
//! agpm upgrade --check          # Check for updates without installing
//! agpm upgrade --status         # Show current and latest versions
//! ```
//!
//! ## Safe Upgrade
//! ```bash
//! agpm upgrade                  # Upgrade to latest with automatic backup
//! agpm upgrade v0.4.0          # Upgrade to specific version
//! ```
//!
//! ## Advanced Options
//! ```bash
//! agpm upgrade --force          # Force upgrade even if on latest
//! agpm upgrade --no-backup      # Skip backup creation (not recommended)
//! agpm upgrade --rollback       # Restore from backup
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
//! use agpm_cli::upgrade::SelfUpdater;
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
//! - Direct GitHub API integration for fetching releases
//! - Implements async/await for non-blocking operations
//! - Supports semver version parsing and comparison
//! - Includes comprehensive logging for debugging
//! - Designed for minimal external dependencies

/// Backup management for AGPM binary upgrades.
///
/// The backup module provides functionality to create, manage, and restore backups
/// of the AGPM binary during upgrade operations. This ensures safe upgrades with
/// the ability to rollback if issues occur.
pub mod backup;
/// Configuration structures for upgrade behavior.
///
/// Defines configuration options that control how AGPM handles self-updates,
/// including backup settings, version checking preferences, and security options.
pub mod config;
/// Core self-update implementation.
///
/// Contains the main `SelfUpdater` struct that handles downloading and installing
/// AGPM updates from GitHub releases with proper version management and safety checks.
pub mod self_updater;
/// Download verification and integrity checking.
///
/// Provides checksum verification and integrity validation for downloaded
/// AGPM binaries to ensure secure and reliable upgrades.
pub mod verification;
/// Version checking and comparison utilities.
///
/// Handles checking for available AGPM updates, comparing versions, and
/// maintaining update check caches to avoid unnecessary network requests.
pub mod version_check;

#[cfg(test)]
mod tests;

pub use self_updater::{ChecksumPolicy, SelfUpdater};
pub use verification::ChecksumVerifier;
pub use version_check::VersionChecker;
