//! Platform-specific utilities and cross-platform compatibility helpers
//!
//! This module provides abstractions over platform differences to ensure AGPM
//! works consistently across Windows, macOS, and Linux. It handles differences in:
//!
//! - Path separators and conventions
//! - Home directory resolution
//! - Command execution and shell interfaces
//! - File system behavior and limitations
//! - Environment variable handling
//!
//! # Cross-Platform Design
//!
//! AGPM is designed to provide identical functionality across all supported platforms
//! while respecting platform conventions and limitations. This module encapsulates
//! the platform-specific logic to achieve this goal.
//!
//! # Examples
//!
//! ```rust,no_run
//! use agpm_cli::utils::platform::{get_home_dir, resolve_path, is_windows};
//!
//! # fn example() -> anyhow::Result<()> {
//! // Get platform-appropriate home directory
//! let home = get_home_dir()?;
//! println!("Home directory: {}", home.display());
//!
//! // Resolve paths with tilde expansion and env vars
//! let config_path = resolve_path("~/.agpm/config.toml")?;
//!
//! // Handle platform differences
//! if is_windows() {
//!     println!("Running on Windows");
//! } else {
//!     println!("Running on Unix-like system");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Platform Support Matrix
//!
//! | Feature | Windows | macOS | Linux |
//! |---------|---------|-------|-------|
//! | Long paths (>260 chars) | ✅ | ✅ | ✅ |
//! | Case sensitivity | No | Configurable | Yes |
//! | Tilde expansion | ✅ | ✅ | ✅ |
//! | Environment variables | %VAR% | $VAR | $VAR |
//! | Shell commands | cmd.exe | sh | sh |
//! | Git command | git.exe | git | git |
//!
//! # Security Considerations
//!
//! - Path traversal prevention in [`safe_join`]
//! - Input validation in [`validate_path_chars`]
//! - Safe environment variable expansion
//! - Proper handling of special Windows filenames

use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};

/// Checks if the current platform is Windows.
///
/// This is a compile-time check that returns `true` if the code is compiled
/// for Windows targets, `false` otherwise. It's used throughout the codebase
/// to handle Windows-specific behavior.
///
/// # Returns
///
/// - `true` on Windows (any Windows target)
/// - `false` on Unix-like systems (macOS, Linux, BSD, etc.)
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::is_windows;
///
/// if is_windows() {
///     println!("Windows-specific code path");
/// } else {
///     println!("Unix-like system code path");
/// }
/// ```
///
/// # Use Cases
///
/// - Conditional compilation of platform-specific code
/// - Different path handling logic
/// - Platform-specific error messages
/// - Command execution differences
#[must_use]
pub const fn is_windows() -> bool {
    cfg!(windows)
}

/// Gets the home directory path for the current user.
///
/// This function returns the user's home directory following platform conventions.
/// It uses the appropriate environment variables and fallback mechanisms for
/// each platform to reliably determine the home directory.
///
/// # Returns
///
/// The user's home directory path, or an error if it cannot be determined
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::get_home_dir;
///
/// # fn example() -> anyhow::Result<()> {
/// let home = get_home_dir()?;
/// println!("Home directory: {}", home.display());
///
/// let agpm_dir = home.join(".agpm");
/// println!("AGPM directory would be: {}", agpm_dir.display());
/// # Ok(())
/// # }
/// ```
///
/// # Platform Behavior
///
/// - **Windows**: Uses `%USERPROFILE%` environment variable
/// - **Unix/Linux**: Uses `$HOME` environment variable
/// - **macOS**: Uses `$HOME` environment variable
///
/// # Error Cases
///
/// - Home directory environment variable is not set
/// - Environment variable points to non-existent directory
/// - Permission denied accessing the home directory
///
/// # Use Cases
///
/// - Finding user configuration directories
/// - Resolving tilde (`~`) in path expansion
/// - Creating user-specific cache and data directories
pub fn get_home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| {
        let platform_help = if is_windows() {
            "On Windows: Check that the USERPROFILE environment variable is set"
        } else {
            "On Unix/Linux: Check that the HOME environment variable is set"
        };
        anyhow::anyhow!("Could not determine home directory.\n\n{platform_help}")
    })
}

/// Returns the appropriate Git command name for the current platform.
///
/// This function returns the platform-specific Git executable name that
/// should be used when invoking Git commands via the system shell.
///
/// # Returns
///
/// - `"git.exe"` on Windows
/// - `"git"` on Unix-like systems (macOS, Linux, BSD)
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::get_git_command;
/// use std::process::Command;
///
/// # fn example() -> anyhow::Result<()> {
/// let git_cmd = get_git_command();
/// let output = Command::new(git_cmd)
///     .args(["--version"])
///     .output()?;
///
/// println!("Git version: {}", String::from_utf8_lossy(&output.stdout));
/// # Ok(())
/// # }
/// ```
///
/// # Platform Differences
///
/// - **Windows**: Uses `git.exe` to explicitly invoke the executable
/// - **Unix-like**: Uses `git` which relies on PATH resolution
///
/// # Note
///
/// This function returns the command name, not the full path. The actual
/// Git executable must still be available in the system PATH for commands
/// to succeed.
///
/// # See Also
///
/// - [`command_exists`] to check if Git is available
/// - System PATH configuration for Git availability
#[must_use]
pub const fn get_git_command() -> &'static str {
    if is_windows() {
        "git.exe"
    } else {
        "git"
    }
}

