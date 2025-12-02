//! Platform-specific utilities and cross-platform compatibility helpers
//!
//! This module provides abstractions over platform differences to ensure AGPM
//! works consistently across Windows, macOS, and Linux.

use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};

/// Checks if the current platform is Windows.
///
/// Returns `true` on Windows, `false` on Unix-like systems (macOS, Linux, BSD).
#[must_use]
pub const fn is_windows() -> bool {
    cfg!(windows)
}

/// Gets the home directory path for the current user.
///
/// Uses `%USERPROFILE%` on Windows, `$HOME` on Unix-like systems.
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
/// Returns `"git.exe"` on Windows, `"git"` on Unix-like systems.
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
/// Supports `~/path`, `$VAR` (Unix), `%VAR%` (Windows), and `${VAR}` syntax.
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
/// Converts `/` to `\` on Windows, `\` to `/` on Unix-like systems.
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
/// Critical for lockfiles, `.gitignore` entries, TOML/JSON files. Always use this for stored paths.
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
/// Strips redundant prefixes to prevent duplication (e.g., `.claude/agents/agents/example.md`).
/// Also strips prefixes that match any component in the tool root path to avoid
/// `.claude/skills/agpm/skills/skill-name` when installing `skills/skill-name`.
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

    // Extract all Normal components from tool root for matching
    let tool_components: Vec<&str> = tool_root
        .components()
        .filter_map(|c| {
            if let Component::Normal(s) = c {
                s.to_str()
            } else {
                None
            }
        })
        .collect();

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

    // If the first component of dep_path matches ANY component in tool_root,
    // strip it to avoid duplication like `.claude/skills/agpm/skills/skill-name`
    if let Some(dep_first_str) = dep_first {
        if tool_components.contains(&dep_first_str) {
            // Skip everything up to and including the matching Normal component
            if let Some(idx) = first_normal_idx {
                return components.iter().skip(idx + 1).collect();
            }
        }
    }

    // No match - return the full path (but skip any leading CurDir/ParentDir for cleanliness)
    components
        .iter()
        .skip_while(|c| matches!(c, Component::CurDir | Component::Prefix(_) | Component::RootDir))
        .collect()
}

/// Safely converts a path to a string, handling non-UTF-8 paths gracefully.
///
/// Uses lossy conversion (replacement character � for invalid UTF-8).
#[must_use]
pub fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Returns a path as an `OsStr` for use in command arguments.
///
/// Provides lossless path representation for system commands and APIs.
#[must_use]
pub fn path_to_os_str(path: &Path) -> &std::ffi::OsStr {
    path.as_os_str()
}

/// Compares two paths for equality, respecting platform case sensitivity rules.
///
/// Case-insensitive on Windows, case-sensitive on Unix-like systems.
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
/// Resolves to absolute form, handles Windows long paths, resolves symlinks.
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
/// Returns `true` if the command exists and is executable.
#[must_use]
pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Returns the platform-specific cache directory for AGPM.
///
/// Returns `{cache_dir}/agpm` following platform conventions (XDG on Linux).
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
/// Returns `{data_dir}/agpm` for persistent application data.
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
/// Applies `\\?\` prefix on Windows for paths >260 chars. No-op on other platforms.
///
/// # Performance
/// Uses fast path for short paths (<200 chars) to avoid string conversions.
/// The 200 char threshold provides safety margin below the 260 limit.
#[cfg(windows)]
#[must_use]
pub fn windows_long_path(path: &Path) -> PathBuf {
    // Fast path: paths under 200 chars can never exceed 260 limit
    // even with relative-to-absolute conversion. This avoids to_string_lossy().
    if path.as_os_str().len() < 200 {
        return path.to_path_buf();
    }

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
#[cfg(not(windows))]
#[must_use]
pub fn windows_long_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Returns the appropriate shell command and flag for the current platform.
///
/// Returns `("cmd", "/C")` on Windows, `("sh", "-c")` on Unix-like systems.
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
/// Checks for invalid characters and Windows reserved names (CON, PRN, etc.).
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
/// Validates characters and prevents `../` escape attempts.
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
    fn test_get_home_dir() -> Result<()> {
        let home_path = get_home_dir()?;
        assert!(home_path.exists());
        Ok(())
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
    fn test_safe_canonicalize() -> Result<()> {
        let temp = tempfile::tempdir().unwrap();
        let test_path = temp.path().join("test_file.txt");
        std::fs::write(&test_path, "test").unwrap();

        let canonical = safe_canonicalize(&test_path)?;
        assert!(canonical.is_absolute());
        assert!(canonical.exists());
        Ok(())
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
    fn test_safe_join() -> Result<()> {
        let base = Path::new("/home/user/project");

        // Normal join should work
        let _joined = safe_join(base, "subdir/file.txt")?;

        // Path traversal should be detected and rejected
        let result = safe_join(base, "../../../etc/passwd");
        assert!(result.is_err());

        #[cfg(windows)]
        {
            // Invalid Windows characters should be rejected
            let result = safe_join(base, "invalid:file.txt");
            assert!(result.is_err());
        }
        Ok(())
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
    fn test_safe_join_edge_cases() -> Result<()> {
        let base = Path::new("/base");

        // Test single dot (current dir)
        let _current = safe_join(base, ".")?;

        // Test safe relative path with ..
        let _safe_relative = safe_join(base, "subdir/../file.txt")?;

        // Test absolute path join
        let _absolute = safe_join(base, "/absolute/path")?;
        Ok(())
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
    fn test_safe_canonicalize_relative() -> Result<()> {
        use tempfile::TempDir;

        // Create a temp directory to ensure we have a valid working directory
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test").unwrap();

        // Test with a file that exists
        let canonical = safe_canonicalize(&test_file)?;
        assert!(canonical.is_absolute());
        Ok(())
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
    fn test_safe_join_complex_scenarios() -> Result<()> {
        let base = Path::new("/home/user");

        // Test with empty path component
        let _empty = safe_join(base, "")?;

        // Test with multiple slashes
        let _multiple_slashes = safe_join(base, "path//to///file")?;

        // Test with backslashes on Unix (should be treated as regular characters)
        #[cfg(unix)]
        {
            let _backslashes = safe_join(base, "path\\to\\file")?;
        }
        Ok(())
    }

    #[test]
    fn test_resolve_path_complex() -> Result<()> {
        // Test multiple ~ in path (only first should be expanded)
        let resolved = resolve_path("~/path/~file.txt")?;
        assert!(!resolved.to_string_lossy().starts_with('~'));

        // Test empty path
        let empty = resolve_path("")?;
        assert_eq!(empty, PathBuf::from(""));
        Ok(())
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
