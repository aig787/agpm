//! Cross-platform utilities and helpers
//!
//! This module provides utility functions for file operations, platform-specific
//! code, and user interface elements like progress bars. All utilities are designed
//! to work consistently across Windows, macOS, and Linux.
//!
//! # Modules
//!
//! - [`fs`] - File system operations with atomic writes and safe copying
//! - [`platform`] - Platform-specific helpers and path resolution
//! - [`progress`] - Progress bars and spinners for long-running operations
//!
//! # Cross-Platform Considerations
//!
//! All utilities handle platform differences:
//! - Path separators (`/` vs `\`)
//! - Line endings (`\n` vs `\r\n`)
//! - File permissions and attributes
//! - Shell commands and environment variables
//!
//! # Example
//!
//! ```rust,no_run
//! use ccpm::utils::{ensure_dir, atomic_write, ProgressBar};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Ensure directory exists
//! ensure_dir(Path::new("output/agents"))?;
//!
//! // Write file atomically
//! atomic_write(Path::new("output/config.toml"), b"content")?;
//!
//! // Show progress
//! let progress = ProgressBar::new(100);
//! progress.set_message("Processing...");
//! # Ok(())
//! # }
//! ```

pub mod fs;
pub mod platform;
pub mod progress;

pub use fs::{atomic_write, copy_dir, ensure_dir, normalize_path, safe_write};
pub use platform::{get_git_command, get_home_dir, is_windows, resolve_path};
pub use progress::{ProgressBar, ProgressStyle};