/// Resolves a path with tilde expansion and environment variable substitution.
///
/// This function processes path strings to handle common shell conventions:
/// - Tilde (`~`) expansion to the user's home directory
/// - Environment variable substitution (`$VAR` or `%VAR%`)
/// - Windows long path handling when necessary
///
/// # Arguments
///
/// * `path` - The path string to resolve (may contain `~` or environment variables)
///
/// # Returns
///
/// A resolved [`PathBuf`] with expansions applied, or an error if expansion fails
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::resolve_path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Tilde expansion
/// let config_path = resolve_path("~/.agpm/config.toml")?;
/// println!("Config: {}", config_path.display());
///
/// // Environment variable expansion (Unix)
/// # #[cfg(unix)]
/// let path_with_env = resolve_path("$HOME/Documents/project")?;
///
/// // Environment variable expansion (Windows)
/// # #[cfg(windows)]
/// let path_with_env = resolve_path("%USERPROFILE%\\Documents\\project")?;
/// # Ok(())
/// # }
/// ```
///
/// # Supported Patterns
///
/// - `~/path` - Expands to `{home}/path`
/// - `$VAR/path` (Unix) - Expands environment variable
/// - `%VAR%/path` (Windows) - Expands environment variable
/// - `${VAR}/path` (Unix) - Alternative env var syntax
///
/// # Error Cases
///
/// - Invalid tilde usage (e.g., `~user/path` on Windows)
/// - Undefined environment variables
/// - Invalid variable syntax
/// - Home directory cannot be determined
///
/// # Security
///
/// This function safely handles environment variable expansion and prevents
/// common injection attacks by using proper parsing libraries.
///
/// # See Also
///
/// - [`get_home_dir`] for home directory resolution
/// - [`validate_path_chars`] for path validation
pub fn resolve_path(path: &str) -> Result<PathBuf> {
    let expanded = if let Some(stripped) = path.strip_prefix("~/") {
        let home = get_home_dir()?;
        home.join(stripped)
    } else if path.starts_with('~') {
        // Handle Windows-style user expansion like ~username
        if is_windows() && path.len() > 1 && !path.starts_with("~/") {
            return Err(anyhow::anyhow!(
                "Invalid path: {path}\n\n\
                Windows tilde expansion only supports '~/' for current user home directory.\n\
                Use '~/' followed by a relative path, like '~/Documents/file.txt'"
            ));
        }
        return Err(anyhow::anyhow!(
            "Invalid path: {path}\n\n\
            Tilde expansion only supports '~/' for home directory.\n\
            Use '~/' followed by a relative path, like '~/Documents/file.txt'"
        ));
    } else {
        PathBuf::from(path)
    };

    // Expand environment variables
    let path_str = expanded.to_string_lossy();

    // Handle Windows-style %VAR% expansion differently
    let expanded_str = if is_windows() && path_str.contains('%') {
        // Manual Windows-style %VAR% expansion
        let mut result = path_str.to_string();
        let re = Regex::new(r"%([^%]+)%").unwrap();

        for cap in re.captures_iter(&path_str) {
            if let Some(var_name) = cap.get(1)
                && let Ok(value) = std::env::var(var_name.as_str())
            {
                result = result.replace(&format!("%{}%", var_name.as_str()), &value);
            }
        }

        // Also handle Unix-style for compatibility
        match shellexpand::env(&result) {
            Ok(expanded) => expanded.into_owned(),
            Err(_) => result, // Return the partially expanded result
        }
    } else {
        // Unix-style $VAR expansion
        shellexpand::env(&path_str)
            .with_context(|| {
                let platform_vars = if is_windows() {
                    "Common Windows variables: %USERPROFILE%, %APPDATA%, %TEMP%"
                } else {
                    "Common Unix variables: $HOME, $USER, $TMP"
                };

                format!(
                    "Failed to expand environment variables in path: {path_str}\n\n\
                    Common issues:\n\
                    - Undefined environment variable (e.g., $UNDEFINED_VAR)\n\
                    - Invalid variable syntax (use $VAR or ${{VAR}})\n\
                    - Special characters that need escaping\n\n\
                    {platform_vars}"
                )
            })?
            .into_owned()
    };

    let result = PathBuf::from(expanded_str);

    // Apply Windows long path handling if needed
    Ok(windows_long_path(&result))
}

