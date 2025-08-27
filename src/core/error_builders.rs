//! Error context builders for consistent error reporting
//!
//! This module provides utilities for creating consistent error contexts
//! throughout the application, reducing boilerplate and ensuring uniform
//! error messages for users.

use crate::core::error::ErrorContext;
use anyhow::{Context, Result};
use std::path::Path;

/// Create an error context for file operations
///
/// # Arguments
///
/// * `operation` - The operation being performed (e.g., "read", "write", "delete")
/// * `path` - The path being operated on
///
/// # Example
///
/// ```no_run
/// # use anyhow::{Context, Result};
/// # fn example() -> Result<()> {
/// use ccpm::core::error_builders::file_error_context;
/// use std::fs;
/// use std::path::Path;
///
/// let path = Path::new("config.toml");
/// let contents = fs::read_to_string(&path)
///     .with_context(|| file_error_context("read", path))?;
/// # Ok(())
/// # }
/// ```
pub fn file_error_context(operation: &str, path: &Path) -> ErrorContext {
    use crate::core::CcpmError;

    ErrorContext {
        error: CcpmError::FileSystemError {
            operation: operation.to_string(),
            path: path.display().to_string(),
        },
        suggestion: match operation {
            "read" => Some("Check that the file exists and you have read permissions".to_string()),
            "write" => Some("Check that you have write permissions for this location".to_string()),
            "create" => Some(
                "Check that the parent directory exists and you have write permissions".to_string(),
            ),
            "delete" => {
                Some("Check that the file exists and you have delete permissions".to_string())
            }
            _ => None,
        },
        details: Some(format!("File path: {}", path.display())),
    }
}

/// Create an error context for git operations
///
/// # Arguments
///
/// * `command` - The git command that failed (e.g., "clone", "fetch", "pull")
/// * `repo` - Optional repository URL or path
///
/// # Example
///
/// ```no_run
/// use ccpm::core::error_builders::git_error_context;
///
/// let context = git_error_context("clone", Some("https://github.com/user/repo.git"));
/// ```
pub fn git_error_context(command: &str, repo: Option<&str>) -> ErrorContext {
    use crate::core::CcpmError;

    ErrorContext {
        error: CcpmError::GitCommandError {
            operation: command.to_string(),
            stderr: format!("Git {} operation failed", command),
        },
        suggestion: match command {
            "clone" => {
                Some("Check your network connection and that the repository exists".to_string())
            }
            "fetch" | "pull" => {
                Some("Check your network connection and repository access".to_string())
            }
            "checkout" => Some("Ensure the branch or tag exists in the repository".to_string()),
            "status" => Some("Ensure you're in a valid git repository".to_string()),
            _ => Some("Check that git is installed and accessible".to_string()),
        },
        details: repo.map(|r| format!("Repository: {}", r)),
    }
}

/// Create an error context for manifest operations
///
/// # Arguments
///
/// * `operation` - The operation being performed (e.g., "load", "parse", "validate")
/// * `details` - Optional additional details
///
/// # Example
///
/// ```no_run
/// use ccpm::core::error_builders::manifest_error_context;
///
/// let context = manifest_error_context("parse", Some("Invalid TOML syntax at line 5"));
/// ```
pub fn manifest_error_context(operation: &str, details: Option<&str>) -> ErrorContext {
    use crate::core::CcpmError;

    let error = match operation {
        "load" => CcpmError::ManifestNotFound,
        "parse" => CcpmError::ManifestParseError {
            file: "ccpm.toml".to_string(),
            reason: details.unwrap_or("Invalid TOML syntax").to_string(),
        },
        "validate" => CcpmError::ManifestValidationError {
            reason: details.unwrap_or("Validation failed").to_string(),
        },
        _ => CcpmError::Other {
            message: format!("Manifest operation '{}' failed", operation),
        },
    };

    ErrorContext {
        error,
        suggestion: match operation {
            "load" => Some("Check that ccpm.toml exists in the project directory".to_string()),
            "parse" => Some("Check that ccpm.toml contains valid TOML syntax".to_string()),
            "validate" => {
                Some("Ensure all required fields are present in the manifest".to_string())
            }
            _ => None,
        },
        details: details.map(|d| d.to_string()),
    }
}

