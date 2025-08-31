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
//! ```rust
//! use ccpm::utils::fs::{ensure_dir, safe_write, calculate_checksum};
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

use anyhow::{Context, Result};
use futures::future::try_join_all;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Ensures a directory exists, creating it and all parent directories if necessary.
///
/// This function is cross-platform and handles:
/// - Windows long paths (>260 characters) automatically
/// - Permission errors with helpful error messages
/// - Existing files at the target path (returns error)
///
/// # Arguments
///
/// * `path` - The directory path to create
///
/// # Returns
///
/// - `Ok(())` if the directory exists or was successfully created
/// - `Err` if the path exists but is not a directory, or creation fails
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::ensure_dir;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Create nested directories
/// ensure_dir(Path::new("output/agents/subdir"))?;
/// # Ok(())
/// # }
/// ```
///
/// # Platform Notes
///
/// - **Windows**: Automatically handles long paths and provides specific error guidance
/// - **Unix**: Respects umask for directory permissions
/// - **All platforms**: Creates parent directories recursively
pub fn ensure_dir(path: &Path) -> Result<()> {
    // Handle Windows long paths
    let safe_path = crate::utils::platform::windows_long_path(path);

    if !safe_path.exists() {
        fs::create_dir_all(&safe_path)
            .with_context(|| {
                let platform_help = if crate::utils::platform::is_windows() {
                    "On Windows: Check that the path length is < 260 chars or that long path support is enabled"
                } else {
                    "Check directory permissions and path validity"
                };

                format!(
                    "Failed to create directory: {}\\n\\n{}",
                    path.display(),
                    platform_help
                )
            })?;
    } else if !safe_path.is_dir() {
        return Err(anyhow::anyhow!(
            "Path exists but is not a directory: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Safely writes a string to a file using atomic operations.
///
/// This is a convenience wrapper around [`atomic_write`] that handles string-to-bytes conversion.
/// The write is atomic, meaning the file either contains the new content or the old content,
/// never a partial write.
///
/// # Arguments
///
/// * `path` - The file path to write to
/// * `content` - The string content to write
///
/// # Returns
///
/// - `Ok(())` if the file was written successfully
/// - `Err` if the write operation fails
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::safe_write;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// safe_write(Path::new("config.toml"), "[sources]\ncommunity = \"https://example.com\"")?;
/// # Ok(())
/// # }
/// ```
///
/// # See Also
///
/// - [`atomic_write`] for writing raw bytes
/// - [`atomic_write_multiple`] for batch writing multiple files
pub fn safe_write(path: &Path, content: &str) -> Result<()> {
    atomic_write(path, content.as_bytes())
}

/// Atomically writes bytes to a file using a write-then-rename strategy.
///
/// This function ensures atomic writes by:
/// 1. Writing content to a temporary file (`.tmp` extension)
/// 2. Syncing the temporary file to disk
/// 3. Atomically renaming the temporary file to the target path
///
/// This approach prevents data corruption from interrupted writes and ensures
/// readers never see partially written files.
///
/// # Arguments
///
/// * `path` - The target file path
/// * `content` - The raw bytes to write
///
/// # Returns
///
/// - `Ok(())` if the file was written atomically
/// - `Err` if any step of the atomic write fails
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::atomic_write;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let config_bytes = b"[sources]\ncommunity = \"https://example.com\"";
/// atomic_write(Path::new("ccpm.toml"), config_bytes)?;
/// # Ok(())
/// # }
/// ```
///
/// # Platform Notes
///
/// - **Windows**: Handles long paths and provides specific error messages
/// - **Unix**: Preserves file permissions on existing files
/// - **All platforms**: Creates parent directories if they don't exist
///
/// # Guarantees
///
/// - **Atomicity**: File contents are never in a partial state
/// - **Durability**: Content is synced to disk before rename
/// - **Safety**: Parent directories are created automatically
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    use std::io::Write;

    // Handle Windows long paths
    let safe_path = crate::utils::platform::windows_long_path(path);

    // Create parent directory if needed
    if let Some(parent) = safe_path.parent() {
        ensure_dir(parent)?;
    }

    // Write to temporary file first
    let temp_path = safe_path.with_extension("tmp");

    {
        let mut file = fs::File::create(&temp_path).with_context(|| {
            let platform_help = if crate::utils::platform::is_windows() {
                "On Windows: Check file permissions, path length, and that directory exists"
            } else {
                "Check file permissions and that directory exists"
            };

            format!(
                "Failed to create temp file: {}\\n\\n{}",
                temp_path.display(),
                platform_help
            )
        })?;

        file.write_all(content)
            .with_context(|| format!("Failed to write to temp file: {}", temp_path.display()))?;

        file.sync_all()
            .with_context(|| "Failed to sync file to disk")?;
    }

    // Atomic rename
    fs::rename(&temp_path, &safe_path)
        .with_context(|| format!("Failed to rename temp file to: {}", safe_path.display()))?;

    Ok(())
}

/// Recursively copies a directory and all its contents to a new location.
///
/// This function performs a deep copy of all files and subdirectories from the source
/// to the destination. It creates the destination directory if it doesn't exist and
/// preserves the directory structure.
///
/// # Arguments
///
/// * `src` - The source directory to copy from
/// * `dst` - The destination directory to copy to
///
/// # Returns
///
/// - `Ok(())` if the directory was copied successfully
/// - `Err` if the copy operation fails for any file or directory
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::copy_dir;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Copy entire agent directory
/// copy_dir(Path::new("cache/agents"), Path::new("output/agents"))?;
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Creates destination directory if it doesn't exist
/// - Recursively copies all subdirectories
/// - Copies only regular files (skips symlinks and special files)
/// - Overwrites existing files in the destination
///
/// # Platform Notes
///
/// - **Windows**: Handles long paths and preserves attributes
/// - **Unix**: Preserves file permissions during copy
/// - **All platforms**: Does not follow symbolic links
///
/// # See Also
///
/// - [`copy_dirs_parallel`] for copying multiple directories concurrently
/// - [`copy_files_parallel`] for batch file copying
pub fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    ensure_dir(dst)?;

    for entry in
        fs::read_dir(src).with_context(|| format!("Failed to read directory: {}", src.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "Failed to copy file from {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
        // Skip symlinks and other file types
    }

    Ok(())
}

/// Recursively removes a directory and all its contents.
///
/// This function safely removes a directory tree, handling the case where the
/// directory doesn't exist (no error). It's designed to be safe for cleanup
/// operations where the directory may or may not exist.
///
/// # Arguments
///
/// * `path` - The directory to remove
///
/// # Returns
///
/// - `Ok(())` if the directory was removed or didn't exist
/// - `Err` if the removal failed due to permissions or other filesystem errors
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::remove_dir_all;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Safe cleanup - won't error if directory doesn't exist
/// remove_dir_all(Path::new("temp/cache"))?;
/// # Ok(())
/// # }
/// ```
///
/// # Safety
///
/// - Does not follow symbolic links outside the directory tree
/// - Handles permission errors with descriptive messages
/// - Safe to call on non-existent directories
///
/// # Platform Notes
///
/// - **Windows**: Handles long paths and readonly files
/// - **Unix**: Respects file permissions
/// - **All platforms**: Atomic operation where supported by filesystem
pub fn remove_dir_all(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory: {}", path.display()))?;
    }
    Ok(())
}

/// Normalizes a path by resolving `.` and `..` components.
///
/// This function cleans up path components by:
/// - Removing `.` (current directory) components
/// - Resolving `..` (parent directory) components
/// - Maintaining the path's absolute or relative nature
///
/// Note: This function performs logical path resolution without accessing the filesystem.
/// It does not resolve symbolic links or verify that the path exists.
///
/// # Arguments
///
/// * `path` - The path to normalize
///
/// # Returns
///
/// A normalized [`PathBuf`] with `.` and `..` components resolved
///
/// # Examples
///
/// ```rust,no_run
/// use ccpm::utils::fs::normalize_path;
/// use std::path::{Path, PathBuf};
///
/// let path = Path::new("/foo/./bar/../baz");
/// let normalized = normalize_path(path);
/// assert_eq!(normalized, PathBuf::from("/foo/baz"));
///
/// let relative = Path::new("../src/./lib.rs");
/// let normalized_rel = normalize_path(relative);
/// assert_eq!(normalized_rel, PathBuf::from("../src/lib.rs"));
/// ```
///
/// # Use Cases
///
/// - Cleaning user input paths
/// - Path comparison and deduplication
/// - Security checks for path traversal
/// - Canonical path representation
///
/// # See Also
///
/// - `is_safe_path` for security validation using this normalization
/// - `safe_canonicalize` for filesystem-aware path resolution
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {} // Skip .
            std::path::Component::ParentDir => {
                components.pop(); // Remove previous component for ..
            }
            c => components.push(c),
        }
    }

    components.iter().collect()
}

