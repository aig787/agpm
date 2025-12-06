//! Error formatting utilities for AGPM
//!
//! This module provides user-friendly error formatting functions that convert
//! internal errors into clear, actionable messages for users.

use super::*;
use crate::core::file_error::FileOperationError;

/// Keywords that indicate template-related errors
const TEMPLATE_ERROR_KEYWORDS: &[&str] = &["template", "variable", "filter"];

/// Keywords that indicate network-related errors
const NETWORK_ERROR_KEYWORDS: &[&str] = &["network", "connection", "timeout"];

/// Keywords that indicate git-related errors
const GIT_ERROR_KEYWORDS: &[&str] = &["git command", "git operation", "git clone", "git fetch"];

/// Keywords that indicate permission-related errors
const PERMISSION_ERROR_KEYWORDS: &[&str] = &["permission", "denied", "access"];

/// Convert any error into a user-friendly format with contextual suggestions
///
/// This function analyzes the error type and provides:
/// - Clear, actionable error messages
/// - Specific suggestions based on the error type
/// - Additional details to help users understand and resolve the issue
///
/// # Arguments
///
/// * `error` - The error to convert to a user-friendly format
///
/// # Returns
///
/// An [`ErrorContext`] with user-friendly messages and suggestions
#[must_use]
pub fn user_friendly_error(error: anyhow::Error) -> ErrorContext {
    // Check for specific error types and provide helpful suggestions
    if let Some(ccmp_error) = error.downcast_ref::<AgpmError>() {
        return create_error_context(ccmp_error);
    }

    // Walk the error chain to find specific errors
    let mut current_error: &dyn std::error::Error = error.as_ref();
    loop {
        // Check for AgpmError in the chain (for errors wrapped by anyhow context)
        if let Some(agpm_error) = current_error.downcast_ref::<AgpmError>() {
            // Any AgpmError in the chain should be handled by create_error_context
            // This ensures ManifestNotFound and other specific errors are properly formatted
            return create_error_context(agpm_error);
        }

        // Check for TemplateError
        if let Some(template_error) =
            current_error.downcast_ref::<crate::templating::TemplateError>()
        {
            // Found a TemplateError - use its detailed formatting
            let formatted = template_error.format_with_context();
            return ErrorContext::new(AgpmError::Other {
                message: formatted.clone(),
            })
            .with_suggestion("Check your template syntax and variable declarations")
            .with_details(formatted);
        }

        // Move to the next error in the chain
        match current_error.source() {
            Some(source) => current_error = source,
            None => break,
        }
    }

    if let Some(file_error) = error.downcast_ref::<FileOperationError>() {
        // Check if the underlying IO error is a permission error
        if file_error.source.kind() == std::io::ErrorKind::PermissionDenied {
            return ErrorContext::new(AgpmError::PermissionDenied {
                operation: file_error.operation.to_string(),
                path: file_error.file_path.to_string_lossy().to_string(),
            })
            .with_suggestion("Check file permissions and try running with appropriate privileges")
            .with_details(format!(
                "Permission denied for '{}' on path: {}",
                file_error.operation,
                file_error.file_path.display()
            ));
        }

        return ErrorContext::new(AgpmError::FileSystemError {
            operation: file_error.operation.to_string(),
            path: file_error.file_path.to_string_lossy().to_string(),
        })
        .with_suggestion("Check that the path exists and you have the necessary permissions")
        .with_details(format!(
            "Failed to {} at path: {}",
            file_error.operation,
            file_error.file_path.display()
        ));
    }

    if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
        match io_error.kind() {
            std::io::ErrorKind::PermissionDenied => {
                return create_error_context(&AgpmError::PermissionDenied {
                    operation: "file access".to_string(),
                    path: "file path not specified in error context".to_string(),
                });
            }
            std::io::ErrorKind::NotFound => {
                return create_error_context(&AgpmError::FileSystemError {
                    operation: "file not found".to_string(),
                    path: "file path not specified in error context".to_string(),
                });
            }
            std::io::ErrorKind::AlreadyExists => {
                return create_error_context(&AgpmError::FileSystemError {
                    operation: "file creation".to_string(),
                    path: "file path not specified in error context".to_string(),
                });
            }
            _ => {
                return ErrorContext::new(AgpmError::FileSystemError {
                    operation: "file operation".to_string(),
                    path: "unknown path".to_string(),
                })
                .with_suggestion("Check file permissions and disk space")
                .with_details(format!("IO error: {}", io_error));
            }
        }
    }

    // Walk the error chain again to check for specific error messages
    let mut current_error: &dyn std::error::Error = error.as_ref();
    loop {
        let error_msg = current_error.to_string();

        // Check for version resolution errors with no matching tags
        if error_msg.contains("No tags found") || error_msg.contains("No tag found") {
            return ErrorContext::new(AgpmError::Other {
                message: error_msg.clone(),
            })
            .with_suggestion("Check available tags with 'git tag -l' in the source repository, or adjust your version constraint")
            .with_details("No tags match the requested version constraint");
        }

        // Move to the next error in the chain
        match current_error.source() {
            Some(source) => current_error = source,
            None => break,
        }
    }

    // Try to extract context from the top-level error message
    let error_msg = error.to_string();

    // Check for template-related errors
    if TEMPLATE_ERROR_KEYWORDS.iter().any(|&keyword| error_msg.contains(keyword)) {
        return ErrorContext::new(AgpmError::Other {
            message: format!("Template error: {}", error_msg),
        })
        .with_suggestion("Check your template syntax and variable names")
        .with_details("Template rendering failed. Make sure all variables are defined and the syntax is correct.");
    }

    // Check for network-related errors
    if NETWORK_ERROR_KEYWORDS.iter().any(|&keyword| error_msg.contains(keyword)) {
        return ErrorContext::new(AgpmError::NetworkError {
            operation: "network request".to_string(),
            reason: error_msg.clone(),
        })
        .with_suggestion("Check your internet connection and try again")
        .with_details("A network operation failed. Please verify your connection and retry.");
    }

    // Check for git-related errors
    if GIT_ERROR_KEYWORDS.iter().any(|&keyword| error_msg.contains(keyword)) {
        return ErrorContext::new(AgpmError::GitCommandError {
            operation: "git operation".to_string(),
            stderr: error_msg.clone(),
        })
        .with_suggestion("Ensure git is installed and configured correctly")
        .with_details(
            "A git operation failed. Check that git is in your PATH and properly configured.",
        );
    }

    // Check for permission-related errors
    // Preserve the original error message to maintain context about what operation failed
    if PERMISSION_ERROR_KEYWORDS.iter().any(|&keyword| error_msg.contains(keyword)) {
        return ErrorContext::new(AgpmError::Other {
            message: error_msg.clone(),
        })
        .with_suggestion("Check file permissions and try running with appropriate privileges")
        .with_details("Permission was denied for the requested operation.");
    }

    // Default fallback for unknown errors
    ErrorContext::new(AgpmError::Other {
        message: error_msg,
    })
    .with_suggestion("Check the error message above for more details")
    .with_details("An unexpected error occurred. Please report this issue if it persists.")
}

