//! Structured file system error handling for AGPM
//!
//! This module provides better error handling for file operations by capturing
//! context at the operation site rather than parsing error messages.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Detailed file operation context for better error messages
#[derive(Debug, Clone)]
pub struct FileOperationContext {
    /// The type of operation being performed
    pub operation: FileOperation,
    /// The file path being accessed
    pub file_path: PathBuf,
    /// Additional context about why the file is being accessed
    pub purpose: String,
    /// The resource/caller that initiated the operation
    pub caller: String,
    /// Optional related paths (e.g., project directory)
    pub related_paths: Vec<PathBuf>,
}

/// Types of file operations
#[derive(Debug, Clone, PartialEq)]
pub enum FileOperation {
    /// Reading a file completely
    Read,
    /// Writing a file
    Write,
    /// Checking if a file exists
    Exists,
    /// Getting file metadata
    Metadata,
    /// Canonicalizing a path
    Canonicalize,
    /// Creating a directory
    CreateDir,
    /// Validating a file path (security checks)
    Validate,
}

impl std::fmt::Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOperation::Read => write!(f, "reading"),
            FileOperation::Write => write!(f, "writing"),
            FileOperation::Exists => write!(f, "checking if file exists"),
            FileOperation::Metadata => write!(f, "getting file metadata"),
            FileOperation::Canonicalize => write!(f, "resolving path"),
            FileOperation::CreateDir => write!(f, "creating directory"),
            FileOperation::Validate => write!(f, "validating file path"),
        }
    }
}

impl FileOperationContext {
    /// Create a new file operation context
    pub fn new(
        operation: FileOperation,
        file_path: impl Into<PathBuf>,
        purpose: impl Into<String>,
        caller: impl Into<String>,
    ) -> Self {
        Self {
            operation,
            file_path: file_path.into(),
            purpose: purpose.into(),
            caller: caller.into(),
            related_paths: Vec::new(),
        }
    }

    /// Add a related path for context
    pub fn with_related_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.related_paths.push(path.into());
        self
    }

    /// Add multiple related paths
    pub fn with_related_paths<I>(mut self, paths: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<PathBuf>,
    {
        for path in paths {
            self.related_paths.push(path.into());
        }
        self
    }
}

/// Enhanced file operation error with full context
#[derive(Error, Debug)]
#[error("File operation failed: {operation} on {file_path}")]
pub struct FileOperationError {
    /// The type of operation that failed
    pub operation: FileOperation,
    /// The file path that was being accessed
    pub file_path: PathBuf,
    /// Why the file was being accessed
    pub purpose: String,
    /// What code initiated the operation
    pub caller: String,
    /// The underlying IO error
    #[source]
    pub source: std::io::Error,
    /// Related paths for additional context
    pub related_paths: Vec<PathBuf>,
}

impl FileOperationError {
    /// Create a new file operation error from context and IO error
    pub fn new(context: FileOperationContext, source: std::io::Error) -> Self {
        Self {
            operation: context.operation,
            file_path: context.file_path,
            purpose: context.purpose,
            caller: context.caller,
            source,
            related_paths: context.related_paths,
        }
    }

    /// Get a user-friendly error message with context
    pub fn user_message(&self) -> String {
        let operation_name = match self.operation {
            FileOperation::Read => "reading",
            FileOperation::Write => "writing",
            FileOperation::Exists => "checking if file exists",
            FileOperation::Metadata => "getting file metadata",
            FileOperation::Canonicalize => "resolving path",
            FileOperation::CreateDir => "creating directory",
            FileOperation::Validate => "validating file path",
        };

        let mut message = format!(
            "Failed {} file '{}' for {} ({})",
            operation_name,
            self.file_path.display(),
            self.purpose,
            self.caller
        );

        // Add specific error details
        match self.source.kind() {
            std::io::ErrorKind::NotFound => {
                message.push_str("\n\nThe file does not exist at the specified path.");

                // Add helpful suggestions based on file type and purpose
                if self.file_path.extension().and_then(|s| s.to_str()) == Some("md") {
                    message.push_str("\n\nFor markdown files, check:");
                    message.push_str("\n- The file exists in the expected location");
                    message.push_str("\n- The filename is spelled correctly (case-sensitive)");
                    message.push_str(&format!(
                        "\n- The file should be relative to: {}",
                        self.related_paths
                            .first()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "project root".to_string())
                    ));
                }

                if self.purpose.contains("template") || self.purpose.contains("render") {
                    message.push_str("\n\nFor template errors, ensure:");
                    message.push_str("\n- All referenced files exist");
                    message.push_str("\n- File paths in templates are correct");
                    message.push_str("\n- Dependencies are properly declared in frontmatter");
                }
            }
            std::io::ErrorKind::PermissionDenied => {
                message.push_str(&format!(
                    "\n\nPermission denied. Check file/directory permissions for: {}",
                    self.file_path.display()
                ));
            }
            std::io::ErrorKind::InvalidData => {
                message.push_str("\n\nThe file contains invalid data or encoding.");
                if self.purpose.contains("UTF-8") || self.purpose.contains("read") {
                    message.push_str("\nEnsure the file contains valid UTF-8 text.");
                }
            }
            _ => {
                message.push_str(&format!("\n\nError details: {}", self.source));
            }
        }

