//! File system utilities for cross-platform file operations
//!
//! This module provides safe, atomic file operations designed to work consistently
//! across Windows, macOS, and Linux. All functions handle platform-specific
//! differences such as path lengths, permissions, and separators.
//!
//! # Key Features
//!
//! - **Atomic operations**: Files are written atomically to prevent corruption
//! - **Cross-platform**: Handles Windows long paths, Unix permissions, and path separators
//! - **Parallel operations**: Async versions for processing multiple files concurrently
//! - **Safety**: Path traversal prevention and safe path handling
//! - **Checksum validation**: SHA-256 checksums for data integrity
//!
//! # Examples
//!
//! ```rust,no_run
//! use agpm_cli::utils::fs::{ensure_dir, safe_write, calculate_checksum};
//! use std::path::Path;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create directory structure
//! ensure_dir(Path::new("output/agents"))?;
//!
//! // Write file atomically
//! safe_write(Path::new("output/config.toml"), "[sources]")?;
//!
//! // Verify file integrity
//! let checksum = calculate_checksum(Path::new("output/config.toml"))?;
//! println!("File checksum: {}", checksum);
//! # Ok(())
//! # }
//! ```
//!
//! # Platform Considerations
//!
//! ## Windows
//! - Supports long paths (>260 characters) using UNC prefixes
//! - Handles case-insensitive file systems
//! - Manages file permissions and attributes correctly
//!
//! ## Unix/Linux
//! - Preserves file permissions during copy operations
//! - Handles case-sensitive file systems
//! - Supports symbolic links appropriately
//!
//! ## macOS
//! - Handles HFS+ case-insensitive by default
//! - Supports extended attributes
//! - Works with case-sensitive APFS volumes

// Module declarations
pub mod atomic;
pub mod dirs;
pub mod discovery;
pub mod formats;
pub mod metadata;
pub mod parallel;
pub mod paths;
pub mod temp;

// Re-export commonly used items from each module

// Directory operations
pub use dirs::{
    copy_dir, copy_dir_all, ensure_dir, ensure_dir_exists, ensure_parent_dir, remove_dir_all,
};

// Atomic write operations
pub use atomic::{atomic_write, atomic_write_multiple, safe_write};

// Path utilities
pub use paths::{find_project_root, get_global_config_path, is_safe_path, normalize_path};

// File discovery
pub use discovery::find_files;

// Temporary directories
pub use temp::TempDir;

// Metadata operations
pub use metadata::{
    calculate_checksum, calculate_checksums_parallel, compare_file_times, dir_size,
    file_exists_and_readable, get_directory_size, get_modified_time,
};

// Parallel operations
pub use parallel::{copy_dirs_parallel, copy_files_parallel, read_files_parallel};

// Format-specific I/O
pub use formats::{
    create_temp_file, read_json_file, read_text_file, read_toml_file, read_yaml_file,
    write_json_file, write_text_file, write_toml_file, write_yaml_file,
};