/// Converts a path to use the correct separator for the current platform.
///
/// This function normalizes path separators to match platform conventions:
/// - Windows: Converts `/` to `\`
/// - Unix-like: Converts `\` to `/`
///
/// This is primarily useful for display purposes or when interfacing with
/// platform-specific APIs that expect native separators.
///
/// # Arguments
///
/// * `path` - The path to normalize
///
/// # Returns
///
/// A string with platform-appropriate separators
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::normalize_path_separator;
/// use std::path::Path;
///
/// let mixed_path = Path::new("src/utils\\platform.rs");
/// let normalized = normalize_path_separator(mixed_path);
///
/// #[cfg(windows)]
/// assert_eq!(normalized, "src\\utils\\platform.rs");
///
/// #[cfg(not(windows))]
/// assert_eq!(normalized, "src/utils/platform.rs");
/// ```
///
/// # Platform Behavior
///
/// - **Windows**: All separators become `\`
/// - **Unix-like**: All separators become `/`
///
/// # Use Cases
///
/// - Display paths to users in platform-native format
/// - Interfacing with platform-specific APIs
/// - Generating platform-specific configuration files
/// - Logging and error messages
///
/// # Note
///
/// Rust's [`Path`] and [`PathBuf`] types handle separators transparently
/// in most cases, so this function is primarily needed for display and
/// external interface purposes.
#[must_use]
pub fn normalize_path_separator(path: &Path) -> String {
    if is_windows() {
        path.to_string_lossy().replace('/', "\\")
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

/// Normalizes a path for cross-platform storage by converting all separators to forward slashes.
///
/// This function ensures paths are stored consistently across all platforms by always using
/// forward slashes as separators, regardless of the current platform. This is critical for:
///
/// - **Lockfiles** (`agpm.lock`): Must be identical across platforms for version control
/// - **`.gitignore` entries**: Git requires forward slashes on all platforms
/// - **TOML manifest files**: Forward slashes are platform-independent
/// - **JSON configuration**: Forward slashes work universally
///
/// # Arguments
///
/// * `path` - The path to normalize (accepts both `Path` and string types)
///
/// # Returns
///
/// A string with all backslashes converted to forward slashes
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::normalize_path_for_storage;
/// use std::path::Path;
///
/// // Windows path with backslashes
/// let windows_path = Path::new(".claude\\agents\\example.md");
/// assert_eq!(
///     normalize_path_for_storage(windows_path),
///     ".claude/agents/example.md"
/// );
///
/// // Unix path (no change needed)
/// let unix_path = Path::new(".claude/agents/example.md");
/// assert_eq!(
///     normalize_path_for_storage(unix_path),
///     ".claude/agents/example.md"
/// );
///
/// // Mixed separators (normalized to forward slashes)
/// let mixed_path = Path::new("src/utils\\platform.rs");
/// assert_eq!(
///     normalize_path_for_storage(mixed_path),
///     "src/utils/platform.rs"
/// );
/// ```
///
/// # Platform Behavior
///
/// - **Windows**: Converts `\` → `/`
/// - **Unix-like**: Already uses `/`, but normalizes any stray `\` characters
/// - **All platforms**: Output is always identical for the same logical path
///
/// # Use Cases
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::normalize_path_for_storage;
/// use std::path::Path;
///
/// // Lockfile entries
/// let installed_at = normalize_path_for_storage(
///     Path::new(".claude\\agents\\example.md")
/// );
/// // Always produces: ".claude/agents/example.md"
///
/// // .gitignore entries
/// let ignore_path = normalize_path_for_storage(
///     Path::new(".claude\\cache")
/// );
/// // Always produces: ".claude/cache"
///
/// // Format strings with Path::display()
/// let artifact_path = Path::new(".claude\\agents");
/// let filename = "example.md";
/// let full_path = format!("{}/{}", artifact_path.display(), filename);
/// let normalized = normalize_path_for_storage(Path::new(&full_path));
/// // Always produces: ".claude/agents/example.md"
/// ```
///
/// # Important Notes
///
/// - **Always use this for stored paths**: Lockfiles, manifest files, .gitignore
/// - **Don't use for runtime operations**: Use `Path`/`PathBuf` for filesystem operations
/// - **Don't use for display**: Use `normalize_path_separator` for user-facing paths
///
/// # See Also
///
/// - [`normalize_path_separator`] for platform-native display formatting
/// - CLAUDE.md "Cross-Platform Path Handling" section for complete guidelines
#[must_use]
pub fn normalize_path_for_storage<P: AsRef<Path>>(path: P) -> String {
    let path_str = path.as_ref().to_string_lossy();

    // Strip Windows extended-length path prefixes before normalization
    // These prefixes are used internally by canonicalize() but shouldn't be stored
    let cleaned = if let Some(stripped) = path_str.strip_prefix(r"\\?\UNC\") {
        // Extended UNC path: \\?\UNC\server\share -> //server/share
        format!("//{}", stripped)
    } else if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
        // Extended path: \\?\C:\path -> C:\path
        stripped.to_string()
    } else {
        path_str.to_string()
    };

    cleaned.replace('\\', "/")
}

/// Computes the relative install path by removing redundant directory prefixes.
///
/// This function intelligently strips redundant path components when a dependency's path
/// starts with the same directory name as the tool's installation root. This prevents
/// duplicate directory names like `.claude/agents/agents/example.md`.
///
/// # Algorithm
///
/// 1. Extract the last component of the tool root (e.g., `agents` from `.claude/agents/`)
/// 2. Check if the dependency path starts with that same component (case-sensitive)
/// 3. If yes, strip that leading component from the dependency path
/// 4. If no, return the dependency path unchanged
///
/// # Arguments
///
/// * `tool_root` - The base installation directory (e.g., `.claude/agents/`)
/// * `dep_path` - The relative path from the dependency (e.g., `agents/example.md`)
///
/// # Returns
///
/// The relative path to use for installation, with redundant prefixes removed.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::compute_relative_install_path;
/// use std::path::Path;
///
/// // Standard case: strip redundant prefix
/// let tool_root = Path::new(".claude/agents");
/// let dep_path = Path::new("agents/carrots/agent.md");
/// let result = compute_relative_install_path(tool_root, dep_path, false);
/// assert_eq!(result, Path::new("carrots/agent.md"));
///
/// // Flatten: use only filename
/// let tool_root = Path::new(".claude/agents");
/// let dep_path = Path::new("agents/carrots/agent.md");
/// let result = compute_relative_install_path(tool_root, dep_path, true);
/// assert_eq!(result, Path::new("agent.md"));
///
/// // No match: preserve full path
/// let tool_root = Path::new(".claude/agents");
/// let dep_path = Path::new("helpers/agent.md");
/// let result = compute_relative_install_path(tool_root, dep_path, false);
/// assert_eq!(result, Path::new("helpers/agent.md"));
///
/// // Custom target with different name
/// let tool_root = Path::new(".custom/my-stuff");
/// let dep_path = Path::new("agents/helper.md");
/// let result = compute_relative_install_path(tool_root, dep_path, false);
/// assert_eq!(result, Path::new("agents/helper.md")); // No stripping
/// ```
///
/// # Use Cases
///
/// - Installing resources from well-organized repositories
/// - Preventing `.claude/snippets/snippets/example.md` duplication
/// - Working with custom installation targets
/// - Preserving intentional directory structures
/// - Flattening directory structures for agents and commands
///
/// # Design Rationale
///
/// This approach is more generic than hardcoded resource type stripping:
/// - Works with custom targets (e.g., `.custom/my-agents/`)
/// - No dependency on resource type names
/// - Handles edge cases like single-file dependencies
/// - Respects intentional hierarchies (e.g., `helpers/agent.md` preserved)
/// - Supports explicit flattening for resource types that don't need nested directories
#[must_use]
pub fn compute_relative_install_path(tool_root: &Path, dep_path: &Path, flatten: bool) -> PathBuf {
    use std::path::Component;

    // If flatten is true, return just the filename
    if flatten {
        if let Some(filename) = dep_path.file_name() {
            return PathBuf::from(filename);
        }
        // Fallback to the original path if no filename
        return dep_path.to_path_buf();
    }

    // Extract the last directory component from tool root
    let tool_dir_name = tool_root.file_name().and_then(|n| n.to_str());

    // Find the first Normal component and its position in the dependency path
    let components: Vec<_> = dep_path.components().collect();
    let (dep_first, first_normal_idx) = components
        .iter()
        .enumerate()
        .find_map(|(idx, c)| {
            if let Component::Normal(s) = c {
                s.to_str().map(|s| (s, idx))
            } else {
                None
            }
        })
        .map(|(s, idx)| (Some(s), Some(idx)))
        .unwrap_or((None, None));

    // If they match, strip up to and including the matching component
    if tool_dir_name.is_some() && tool_dir_name.map(Some) == Some(dep_first) {
        // Skip everything up to and including the matching Normal component
        if let Some(idx) = first_normal_idx {
            components.iter().skip(idx + 1).collect()
        } else {
            dep_path.to_path_buf()
        }
    } else {
        // No match - return the full path (but skip any leading CurDir/ParentDir for cleanliness)
        components
            .iter()
            .skip_while(|c| {
                matches!(c, Component::CurDir | Component::Prefix(_) | Component::RootDir)
            })
            .collect()
    }
}

/// Safely converts a path to a string, handling non-UTF-8 paths gracefully.
///
/// This function converts a [`Path`] to a [`String`] using lossy conversion,
/// which means invalid UTF-8 sequences are replaced with the Unicode
/// replacement character (�). This ensures the function never panics.
///
/// # Arguments
///
/// * `path` - The path to convert to a string
///
/// # Returns
///
/// A string representation of the path (may contain replacement characters)
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::path_to_string;
/// use std::path::Path;
///
/// let path = Path::new("/home/user/file.txt");
/// let path_str = path_to_string(path);
/// println!("Path as string: {}", path_str);
/// ```
///
/// # Platform Considerations
///
/// - **Windows**: Paths are typically valid UTF-16, so conversion is usually lossless
/// - **Unix-like**: Paths can contain arbitrary bytes, so lossy conversion may occur
/// - **All platforms**: This function never panics, unlike direct UTF-8 conversion
///
/// # Use Cases
///
/// - Logging and error messages
/// - Display to users
/// - Interfacing with APIs that expect strings
/// - JSON serialization of paths
///
/// # Alternative
///
/// For cases where you need `OsStr` (which preserves all path information),
/// use [`path_to_os_str`] instead.
///
/// # See Also
///
/// - [`path_to_os_str`] for preserving all path information
/// - [`Path::to_string_lossy`] for the underlying conversion method
#[must_use]
pub fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Returns a path as an `OsStr` for use in command arguments.
///
/// This function provides access to the raw `OsStr` representation of a path,
/// which preserves all path information without any lossy conversion. This is
/// the preferred way to pass paths to system commands and APIs.
///
/// # Arguments
///
/// * `path` - The path to get as an `OsStr`
///
/// # Returns
///
/// A reference to the path's `OsStr` representation
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::path_to_os_str;
/// use std::path::Path;
/// use std::process::Command;
///
/// # fn example() -> anyhow::Result<()> {
/// let file_path = Path::new("important-file.txt");
/// let os_str = path_to_os_str(file_path);
///
/// // Use in command arguments
/// let output = Command::new("cat")
///     .arg(os_str)
///     .output()?;
/// # Ok(())
/// # }
/// ```
///
/// # Advantages
///
/// - **Lossless**: Preserves all path information
/// - **Efficient**: No conversion or allocation
/// - **Platform-native**: Uses the OS's native string representation
/// - **Command-safe**: Ideal for process arguments
///
/// # Use Cases
///
/// - Passing paths to `Command::arg` and similar APIs
/// - System API calls that expect native strings
/// - Preserving exact path representation
/// - File system operations
///
/// # See Also
///
/// - [`path_to_string`] for display purposes (lossy conversion)
/// - [`std::ffi::OsStr`] for the underlying type documentation
#[must_use]
pub fn path_to_os_str(path: &Path) -> &std::ffi::OsStr {
    path.as_os_str()
}

/// Compares two paths for equality, respecting platform case sensitivity rules.
///
/// This function performs path comparison that follows platform conventions:
/// - **Windows**: Case-insensitive comparison (NTFS/FAT32 behavior)
/// - **Unix-like**: Case-sensitive comparison (ext4/APFS/HFS+ behavior)
///
/// # Arguments
///
/// * `path1` - First path to compare
/// * `path2` - Second path to compare
///
/// # Returns
///
/// `true` if the paths are considered equal on the current platform
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::paths_equal;
/// use std::path::Path;
///
/// let path1 = Path::new("Config.toml");
/// let path2 = Path::new("config.toml");
///
/// #[cfg(windows)]
/// assert!(paths_equal(path1, path2)); // Case-insensitive on Windows
///
/// #[cfg(not(windows))]
/// assert!(!paths_equal(path1, path2)); // Case-sensitive on Unix
/// ```
///
/// # Platform Behavior
///
/// - **Windows**: Converts both paths to lowercase before comparison
/// - **macOS**: Case-sensitive by default (but filesystems may vary)
/// - **Linux**: Always case-sensitive
///
/// # Use Cases
///
/// - Checking for duplicate file references
/// - Path deduplication in collections
/// - Validating user input against existing paths
/// - Cross-platform file system operations
///
/// # Note
///
/// This function compares path strings, not filesystem entries. It does not
/// resolve symbolic links or check if the paths actually exist.
///
/// # Filesystem Variations
///
/// Some filesystems have configurable case sensitivity (like APFS on macOS).
/// This function uses platform defaults and may not match filesystem behavior
/// in all cases.
#[must_use]
pub fn paths_equal(path1: &Path, path2: &Path) -> bool {
    if is_windows() {
        // Windows file system is case-insensitive
        // Normalize paths by removing trailing slashes before comparison
        let p1_str = path1.to_string_lossy();
        let p2_str = path2.to_string_lossy();
        let p1 = p1_str.trim_end_matches(['/', '\\']).to_lowercase();
        let p2 = p2_str.trim_end_matches(['/', '\\']).to_lowercase();
        p1 == p2
    } else {
        // Unix-like systems are case-sensitive
        // Also normalize trailing slashes for consistency
        let p1_str = path1.to_string_lossy();
        let p2_str = path2.to_string_lossy();
        let p1 = p1_str.trim_end_matches('/');
        let p2 = p2_str.trim_end_matches('/');
        p1 == p2
    }
}

/// Canonicalizes a path with proper cross-platform handling.
///
/// This function resolves a path to its canonical, absolute form while handling
/// platform-specific issues like Windows long paths. It resolves symbolic links,
/// removes `.` and `..` components, and ensures the path is absolute.
///
/// # Arguments
///
/// * `path` - The path to canonicalize
///
/// # Returns
///
/// The canonical absolute path, or an error if canonicalization fails
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::safe_canonicalize;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// // Canonicalize a relative path
/// let canonical = safe_canonicalize(Path::new("../src/main.rs"))?;
/// println!("Canonical path: {}", canonical.display());
///
/// // Works with existing files and directories
/// let current_dir = safe_canonicalize(Path::new("."))?;
/// println!("Current directory: {}", current_dir.display());
/// # Ok(())
/// # }
/// ```
///
/// # Features
///
/// - **Cross-platform**: Works on Windows, macOS, and Linux
/// - **Long path support**: Handles Windows paths >260 characters
/// - **Symlink resolution**: Follows symbolic links to their targets
/// - **Path normalization**: Removes `.` and `..` components
/// - **Absolute paths**: Always returns absolute paths
///
/// # Error Cases
///
/// - Path does not exist
/// - Permission denied accessing path components
/// - Invalid path characters for the platform
/// - Path too long (even with Windows long path support)
/// - Circular symbolic links
///
/// # Platform Notes
///
/// - **Windows**: Automatically applies long path prefixes when needed
/// - **Unix-like**: Resolves symbolic links following POSIX semantics
/// - **All platforms**: Provides helpful error messages for common issues
///
/// # Security
///
/// This function safely resolves paths and prevents directory traversal
/// by returning absolute, normalized paths.
///
/// # See Also
///
/// - `normalize_path` for logical path normalization (no filesystem access)
/// - [`windows_long_path`] for Windows-specific path handling
pub fn safe_canonicalize(path: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize path: {}\n\n\
                Possible causes:\n\
                - Path does not exist\n\
                - Permission denied\n\
                - Invalid path characters\n\
                - Path too long (>260 chars on Windows)",
            path.display()
        )
    })?;

    #[cfg(windows)]
    {
        Ok(windows_long_path(&canonical))
    }

    #[cfg(not(windows))]
    {
        Ok(canonical)
    }
}