/// Create a user-friendly error context from an [`AgpmError`]
///
/// This function analyzes the error type and provides:
/// - Clear, actionable error messages
/// - Specific suggestions based on the error type
/// - Additional details to help users understand and resolve the issue
pub fn create_error_context(error: &AgpmError) -> ErrorContext {
    match &error {
        AgpmError::GitNotFound => ErrorContext::new(AgpmError::GitNotFound)
            .with_suggestion("Install git from https://git-scm.com/ or your package manager")
            .with_details("AGPM requires git to be installed and available in your PATH"),
        AgpmError::ManifestNotFound => ErrorContext::new(AgpmError::ManifestNotFound)
            .with_suggestion("Run 'agpm init' to create a new manifest, or navigate to a directory with an existing agpm.toml")
            .with_details("AGPM searches for agpm.toml in the current directory and parent directories"),
        AgpmError::GitCommandError {
            operation,
            stderr,
        } => {
            let suggestion = match operation.as_str() {
                "fetch" => "Check your internet connection and try again",
                "checkout" => "Verify the branch, tag, or commit reference exists",
                "pull" => "Check your git configuration and remote settings",
                "clone" => "Verify the repository URL and your network connection",
                _ => "Ensure git is properly configured and try again",
            };
            ErrorContext::new(AgpmError::GitCommandError {
                operation: operation.clone(),
                stderr: stderr.clone(),
            })
            .with_suggestion(suggestion)
            .with_details(format!("Git {} operation failed: {}", operation, stderr))
        }
        AgpmError::GitCloneFailed {
            url,
            reason,
        } => ErrorContext::new(AgpmError::GitCloneFailed {
            url: url.clone(),
            reason: reason.clone(),
        })
        .with_suggestion(format!("Verify the repository URL '{}' is correct and accessible", url))
        .with_details(format!("Failed to clone repository: {}", reason)),
        AgpmError::ResourceNotFound {
            name,
        } => ErrorContext::new(AgpmError::ResourceNotFound {
            name: name.clone(),
        })
        .with_suggestion("Check that the resource is installed and available")
        .with_details(format!("Resource '{}' not found", name)),
        AgpmError::ResourceFileNotFound {
            path,
            source_name,
        } => ErrorContext::new(AgpmError::ResourceFileNotFound {
            path: path.clone(),
            source_name: source_name.clone(),
        })
        .with_suggestion(format!(
            "Check that '{}' exists in source '{}' and the version/tag is correct",
            path, source_name
        ))
        .with_details(format!("Resource file '{}' not found in source '{}'", path, source_name)),
        AgpmError::ManifestParseError {
            file,
            reason,
        } => ErrorContext::new(AgpmError::ManifestParseError {
            file: file.clone(),
            reason: reason.clone(),
        })
        .with_suggestion(format!("Check the syntax in '{}' - TOML format must be valid", file))
        .with_details(format!("Failed to parse manifest file: {}", reason)),
        AgpmError::FileSystemError {
            operation,
            path,
        } => ErrorContext::new(AgpmError::FileSystemError {
            operation: operation.clone(),
            path: path.clone(),
        })
        .with_suggestion("Check that the path exists and you have the necessary permissions")
        .with_details(format!("Failed to {} at path: {}", operation, path)),
        AgpmError::PermissionDenied {
            operation,
            path,
        } => ErrorContext::new(AgpmError::PermissionDenied {
            operation: operation.clone(),
            path: path.clone(),
        })
        .with_suggestion("Check file permissions and try running with appropriate privileges")
        .with_details(format!("Permission denied for '{}' on path: {}", operation, path)),
        AgpmError::DependencyResolutionMismatch {
            resource,
            declared_count,
            resolved_count,
            declared_deps,
        } => {
            let mut details = format!(
                "Declared {} dependencies in frontmatter:\n",
                declared_count
            );
            for (resource_type, path) in declared_deps {
                details.push_str(&format!("  - {}: {}\n", resource_type, path));
            }
            details.push_str(&format!("\nResolved: {} dependencies", resolved_count));

            ErrorContext::new(AgpmError::DependencyResolutionMismatch {
                resource: resource.clone(),
                declared_count: *declared_count,
                resolved_count: *resolved_count,
                declared_deps: declared_deps.clone(),
            })
            .with_suggestion(
                "This indicates a bug in dependency resolution. Run with RUST_LOG=debug for more details and report at https://github.com/aig787/agpm/issues",
            )
            .with_details(details)
        }
        // Default fallback for unhandled error types
        _ => ErrorContext::new(AgpmError::Other {
            message: error.to_string(),
        })
        .with_suggestion("Check the error message above for more details")
        .with_details("An unexpected error occurred. Please report this issue if it persists."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_user_friendly_error_io_permission_denied() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "Access denied");
        let error = anyhow::Error::from(io_err);
        let ctx = user_friendly_error(error);

        // IO permission errors are converted to PermissionDenied variant
        assert!(matches!(ctx.error, AgpmError::PermissionDenied { .. }));
        assert!(ctx.suggestion.is_some());
    }

    #[test]
    fn test_user_friendly_error_io_not_found() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "File not found");
        let error = anyhow::Error::from(io_err);
        let ctx = user_friendly_error(error);

        assert!(matches!(ctx.error, AgpmError::FileSystemError { .. }));
        assert!(ctx.suggestion.is_some());
    }

    #[test]
    fn test_user_friendly_error_template_error() {
        let error = anyhow::Error::msg("Failed to render template: variable 'foo' not found");
        let ctx = user_friendly_error(error);

        // Template errors become generic errors
        assert!(ctx.suggestion.is_some());
    }

    #[test]
    fn test_user_friendly_error_network_error() {
        let error = anyhow::Error::msg("Network connection failed");
        let ctx = user_friendly_error(error);

        assert!(matches!(ctx.error, AgpmError::NetworkError { .. }));
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("internet connection"));
    }

    #[test]
    fn test_user_friendly_error_git_error() {
        let error = anyhow::Error::msg("git command failed: repository not found");
        let ctx = user_friendly_error(error);

        assert!(matches!(ctx.error, AgpmError::GitCommandError { .. }));
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("git is installed"));
    }

    #[test]
    fn test_user_friendly_error_fallback() {
        let error = anyhow::Error::msg("Some completely unknown error type");
        let ctx = user_friendly_error(error);

        assert!(matches!(ctx.error, AgpmError::Other { .. }));
        assert!(ctx.suggestion.is_some());
        // The suggestion might vary, so just check it exists
    }

    #[test]
    fn test_dependency_resolution_mismatch_error_formatting() {
        let error = AgpmError::DependencyResolutionMismatch {
            resource: "agents/my-agent".to_string(),
            declared_count: 3,
            resolved_count: 0,
            declared_deps: vec![
                ("snippets".to_string(), "../../snippets/styleguide.md".to_string()),
                ("snippets".to_string(), "../../snippets/tooling.md".to_string()),
                ("agents".to_string(), "../helper.md".to_string()),
            ],
        };

        let ctx = create_error_context(&error);

        // Verify the error is correctly typed
        assert!(matches!(ctx.error, AgpmError::DependencyResolutionMismatch { .. }));

        // Verify suggestion contains bug report info
        let suggestion = ctx.suggestion.expect("Should have suggestion");
        assert!(suggestion.contains("bug"), "Suggestion should mention this is a bug");
        assert!(suggestion.contains("github"), "Suggestion should point to GitHub issues");

        // Verify details contain the declared dependencies
        let details = ctx.details.expect("Should have details");
        assert!(details.contains("Declared 3 dependencies"), "Details should show declared count");
        assert!(
            details.contains("snippets: ../../snippets/styleguide.md"),
            "Details should list declared deps"
        );
        assert!(details.contains("Resolved: 0 dependencies"), "Details should show resolved count");
    }
}