/// Create an error context for dependency resolution
///
/// # Arguments
///
/// * `dependency` - The dependency that caused the error
/// * `reason` - The reason for the failure
///
/// # Example
///
/// ```no_run
/// use ccpm::core::error_builders::dependency_error_context;
///
/// let context = dependency_error_context("my-agent", "Version conflict with existing dependency");
/// ```
pub fn dependency_error_context(dependency: &str, reason: &str) -> ErrorContext {
    use crate::core::CcpmError;

    ErrorContext {
        error: CcpmError::InvalidDependency {
            name: dependency.to_string(),
            reason: reason.to_string(),
        },
        suggestion: Some("Try running 'ccpm update' to update dependencies".to_string()),
        details: Some(reason.to_string()),
    }
}

/// Create an error context for network operations
///
/// # Arguments
///
/// * `operation` - The network operation (e.g., "download", "fetch", "connect")
/// * `url` - Optional URL being accessed
///
/// # Example
///
/// ```no_run
/// use ccpm::core::error_builders::network_error_context;
///
/// let context = network_error_context("fetch", Some("https://api.example.com"));
/// ```
pub fn network_error_context(operation: &str, url: Option<&str>) -> ErrorContext {
    use crate::core::CcpmError;

    ErrorContext {
        error: CcpmError::NetworkError {
            operation: operation.to_string(),
            reason: format!("Network {} failed", operation),
        },
        suggestion: Some("Check your internet connection and try again".to_string()),
        details: url.map(|u| format!("URL: {}", u)),
    }
}

/// Create an error context for configuration issues
///
/// # Arguments
///
/// * `config_type` - The type of configuration (e.g., "global", "project", "mcp")
/// * `issue` - Description of the issue
///
/// # Example
///
/// ```no_run
/// use ccpm::core::error_builders::config_error_context;
///
/// let context = config_error_context("global", "Missing authentication token");
/// ```
pub fn config_error_context(config_type: &str, issue: &str) -> ErrorContext {
    use crate::core::CcpmError;

    ErrorContext {
        error: CcpmError::ConfigError {
            message: format!("Configuration error in {} config: {}", config_type, issue),
        },
        suggestion: match config_type {
            "global" => Some("Check ~/.ccpm/config.toml for correct settings".to_string()),
            "project" => Some("Check ccpm.toml in your project directory".to_string()),
            "mcp" => Some("Check .mcp.json for valid MCP server configurations".to_string()),
            _ => None,
        },
        details: Some(issue.to_string()),
    }
}

/// Create an error context for permission issues
///
/// # Arguments
///
/// * `resource` - The resource that requires permissions
/// * `operation` - The operation that failed
///
/// # Example
///
/// ```no_run
/// use ccpm::core::error_builders::permission_error_context;
///
/// let context = permission_error_context("/usr/local/bin", "write");
/// ```
pub fn permission_error_context(resource: &str, operation: &str) -> ErrorContext {
    use crate::core::CcpmError;

    ErrorContext {
        error: CcpmError::PermissionDenied {
            operation: operation.to_string(),
            path: resource.to_string(),
        },
        suggestion: Some(format!(
            "Check that you have {} permissions for: {}",
            operation, resource
        )),
        details: if cfg!(windows) {
            Some("On Windows, you may need to run as Administrator".to_string())
        } else {
            Some("On Unix systems, you may need to use sudo or change file permissions".to_string())
        },
    }
}

/// Helper trait to easily attach error contexts
pub trait ErrorContextExt<T> {
    /// Attach a file error context
    fn file_context(self, operation: &str, path: &Path) -> Result<T>;

    /// Attach a git error context
    fn git_context(self, command: &str, repo: Option<&str>) -> Result<T>;

    /// Attach a manifest error context
    fn manifest_context(self, operation: &str, details: Option<&str>) -> Result<T>;

    /// Attach a dependency error context
    fn dependency_context(self, dependency: &str, reason: &str) -> Result<T>;

    /// Attach a network error context
    fn network_context(self, operation: &str, url: Option<&str>) -> Result<T>;
}

impl<T, E> ErrorContextExt<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn file_context(self, operation: &str, path: &Path) -> Result<T> {
        self.with_context(|| file_error_context(operation, path))
    }

    fn git_context(self, command: &str, repo: Option<&str>) -> Result<T> {
        self.with_context(|| git_error_context(command, repo))
    }

    fn manifest_context(self, operation: &str, details: Option<&str>) -> Result<T> {
        self.with_context(|| manifest_error_context(operation, details))
    }

    fn dependency_context(self, dependency: &str, reason: &str) -> Result<T> {
        self.with_context(|| dependency_error_context(dependency, reason))
    }

    fn network_context(self, operation: &str, url: Option<&str>) -> Result<T> {
        self.with_context(|| network_error_context(operation, url))
    }
}