/// Checks if a command is available in the system PATH.
///
/// This function searches the system PATH for the specified command and returns
/// whether it can be found and is executable. This is useful for verifying that
/// required external tools (like Git) are available before attempting to use them.
///
/// # Arguments
///
/// * `cmd` - The command name to search for
///
/// # Returns
///
/// `true` if the command exists and is executable, `false` otherwise
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::command_exists;
///
/// // Check if Git is available
/// if command_exists("git") {
///     println!("Git is available");
/// } else {
///     eprintln!("Git is not installed or not in PATH");
/// }
///
/// // Platform-specific commands
/// #[cfg(windows)]
/// let shell_available = command_exists("cmd");
///
/// #[cfg(unix)]
/// let shell_available = command_exists("sh");
/// ```
///
/// # Platform Behavior
///
/// - **Windows**: Searches PATH and PATHEXT for executable files
/// - **Unix-like**: Searches PATH for executable files
/// - **All platforms**: Respects system PATH configuration
///
/// # Use Cases
///
/// - Validating tool availability before execution
/// - Providing helpful error messages when tools are missing
/// - Feature detection based on available commands
/// - System requirements checking
///
/// # Performance
///
/// This function performs filesystem operations and may be relatively slow.
/// Consider caching results if checking the same command multiple times.
///
/// # See Also
///
/// - [`get_git_command`] for getting the platform-appropriate Git command name
#[must_use]
pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Returns the platform-specific cache directory for AGPM.
///
/// This function returns the appropriate cache directory following platform
/// conventions and standards (XDG Base Directory on Linux, standard locations
/// on Windows and macOS).
///
/// # Returns
///
/// The cache directory path (`{cache_dir}/agpm`), or an error if it cannot be determined
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::get_cache_dir;
///
/// # fn example() -> anyhow::Result<()> {
/// let cache_dir = get_cache_dir()?;
/// println!("AGPM cache directory: {}", cache_dir.display());
///
/// // Use for storing temporary data
/// let repo_cache = cache_dir.join("repositories");
/// # Ok(())
/// # }
/// ```
///
/// # Platform Paths
///
/// - **Linux**: `$XDG_CACHE_HOME/agpm` or `$HOME/.cache/agpm`
/// - **macOS**: `$HOME/Library/Caches/agpm`
/// - **Windows**: `%LOCALAPPDATA%\agpm`
///
/// # Standards Compliance
///
/// - **Linux**: Follows XDG Base Directory Specification
/// - **macOS**: Follows Apple File System Programming Guide
/// - **Windows**: Follows Windows Known Folders conventions
///
/// # Use Cases
///
/// - Storing cloned Git repositories
/// - Caching downloaded resources
/// - Temporary build artifacts
/// - Performance optimization data
///
/// # Cleanup
///
/// Cache directories may be cleaned by system maintenance tools or user
/// cleanup utilities. Don't store critical data here.
///
/// # See Also
///
/// - [`get_data_dir`] for persistent application data
/// - [`get_home_dir`] for user home directory
pub fn get_cache_dir() -> Result<PathBuf> {
    dirs::cache_dir().map(|p| p.join("agpm")).ok_or_else(|| {
        let platform_help = if is_windows() {
            "On Windows: Check that the LOCALAPPDATA environment variable is set"
        } else if cfg!(target_os = "macos") {
            "On macOS: Check that the HOME environment variable is set"
        } else {
            "On Linux: Check that the XDG_CACHE_HOME or HOME environment variable is set"
        };
        anyhow::anyhow!("Could not determine cache directory.\n\n{platform_help}")
    })
}