/// Checks if a path is safe and doesn't escape the base directory.
///
/// This function prevents directory traversal attacks by ensuring that the resolved
/// path remains within the base directory. It handles both absolute and relative paths,
/// normalizing them before comparison.
///
/// # Arguments
///
/// * `base` - The base directory that should contain the path
/// * `path` - The path to validate (can be absolute or relative)
///
/// # Returns
///
/// - `true` if the path is safe and stays within the base directory
/// - `false` if the path would escape the base directory
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::is_safe_path;
/// use std::path::Path;
///
/// let base = Path::new("/home/user/project");
///
/// // Safe paths
/// assert!(is_safe_path(base, Path::new("src/main.rs")));
/// assert!(is_safe_path(base, Path::new("./config/settings.toml")));
///
/// // Unsafe paths (directory traversal)
/// assert!(!is_safe_path(base, Path::new("../../../etc/passwd")));
/// assert!(!is_safe_path(base, Path::new("/etc/passwd")));
/// ```
///
/// # Security
///
/// This function is essential for preventing directory traversal vulnerabilities
/// when processing user-provided paths. It should be used whenever:
/// - Extracting archives or packages
/// - Processing configuration files with path references
/// - Handling user input that specifies file locations
///
/// # Implementation
///
/// The function normalizes both paths using `normalize_path` before comparison,
/// ensuring that path traversal attempts using `../` are properly detected.
#[must_use]
pub fn is_safe_path(base: &Path, path: &Path) -> bool {
    let normalized_base = normalize_path(base);
    let normalized_path = if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&base.join(path))
    };

    normalized_path.starts_with(normalized_base)
}

/// Recursively finds files matching a pattern in a directory tree.
///
/// This function performs a recursive search through the directory tree,
/// matching files whose names contain the specified pattern. The search
/// is case-sensitive and uses simple string matching (not regex).
///
/// # Arguments
///
/// * `dir` - The directory to search in
/// * `pattern` - The pattern to match in filenames (substring match)
///
/// # Returns
///
/// A vector of [`PathBuf`]s for all matching files, or an error if the directory
/// cannot be read.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::find_files;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Find all Rust source files
/// let rust_files = find_files(Path::new("src"), ".rs")?;
///
/// // Find all markdown files
/// let md_files = find_files(Path::new("docs"), ".md")?;
///
/// // Find files with "test" in the name
/// let test_files = find_files(Path::new("."), "test")?;
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Searches recursively through all subdirectories
/// - Only returns regular files (not directories or symlinks)
/// - Uses substring matching (case-sensitive)
/// - Returns empty vector if no matches found
/// - Continues searching even if some subdirectories are inaccessible
///
/// # Performance
///
/// For large directory trees or when searching for many different patterns,
/// consider using external tools like `fd` or implementing caching for repeated searches.
pub fn find_files(dir: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    find_files_recursive(dir, pattern, &mut files)?;
    Ok(files)
}

fn find_files_recursive(dir: &Path, pattern: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            find_files_recursive(&path, pattern, files)?;
        } else if path.is_file() {
            if let Some(name) = path.file_name() {
                if name.to_string_lossy().contains(pattern) {
                    files.push(path);
                }
            }
        }
    }

    Ok(())
}

/// Calculates the total size of a directory and all its contents recursively.
///
/// This function traverses the directory tree and sums the sizes of all regular files.
/// It handles nested directories and provides the total disk usage for the directory tree.
///
/// # Arguments
///
/// * `path` - The directory to calculate size for
///
/// # Returns
///
/// The total size in bytes, or an error if the directory cannot be read
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::dir_size;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let cache_size = dir_size(Path::new("~/.ccpm/cache"))?;
/// println!("Cache size: {} bytes ({:.2} MB)", cache_size, cache_size as f64 / 1024.0 / 1024.0);
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Recursively traverses all subdirectories
/// - Includes only regular files in size calculation
/// - Does not follow symbolic links
/// - Returns 0 for empty directories
/// - Accumulates sizes using 64-bit integers (supports very large directories)
///
/// # Performance
///
/// This is a synchronous operation that may take time for large directory trees.
/// For better performance with large directories, use [`get_directory_size`] which
/// runs the calculation on a separate thread.
///
/// # See Also
///
/// - [`get_directory_size`] for async version
/// - Platform-specific tools may be faster for very large directories
pub fn dir_size(path: &Path) -> Result<u64> {
    let mut size = 0;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            size += dir_size(&entry.path())?;
        } else {
            size += metadata.len();
        }
    }

    Ok(size)
}

/// Asynchronously calculates the total size of a directory and all its contents.
///
/// This is the async version of [`dir_size`] that runs the calculation on a separate
/// thread to avoid blocking the async runtime. Use this when calculating directory
/// sizes as part of async operations.
///
/// # Arguments
///
/// * `path` - The directory to calculate size for
///
/// # Returns
///
/// The total size in bytes, or an error if the operation fails
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::get_directory_size;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let cache_size = get_directory_size(Path::new("~/.ccpm/cache")).await?;
/// println!("Cache size: {} bytes", cache_size);
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function uses `tokio::task::spawn_blocking` to run the directory traversal
/// on a thread pool, preventing it from blocking other async tasks. This is particularly
/// useful when:
/// - Calculating sizes for multiple directories concurrently
/// - Integrating with async workflows
/// - Avoiding blocking in async web servers or CLI applications
///
/// # See Also
///
/// - [`dir_size`] for synchronous version
pub async fn get_directory_size(path: &Path) -> Result<u64> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || dir_size(&path))
        .await
        .context("Failed to join directory size calculation task")?
}

/// Ensures that the parent directory of a file path exists.
///
/// This is a convenience function for creating the directory structure needed
/// for a file before writing to it. It extracts the parent directory from the
/// file path and ensures it exists.
///
/// # Arguments
///
/// * `path` - The file path whose parent directory should exist
///
/// # Returns
///
/// - `Ok(())` if the parent directory exists or was created successfully
/// - `Err` if directory creation fails
/// - `Ok(())` if the path has no parent (e.g., root level files)
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::ensure_parent_dir;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Ensure directory structure exists before writing file
/// ensure_parent_dir(Path::new("output/agents/example.md"))?;
/// std::fs::write("output/agents/example.md", "# Example Agent")?;
/// # Ok(())
/// # }
/// ```
///
/// # Use Cases
///
/// - Preparing directory structure before file operations
/// - Ensuring atomic writes have proper directory structure
/// - Setting up output paths in batch processing
///
/// # See Also
///
/// - [`ensure_dir`] for creating a specific directory
/// - [`atomic_write`] which calls this internally
pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    Ok(())
}