        // Add related paths context
        if !self.related_paths.is_empty() {
            message.push_str("\n\nRelated paths:");
            for path in &self.related_paths {
                message.push_str(&format!("\n  - {}", path.display()));
            }
        }

        message
    }
}

/// Extension trait for Result types to add file operation context
pub trait FileResultExt<T> {
    /// Add file operation context to a Result
    fn with_file_context(
        self,
        operation: FileOperation,
        file_path: impl Into<PathBuf>,
        purpose: impl Into<String>,
        caller: impl Into<String>,
    ) -> Result<T, FileOperationError>;
}

impl<T> FileResultExt<T> for Result<T, std::io::Error> {
    fn with_file_context(
        self,
        operation: FileOperation,
        file_path: impl Into<PathBuf>,
        purpose: impl Into<String>,
        caller: impl Into<String>,
    ) -> Result<T, FileOperationError> {
        self.map_err(|io_error| {
            let context = FileOperationContext::new(operation, file_path, purpose, caller);
            FileOperationError::new(context, io_error)
        })
    }
}

/// Convenience functions for common file operations with context
pub struct FileOps;

impl FileOps {
    /// Read a file with full context
    pub async fn read_with_context(
        path: &Path,
        purpose: &str,
        caller: &str,
    ) -> Result<String, FileOperationError> {
        tokio::fs::read_to_string(path).await.with_file_context(
            FileOperation::Read,
            path,
            purpose,
            caller,
        )
    }

    /// Check if a file exists with context
    pub async fn exists_with_context(
        path: &Path,
        purpose: &str,
        caller: &str,
    ) -> Result<bool, FileOperationError> {
        tokio::fs::metadata(path)
            .await
            .map(|_| true)
            .or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(false)
                } else {
                    Err(e)
                }
            })
            .with_file_context(FileOperation::Exists, path, purpose, caller)
    }

    /// Get file metadata with context
    pub async fn metadata_with_context(
        path: &Path,
        purpose: &str,
        caller: &str,
    ) -> Result<std::fs::Metadata, FileOperationError> {
        tokio::fs::metadata(path).await.with_file_context(
            FileOperation::Metadata,
            path,
            purpose,
            caller,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn test_file_operation_context_creation() {
        let context = FileOperationContext::new(
            FileOperation::Read,
            "/path/to/file.md",
            "template rendering",
            "content_filter",
        );

        assert_eq!(context.operation, FileOperation::Read);
        assert_eq!(context.file_path, PathBuf::from("/path/to/file.md"));
        assert_eq!(context.purpose, "template rendering");
        assert_eq!(context.caller, "content_filter");
    }

    #[test]
    fn test_file_operation_error_user_message() {
        let io_error = Error::new(ErrorKind::NotFound, "file not found");
        let context = FileOperationContext::new(
            FileOperation::Read,
            "docs/styleguide.md",
            "template rendering",
            "content_filter",
        )
        .with_related_path("/project/root");

        let file_error = FileOperationError::new(context, io_error);
        let message = file_error.user_message();

        assert!(message.contains("Failed reading file"));
        assert!(message.contains("docs/styleguide.md"));
        assert!(message.contains("template rendering"));
        assert!(message.contains("content_filter"));
        assert!(message.contains("does not exist"));
        assert!(message.contains("Related paths"));
    }

    #[test]
    fn test_file_result_ext() {
        let io_error = Error::new(ErrorKind::PermissionDenied, "access denied");
        let result: Result<String, std::io::Error> = Err(io_error);

        let enhanced_result = result.with_file_context(
            FileOperation::Write,
            "/tmp/test.txt",
            "saving configuration",
            "config_module",
        );

        assert!(enhanced_result.is_err());
        let error = enhanced_result.unwrap_err();
        assert_eq!(error.operation, FileOperation::Write);
        assert_eq!(error.purpose, "saving configuration");
        assert_eq!(error.caller, "config_module");
    }
}