/// Macro for creating custom error contexts quickly
///
/// # Example
///
/// ```
/// use ccpm::{error_context, core::CcpmError};
///
/// let context = error_context! {
///     error: CcpmError::Other { message: "Operation failed".to_string() },
///     suggestion: "Try again later",
///     details: "Additional information"
/// };
/// ```
#[macro_export]
macro_rules! error_context {
    (error: $err:expr) => {
        $crate::core::error::ErrorContext {
            error: $err,
            suggestion: None,
            details: None,
        }
    };
    (error: $err:expr, suggestion: $sug:expr) => {
        $crate::core::error::ErrorContext {
            error: $err,
            suggestion: Some($sug.to_string()),
            details: None,
        }
    };
    (error: $err:expr, suggestion: $sug:expr, details: $det:expr) => {
        $crate::core::error::ErrorContext {
            error: $err,
            suggestion: Some($sug.to_string()),
            details: Some($det.to_string()),
        }
    };
    (error: $err:expr, details: $det:expr) => {
        $crate::core::error::ErrorContext {
            error: $err,
            suggestion: None,
            details: Some($det.to_string()),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_error_context() {
        let context = file_error_context("read", Path::new("/tmp/test.txt"));
        assert!(matches!(
            context.error,
            crate::core::CcpmError::FileSystemError { .. }
        ));
        assert!(context.suggestion.is_some());
        assert!(context.details.unwrap().contains("/tmp/test.txt"));
    }

    #[test]
    fn test_git_error_context() {
        let context = git_error_context("clone", Some("https://github.com/test/repo"));
        assert!(matches!(
            context.error,
            crate::core::CcpmError::GitCommandError { .. }
        ));
        assert!(context.suggestion.unwrap().contains("network"));
        assert!(context.details.unwrap().contains("github.com"));
    }

    #[test]
    fn test_error_context_macro() {
        use crate::core::CcpmError;

        let context = error_context! {
            error: CcpmError::Other { message: "Test error".to_string() },
            suggestion: "Test suggestion",
            details: "Test details"
        };
        assert!(matches!(context.error, CcpmError::Other { .. }));
        assert_eq!(context.suggestion.unwrap(), "Test suggestion");
        assert_eq!(context.details.unwrap(), "Test details");
    }

    #[test]
    fn test_permission_error_context() {
        let context = permission_error_context("/usr/local", "write");
        assert!(matches!(
            context.error,
            crate::core::CcpmError::PermissionDenied { .. }
        ));
        assert!(context.suggestion.unwrap().contains("write permissions"));
        assert!(context.details.is_some());
    }

    #[test]
    fn test_manifest_error_context_all_operations() {
        // Test load operation
        let context = manifest_error_context("load", None);
        assert!(matches!(
            context.error,
            crate::core::CcpmError::ManifestNotFound
        ));
        assert!(context.suggestion.unwrap().contains("ccpm.toml exists"));

        // Test parse operation with details
        let context = manifest_error_context("parse", Some("Syntax error at line 10"));
        assert!(matches!(
            context.error,
            crate::core::CcpmError::ManifestParseError { .. }
        ));
        assert!(context.suggestion.unwrap().contains("valid TOML syntax"));
        assert_eq!(context.details.unwrap(), "Syntax error at line 10");

        // Test validate operation
        let context = manifest_error_context("validate", Some("Missing required field"));
        assert!(matches!(
            context.error,
            crate::core::CcpmError::ManifestValidationError { .. }
        ));
        assert!(context.suggestion.unwrap().contains("required fields"));
        assert_eq!(context.details.unwrap(), "Missing required field");

        // Test unknown operation
        let context = manifest_error_context("unknown", None);
        assert!(matches!(
            context.error,
            crate::core::CcpmError::Other { .. }
        ));
        assert!(context.suggestion.is_none());
    }

    #[test]
    fn test_dependency_error_context() {
        let context = dependency_error_context("test-agent", "Version not found");
        assert!(matches!(
            context.error,
            crate::core::CcpmError::InvalidDependency { .. }
        ));
        assert!(context.suggestion.unwrap().contains("ccpm update"));
        assert_eq!(context.details.unwrap(), "Version not found");
    }

    #[test]
    fn test_network_error_context() {
        let context = network_error_context("download", Some("https://example.com/file"));
        assert!(matches!(
            context.error,
            crate::core::CcpmError::NetworkError { .. }
        ));
        assert!(context.suggestion.unwrap().contains("internet connection"));
        assert!(context.details.unwrap().contains("example.com"));
    }

    #[test]
    fn test_config_error_context_types() {
        // Test global config
        let context = config_error_context("global", "Invalid format");
        assert!(matches!(
            context.error,
            crate::core::CcpmError::ConfigError { .. }
        ));
        assert!(context.suggestion.unwrap().contains("~/.ccpm/config.toml"));

        // Test project config
        let context = config_error_context("project", "Missing dependency");
        assert!(context.suggestion.unwrap().contains("ccpm.toml"));

        // Test MCP config
        let context = config_error_context("mcp", "Invalid server");
        assert!(context.suggestion.unwrap().contains(".mcp.json"));

        // Test unknown config type
        let context = config_error_context("unknown", "Some issue");
        assert!(context.suggestion.is_none());
    }

    #[test]
    fn test_file_error_context_operations() {
        // Test read operation
        let context = file_error_context("read", Path::new("/test/file.txt"));
        assert!(context.suggestion.unwrap().contains("read permissions"));

        // Test write operation
        let context = file_error_context("write", Path::new("/test/file.txt"));
        assert!(context.suggestion.unwrap().contains("write permissions"));

        // Test create operation
        let context = file_error_context("create", Path::new("/test/file.txt"));
        assert!(context.suggestion.unwrap().contains("parent directory"));

        // Test delete operation
        let context = file_error_context("delete", Path::new("/test/file.txt"));
        assert!(context.suggestion.unwrap().contains("delete permissions"));

        // Test unknown operation
        let context = file_error_context("unknown", Path::new("/test/file.txt"));
        assert!(context.suggestion.is_none());
    }

    #[test]
    fn test_git_error_context_commands() {
        // Test clone command
        let context = git_error_context("clone", Some("repo.git"));
        assert!(context.suggestion.unwrap().contains("repository exists"));

        // Test fetch command
        let context = git_error_context("fetch", None);
        assert!(context.suggestion.unwrap().contains("repository access"));

        // Test pull command
        let context = git_error_context("pull", Some("origin"));
        assert!(context.suggestion.unwrap().contains("repository access"));

        // Test checkout command
        let context = git_error_context("checkout", Some("branch"));
        assert!(context.suggestion.unwrap().contains("branch or tag exists"));

        // Test status command
        let context = git_error_context("status", None);
        assert!(context.suggestion.unwrap().contains("valid git repository"));

        // Test unknown command
        let context = git_error_context("unknown", None);
        assert!(context.suggestion.unwrap().contains("git is installed"));
    }

    #[test]
    fn test_error_context_ext_trait() {
        use std::io;

        // Test file_context
        let result: Result<(), io::Error> = Err(io::Error::new(io::ErrorKind::NotFound, "test"));
        let result = result.file_context("read", Path::new("/test.txt"));
        assert!(result.is_err());

        // Test git_context
        let result: Result<(), io::Error> = Err(io::Error::new(io::ErrorKind::Other, "test"));
        let result = result.git_context("clone", Some("repo"));
        assert!(result.is_err());

        // Test manifest_context
        let result: Result<(), io::Error> = Err(io::Error::new(io::ErrorKind::InvalidData, "test"));
        let result = result.manifest_context("parse", Some("details"));
        assert!(result.is_err());

        // Test dependency_context
        let result: Result<(), io::Error> = Err(io::Error::new(io::ErrorKind::Other, "test"));
        let result = result.dependency_context("dep", "reason");
        assert!(result.is_err());

        // Test network_context
        let result: Result<(), io::Error> = Err(io::Error::new(io::ErrorKind::TimedOut, "test"));
        let result = result.network_context("fetch", Some("url"));
        assert!(result.is_err());
    }

    #[test]
    fn test_permission_error_context_platforms() {
        let context = permission_error_context("/path", "execute");
        assert!(context.details.is_some());

        #[cfg(windows)]
        assert!(context.details.unwrap().contains("Administrator"));

        #[cfg(not(windows))]
        assert!(context.details.unwrap().contains("sudo"));
    }

    #[test]
    fn test_error_context_macro_variants() {
        use crate::core::CcpmError;

        // Test with error only
        let context = error_context! {
            error: CcpmError::Other { message: "Error only".to_string() }
        };
        assert!(context.suggestion.is_none());
        assert!(context.details.is_none());

        // Test with error and suggestion
        let context = error_context! {
            error: CcpmError::Other { message: "Error".to_string() },
            suggestion: "Do this"
        };
        assert_eq!(context.suggestion.unwrap(), "Do this");
        assert!(context.details.is_none());

        // Test with error and details
        let context = error_context! {
            error: CcpmError::Other { message: "Error".to_string() },
            details: "More info"
        };
        assert!(context.suggestion.is_none());
        assert_eq!(context.details.unwrap(), "More info");
    }
}