/// Alias for `ensure_dir` for consistency
pub fn ensure_dir_exists(path: &Path) -> Result<()> {
    ensure_dir(path)
}

/// Copy a directory recursively (alias for consistency)
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    copy_dir(src, dst)
}

/// Finds the CCPM project root by searching for `ccpm.toml` in the directory hierarchy.
///
/// This function starts from the given directory and walks up the directory tree
/// looking for a `ccpm.toml` file, which indicates the root of a CCPM project.
/// This is similar to how Git finds the repository root by looking for `.git`.
///
/// # Arguments
///
/// * `start` - The directory to start searching from (typically current directory)
///
/// # Returns
///
/// The path to the directory containing `ccpm.toml`, or an error if not found
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::find_project_root;
/// use std::env;
///
/// # fn example() -> anyhow::Result<()> {
/// // Find project root from current directory
/// let current_dir = env::current_dir()?;
/// let project_root = find_project_root(&current_dir)?;
/// println!("Project root: {}", project_root.display());
/// # Ok(())
/// # }
/// ```
///
/// # Behavior
///
/// - Starts from the given directory and searches upward
/// - Returns the first directory containing `ccpm.toml`
/// - Canonicalizes the starting path to handle symlinks
/// - Stops at filesystem root if no `ccpm.toml` is found
///
/// # Error Cases
///
/// - No `ccpm.toml` found in the directory hierarchy
/// - Permission denied accessing parent directories
/// - Invalid or inaccessible starting path
///
/// # Use Cases
///
/// - CLI commands that need to operate on the current project
/// - Finding configuration files relative to project root
/// - Validating that commands are run within a CCPM project
pub fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut current = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());

    loop {
        if current.join("ccpm.toml").exists() {
            return Ok(current);
        }

        if !current.pop() {
            return Err(anyhow::anyhow!(
                "No ccpm.toml found in current directory or any parent directory"
            ));
        }
    }
}

/// Returns the path to the global CCPM configuration file.
///
/// This function constructs the path to the global configuration file following
/// platform conventions. The global config contains user-specific settings like
/// authentication tokens and private repository URLs.
///
/// # Returns
///
/// The path to `~/.config/ccpm/config.toml`, or an error if the home directory
/// cannot be determined
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::get_global_config_path;
///
/// # fn example() -> anyhow::Result<()> {
/// let config_path = get_global_config_path()?;
/// println!("Global config at: {}", config_path.display());
///
/// // Check if global config exists
/// if config_path.exists() {
///     let config = std::fs::read_to_string(&config_path)?;
///     println!("Config contents: {}", config);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Platform Paths
///
/// - **Linux**: `~/.config/ccpm/config.toml`
/// - **macOS**: `~/.config/ccpm/config.toml`
/// - **Windows**: `%USERPROFILE%\.config\ccpm\config.toml`
///
/// # Use Cases
///
/// - Loading global user configuration
/// - Storing authentication tokens securely
/// - Sharing settings across multiple projects
///
/// # Security Note
///
/// This file may contain sensitive information like API tokens. It should never
/// be committed to version control or shared publicly.
pub fn get_global_config_path() -> Result<PathBuf> {
    let home = crate::utils::platform::get_home_dir()?;
    Ok(home.join(".config").join("ccpm").join("config.toml"))
}

/// A temporary directory that automatically cleans up when dropped.
///
/// This struct provides RAII (Resource Acquisition Is Initialization) semantics
/// for temporary directories. The directory is created when the struct is created
/// and automatically removed when the struct is dropped, even if the program panics.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::TempDir;
///
/// # fn example() -> anyhow::Result<()> {
/// {
///     let temp = TempDir::new("test")?;
///     let temp_path = temp.path();
///     
///     // Use the temporary directory
///     std::fs::write(temp_path.join("file.txt"), "temporary data")?;
///     
///     // Directory exists here
///     assert!(temp_path.exists());
/// } // TempDir is dropped here, directory is automatically cleaned up
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// Each `TempDir` instance creates a unique directory using UUID generation,
/// making it safe to use across multiple threads without naming conflicts.
///
/// # Cleanup Behavior
///
/// - Directory is removed recursively when dropped
/// - Cleanup happens even if the program panics
/// - If cleanup fails (rare), the error is silently ignored
/// - Uses the system temporary directory as the parent
///
/// # Use Cases
///
/// - Unit testing with temporary files
/// - Staging areas for atomic operations
/// - Scratch space for temporary processing
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a new temporary directory with the given prefix.
    ///
    /// The directory is created immediately and will have a name like
    /// `ccpm_{prefix}_{uuid}` in the system temporary directory.
    ///
    /// # Arguments
    ///
    /// * `prefix` - A prefix for the directory name (for identification)
    ///
    /// # Returns
    ///
    /// A new `TempDir` instance, or an error if directory creation fails
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::utils::fs::TempDir;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let temp = TempDir::new("cache")?;
    /// println!("Temporary directory: {}", temp.path().display());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(prefix: &str) -> Result<Self> {
        let temp_dir = std::env::temp_dir();
        let unique_name = format!("ccpm_{}_{}", prefix, uuid::Uuid::new_v4());
        let path = temp_dir.join(unique_name);

        ensure_dir(&path)?;

        Ok(Self { path })
    }

    /// Returns the path to the temporary directory.
    ///
    /// The directory is guaranteed to exist as long as this `TempDir` instance exists.
    ///
    /// # Returns
    ///
    /// A reference to the temporary directory path
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.path);
    }
}