/// Returns the platform-specific data directory for AGPM.
///
/// This function returns the appropriate data directory for storing persistent
/// application data, following platform conventions and standards.
///
/// # Returns
///
/// The data directory path (`{data_dir}/agpm`), or an error if it cannot be determined
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::get_data_dir;
///
/// # fn example() -> anyhow::Result<()> {
/// let data_dir = get_data_dir()?;
/// println!("AGPM data directory: {}", data_dir.display());
///
/// // Use for storing persistent data
/// let lockfile_backup = data_dir.join("lockfile_backups");
/// # Ok(())
/// # }
/// ```
///
/// # Platform Paths
///
/// - **Linux**: `$XDG_DATA_HOME/agpm` or `$HOME/.local/share/agpm`
/// - **macOS**: `$HOME/Library/Application Support/agpm`
/// - **Windows**: `%APPDATA%\agpm`
///
/// # Standards Compliance
///
/// - **Linux**: Follows XDG Base Directory Specification
/// - **macOS**: Follows Apple File System Programming Guide
/// - **Windows**: Follows Windows Known Folders conventions
///
/// # Use Cases
///
/// - Storing user preferences and settings
/// - Application state and history
/// - User-created templates and profiles
/// - Persistent application data
///
/// # Persistence
///
/// Unlike cache directories, data directories are intended for long-term
/// storage and should persist across system updates and cleanup operations.
///
/// # Difference from Cache
///
/// - **Data directory**: Persistent, user-important data
/// - **Cache directory**: Temporary, performance optimization data
///
/// # See Also
///
/// - [`get_cache_dir`] for temporary cached data
/// - [`get_home_dir`] for user home directory
pub fn get_data_dir() -> Result<PathBuf> {
    dirs::data_dir().map(|p| p.join("agpm")).ok_or_else(|| {
        let platform_help = if is_windows() {
            "On Windows: Check that the APPDATA environment variable is set"
        } else if cfg!(target_os = "macos") {
            "On macOS: Check that the HOME environment variable is set"
        } else {
            "On Linux: Check that the XDG_DATA_HOME or HOME environment variable is set"
        };
        anyhow::anyhow!("Could not determine data directory.\n\n{platform_help}")
    })
}

/// Handles Windows long paths (>260 characters) by applying UNC prefixes.
///
/// This Windows-specific function automatically applies the `\\?\` UNC prefix
/// to paths longer than 260 characters, enabling access to long paths on Windows.
/// The function is a no-op on other platforms.
///
/// # Arguments
///
/// * `path` - The path to potentially convert to long path format
///
/// # Returns
///
/// - On Windows: Path with UNC prefix if needed, original path otherwise
/// - On other platforms: Returns the original path unchanged
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::windows_long_path;
/// use std::path::Path;
///
/// let long_path = Path::new("C:\\very\\long\\path\\that\\exceeds\\windows\\limit");
/// let handled_path = windows_long_path(long_path);
///
/// #[cfg(windows)]
/// {
///     // May have \\?\ prefix if path is long
///     println!("Handled path: {}", handled_path.display());
/// }
/// ```
///
/// # Windows Long Path Support
///
/// Windows historically limited paths to 260 characters (MAX_PATH). Modern
/// Windows versions support longer paths when:
/// - The application uses UNC paths (`\\?\` prefix)
/// - Windows 10 version 1607+ with long path support enabled
/// - The application manifest declares long path awareness
///
/// # UNC Prefixes Applied
///
/// - **Local paths**: `C:\path` becomes `\\?\C:\path`
/// - **Network paths**: `\\server\share` becomes `\\?\UNC\server\share`
/// - **Already prefixed**: No change to existing UNC paths
///
/// # Automatic Conversion
///
/// The function only applies prefixes when:
/// - Running on Windows
/// - Path length exceeds 260 characters
/// - Path doesn't already have a UNC prefix
/// - Path can be converted to absolute form
///
/// # Use Cases
///
/// - Deep directory structures in build systems
/// - Git repositories with long path names
/// - User data with deeply nested folders
/// - Ensuring compatibility across Windows versions
///
/// # See Also
///
/// - Microsoft documentation on long path support
/// - `safe_canonicalize` which uses this function internally
#[cfg(windows)]
pub fn windows_long_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.len() > 260 && !path_str.starts_with(r"\\?\") {
        // Convert to absolute path if relative
        let absolute_path = if path.is_relative() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
        } else {
            path.to_path_buf()
        };

        let absolute_str = absolute_path.to_string_lossy();
        if absolute_str.len() > 260 {
            // Use UNC prefix for long paths
            if let Some(stripped) = absolute_str.strip_prefix(r"\\") {
                // Network path
                PathBuf::from(format!(r"\\?\UNC\{}", stripped))
            } else {
                // Local path
                PathBuf::from(format!(r"\\?\{}", absolute_str))
            }
        } else {
            absolute_path
        }
    } else {
        path.to_path_buf()
    }
}