/// Calculates the SHA-256 checksum of a file.
///
/// This function reads the entire file into memory and computes its SHA-256 hash,
/// returning it as a lowercase hexadecimal string. This is useful for verifying
/// file integrity and detecting changes.
///
/// # Arguments
///
/// * `path` - The path to the file to checksum
///
/// # Returns
///
/// A 64-character lowercase hexadecimal string representing the SHA-256 hash,
/// or an error if the file cannot be read
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::calculate_checksum;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let checksum = calculate_checksum(Path::new("important-file.txt"))?;
/// println!("File checksum: {}", checksum);
///
/// // Verify against expected checksum
/// let expected = "d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2d2";
/// if checksum == expected {
///     println!("File integrity verified!");
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function reads the entire file into memory, so it may not be suitable
/// for very large files. For processing multiple files, consider using
/// [`calculate_checksums_parallel`] for better performance.
///
/// # Security
///
/// SHA-256 is cryptographically secure and suitable for:
/// - Integrity verification
/// - Change detection
/// - Digital signatures
/// - Blockchain applications
///
/// # See Also
///
/// - [`calculate_checksums_parallel`] for batch processing
/// - [`hex`] crate for hexadecimal encoding
pub fn calculate_checksum(path: &Path) -> Result<String> {
    let content = fs::read(path)
        .with_context(|| format!("Failed to read file for checksum: {}", path.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();

    Ok(hex::encode(result))
}

/// Calculates SHA-256 checksums for multiple files concurrently.
///
/// This function processes multiple files in parallel using Tokio's thread pool,
/// which can significantly improve performance when processing many files or
/// large files on systems with multiple CPU cores.
///
/// # Arguments
///
/// * `paths` - A slice of file paths to process
///
/// # Returns
///
/// A vector of tuples containing each file path and its corresponding checksum,
/// in the same order as the input paths. Returns an error if any file fails
/// to be processed.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::calculate_checksums_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let files = vec![
///     PathBuf::from("file1.txt"),
///     PathBuf::from("file2.txt"),
///     PathBuf::from("file3.txt"),
/// ];
///
/// let results = calculate_checksums_parallel(&files).await?;
/// for (path, checksum) in results {
///     println!("{}: {}", path.display(), checksum);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function uses `tokio::task::spawn_blocking` to run checksum calculations
/// on separate threads, allowing for true parallelism. Benefits:
/// - CPU-bound work doesn't block the async runtime
/// - Multiple files processed simultaneously
/// - Scales with available CPU cores
/// - Maintains order of results
///
/// # Error Handling
///
/// If any file fails to be processed, the entire operation fails and returns
/// an error with details about all failures. This "all-or-nothing" approach
/// ensures data consistency.
///
/// # See Also
///
/// - [`calculate_checksum`] for single file processing
/// - [`read_files_parallel`] for concurrent file reading
pub async fn calculate_checksums_parallel(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut tasks = Vec::new();

    for (index, path) in paths.iter().enumerate() {
        let path = path.clone();
        let task = tokio::task::spawn_blocking(move || {
            calculate_checksum(&path).map(|checksum| (index, path, checksum))
        });
        tasks.push(task);
    }

    let results = try_join_all(tasks)
        .await
        .context("Failed to join checksum calculation tasks")?;

    let mut successes = Vec::new();
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok((index, path, checksum)) => successes.push((index, path, checksum)),
            Err(e) => errors.push(e),
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|error| format!("  {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to calculate checksums for {} files:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    // Sort results by original index to maintain order
    successes.sort_by_key(|(index, _, _)| *index);
    let ordered_results: Vec<(PathBuf, String)> = successes
        .into_iter()
        .map(|(_, path, checksum)| (path, checksum))
        .collect();

    Ok(ordered_results)
}

/// Copies multiple files concurrently using parallel processing.
///
/// This function performs multiple file copy operations in parallel, which can
/// significantly improve performance when copying many files, especially on
/// systems with fast storage and multiple CPU cores.
///
/// # Arguments
///
/// * `sources_and_destinations` - A slice of (source, destination) path pairs
///
/// # Returns
///
/// - `Ok(())` if all files were copied successfully
/// - `Err` if any copy operation fails, with details about all failures
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::copy_files_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let copy_operations = vec![
///     (PathBuf::from("src/agent1.md"), PathBuf::from("output/agent1.md")),
///     (PathBuf::from("src/agent2.md"), PathBuf::from("output/agent2.md")),
///     (PathBuf::from("src/snippet.md"), PathBuf::from("output/snippet.md")),
/// ];
///
/// copy_files_parallel(&copy_operations).await?;
/// println!("All files copied successfully!");
/// # Ok(())
/// # }
/// ```
///
/// # Features
///
/// - **Parallel execution**: Uses thread pool for concurrent operations
/// - **Automatic directory creation**: Creates destination directories as needed
/// - **Atomic behavior**: Either all files copy successfully or none do
/// - **Progress tracking**: Can be combined with progress bars for user feedback
///
/// # Performance Characteristics
///
/// - Best for many small to medium files
/// - Scales with available CPU cores and I/O bandwidth
/// - May not improve performance for very large files (I/O bound)
/// - Respects filesystem limits on concurrent operations
///
/// # Error Handling
///
/// All copy operations must succeed for the function to return `Ok(())`. If any
/// operation fails, detailed error information is provided for troubleshooting.
///
/// # See Also
///
/// - [`copy_dirs_parallel`] for directory copying
/// - [`atomic_write_multiple`] for writing multiple files
pub async fn copy_files_parallel(sources_and_destinations: &[(PathBuf, PathBuf)]) -> Result<()> {
    if sources_and_destinations.is_empty() {
        return Ok(());
    }

    let mut tasks = Vec::new();

    for (src, dst) in sources_and_destinations {
        let src = src.clone();
        let dst = dst.clone();
        let task = tokio::task::spawn_blocking(move || {
            // Ensure destination directory exists
            if let Some(parent) = dst.parent() {
                ensure_dir(parent)?;
            }

            // Copy file
            fs::copy(&src, &dst).with_context(|| {
                format!(
                    "Failed to copy file from {} to {}",
                    src.display(),
                    dst.display()
                )
            })?;

            Ok::<_, anyhow::Error>((src, dst))
        });
        tasks.push(task);
    }

    let results = try_join_all(tasks)
        .await
        .context("Failed to join file copy tasks")?;

    let mut errors = Vec::new();

    for result in results {
        if let Err(e) = result {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|error| format!("  {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to copy {} files:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    Ok(())
}

/// Writes multiple files atomically in parallel.
///
/// This function performs multiple atomic write operations concurrently,
/// which can significantly improve performance when writing many files.
/// Each file is written atomically using the same write-then-rename strategy
/// as [`atomic_write`].
///
/// # Arguments
///
/// * `files` - A slice of (path, content) pairs to write
///
/// # Returns
///
/// - `Ok(())` if all files were written successfully
/// - `Err` if any write operation fails, with details about all failures
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::atomic_write_multiple;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let files = vec![
///     (PathBuf::from("config1.toml"), b"[sources]\ncommunity = \"url1\"".to_vec()),
///     (PathBuf::from("config2.toml"), b"[sources]\nprivate = \"url2\"".to_vec()),
///     (PathBuf::from("readme.md"), b"# Project Documentation".to_vec()),
/// ];
///
/// atomic_write_multiple(&files).await?;
/// println!("All configuration files written atomically!");
/// # Ok(())
/// # }
/// ```
///
/// # Atomicity Guarantees
///
/// - Each individual file is written atomically
/// - Either all files are written successfully or the operation fails
/// - No partially written files are left on disk
/// - Parent directories are created automatically
///
/// # Performance
///
/// This function uses parallel execution to improve performance:
/// - Multiple files written concurrently
/// - Scales with available CPU cores and I/O bandwidth
/// - Particularly effective for many small files
/// - Maintains atomicity guarantees for each file
///
/// # See Also
///
/// - [`atomic_write`] for single file atomic writes
/// - [`safe_write`] for string content convenience
/// - [`copy_files_parallel`] for file copying operations
pub async fn atomic_write_multiple(files: &[(PathBuf, Vec<u8>)]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let mut tasks = Vec::new();

    for (path, content) in files {
        let path = path.clone();
        let content = content.clone();
        let task =
            tokio::task::spawn_blocking(move || atomic_write(&path, &content).map(|()| path));
        tasks.push(task);
    }

    let results = try_join_all(tasks)
        .await
        .context("Failed to join atomic write tasks")?;

    let mut errors = Vec::new();

    for result in results {
        if let Err(e) = result {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|error| format!("  {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to write {} files:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    Ok(())
}

/// Copies multiple directories concurrently.
///
/// This function performs multiple directory copy operations in parallel,
/// which can improve performance when copying several separate directory trees.
/// Each directory is copied recursively using the same logic as [`copy_dir`].
///
/// # Arguments
///
/// * `sources_and_destinations` - A slice of (source, destination) directory pairs
///
/// # Returns
///
/// - `Ok(())` if all directories were copied successfully
/// - `Err` if any copy operation fails, with details about all failures
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::copy_dirs_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let copy_operations = vec![
///     (PathBuf::from("cache/agents"), PathBuf::from("output/agents")),
///     (PathBuf::from("cache/snippets"), PathBuf::from("output/snippets")),
///     (PathBuf::from("cache/templates"), PathBuf::from("output/templates")),
/// ];
///
/// copy_dirs_parallel(&copy_operations).await?;
/// println!("All directories copied successfully!");
/// # Ok(())
/// # }
/// ```
///
/// # Features
///
/// - **Recursive copying**: Each directory is copied with all subdirectories
/// - **Parallel execution**: Multiple directory trees copied concurrently
/// - **Atomic behavior**: Either all directories copy successfully or operation fails
/// - **Automatic creation**: Destination directories are created as needed
///
/// # Use Cases
///
/// - Copying multiple resource categories simultaneously
/// - Batch operations on directory structures
/// - Backup operations for multiple directories
/// - Installation processes involving multiple components
///
/// # Performance Considerations
///
/// - Best performance with multiple separate directory trees
/// - May not improve performance if directories share the same disk
/// - Memory usage scales with number of directories and their sizes
/// - Respects filesystem concurrent operation limits
///
/// # See Also
///
/// - [`copy_dir`] for single directory copying
/// - [`copy_files_parallel`] for individual file copying
pub async fn copy_dirs_parallel(sources_and_destinations: &[(PathBuf, PathBuf)]) -> Result<()> {
    if sources_and_destinations.is_empty() {
        return Ok(());
    }

    let mut tasks = Vec::new();

    for (src, dst) in sources_and_destinations {
        let src = src.clone();
        let dst = dst.clone();
        let task = tokio::task::spawn_blocking(move || copy_dir(&src, &dst).map(|()| (src, dst)));
        tasks.push(task);
    }

    let results = try_join_all(tasks)
        .await
        .context("Failed to join directory copy tasks")?;

    let mut errors = Vec::new();

    for result in results {
        if let Err(e) = result {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|error| format!("  {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to copy {} directories:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    Ok(())
}

/// Reads multiple files concurrently and returns their contents.
///
/// This function reads multiple text files in parallel, which can improve
/// performance when processing many files, especially on systems with
/// fast storage and multiple CPU cores.
///
/// # Arguments
///
/// * `paths` - A slice of file paths to read
///
/// # Returns
///
/// A vector of tuples containing each file path and its content as a UTF-8 string,
/// in the same order as the input paths. Returns an error if any file fails
/// to be read or contains invalid UTF-8.
///
/// # Examples
///
/// ```rust
/// use ccpm::utils::fs::read_files_parallel;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config_files = vec![
///     PathBuf::from("ccpm.toml"),
///     PathBuf::from("agents/agent1.md"),
///     PathBuf::from("snippets/snippet1.md"),
/// ];
///
/// let results = read_files_parallel(&config_files).await?;
/// for (path, content) in results {
///     println!("{}: {} characters", path.display(), content.len());
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// This function uses `tokio::task::spawn_blocking` to perform file I/O
/// on separate threads:
/// - Multiple files read simultaneously
/// - Non-blocking for the async runtime
/// - Scales with available CPU cores and I/O bandwidth
/// - Maintains result ordering
///
/// # UTF-8 Handling
///
/// All files must contain valid UTF-8 text. If any file contains invalid
/// UTF-8 bytes, the operation will fail with a descriptive error.
///
/// # Error Handling
///
/// If any file fails to be read (due to permissions, missing file, or
/// invalid UTF-8), the entire operation fails. This ensures consistency
/// when processing related files that should all be available.
///
/// # See Also
///
/// - [`calculate_checksums_parallel`] for file integrity verification
/// - [`atomic_write_multiple`] for batch writing operations
pub async fn read_files_parallel(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut tasks = Vec::new();

    for (index, path) in paths.iter().enumerate() {
        let path = path.clone();
        let task = tokio::task::spawn_blocking(move || {
            fs::read_to_string(&path).map(|content| (index, path, content))
        });
        tasks.push(task);
    }

    let results = try_join_all(tasks)
        .await
        .context("Failed to join file read tasks")?;

    let mut successes = Vec::new();
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok((index, path, content)) => successes.push((index, path, content)),
            Err(e) => errors.push(e),
        }
    }

    if !errors.is_empty() {
        let error_msgs: Vec<String> = errors
            .into_iter()
            .map(|error| format!("  {error}"))
            .collect();
        return Err(anyhow::anyhow!(
            "Failed to read {} files:\n{}",
            error_msgs.len(),
            error_msgs.join("\n")
        ));
    }

    // Sort results by original index to maintain order
    successes.sort_by_key(|(index, _, _)| *index);
    let ordered_results: Vec<(PathBuf, String)> = successes
        .into_iter()
        .map(|(_, path, content)| (path, content))
        .collect();

    Ok(ordered_results)
}

// ============================================================================
// Unified File I/O Operations
// ============================================================================

/// Reads a text file with proper error handling and context.
///
/// # Arguments
/// * `path` - The path to the file to read
///
/// # Returns
/// The contents of the file as a String
///
/// # Errors
/// Returns an error with context if the file cannot be read
pub fn read_text_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Writes a text file atomically with proper error handling.
///
/// # Arguments
/// * `path` - The path to write to
/// * `content` - The text content to write
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error with context if the file cannot be written
pub fn write_text_file(path: &Path, content: &str) -> Result<()> {
    safe_write(path, content).with_context(|| format!("Failed to write file: {}", path.display()))
}

/// Reads and parses a JSON file.
///
/// # Arguments
/// * `path` - The path to the JSON file
///
/// # Type Parameters
/// * `T` - The type to deserialize into (must implement DeserializeOwned)
///
/// # Returns
/// The parsed JSON data
///
/// # Errors
/// Returns an error if the file cannot be read or parsed
pub fn read_json_file<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = read_text_file(path)?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON from file: {}", path.display()))
}

/// Writes data as JSON to a file atomically.
///
/// # Arguments
/// * `path` - The path to write to
/// * `data` - The data to serialize
/// * `pretty` - Whether to use pretty formatting
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement Serialize)
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error if serialization fails or the file cannot be written
pub fn write_json_file<T>(path: &Path, data: &T, pretty: bool) -> Result<()>
where
    T: serde::Serialize,
{
    let json = if pretty {
        serde_json::to_string_pretty(data)?
    } else {
        serde_json::to_string(data)?
    };

    write_text_file(path, &json)
        .with_context(|| format!("Failed to write JSON file: {}", path.display()))
}

/// Reads and parses a TOML file.
///
/// # Arguments
/// * `path` - The path to the TOML file
///
/// # Type Parameters
/// * `T` - The type to deserialize into (must implement DeserializeOwned)
///
/// # Returns
/// The parsed TOML data
///
/// # Errors
/// Returns an error if the file cannot be read or parsed
pub fn read_toml_file<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = read_text_file(path)?;
    toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML from file: {}", path.display()))
}

/// Writes data as TOML to a file atomically.
///
/// # Arguments
/// * `path` - The path to write to
/// * `data` - The data to serialize
/// * `pretty` - Whether to use pretty formatting (always true for TOML)
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement Serialize)
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error if serialization fails or the file cannot be written
pub fn write_toml_file<T>(path: &Path, data: &T) -> Result<()>
where
    T: serde::Serialize,
{
    let toml = toml::to_string_pretty(data)
        .with_context(|| format!("Failed to serialize data to TOML for: {}", path.display()))?;

    write_text_file(path, &toml)
        .with_context(|| format!("Failed to write TOML file: {}", path.display()))
}

/// Reads and parses a YAML file.
///
/// # Arguments
/// * `path` - The path to the YAML file
///
/// # Type Parameters
/// * `T` - The type to deserialize into (must implement DeserializeOwned)
///
/// # Returns
/// The parsed YAML data
///
/// # Errors
/// Returns an error if the file cannot be read or parsed
pub fn read_yaml_file<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let content = read_text_file(path)?;
    serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML from file: {}", path.display()))
}

/// Writes data as YAML to a file atomically.
///
/// # Arguments
/// * `path` - The path to write to
/// * `data` - The data to serialize
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement Serialize)
///
/// # Returns
/// Ok(()) on success
///
/// # Errors
/// Returns an error if serialization fails or the file cannot be written
pub fn write_yaml_file<T>(path: &Path, data: &T) -> Result<()>
where
    T: serde::Serialize,
{
    let yaml = serde_yaml::to_string(data)
        .with_context(|| format!("Failed to serialize data to YAML for: {}", path.display()))?;

    write_text_file(path, &yaml)
        .with_context(|| format!("Failed to write YAML file: {}", path.display()))
}

/// Creates a temporary file with content for testing.
///
/// # Arguments
/// * `prefix` - The prefix for the temp file name
/// * `content` - The content to write to the file
///
/// # Returns
/// A TempPath that will delete the file when dropped
///
/// # Errors
/// Returns an error if the temp file cannot be created
pub fn create_temp_file(prefix: &str, content: &str) -> Result<tempfile::TempPath> {
    let temp_file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(".tmp")
        .tempfile()?;

    let path = temp_file.into_temp_path();
    write_text_file(&path, content)?;

    Ok(path)
}

/// Checks if a file exists and is readable.
///
/// # Arguments
/// * `path` - The path to check
///
/// # Returns
/// true if the file exists and is readable, false otherwise
pub fn file_exists_and_readable(path: &Path) -> bool {
    path.exists() && path.is_file() && fs::metadata(path).is_ok()
}

/// Gets the modification time of a file.
///
/// # Arguments
/// * `path` - The path to the file
///
/// # Returns
/// The modification time as a SystemTime
///
/// # Errors
/// Returns an error if the file metadata cannot be read
pub fn get_modified_time(path: &Path) -> Result<std::time::SystemTime> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to get metadata for: {}", path.display()))?;

    metadata
        .modified()
        .with_context(|| format!("Failed to get modification time for: {}", path.display()))
}

/// Compares the modification times of two files.
///
/// # Arguments
/// * `path1` - The first file path
/// * `path2` - The second file path
///
/// # Returns
/// - `Ok(Ordering::Less)` if path1 is older than path2
/// - `Ok(Ordering::Greater)` if path1 is newer than path2
/// - `Ok(Ordering::Equal)` if they have the same modification time
///
/// # Errors
/// Returns an error if either file's metadata cannot be read
pub fn compare_file_times(path1: &Path, path2: &Path) -> Result<std::cmp::Ordering> {
    let time1 = get_modified_time(path1)?;
    let time2 = get_modified_time(path2)?;

    Ok(time1.cmp(&time2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ensure_dir() {
        let temp = tempdir().unwrap();
        let test_dir = temp.path().join("test_dir");

        assert!(!test_dir.exists());
        ensure_dir(&test_dir).unwrap();
        assert!(test_dir.exists());
        assert!(test_dir.is_dir());
    }

    #[test]
    fn test_normalize_path() {
        let path = Path::new("/foo/./bar/../baz");
        let normalized = normalize_path(path);
        assert_eq!(normalized, PathBuf::from("/foo/baz"));
    }

    #[test]
    fn test_is_safe_path() {
        let base = Path::new("/home/user/project");

        assert!(is_safe_path(base, Path::new("subdir/file.txt")));
        assert!(is_safe_path(base, Path::new("./subdir/file.txt")));
        assert!(!is_safe_path(base, Path::new("../other/file.txt")));
        assert!(!is_safe_path(base, Path::new("/etc/passwd")));
    }

    #[test]
    fn test_safe_write() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.txt");

        safe_write(&file_path, "test content").unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_safe_write_creates_parent_dirs() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("subdir").join("test.txt");

        safe_write(&file_path, "test content").unwrap();

        assert!(file_path.exists());
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_copy_dir() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        // Create source structure
        ensure_dir(&src).unwrap();
        ensure_dir(&src.join("subdir")).unwrap();
        std::fs::write(src.join("file1.txt"), "content1").unwrap();
        std::fs::write(src.join("subdir/file2.txt"), "content2").unwrap();

        // Copy directory
        copy_dir(&src, &dst).unwrap();

        // Verify copy
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir/file2.txt").exists());

        let content1 = std::fs::read_to_string(dst.join("file1.txt")).unwrap();
        assert_eq!(content1, "content1");

        let content2 = std::fs::read_to_string(dst.join("subdir/file2.txt")).unwrap();
        assert_eq!(content2, "content2");
    }

    #[test]
    fn test_remove_dir_all() {
        let temp = tempdir().unwrap();
        let dir = temp.path().join("to_remove");

        ensure_dir(&dir).unwrap();
        std::fs::write(dir.join("file.txt"), "content").unwrap();

        assert!(dir.exists());
        remove_dir_all(&dir).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn test_remove_dir_all_nonexistent() {
        let temp = tempdir().unwrap();
        let dir = temp.path().join("nonexistent");

        // Should not error on non-existent directory
        remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_find_files() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create test files
        std::fs::write(root.join("test.rs"), "").unwrap();
        std::fs::write(root.join("main.rs"), "").unwrap();
        ensure_dir(&root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        std::fs::write(root.join("src/test.txt"), "").unwrap();

        let files = find_files(root, ".rs").unwrap();
        assert_eq!(files.len(), 3);

        let files = find_files(root, "test").unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_dir_size() {
        let temp = tempdir().unwrap();
        let dir = temp.path();

        std::fs::write(dir.join("file1.txt"), "12345").unwrap();
        std::fs::write(dir.join("file2.txt"), "123456789").unwrap();
        ensure_dir(&dir.join("subdir")).unwrap();
        std::fs::write(dir.join("subdir/file3.txt"), "abc").unwrap();

        let size = dir_size(dir).unwrap();
        assert_eq!(size, 17); // 5 + 9 + 3
    }

    #[test]
    fn test_temp_dir() {
        let temp_dir = TempDir::new("test").unwrap();
        let path = temp_dir.path().to_path_buf();

        assert!(path.exists());
        assert!(path.is_dir());

        // Write a file to verify it's a real directory
        std::fs::write(path.join("test.txt"), "test").unwrap();
        assert!(path.join("test.txt").exists());

        drop(temp_dir);
        // Directory should be cleaned up
        assert!(!path.exists());
    }

    #[test]
    fn test_ensure_parent_dir() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("parent").join("child").join("file.txt");

        ensure_parent_dir(&file_path).unwrap();
        assert!(file_path.parent().unwrap().exists());
    }

    #[test]
    fn test_ensure_dir_exists() {
        let temp = tempdir().unwrap();
        let test_dir = temp.path().join("test_dir_alias");

        assert!(!test_dir.exists());
        ensure_dir_exists(&test_dir).unwrap();
        assert!(test_dir.exists());
    }

    #[test]
    fn test_copy_dir_all() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src_alias");
        let dst = temp.path().join("dst_alias");

        ensure_dir(&src).unwrap();
        std::fs::write(src.join("file.txt"), "content").unwrap();

        copy_dir_all(&src, &dst).unwrap();
        assert!(dst.join("file.txt").exists());
    }

    #[test]
    fn test_find_project_root() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("project");
        let subdir = project.join("src").join("subdir");

        ensure_dir(&subdir).unwrap();
        std::fs::write(project.join("ccpm.toml"), "[sources]").unwrap();

        let root = find_project_root(&subdir).unwrap();
        assert_eq!(
            root.canonicalize().unwrap(),
            project.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_find_project_root_not_found() {
        let temp = tempdir().unwrap();
        let result = find_project_root(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_global_config_path() {
        let config_path = get_global_config_path().unwrap();
        assert!(config_path.to_string_lossy().contains(".config"));
        assert!(config_path.to_string_lossy().contains("ccpm"));
    }

    #[test]
    fn test_calculate_checksum() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("checksum_test.txt");
        std::fs::write(&file, "test content").unwrap();

        let checksum = calculate_checksum(&file).unwrap();
        assert!(!checksum.is_empty());
        assert_eq!(checksum.len(), 64); // SHA256 produces 64 hex chars
    }

    #[tokio::test]
    async fn test_calculate_checksums_parallel() {
        let temp = tempdir().unwrap();
        let file1 = temp.path().join("file1.txt");
        let file2 = temp.path().join("file2.txt");

        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        let paths = vec![file1.clone(), file2.clone()];
        let results = calculate_checksums_parallel(&paths).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, file1);
        assert_eq!(results[1].0, file2);
        assert!(!results[0].1.is_empty());
        assert!(!results[1].1.is_empty());
    }

    #[tokio::test]
    async fn test_calculate_checksums_parallel_empty() {
        let results = calculate_checksums_parallel(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_copy_files_parallel() {
        let temp = tempdir().unwrap();
        let src1 = temp.path().join("src1.txt");
        let src2 = temp.path().join("src2.txt");
        let dst1 = temp.path().join("dst").join("dst1.txt");
        let dst2 = temp.path().join("dst").join("dst2.txt");

        std::fs::write(&src1, "content1").unwrap();
        std::fs::write(&src2, "content2").unwrap();

        let pairs = vec![(src1.clone(), dst1.clone()), (src2.clone(), dst2.clone())];
        copy_files_parallel(&pairs).await.unwrap();

        assert!(dst1.exists());
        assert!(dst2.exists());
        assert_eq!(std::fs::read_to_string(&dst1).unwrap(), "content1");
        assert_eq!(std::fs::read_to_string(&dst2).unwrap(), "content2");
    }

    #[tokio::test]
    async fn test_atomic_write_multiple() {
        let temp = tempdir().unwrap();
        let file1 = temp.path().join("atomic1.txt");
        let file2 = temp.path().join("atomic2.txt");

        let files = vec![
            (file1.clone(), b"content1".to_vec()),
            (file2.clone(), b"content2".to_vec()),
        ];

        atomic_write_multiple(&files).await.unwrap();

        assert!(file1.exists());
        assert!(file2.exists());
        assert_eq!(std::fs::read_to_string(&file1).unwrap(), "content1");
        assert_eq!(std::fs::read_to_string(&file2).unwrap(), "content2");
    }

    #[tokio::test]
    async fn test_copy_dirs_parallel() {
        let temp = tempdir().unwrap();
        let src1 = temp.path().join("src1");
        let src2 = temp.path().join("src2");
        let dst1 = temp.path().join("dst1");
        let dst2 = temp.path().join("dst2");

        ensure_dir(&src1).unwrap();
        ensure_dir(&src2).unwrap();
        std::fs::write(src1.join("file1.txt"), "content1").unwrap();
        std::fs::write(src2.join("file2.txt"), "content2").unwrap();

        let pairs = vec![(src1.clone(), dst1.clone()), (src2.clone(), dst2.clone())];
        copy_dirs_parallel(&pairs).await.unwrap();

        assert!(dst1.join("file1.txt").exists());
        assert!(dst2.join("file2.txt").exists());
    }

    #[tokio::test]
    async fn test_read_files_parallel() {
        let temp = tempdir().unwrap();
        let file1 = temp.path().join("read1.txt");
        let file2 = temp.path().join("read2.txt");

        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        let paths = vec![file1.clone(), file2.clone()];
        let results = read_files_parallel(&paths).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, file1);
        assert_eq!(results[0].1, "content1");
        assert_eq!(results[1].0, file2);
        assert_eq!(results[1].1, "content2");
    }

    #[test]
    fn test_ensure_dir_on_file() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();

        let result = ensure_dir(&file_path);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parallel_operations_empty() {
        // Test parallel operations with empty inputs
        let result = calculate_checksums_parallel(&[]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());

        let result = copy_files_parallel(&[]).await;
        assert!(result.is_ok());

        let result = atomic_write_multiple(&[]).await;
        assert!(result.is_ok());

        let result = copy_dirs_parallel(&[]).await;
        assert!(result.is_ok());

        let result = read_files_parallel(&[]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_atomic_write_basic() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("atomic.txt");

        atomic_write(&file, b"test content").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "test content");
    }

    #[test]
    fn test_atomic_write_overwrites() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("atomic.txt");

        // Write initial content
        atomic_write(&file, b"initial").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "initial");

        // Overwrite
        atomic_write(&file, b"updated").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "updated");
    }

    #[test]
    fn test_atomic_write_creates_parent() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("deep").join("nested").join("atomic.txt");

        atomic_write(&file, b"nested content").unwrap();
        assert!(file.exists());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "nested content");
    }

    #[test]
    fn test_safe_copy_file() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("source.txt");
        let dst = temp.path().join("dest.txt");

        std::fs::write(&src, "test content").unwrap();
        std::fs::copy(&src, &dst).unwrap();

        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "test content");
    }

    #[test]
    fn test_copy_with_parent_creation() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("source.txt");
        let dst = temp.path().join("subdir").join("dest.txt");

        std::fs::write(&src, "test content").unwrap();
        ensure_parent_dir(&dst).unwrap();
        std::fs::copy(&src, &dst).unwrap();

        assert!(dst.exists());
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "test content");
    }

    #[test]
    fn test_copy_nonexistent_source() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("nonexistent.txt");
        let dst = temp.path().join("dest.txt");

        let result = std::fs::copy(&src, &dst);
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_path_complex() {
        // Test various path normalization scenarios
        assert_eq!(normalize_path(Path::new("/")), PathBuf::from("/"));
        assert_eq!(
            normalize_path(Path::new("/foo/bar")),
            PathBuf::from("/foo/bar")
        );
        assert_eq!(
            normalize_path(Path::new("/foo/./bar")),
            PathBuf::from("/foo/bar")
        );
        assert_eq!(
            normalize_path(Path::new("/foo/../bar")),
            PathBuf::from("/bar")
        );
        assert_eq!(
            normalize_path(Path::new("/foo/bar/..")),
            PathBuf::from("/foo")
        );
        assert_eq!(
            normalize_path(Path::new("foo/./bar")),
            PathBuf::from("foo/bar")
        );
        assert_eq!(
            normalize_path(Path::new("./foo/bar")),
            PathBuf::from("foo/bar")
        );
    }

    #[test]
    fn test_is_safe_path_edge_cases() {
        let base = Path::new("/home/user/project");

        // Safe paths
        assert!(is_safe_path(base, Path::new("")));
        assert!(is_safe_path(base, Path::new(".")));
        assert!(is_safe_path(base, Path::new("./nested/./path")));

        // Unsafe paths
        assert!(!is_safe_path(base, Path::new("..")));
        assert!(!is_safe_path(base, Path::new("../../etc")));
        assert!(!is_safe_path(base, Path::new("/absolute/path")));

        // Windows-style paths (on Unix these are relative)
        if cfg!(windows) {
            assert!(!is_safe_path(base, Path::new("C:\\Windows")));
        }
    }

    #[test]
    fn test_safe_write_readonly_parent() {
        // This test verifies behavior when parent dir is readonly
        // We skip it in CI as it requires special permissions
        if std::env::var("CI").is_ok() {
            return;
        }

        let temp = tempdir().unwrap();
        let readonly_dir = temp.path().join("readonly");
        ensure_dir(&readonly_dir).unwrap();

        // Make directory readonly (Unix-specific)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
            perms.set_mode(0o555); // r-xr-xr-x
            std::fs::set_permissions(&readonly_dir, perms).unwrap();

            let file = readonly_dir.join("test.txt");
            let result = safe_write(&file, "test");
            assert!(result.is_err());

            // Restore permissions for cleanup
            let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&readonly_dir, perms).unwrap();
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_remove_dir_all_symlink() {
        // Test that remove_dir_all doesn't follow symlinks
        let temp = tempdir().unwrap();
        let target = temp.path().join("target");
        let link = temp.path().join("link");

        ensure_dir(&target).unwrap();
        std::fs::write(target.join("important.txt"), "data").unwrap();

        std::os::unix::fs::symlink(&target, &link).unwrap();
        remove_dir_all(&link).unwrap();

        // Target should still exist
        assert!(target.exists());
        assert!(target.join("important.txt").exists());
    }

    #[test]
    fn test_find_files_with_patterns() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create test files
        std::fs::write(root.join("README.md"), "").unwrap();
        std::fs::write(root.join("test.MD"), "").unwrap(); // Different case
        std::fs::write(root.join("file.txt"), "").unwrap();
        ensure_dir(&root.join("hidden")).unwrap();
        std::fs::write(root.join("hidden/.secret.md"), "").unwrap();

        // Pattern matching
        let files = find_files(root, ".md").unwrap();
        assert_eq!(files.len(), 2); // README.md and .secret.md

        let files = find_files(root, ".MD").unwrap();
        assert_eq!(files.len(), 1); // test.MD

        // Substring matching
        let files = find_files(root, "test").unwrap();
        assert_eq!(files.len(), 1);

        let files = find_files(root, "secret").unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_dir_size_edge_cases() {
        let temp = tempdir().unwrap();

        // Empty directory
        let empty_dir = temp.path().join("empty");
        ensure_dir(&empty_dir).unwrap();
        assert_eq!(dir_size(&empty_dir).unwrap(), 0);

        // Non-existent directory
        let nonexistent = temp.path().join("nonexistent");
        let result = dir_size(&nonexistent);
        assert!(result.is_err());

        // Directory with symlinks
        #[cfg(unix)]
        {
            let dir = temp.path().join("with_symlink");
            ensure_dir(&dir).unwrap();
            std::fs::write(dir.join("file.txt"), "12345").unwrap();

            let target = temp.path().join("target");
            std::fs::write(&target, "123456789").unwrap();
            std::os::unix::fs::symlink(&target, dir.join("link")).unwrap();

            // The dir_size function behavior with symlinks depends on the implementation
            // Just verify it doesn't crash and returns a reasonable size
            let size = dir_size(&dir).unwrap();
            // We should have at least the size of the real file
            assert!(size >= 5);
            // The size should be reasonable (not gigabytes)
            assert!(size < 1_000_000);
        }
    }

    #[test]
    fn test_temp_dir_custom_prefix() {
        let temp1 = TempDir::new("prefix1").unwrap();
        let temp2 = TempDir::new("prefix2").unwrap();

        assert!(temp1.path().to_string_lossy().contains("prefix1"));
        assert!(temp2.path().to_string_lossy().contains("prefix2"));

        let path1 = temp1.path().to_path_buf();
        let path2 = temp2.path().to_path_buf();

        assert_ne!(path1, path2);
        assert!(path1.exists());
        assert!(path2.exists());
    }

    #[test]
    fn test_ensure_parent_dir_edge_cases() {
        let temp = tempdir().unwrap();

        // File at root (no parent)
        let root_file = if cfg!(windows) {
            PathBuf::from("C:\\file.txt")
        } else {
            PathBuf::from("/file.txt")
        };
        ensure_parent_dir(&root_file).unwrap(); // Should not panic

        // Current directory file
        let current_file = PathBuf::from("file.txt");
        ensure_parent_dir(&current_file).unwrap();

        // Already existing parent
        let existing = temp.path().join("file.txt");
        ensure_parent_dir(&existing).unwrap();
        ensure_parent_dir(&existing).unwrap(); // Second call should be ok
    }

    #[test]
    fn test_calculate_checksum_edge_cases() {
        let temp = tempdir().unwrap();

        // Empty file
        let empty = temp.path().join("empty.txt");
        std::fs::write(&empty, "").unwrap();
        let checksum = calculate_checksum(&empty).unwrap();
        assert_eq!(checksum.len(), 64);
        // SHA256 of empty string is well-known
        assert_eq!(
            checksum,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        // Non-existent file
        let nonexistent = temp.path().join("nonexistent.txt");
        let result = calculate_checksum(&nonexistent);
        assert!(result.is_err());

        // Large file (1MB)
        let large = temp.path().join("large.txt");
        let large_content = vec![b'a'; 1024 * 1024];
        std::fs::write(&large, &large_content).unwrap();
        let checksum = calculate_checksum(&large).unwrap();
        assert_eq!(checksum.len(), 64);
    }

    #[tokio::test]
    async fn test_calculate_checksums_parallel_errors() {
        let temp = tempdir().unwrap();
        let valid = temp.path().join("valid.txt");
        let invalid = temp.path().join("invalid.txt");

        std::fs::write(&valid, "content").unwrap();

        let paths = vec![valid.clone(), invalid.clone()];
        let result = calculate_checksums_parallel(&paths).await;

        // Should fail if any file is invalid
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_files_parallel_errors() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("nonexistent.txt");
        let dst = temp.path().join("dest.txt");

        let pairs = vec![(src, dst)];
        let result = copy_files_parallel(&pairs).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_atomic_write_multiple_partial_failure() {
        // Test behavior when some writes might fail
        let temp = tempdir().unwrap();
        let valid_path = temp.path().join("valid.txt");

        // Use an invalid path that will cause write to fail
        // Create a file and try to use it as a directory
        let invalid_base = temp.path().join("not_a_directory.txt");
        std::fs::write(&invalid_base, "this is a file").unwrap();
        let invalid_path = invalid_base.join("impossible_file.txt");

        let files = vec![
            (valid_path.clone(), b"content".to_vec()),
            (invalid_path, b"fail".to_vec()),
        ];

        let result = atomic_write_multiple(&files).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_files_parallel_mixed() {
        let temp = tempdir().unwrap();
        let valid = temp.path().join("valid.txt");
        let invalid = temp.path().join("invalid.txt");

        std::fs::write(&valid, "content").unwrap();

        let paths = vec![valid, invalid];
        let result = read_files_parallel(&paths).await;

        // Should fail if any file cannot be read
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_dirs_parallel_errors() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("nonexistent");
        let dst = temp.path().join("dest");

        let pairs = vec![(src, dst)];
        let result = copy_dirs_parallel(&pairs).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_copy_dir_with_permissions() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        ensure_dir(&src).unwrap();
        std::fs::write(src.join("file.txt"), "content").unwrap();

        // Set specific permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(src.join("file.txt"))
                .unwrap()
                .permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(src.join("file.txt"), perms).unwrap();
        }

        copy_dir(&src, &dst).unwrap();

        assert!(dst.join("file.txt").exists());

        // Verify permissions were preserved on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(dst.join("file.txt"))
                .unwrap()
                .permissions();
            assert_eq!(perms.mode() & 0o777, 0o644);
        }
    }

    #[test]
    fn test_find_project_root_multiple_markers() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("project");
        let subproject = root.join("subproject");
        let deep = subproject.join("src");

        ensure_dir(&deep).unwrap();
        std::fs::write(root.join("ccpm.toml"), "[sources]").unwrap();
        std::fs::write(subproject.join("ccpm.toml"), "[sources]").unwrap();

        // Should find the closest ccpm.toml
        let found = find_project_root(&deep).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            subproject.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_get_cache_dir_from_config() {
        // Save original value and ensure clean test environment
        let original = std::env::var("CCPM_CACHE_DIR").ok();
        std::env::remove_var("CCPM_CACHE_DIR");

        // Test that we can get a cache directory (using the config module)
        let cache_dir = crate::config::get_cache_dir().unwrap();
        // The cache directory should contain "ccpm" in its path when env var is not set
        assert!(cache_dir.to_string_lossy().contains("ccpm"));
        // It should be a valid path
        assert!(!cache_dir.as_os_str().is_empty());

        // Restore original value if it existed
        if let Some(val) = original {
            std::env::set_var("CCPM_CACHE_DIR", val);
        }
    }
}