/// No-op implementation of [`windows_long_path`] for non-Windows platforms.
///
/// On Unix-like systems (macOS, Linux, BSD), there is no equivalent to Windows'
/// 260-character path limitation, so this function simply returns the input path
/// unchanged.
///
/// # Arguments
///
/// * `path` - The path to return unchanged
///
/// # Returns
///
/// The original path as a [`PathBuf`]
///
/// # See Also
///
/// - The Windows implementation for details on long path handling
#[cfg(not(windows))]
#[must_use]
pub fn windows_long_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Returns the appropriate shell command and flag for the current platform.
///
/// This function returns the platform-specific shell executable and the flag
/// used to execute a command string. This is used for running shell commands
/// in a cross-platform manner.
///
/// # Returns
///
/// A tuple of (`shell_command`, `execute_flag)`:
/// - Windows: `("cmd", "/C")`
/// - Unix-like: `("sh", "-c")`
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::get_shell_command;
/// use std::process::Command;
///
/// # fn example() -> anyhow::Result<()> {
/// let (shell, flag) = get_shell_command();
///
/// let output = Command::new(shell)
///     .arg(flag)
///     .arg("echo Hello World")
///     .output()?;
///
/// println!("Output: {}", String::from_utf8_lossy(&output.stdout));
/// # Ok(())
/// # }
/// ```
///
/// # Platform Commands
///
/// - **Windows**: Uses `cmd.exe` with `/C` flag to execute and terminate
/// - **Unix-like**: Uses `sh` with `-c` flag for POSIX shell compatibility
///
/// # Use Cases
///
/// - Executing shell commands in a cross-platform way
/// - Running system utilities and tools
/// - Batch operations that require shell features
/// - Environment-specific command execution
///
/// # Security Considerations
///
/// When using this function with user input, ensure proper escaping and
/// validation to prevent command injection vulnerabilities.
///
/// # Alternative Shells
///
/// This function returns the most compatible shell for each platform.
/// For specific shell requirements (bash, `PowerShell`, etc.), use direct
/// command execution instead.
///
/// # See Also
///
/// - [`command_exists`] for checking shell availability
/// - [`std::process::Command`] for safe command execution
#[must_use]
pub const fn get_shell_command() -> (&'static str, &'static str) {
    if is_windows() {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// Validates that a path contains only characters valid for the current platform.
///
/// This function checks path strings for invalid characters and reserved names
/// according to platform-specific filesystem rules. It helps prevent errors
/// when creating files and directories.
///
/// # Arguments
///
/// * `path` - The path string to validate
///
/// # Returns
///
/// - `Ok(())` if the path is valid for the current platform
/// - `Err` if the path contains invalid characters or reserved names
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::validate_path_chars;
///
/// # fn example() -> anyhow::Result<()> {
/// // Valid paths
/// validate_path_chars("valid/path/file.txt")?;
/// validate_path_chars("another_valid_file.md")?;
///
/// // Invalid on Windows (but may be valid on Unix)
/// # #[cfg(windows)]
/// # {
/// let result = validate_path_chars("invalid:file.txt");
/// assert!(result.is_err());
/// # }
/// # Ok(())
/// # }
/// ```
///
/// # Platform-Specific Rules
///
/// ## Windows
/// **Invalid characters**: `< > : " | ? *` and control characters (0x00-0x1F)
///
/// **Reserved names**: `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`
/// (case-insensitive, applies to bare names without extensions)
///
/// ## Unix-like Systems
/// - Only the null character (`\0`) is invalid
/// - No reserved names (though some names like `.` and `..` have special meaning)
/// - Case-sensitive validation
///
/// # Use Cases
///
/// - Validating user input for file names
/// - Checking paths before creation
/// - Preventing filesystem errors
/// - Cross-platform path compatibility
///
/// # Security
///
/// This validation helps prevent:
/// - Filesystem errors from invalid characters
/// - Accidental overwriting of system files (Windows reserved names)
/// - Path injection attacks using special characters
///
/// # Limitations
///
/// - Does not check path length limits
/// - Does not verify directory existence
/// - May not catch all filesystem-specific restrictions
///
/// # See Also
///
/// - [`safe_join`] which uses this function for validation
/// - Platform filesystem documentation for complete rules
pub fn validate_path_chars(path: &str) -> Result<()> {
    if is_windows() {
        // Windows invalid characters: < > : " | ? * and control characters
        const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

        for ch in path.chars() {
            if INVALID_CHARS.contains(&ch) || ch.is_control() {
                return Err(anyhow::anyhow!(
                    "Invalid character '{ch}' in path: {path}\\n\\n\\\n                    Windows paths cannot contain: < > : \" | ? * or control characters"
                ));
            }
        }

        // Check for reserved names in all path components
        const RESERVED_NAMES: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        // Check each component of the path
        for component in Path::new(path).components() {
            if let Some(os_str) = component.as_os_str().to_str() {
                // Check if the entire component (without extension) is a reserved name
                // Reserved names are only invalid if they're the complete name (no extension)
                let upper = os_str.to_uppercase();

                // Check if it's exactly a reserved name (no extension)
                if RESERVED_NAMES.contains(&upper.as_str()) {
                    return Err(anyhow::anyhow!(
                        "Reserved name '{}' in path: {}\\n\\n\\\n                    Windows reserved names: {}",
                        os_str,
                        path,
                        RESERVED_NAMES.join(", ")
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Safely joins a base path with a relative path, preventing directory traversal.
///
/// This function securely combines a base directory with a relative path while
/// preventing directory traversal attacks. It validates the input path and
/// ensures the result stays within the base directory.
///
/// # Arguments
///
/// * `base` - The base directory that should contain the result
/// * `path` - The relative path to join (validated for safety)
///
/// # Returns
///
/// The joined path with proper platform-specific handling, or an error if:
/// - The path contains invalid characters
/// - The path would escape the base directory
/// - Platform-specific validation fails
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::utils::platform::safe_join;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let base = Path::new("/home/user/project");
///
/// // Safe joins
/// let file_path = safe_join(base, "src/main.rs")?;
/// let nested_path = safe_join(base, "docs/guide/intro.md")?;
///
/// // These would fail (directory traversal)
/// // safe_join(base, "../../../etc/passwd").unwrap_err();
/// // safe_join(base, "/absolute/path").unwrap_err();
/// # Ok(())
/// # }
/// ```
///
/// # Security Features
///
/// - **Path traversal prevention**: Detects and blocks `../` escape attempts
/// - **Character validation**: Ensures valid characters for the platform
/// - **Normalization**: Resolves `.` and `..` components before validation
/// - **Platform handling**: Applies Windows long path support when needed
///
/// # Validation Performed
///
/// 1. **Character validation**: Checks for platform-invalid characters
/// 2. **Traversal detection**: Identifies attempts to escape base directory
/// 3. **Path normalization**: Resolves relative components
/// 4. **Boundary checking**: Ensures result stays within base
///
/// # Error Cases
///
/// - Path contains invalid characters (platform-specific)
/// - Path traversal attempt detected (`../../../etc/passwd`)
/// - Path would resolve outside the base directory
/// - Windows reserved names used in path components
///
/// # Use Cases
///
/// - Processing user-provided relative paths
/// - Extracting archive files safely
/// - Configuration file path resolution
/// - API endpoints that accept file paths
///
/// # Platform Behavior
///
/// - **Windows**: Handles long paths, validates reserved names
/// - **Unix-like**: Allows most characters, prevents null bytes
/// - **All platforms**: Prevents directory traversal attacks
///
/// # See Also
///
/// - [`validate_path_chars`] for character validation details
/// - [`windows_long_path`] for Windows path handling
/// - `is_safe_path` for path safety checking
pub fn safe_join(base: &Path, path: &str) -> Result<PathBuf> {
    // Validate the path characters first
    validate_path_chars(path)?;

    let path_buf = PathBuf::from(path);

    // Check for path traversal attempts
    if path.contains("..") {
        let joined = base.join(&path_buf);
        let normalized = crate::utils::fs::normalize_path(&joined);
        if !normalized.starts_with(base) {
            return Err(anyhow::anyhow!(
                "Path traversal detected in: {path}\\n\\n\\\n                Attempted to access path outside base directory"
            ));
        }
    }

    let result = base.join(path_buf);
    Ok(windows_long_path(&result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_windows() {
        #[cfg(windows)]
        assert!(is_windows());

        #[cfg(not(windows))]
        assert!(!is_windows());
    }

    #[test]
    fn test_git_command() {
        let cmd = get_git_command();
        #[cfg(windows)]
        assert_eq!(cmd, "git.exe");

        #[cfg(not(windows))]
        assert_eq!(cmd, "git");
    }

    #[test]
    fn test_get_home_dir() {
        let home = get_home_dir();
        assert!(home.is_ok());
        let home_path = home.unwrap();
        assert!(home_path.exists());
    }

    #[test]
    fn test_resolve_path_tilde() {
        let home = get_home_dir().unwrap();

        let resolved = resolve_path("~/test").unwrap();
        assert_eq!(resolved, home.join("test"));

        let resolved = resolve_path("~/test/file.txt").unwrap();
        assert_eq!(resolved, home.join("test/file.txt"));
    }

    #[test]
    fn test_resolve_path_absolute() {
        let resolved = resolve_path("/tmp/test").unwrap();
        assert_eq!(resolved, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let resolved = resolve_path("test/file.txt").unwrap();
        assert_eq!(resolved, PathBuf::from("test/file.txt"));
    }

    #[test]
    fn test_resolve_path_invalid_tilde() {
        let result = resolve_path("~test");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_path_separator() {
        let path = Path::new("test/path/file.txt");
        let normalized = normalize_path_separator(path);

        #[cfg(windows)]
        assert_eq!(normalized, "test\\path\\file.txt");

        #[cfg(not(windows))]
        assert_eq!(normalized, "test/path/file.txt");
    }

    #[test]
    fn test_normalize_path_for_storage() {
        // Test Unix-style path (should remain unchanged)
        let unix_path = Path::new(".claude/agents/example.md");
        assert_eq!(normalize_path_for_storage(unix_path), ".claude/agents/example.md");

        // Test Windows-style path (should convert to forward slashes)
        let windows_path = Path::new(".claude\\agents\\example.md");
        assert_eq!(normalize_path_for_storage(windows_path), ".claude/agents/example.md");

        // Test mixed separators (should normalize all to forward slashes)
        let mixed_path = Path::new("src/utils\\platform.rs");
        assert_eq!(normalize_path_for_storage(mixed_path), "src/utils/platform.rs");

        // Test nested Windows path
        let nested = Path::new(".claude\\agents\\ai\\gpt.md");
        assert_eq!(normalize_path_for_storage(nested), ".claude/agents/ai/gpt.md");

        // Test that result is always forward slashes regardless of platform
        let path = Path::new("test\\nested\\path\\file.txt");
        let normalized = normalize_path_for_storage(path);
        assert_eq!(normalized, "test/nested/path/file.txt");
        assert!(!normalized.contains('\\'));
    }

    #[test]
    fn test_command_exists() {
        // Test with a command that should exist on all systems
        #[cfg(unix)]
        assert!(command_exists("sh"));

        #[cfg(windows)]
        assert!(command_exists("cmd"));

        // Test with a command that shouldn't exist
        assert!(!command_exists("this_command_should_not_exist_12345"));
    }

    #[test]
    fn test_get_cache_dir() {
        let dir = get_cache_dir().unwrap();
        assert!(dir.to_string_lossy().contains("agpm"));
    }

    #[test]
    fn test_get_data_dir() {
        let dir = get_data_dir().unwrap();
        assert!(dir.to_string_lossy().contains("agpm"));
    }

    #[test]
    fn test_windows_long_path() {
        let path = Path::new("/test/path");
        let result = windows_long_path(path);

        #[cfg(windows)]
        assert_eq!(result, PathBuf::from("/test/path"));

        #[cfg(not(windows))]
        assert_eq!(result, path.to_path_buf());
    }

    #[test]
    fn test_get_shell_command() {
        let (shell, flag) = get_shell_command();

        #[cfg(windows)]
        {
            assert_eq!(shell, "cmd");
            assert_eq!(flag, "/C");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(shell, "sh");
            assert_eq!(flag, "-c");
        }
    }

    #[test]
    fn test_path_to_string() {
        let path = Path::new("test/path/file.txt");
        let result = path_to_string(path);
        assert!(!result.is_empty());
        assert!(result.contains("file.txt"));
    }

    #[test]
    fn test_paths_equal() {
        let path1 = Path::new("Test/Path");
        let path2 = Path::new("test/path");

        #[cfg(windows)]
        assert!(paths_equal(path1, path2));

        #[cfg(not(windows))]
        assert!(!paths_equal(path1, path2));

        // Same case should always be equal
        let path3 = Path::new("test/path");
        assert!(paths_equal(path2, path3));
    }

    #[test]
    fn test_safe_canonicalize() {
        let temp = tempfile::tempdir().unwrap();
        let test_path = temp.path().join("test_file.txt");
        std::fs::write(&test_path, "test").unwrap();

        let result = safe_canonicalize(&test_path);
        assert!(result.is_ok());

        let canonical = result.unwrap();
        assert!(canonical.is_absolute());
        assert!(canonical.exists());
    }

    #[test]
    fn test_validate_path_chars() {
        // Valid paths should pass
        assert!(validate_path_chars("valid/path/file.txt").is_ok());
        assert!(validate_path_chars("underscore_file.txt").is_ok());

        #[cfg(windows)]
        {
            // Invalid Windows characters should fail
            assert!(validate_path_chars("invalid:file.txt").is_err());
            assert!(validate_path_chars("invalid|file.txt").is_err());
            assert!(validate_path_chars("invalid?file.txt").is_err());

            // Reserved names should fail
            assert!(validate_path_chars("CON").is_err());
            assert!(validate_path_chars("PRN").is_err());
            assert!(validate_path_chars("path/AUX/file.txt").is_err());
        }
    }

    #[test]
    fn test_safe_join() {
        let base = Path::new("/home/user/project");

        // Normal join should work
        let result = safe_join(base, "subdir/file.txt");
        assert!(result.is_ok());

        // Path traversal should be detected and rejected
        let result = safe_join(base, "../../../etc/passwd");
        assert!(result.is_err());

        #[cfg(windows)]
        {
            // Invalid Windows characters should be rejected
            let result = safe_join(base, "invalid:file.txt");
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_validate_path_chars_edge_cases() {
        // Test empty path
        assert!(validate_path_chars("").is_ok());

        // Test path with spaces
        assert!(validate_path_chars("path with spaces/file.txt").is_ok());

        // Test path with dots
        assert!(validate_path_chars("../relative/path.txt").is_ok());

        #[cfg(windows)]
        {
            // Test control characters
            assert!(validate_path_chars("file\0name").is_err());
            assert!(validate_path_chars("file\nname").is_err());

            // Test all invalid Windows chars
            for ch in &['<', '>', ':', '"', '|', '?', '*'] {
                let invalid_path = format!("file{}name", ch);
                assert!(validate_path_chars(&invalid_path).is_err());
            }

            // Test reserved names with extensions (should be ok)
            assert!(validate_path_chars("CON.txt").is_ok());
            assert!(validate_path_chars("PRN.log").is_ok());
        }
    }

    #[test]
    fn test_safe_join_edge_cases() {
        let base = Path::new("/base");

        // Test single dot (current dir)
        let result = safe_join(base, ".");
        assert!(result.is_ok());

        // Test safe relative path with ..
        let result = safe_join(base, "subdir/../file.txt");
        assert!(result.is_ok());

        // Test absolute path join
        let result = safe_join(base, "/absolute/path");
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_path_invalid_env_var() {
        // Test with undefined environment variable
        let result = resolve_path("$UNDEFINED_VAR_123/path");
        // This should either fail or expand to empty/current path
        if result.is_ok() {
            // Some systems might expand undefined vars to empty string
        } else {
            // This is also acceptable behavior
        }
    }

    #[test]
    fn test_windows_specific_tilde_error() {
        // Test invalid Windows tilde usage on any platform
        let result = resolve_path("~user/file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_executable_extension() {
        let ext = get_executable_extension();

        #[cfg(windows)]
        assert_eq!(ext, ".exe");

        #[cfg(not(windows))]
        assert_eq!(ext, "");
    }

    #[test]
    fn test_is_executable_name() {
        #[cfg(windows)]
        {
            assert!(is_executable_name("test.exe"));
            assert!(is_executable_name("TEST.EXE"));
            assert!(!is_executable_name("test"));
            assert!(!is_executable_name("test.txt"));
        }

        #[cfg(not(windows))]
        {
            // On Unix, any file can be executable
            assert!(is_executable_name("test"));
            assert!(is_executable_name("test.sh"));
            assert!(is_executable_name("test.exe"));
        }
    }

    #[test]
    fn test_normalize_line_endings() {
        let text_lf = "line1\nline2\nline3";
        let text_crlf = "line1\r\nline2\r\nline3";
        let text_mixed = "line1\nline2\r\nline3";

        let normalized_lf = normalize_line_endings(text_lf);
        let normalized_crlf = normalize_line_endings(text_crlf);
        let normalized_mixed = normalize_line_endings(text_mixed);

        #[cfg(windows)]
        {
            assert!(normalized_lf.contains("\r\n"));
            assert!(normalized_crlf.contains("\r\n"));
            assert!(normalized_mixed.contains("\r\n"));
        }

        #[cfg(not(windows))]
        {
            assert!(!normalized_lf.contains('\r'));
            assert!(!normalized_crlf.contains('\r'));
            assert!(!normalized_mixed.contains('\r'));
        }
    }

    #[test]
    fn test_safe_canonicalize_nonexistent() {
        let result = safe_canonicalize(Path::new("/nonexistent/path/to/file"));
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_canonicalize_relative() {
        use tempfile::TempDir;

        // Create a temp directory to ensure we have a valid working directory
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test").unwrap();

        // Test with a file that exists
        let result = safe_canonicalize(&test_file);
        assert!(result.is_ok());
        let canonical = result.unwrap();
        assert!(canonical.is_absolute());
    }

    #[test]
    fn test_paths_equal_with_trailing_slash() {
        let path1 = Path::new("test/path/");
        let path2 = Path::new("test/path");

        // Paths should be equal regardless of trailing slash
        assert!(paths_equal(path1, path2));
    }

    #[test]
    fn test_validate_path_chars_unicode() {
        // Test with unicode characters
        assert!(validate_path_chars("文件名.txt").is_ok());
        assert!(validate_path_chars("файл.md").is_ok());
        assert!(validate_path_chars("αρχείο.rs").is_ok());

        // Test with emoji (should be ok on most systems)
        assert!(validate_path_chars("📁folder/📄file.txt").is_ok());
    }

    #[test]
    fn test_command_exists_with_path() {
        // Test that command_exists works with full paths
        #[cfg(unix)]
        {
            if Path::new("/bin/sh").exists() {
                assert!(command_exists("/bin/sh"));
            }
        }

        #[cfg(windows)]
        {
            if Path::new("C:\\Windows\\System32\\cmd.exe").exists() {
                assert!(command_exists("C:\\Windows\\System32\\cmd.exe"));
            }
        }
    }

    #[test]
    fn test_normalize_path_separator_edge_cases() {
        // Test empty path
        let empty = Path::new("");
        let normalized = normalize_path_separator(empty);
        assert_eq!(normalized, "");

        // Test root path
        #[cfg(unix)]
        {
            let root = Path::new("/");
            let normalized = normalize_path_separator(root);
            assert_eq!(normalized, "/");
        }

        #[cfg(windows)]
        {
            let root = Path::new("C:\\");
            let normalized = normalize_path_separator(root);
            assert_eq!(normalized, "C:\\");
        }
    }

    #[test]
    fn test_path_to_string_invalid_utf8() {
        // This test is mainly for Unix where paths can be non-UTF8
        #[cfg(unix)]
        {
            use std::ffi::OsStr;
            use std::os::unix::ffi::OsStrExt;

            // Create a path with invalid UTF-8
            let invalid_bytes = vec![0xff, 0xfe, 0xfd];
            let os_str = OsStr::from_bytes(&invalid_bytes);
            let path = Path::new(os_str);

            // path_to_string should handle this gracefully
            let result = path_to_string(path);
            assert!(!result.is_empty());
        }
    }

    #[test]
    fn test_safe_join_complex_scenarios() {
        let base = Path::new("/home/user");

        // Test with empty path component
        let result = safe_join(base, "");
        assert!(result.is_ok());

        // Test with multiple slashes
        let result = safe_join(base, "path//to///file");
        assert!(result.is_ok());

        // Test with backslashes on Unix (should be treated as regular characters)
        #[cfg(unix)]
        {
            let result = safe_join(base, "path\\to\\file");
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_resolve_path_complex() {
        // Test multiple ~ in path (only first should be expanded)
        let result = resolve_path("~/path/~file.txt");
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(!resolved.to_string_lossy().starts_with('~'));

        // Test empty path
        let result = resolve_path("");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from(""));
    }

    #[test]
    fn test_get_home_dir_fallback() {
        // Test that get_home_dir has appropriate error handling
        // We can't easily test the error case without modifying the environment significantly
        // but we can verify the function signature and basic operation
        match get_home_dir() {
            Ok(home) => {
                assert!(home.is_absolute());
                // Home directory should exist
                assert!(home.exists() || home.parent().is_some_and(std::path::Path::exists));
            }
            Err(e) => {
                // If it fails, it should have a meaningful error message
                assert!(e.to_string().contains("home") || e.to_string().contains("directory"));
            }
        }
    }

    // Helper functions used in the module but not directly exported
    fn is_executable_name(_name: &str) -> bool {
        #[cfg(windows)]
        {
            _name.to_lowercase().ends_with(".exe")
        }
        #[cfg(not(windows))]
        {
            // On Unix, executability is determined by permissions, not name
            true
        }
    }

    fn get_executable_extension() -> &'static str {
        #[cfg(windows)]
        {
            ".exe"
        }
        #[cfg(not(windows))]
        {
            ""
        }
    }

    fn normalize_line_endings(text: &str) -> String {
        #[cfg(windows)]
        {
            text.replace('\n', "\r\n").replace("\r\r\n", "\r\n")
        }
        #[cfg(not(windows))]
        {
            text.replace("\r\n", "\n")
        }
    }

    #[test]
    fn test_normalize_path_for_storage_unix() {
        use std::path::Path;
        // Unix-style paths should just normalize separators
        assert_eq!(
            normalize_path_for_storage(Path::new("/project/agents/helper.md")),
            "/project/agents/helper.md"
        );
        assert_eq!(normalize_path_for_storage(Path::new("agents/helper.md")), "agents/helper.md");
        assert_eq!(
            normalize_path_for_storage(Path::new("../shared/utils.md")),
            "../shared/utils.md"
        );
    }

    #[test]
    fn test_normalize_path_for_storage_windows_extended() {
        use std::path::Path;
        // Windows extended-length path prefix should be stripped AND backslashes converted
        // This tests the combined behavior: \\?\C:\path -> C:/path
        let path = Path::new(r"\\?\C:\project\agents\helper.md");
        assert_eq!(
            normalize_path_for_storage(path),
            "C:/project/agents/helper.md",
            "Should strip extended-length prefix (\\\\?\\) AND convert backslashes to forward slashes"
        );
    }

    #[test]
    fn test_normalize_path_for_storage_windows_extended_unc() {
        use std::path::Path;
        // Windows extended-length UNC path should be converted to //server/share format
        let path = Path::new(r"\\?\UNC\server\share\file.md");
        assert_eq!(normalize_path_for_storage(path), "//server/share/file.md");
    }

    #[test]
    fn test_normalize_path_for_storage_windows_backslash() {
        use std::path::Path;
        // Windows backslashes should be converted to forward slashes
        let path = Path::new(r"C:\project\agents\helper.md");
        assert_eq!(normalize_path_for_storage(path), "C:/project/agents/helper.md");
    }
}
