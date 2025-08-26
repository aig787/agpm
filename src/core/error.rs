//! Error handling for CCPM
//!
//! This module provides comprehensive error types and user-friendly error reporting for the
//! CCPM package manager. The error system is designed around two core principles:
//! 1. **Strongly-typed errors** for precise error handling in code
//! 2. **User-friendly messages** with actionable suggestions for CLI users
//!
//! # Architecture
//!
//! The error system consists of two main types:
//! - [`CcpmError`] - Enumerated error types for all failure cases in CCPM
//! - [`ErrorContext`] - Wrapper that adds user-friendly messages and suggestions
//!
//! # Error Categories
//!
//! CCPM errors are organized into several categories:
//! - **Git Operations**: [`CcpmError::GitNotFound`], [`CcpmError::GitCommandError`], etc.
//! - **File System**: [`CcpmError::FileSystemError`], [`CcpmError::PermissionDenied`], etc.
//! - **Configuration**: [`CcpmError::ManifestNotFound`], [`CcpmError::ManifestParseError`], etc.
//! - **Dependencies**: [`CcpmError::CircularDependency`], [`CcpmError::DependencyNotMet`], etc.
//! - **Resources**: [`CcpmError::ResourceNotFound`], [`CcpmError::InvalidResource`], etc.
//!
//! # Error Conversion and Context
//!
//! Common standard library errors are automatically converted to CCPM errors:
//! - [`std::io::Error`] → [`CcpmError::IoError`]
//! - [`toml::de::Error`] → [`CcpmError::TomlError`]
//! - [`semver::Error`] → [`CcpmError::SemverError`]
//!
//! Use [`user_friendly_error`] to convert any error into a user-friendly format with
//! contextual suggestions.
//!
//! # Examples
//!
//! ## Basic Error Handling
//!
//! ```rust
//! use ccpm::core::{CcpmError, ErrorContext, user_friendly_error};
//!
//! fn handle_git_operation() -> Result<(), CcpmError> {
//!     // Simulate a git operation failure
//!     Err(CcpmError::GitNotFound)
//! }
//!
//! match handle_git_operation() {
//!     Ok(_) => println!("Success!"),
//!     Err(e) => {
//!         let ctx = user_friendly_error(anyhow::Error::from(e));
//!         ctx.display(); // Shows colored error with suggestions
//!     }
//! }
//! ```
//!
//! ## Creating Error Context Manually
//!
//! ```rust
//! use ccpm::core::{CcpmError, ErrorContext};
//!
//! let error = CcpmError::ManifestNotFound;
//! let context = ErrorContext::new(error)
//!     .with_suggestion("Create a ccpm.toml file in your project directory")
//!     .with_details("CCPM searches for ccpm.toml in current and parent directories");
//!
//! // Display with colors in terminal
//! context.display();
//!
//! // Or get as string for logging
//! let message = format!("{}", context);
//! ```
//!
//! ## Error Recovery Patterns
//!
//! ```rust
//! use ccpm::core::{CcpmError, user_friendly_error};
//! use anyhow::Context;
//!
//! fn install_dependency(name: &str) -> anyhow::Result<()> {
//!     // Try installation
//!     perform_installation(name)
//!         .with_context(|| format!("Failed to install dependency '{}'", name))
//!         .map_err(|e| {
//!             // Convert to user-friendly error for CLI display
//!             let friendly = user_friendly_error(e);
//!             friendly.display();
//!             anyhow::anyhow!("Installation failed")
//!         })
//! }
//!
//! fn perform_installation(_name: &str) -> anyhow::Result<()> {
//!     // Implementation would go here
//!     Ok(())
//! }
//! ```

use colored::*;
use std::fmt;
use thiserror::Error;

/// The main error type for CCPM operations
///
/// This enum represents all possible errors that can occur during CCPM operations.
/// Each variant is designed to provide specific context about the failure and enable
/// appropriate error handling strategies.
///
/// # Design Philosophy
///
/// - **Specific Error Types**: Each error variant represents a specific failure mode
/// - **Rich Context**: Errors include relevant details like file paths, URLs, and reasons
/// - **User-Friendly**: Error messages are written for end users, not just developers
/// - **Actionable**: Most errors provide clear guidance on how to resolve the issue
///
/// # Error Categories
///
/// ## Git Operations
/// - [`GitNotFound`] - Git executable not available
/// - [`GitCommandError`] - Git command execution failed
/// - [`GitAuthenticationFailed`] - Git authentication problems
/// - [`GitCloneFailed`] - Repository cloning failed
/// - [`GitCheckoutFailed`] - Git checkout operation failed
///
/// ## File System Operations  
/// - [`FileSystemError`] - General file system operations
/// - [`PermissionDenied`] - Insufficient permissions
/// - [`DirectoryNotEmpty`] - Directory contains files when empty expected
/// - [`IoError`] - Standard I/O errors from [`std::io::Error`]
///
/// ## Configuration and Parsing
/// - [`ManifestNotFound`] - ccpm.toml file missing
/// - [`ManifestParseError`] - Invalid TOML syntax in manifest
/// - [`ManifestValidationError`] - Manifest content validation failed
/// - [`LockfileParseError`] - Invalid lockfile format
/// - [`ConfigError`] - Configuration file issues
/// - [`TomlError`] - TOML parsing errors from [`toml::de::Error`]
/// - [`TomlSerError`] - TOML serialization errors from [`toml::ser::Error`]
///
/// ## Resource Management
/// - [`ResourceNotFound`] - Named resource doesn't exist
/// - [`ResourceFileNotFound`] - Resource file missing from repository
/// - [`InvalidResourceType`] - Unknown resource type specified
/// - [`InvalidResourceStructure`] - Resource content is malformed
/// - [`InvalidResource`] - Resource validation failed
/// - [`AlreadyInstalled`] - Resource already exists
///
/// ## Dependency Resolution
/// - [`CircularDependency`] - Dependency cycle detected
/// - [`DependencyResolutionFailed`] - Cannot resolve dependencies
/// - [`DependencyNotMet`] - Version constraint not satisfied
/// - [`InvalidDependency`] - Malformed dependency specification
/// - [`InvalidVersionConstraint`] - Invalid version format
/// - [`VersionNotFound`] - Requested version doesn't exist
/// - [`SemverError`] - Semantic version parsing from [`semver::Error`]
///
/// ## Source Management
/// - [`SourceNotFound`] - Named source not defined
/// - [`SourceUnreachable`] - Cannot connect to source repository
///
/// ## Platform and Network
/// - [`NetworkError`] - Network connectivity issues
/// - [`PlatformNotSupported`] - Operation not supported on current platform
/// - [`ChecksumMismatch`] - File integrity verification failed
///
/// # Examples
///
/// ## Pattern Matching on Errors
///
/// ```rust
/// use ccpm::core::CcpmError;
///
/// fn handle_error(error: CcpmError) {
///     match error {
///         CcpmError::GitNotFound => {
///             eprintln!("Please install git to use CCPM");
///             std::process::exit(1);
///         }
///         CcpmError::ManifestNotFound => {
///             eprintln!("Run 'ccpm init' to create a manifest file");
///         }
///         CcpmError::NetworkError { operation, .. } => {
///             eprintln!("Network error during {}: check your connection", operation);
///         }
///         _ => {
///             eprintln!("Unexpected error: {}", error);
///         }
///     }
/// }
/// ```
///
/// ## Creating Specific Errors
///
/// ```rust
/// use ccpm::core::CcpmError;
///
/// // Create a git command error with context
/// let error = CcpmError::GitCommandError {
///     operation: "clone".to_string(),
///     stderr: "repository not found".to_string(),
/// };
///
/// // Create a resource not found error
/// let error = CcpmError::ResourceNotFound {
///     name: "my-agent".to_string(),
/// };
///
/// // Create a version constraint error
/// let error = CcpmError::InvalidVersionConstraint {
///     constraint: "~1.x.y".to_string(),
/// };
/// ```
///
/// [`GitNotFound`]: CcpmError::GitNotFound
/// [`GitCommandError`]: CcpmError::GitCommandError
/// [`GitAuthenticationFailed`]: CcpmError::GitAuthenticationFailed
/// [`GitCloneFailed`]: CcpmError::GitCloneFailed
/// [`GitCheckoutFailed`]: CcpmError::GitCheckoutFailed
/// [`FileSystemError`]: CcpmError::FileSystemError
/// [`PermissionDenied`]: CcpmError::PermissionDenied
/// [`DirectoryNotEmpty`]: CcpmError::DirectoryNotEmpty
/// [`IoError`]: CcpmError::IoError
/// [`ManifestNotFound`]: CcpmError::ManifestNotFound
/// [`ManifestParseError`]: CcpmError::ManifestParseError
/// [`ManifestValidationError`]: CcpmError::ManifestValidationError
/// [`LockfileParseError`]: CcpmError::LockfileParseError
/// [`ConfigError`]: CcpmError::ConfigError
/// [`TomlError`]: CcpmError::TomlError
/// [`TomlSerError`]: CcpmError::TomlSerError
/// [`ResourceNotFound`]: CcpmError::ResourceNotFound
/// [`ResourceFileNotFound`]: CcpmError::ResourceFileNotFound
/// [`InvalidResourceType`]: CcpmError::InvalidResourceType
/// [`InvalidResourceStructure`]: CcpmError::InvalidResourceStructure
/// [`InvalidResource`]: CcpmError::InvalidResource
/// [`AlreadyInstalled`]: CcpmError::AlreadyInstalled
/// [`CircularDependency`]: CcpmError::CircularDependency
/// [`DependencyResolutionFailed`]: CcpmError::DependencyResolutionFailed
/// [`DependencyNotMet`]: CcpmError::DependencyNotMet
/// [`InvalidDependency`]: CcpmError::InvalidDependency
/// [`InvalidVersionConstraint`]: CcpmError::InvalidVersionConstraint
/// [`VersionNotFound`]: CcpmError::VersionNotFound
/// [`SemverError`]: CcpmError::SemverError
/// [`SourceNotFound`]: CcpmError::SourceNotFound
/// [`SourceUnreachable`]: CcpmError::SourceUnreachable
/// [`NetworkError`]: CcpmError::NetworkError
/// [`PlatformNotSupported`]: CcpmError::PlatformNotSupported
/// [`ChecksumMismatch`]: CcpmError::ChecksumMismatch
#[derive(Error, Debug)]
pub enum CcpmError {
    /// Git operation failed during execution
    ///
    /// This error occurs when a git command returns a non-zero exit code.
    /// Common causes include network issues, authentication problems, or
    /// invalid git repository states.
    ///
    /// # Fields
    /// - `operation`: The git operation that failed (e.g., "clone", "fetch", "checkout")
    /// - `stderr`: The error output from the git command
    #[error("Git operation failed: {operation}")]
    GitCommandError {
        /// The git operation that failed (e.g., "clone", "fetch", "checkout")
        operation: String,
        /// The error output from the git command
        stderr: String,
    },

    /// Git executable not found in PATH
    ///
    /// This error occurs when CCPM cannot locate the `git` command in the system PATH.
    /// CCPM requires git to be installed and available for repository operations.
    ///
    /// Common solutions:
    /// - Install git from <https://git-scm.com/>
    /// - Use a package manager: `brew install git`, `apt install git`, etc.
    /// - Ensure git is in your PATH environment variable
    #[error("Git is not installed or not found in PATH")]
    GitNotFound,

    /// Git repository is invalid or corrupted
    ///
    /// This error occurs when a directory exists but doesn't contain a valid
    /// git repository structure (missing .git directory or corrupted).
    ///
    /// # Fields
    /// - `path`: The path that was expected to contain a git repository
    #[error("Not a valid git repository: {path}")]
    GitRepoInvalid {
        /// The path that was expected to contain a git repository
        path: String,
    },

    /// Git authentication failed for repository access
    ///
    /// This error occurs when git cannot authenticate with a remote repository.
    /// Common for private repositories or when credentials are missing/expired.
    ///
    /// # Fields
    /// - `url`: The repository URL that failed authentication
    #[error("Git authentication failed for repository: {url}")]
    GitAuthenticationFailed {
        /// The repository URL that failed authentication
        url: String,
    },

    /// Git repository clone failed
    #[error("Failed to clone repository: {url}")]
    GitCloneFailed {
        /// The repository URL that failed to clone
        url: String,
        /// The reason for the clone failure
        reason: String,
    },

    /// Git checkout failed
    #[error("Failed to checkout reference '{reference}' in repository")]
    GitCheckoutFailed {
        /// The git reference (branch, tag, or commit) that failed to checkout
        reference: String,
        /// The reason for the checkout failure
        reason: String,
    },

    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigError {
        /// Description of the configuration error
        message: String,
    },

    /// Manifest file (ccpm.toml) not found
    ///
    /// This error occurs when CCPM cannot locate a ccpm.toml file in the current
    /// directory or any parent directory up to the filesystem root.
    ///
    /// CCPM searches for ccpm.toml starting from the current working directory
    /// and walking up the directory tree, similar to how git searches for .git.
    #[error("Manifest file ccpm.toml not found in current directory or any parent directory")]
    ManifestNotFound,

    /// Manifest parsing error
    #[error("Invalid manifest file syntax in {file}")]
    ManifestParseError {
        /// Path to the manifest file that failed to parse
        file: String,
        /// Specific reason for the parsing failure
        reason: String,
    },

    /// Manifest validation error
    #[error("Manifest validation failed: {reason}")]
    ManifestValidationError {
        /// Reason why manifest validation failed
        reason: String,
    },

    /// Lockfile parsing error
    #[error("Invalid lockfile syntax in {file}")]
    LockfileParseError {
        /// Path to the lockfile that failed to parse
        file: String,
        /// Specific reason for the parsing failure
        reason: String,
    },

    /// Resource not found
    #[error("Resource '{name}' not found")]
    ResourceNotFound {
        /// Name of the resource that could not be found
        name: String,
    },

    /// Resource file not found in repository
    #[error("Resource file '{path}' not found in source '{source_name}'")]
    ResourceFileNotFound {
        /// Path to the resource file within the source repository
        path: String,
        /// Name of the source repository where the file was expected
        source_name: String,
    },

    /// Source repository not found
    #[error("Source repository '{name}' not defined in manifest")]
    SourceNotFound {
        /// Name of the source repository that is not defined
        name: String,
    },

    /// Source repository unreachable
    #[error("Cannot reach source repository '{name}' at {url}")]
    SourceUnreachable {
        /// Name of the source repository
        name: String,
        /// URL of the unreachable repository
        url: String,
    },

    /// Invalid version constraint
    #[error("Invalid version constraint: {constraint}")]
    InvalidVersionConstraint {
        /// The invalid version constraint string
        constraint: String,
    },

    /// Version not found
    #[error("Version '{version}' not found for resource '{resource}'")]
    VersionNotFound {
        /// Name of the resource for which the version was not found
        resource: String,
        /// The version string that could not be found
        version: String,
    },

    /// Resource already installed
    #[error("Resource '{name}' is already installed")]
    AlreadyInstalled {
        /// Name of the resource that is already installed
        name: String,
    },

    /// Invalid resource type
    #[error("Invalid resource type: {resource_type}")]
    InvalidResourceType {
        /// The invalid resource type that was specified
        resource_type: String,
    },

    /// Invalid resource structure
    #[error("Invalid resource structure in '{file}': {reason}")]
    InvalidResourceStructure {
        /// Path to the file with invalid resource structure
        file: String,
        /// Reason why the resource structure is invalid
        reason: String,
    },

    /// Circular dependency detected in dependency graph
    ///
    /// This error occurs when resources depend on each other in a cycle,
    /// making it impossible to determine installation order.
    ///
    /// Example: A depends on B, B depends on C, C depends on A
    ///
    /// # Fields
    /// - `chain`: The dependency chain showing the circular reference
    #[error("Circular dependency detected: {chain}")]
    CircularDependency {
        /// String representation of the circular dependency chain
        chain: String,
    },

    /// Dependency resolution failed
    #[error("Cannot resolve dependencies: {reason}")]
    DependencyResolutionFailed {
        /// Reason why dependency resolution failed
        reason: String,
    },

    /// Network error
    #[error("Network error: {operation}")]
    NetworkError {
        /// The network operation that failed
        operation: String,
        /// Reason for the network failure
        reason: String,
    },

    /// File system error
    #[error("File system error: {operation}")]
    FileSystemError {
        /// The file system operation that failed
        operation: String,
        /// Path where the file system error occurred
        path: String,
    },

    /// Permission denied
    #[error("Permission denied: {operation}")]
    PermissionDenied {
        /// The operation that was denied due to insufficient permissions
        operation: String,
        /// Path where permission was denied
        path: String,
    },

    /// Directory not empty
    #[error("Directory is not empty: {path}")]
    DirectoryNotEmpty {
        /// Path to the directory that is not empty
        path: String,
    },

    /// Invalid dependency specification
    #[error("Invalid dependency specification for '{name}': {reason}")]
    InvalidDependency {
        /// Name of the invalid dependency
        name: String,
        /// Reason why the dependency specification is invalid
        reason: String,
    },

    /// Invalid resource content
    #[error("Invalid resource content in '{name}': {reason}")]
    InvalidResource {
        /// Name of the invalid resource
        name: String,
        /// Reason why the resource content is invalid
        reason: String,
    },

    /// Dependency not met
    #[error("Dependency '{name}' requires version {required}, but {found} was found")]
    DependencyNotMet {
        /// Name of the dependency that is not satisfied
        name: String,
        /// The required version constraint
        required: String,
        /// The version that was actually found
        found: String,
    },

    /// Config file not found
    #[error("Configuration file not found: {path}")]
    ConfigNotFound {
        /// Path to the configuration file that was not found
        path: String,
    },

    /// Checksum mismatch
    #[error("Checksum mismatch for resource '{name}': expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Name of the resource with checksum mismatch
        name: String,
        /// The expected checksum value
        expected: String,
        /// The actual checksum that was computed
        actual: String,
    },

    /// Platform not supported
    #[error("Operation not supported on this platform: {operation}")]
    PlatformNotSupported {
        /// The operation that is not supported on this platform
        operation: String,
    },

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// TOML parsing error
    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml::de::Error),

    /// TOML serialization error
    #[error("TOML serialization error: {0}")]
    TomlSerError(#[from] toml::ser::Error),

    /// Semver parsing error
    #[error("Semver parsing error: {0}")]
    SemverError(#[from] semver::Error),

    /// Other error
    #[error("{message}")]
    Other {
        /// Generic error message
        message: String,
    },
}

impl Clone for CcpmError {
    fn clone(&self) -> Self {
        match self {
            CcpmError::GitCommandError { operation, stderr } => CcpmError::GitCommandError {
                operation: operation.clone(),
                stderr: stderr.clone(),
            },
            CcpmError::GitNotFound => CcpmError::GitNotFound,
            CcpmError::GitRepoInvalid { path } => CcpmError::GitRepoInvalid { path: path.clone() },
            CcpmError::GitAuthenticationFailed { url } => {
                CcpmError::GitAuthenticationFailed { url: url.clone() }
            }
            CcpmError::GitCloneFailed { url, reason } => CcpmError::GitCloneFailed {
                url: url.clone(),
                reason: reason.clone(),
            },
            CcpmError::GitCheckoutFailed { reference, reason } => CcpmError::GitCheckoutFailed {
                reference: reference.clone(),
                reason: reason.clone(),
            },
            CcpmError::ConfigError { message } => CcpmError::ConfigError {
                message: message.clone(),
            },
            CcpmError::ManifestNotFound => CcpmError::ManifestNotFound,
            CcpmError::ManifestParseError { file, reason } => CcpmError::ManifestParseError {
                file: file.clone(),
                reason: reason.clone(),
            },
            CcpmError::ManifestValidationError { reason } => CcpmError::ManifestValidationError {
                reason: reason.clone(),
            },
            CcpmError::LockfileParseError { file, reason } => CcpmError::LockfileParseError {
                file: file.clone(),
                reason: reason.clone(),
            },
            CcpmError::ResourceNotFound { name } => {
                CcpmError::ResourceNotFound { name: name.clone() }
            }
            CcpmError::ResourceFileNotFound { path, source_name } => {
                CcpmError::ResourceFileNotFound {
                    path: path.clone(),
                    source_name: source_name.clone(),
                }
            }
            CcpmError::SourceNotFound { name } => CcpmError::SourceNotFound { name: name.clone() },
            CcpmError::SourceUnreachable { name, url } => CcpmError::SourceUnreachable {
                name: name.clone(),
                url: url.clone(),
            },
            CcpmError::InvalidVersionConstraint { constraint } => {
                CcpmError::InvalidVersionConstraint {
                    constraint: constraint.clone(),
                }
            }
            CcpmError::VersionNotFound { resource, version } => CcpmError::VersionNotFound {
                resource: resource.clone(),
                version: version.clone(),
            },
            CcpmError::AlreadyInstalled { name } => {
                CcpmError::AlreadyInstalled { name: name.clone() }
            }
            CcpmError::InvalidResourceType { resource_type } => CcpmError::InvalidResourceType {
                resource_type: resource_type.clone(),
            },
            CcpmError::InvalidResourceStructure { file, reason } => {
                CcpmError::InvalidResourceStructure {
                    file: file.clone(),
                    reason: reason.clone(),
                }
            }
            CcpmError::CircularDependency { chain } => CcpmError::CircularDependency {
                chain: chain.clone(),
            },
            CcpmError::DependencyResolutionFailed { reason } => {
                CcpmError::DependencyResolutionFailed {
                    reason: reason.clone(),
                }
            }
            CcpmError::NetworkError { operation, reason } => CcpmError::NetworkError {
                operation: operation.clone(),
                reason: reason.clone(),
            },
            CcpmError::FileSystemError { operation, path } => CcpmError::FileSystemError {
                operation: operation.clone(),
                path: path.clone(),
            },
            CcpmError::PermissionDenied { operation, path } => CcpmError::PermissionDenied {
                operation: operation.clone(),
                path: path.clone(),
            },
            CcpmError::DirectoryNotEmpty { path } => {
                CcpmError::DirectoryNotEmpty { path: path.clone() }
            }
            CcpmError::InvalidDependency { name, reason } => CcpmError::InvalidDependency {
                name: name.clone(),
                reason: reason.clone(),
            },
            CcpmError::InvalidResource { name, reason } => CcpmError::InvalidResource {
                name: name.clone(),
                reason: reason.clone(),
            },
            CcpmError::DependencyNotMet {
                name,
                required,
                found,
            } => CcpmError::DependencyNotMet {
                name: name.clone(),
                required: required.clone(),
                found: found.clone(),
            },
            CcpmError::ConfigNotFound { path } => CcpmError::ConfigNotFound { path: path.clone() },
            CcpmError::ChecksumMismatch {
                name,
                expected,
                actual,
            } => CcpmError::ChecksumMismatch {
                name: name.clone(),
                expected: expected.clone(),
                actual: actual.clone(),
            },
            CcpmError::PlatformNotSupported { operation } => CcpmError::PlatformNotSupported {
                operation: operation.clone(),
            },
            // For errors that don't implement Clone, convert to Other
            CcpmError::IoError(e) => CcpmError::Other {
                message: format!("IO error: {}", e),
            },
            CcpmError::TomlError(e) => CcpmError::Other {
                message: format!("TOML parsing error: {}", e),
            },
            CcpmError::TomlSerError(e) => CcpmError::Other {
                message: format!("TOML serialization error: {}", e),
            },
            CcpmError::SemverError(e) => CcpmError::Other {
                message: format!("Semver parsing error: {}", e),
            },
            CcpmError::Other { message } => CcpmError::Other {
                message: message.clone(),
            },
        }
    }
}

/// Error context wrapper that provides user-friendly error information
///
/// `ErrorContext` wraps a [`CcpmError`] and adds optional user-friendly messages,
/// suggestions for resolution, and additional details. This is the primary way
/// CCPM presents errors to CLI users.
///
/// # Design Philosophy
///
/// Error contexts are designed to be:
/// - **Actionable**: Include specific suggestions for resolving the error
/// - **Informative**: Provide context about why the error occurred
/// - **Colorized**: Use terminal colors to highlight important information
/// - **Consistent**: Follow a standard format across all error types
///
/// # Display Format
///
/// When displayed, errors show:
/// 1. **Error**: The main error message in red
/// 2. **Details**: Additional context about the error in yellow (optional)
/// 3. **Suggestion**: Actionable steps to resolve the issue in green (optional)
///
/// # Examples
///
/// ## Creating Error Context
///
/// ```rust
/// use ccpm::core::{CcpmError, ErrorContext};
///
/// let error = CcpmError::GitNotFound;
/// let context = ErrorContext::new(error)
///     .with_suggestion("Install git from https://git-scm.com/")
///     .with_details("CCPM requires git for repository operations");
///
/// // Display to terminal with colors
/// context.display();
///
/// // Or convert to string for logging
/// let message = context.to_string();
/// ```
///
/// ## Builder Pattern Usage
///
/// ```rust
/// use ccpm::core::{CcpmError, ErrorContext};
///
/// let context = ErrorContext::new(CcpmError::ManifestNotFound)
///     .with_suggestion("Create a ccpm.toml file in your project directory")
///     .with_details("CCPM searches current and parent directories for ccpm.toml");
///
/// println!("{}", context);
/// ```
///
/// ## Quick Suggestion Creation
///
/// ```rust
/// use ccpm::core::ErrorContext;
///
/// // Create context with just a suggestion (useful for generic errors)
/// let context = ErrorContext::suggestion("Try running the command with --verbose");
/// ```
#[derive(Debug)]
pub struct ErrorContext {
    /// The underlying CCPM error
    pub error: CcpmError,
    /// Optional suggestion for resolving the error
    pub suggestion: Option<String>,
    /// Optional additional details about the error
    pub details: Option<String>,
}

impl ErrorContext {
    /// Create a new error context from a [`CcpmError`]
    ///
    /// This creates a basic error context with no additional suggestions or details.
    /// Use the builder methods [`with_suggestion`] and [`with_details`] to add
    /// user-friendly information.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::{CcpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(CcpmError::GitNotFound);
    /// ```
    ///
    /// [`with_suggestion`]: ErrorContext::with_suggestion
    /// [`with_details`]: ErrorContext::with_details
    pub fn new(error: CcpmError) -> Self {
        Self {
            error,
            suggestion: None,
            details: None,
        }
    }

    /// Add a suggestion for resolving the error
    ///
    /// Suggestions should be actionable steps that users can take to resolve
    /// the error. They are displayed in green in the terminal to draw attention.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::{CcpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(CcpmError::GitNotFound)
    ///     .with_suggestion("Install git using 'brew install git' or visit https://git-scm.com/");
    /// ```
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Add additional details explaining the error
    ///
    /// Details provide context about why the error occurred or what it means.
    /// They are displayed in yellow in the terminal to provide additional context
    /// without being as prominent as the main error or suggestion.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::{CcpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(CcpmError::ManifestNotFound)
    ///     .with_details("CCPM looks for ccpm.toml in current directory and parent directories");
    /// ```
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Display the error context to stderr with terminal colors
    ///
    /// This method prints the error, details, and suggestion to stderr using
    /// color coding:
    /// - Error message: Red and bold
    /// - Details: Yellow
    /// - Suggestion: Green
    ///
    /// This is the primary way CCPM presents errors to users in the CLI.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::{CcpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(CcpmError::GitNotFound)
    ///     .with_suggestion("Install git from https://git-scm.com/")
    ///     .with_details("CCPM requires git for repository operations");
    ///
    /// context.display(); // Prints colored error to stderr
    /// ```
    pub fn display(&self) {
        eprintln!("{}: {}", "error".red().bold(), self.error);

        if let Some(details) = &self.details {
            eprintln!("{}: {}", "details".yellow(), details);
        }

        if let Some(suggestion) = &self.suggestion {
            eprintln!("{}: {}", "suggestion".green(), suggestion);
        }
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)?;

        if let Some(details) = &self.details {
            write!(f, "\nDetails: {}", details)?;
        }

        if let Some(suggestion) = &self.suggestion {
            write!(f, "\nSuggestion: {}", suggestion)?;
        }

        Ok(())
    }
}

impl std::error::Error for ErrorContext {}

/// Extension trait for converting [`CcpmError`] to [`anyhow::Error`] with context
///
/// This trait provides a method to convert CCPM-specific errors into generic
/// [`anyhow::Error`] instances while preserving user-friendly context information.
///
/// # Examples
///
/// ```rust
/// use ccpm::core::{CcpmError, ErrorContext, IntoAnyhowWithContext};
///
/// let error = CcpmError::GitNotFound;
/// let context = ErrorContext::new(CcpmError::Other { message: "dummy".to_string() })
///     .with_suggestion("Install git");
///
/// let anyhow_error = error.into_anyhow_with_context(context);
/// ```
pub trait IntoAnyhowWithContext {
    /// Convert the error to an [`anyhow::Error`] with the provided context
    fn into_anyhow_with_context(self, context: ErrorContext) -> anyhow::Error;
}

impl IntoAnyhowWithContext for CcpmError {
    fn into_anyhow_with_context(self, context: ErrorContext) -> anyhow::Error {
        anyhow::Error::new(ErrorContext {
            error: self,
            suggestion: context.suggestion,
            details: context.details,
        })
    }
}

impl ErrorContext {
    /// Create an [`ErrorContext`] with only a suggestion (no specific error)
    ///
    /// This is useful for generic errors where you want to provide a suggestion
    /// but don't have a specific [`CcpmError`] variant.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::ErrorContext;
    ///
    /// let context = ErrorContext::suggestion("Try running with --verbose for more information");
    /// context.display();
    /// ```
    pub fn suggestion(suggestion: impl Into<String>) -> Self {
        Self {
            error: CcpmError::Other {
                message: "".to_string(),
            },
            suggestion: Some(suggestion.into()),
            details: None,
        }
    }
}

/// Convert any error to a user-friendly [`ErrorContext`] with actionable suggestions
///
/// This function is the main entry point for converting arbitrary errors into
/// user-friendly error messages for CLI display. It recognizes common error types
/// and provides appropriate context and suggestions.
///
/// # Error Recognition
///
/// The function recognizes and provides specific handling for:
/// - [`CcpmError`] variants with tailored suggestions
/// - [`std::io::Error`] with filesystem-specific guidance
/// - [`toml::de::Error`] with TOML syntax help
/// - Generic errors with basic context
///
/// # Examples
///
/// ## Converting CCPM Errors
///
/// ```rust
/// use ccpm::core::{CcpmError, user_friendly_error};
///
/// let error = CcpmError::GitNotFound;
/// let anyhow_error = anyhow::Error::from(error);
/// let context = user_friendly_error(anyhow_error);
///
/// context.display(); // Shows git installation suggestions
/// ```
///
/// ## Converting IO Errors
///
/// ```rust
/// use ccpm::core::user_friendly_error;
/// use std::io::{Error, ErrorKind};
///
/// let io_error = Error::new(ErrorKind::PermissionDenied, "access denied");
/// let anyhow_error = anyhow::Error::from(io_error);
/// let context = user_friendly_error(anyhow_error);
///
/// context.display(); // Shows permission-related suggestions
/// ```
///
/// ## Converting Generic Errors
///
/// ```rust
/// use ccpm::core::user_friendly_error;
///
/// let error = anyhow::anyhow!("Something went wrong");
/// let context = user_friendly_error(error);
///
/// context.display(); // Shows the error with generic formatting
/// ```
pub fn user_friendly_error(error: anyhow::Error) -> ErrorContext {
    // Check for specific error types and provide helpful suggestions
    if let Some(ccmp_error) = error.downcast_ref::<CcpmError>() {
        return create_error_context(ccmp_error.clone());
    }

    if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
        match io_error.kind() {
            std::io::ErrorKind::PermissionDenied => {
                return ErrorContext::new(CcpmError::PermissionDenied {
                    operation: "file access".to_string(),
                    path: "unknown".to_string(),
                })
                .with_suggestion(
                    "Try running with elevated permissions (sudo/Administrator) or check file ownership",
                )
                .with_details("This error occurs when CCPM doesn't have permission to read or write files");
            }
            std::io::ErrorKind::NotFound => {
                return ErrorContext::new(CcpmError::FileSystemError {
                    operation: "file access".to_string(),
                    path: "unknown".to_string(),
                })
                .with_suggestion("Check that the file or directory exists and the path is correct")
                .with_details(
                    "This error occurs when a required file or directory cannot be found",
                );
            }
            std::io::ErrorKind::AlreadyExists => {
                return ErrorContext::new(CcpmError::FileSystemError {
                    operation: "file creation".to_string(),
                    path: "unknown".to_string(),
                })
                .with_suggestion("Remove the existing file or use --force to overwrite")
                .with_details("The target file or directory already exists");
            }
            std::io::ErrorKind::InvalidData => {
                return ErrorContext::new(CcpmError::InvalidResource {
                    name: "unknown".to_string(),
                    reason: "invalid file format".to_string(),
                })
                .with_suggestion("Check the file format and ensure it's a valid resource file")
                .with_details("The file contains invalid or corrupted data");
            }
            _ => {}
        }
    }

    if let Some(toml_error) = error.downcast_ref::<toml::de::Error>() {
        return ErrorContext::new(CcpmError::ManifestParseError {
            file: "ccpm.toml".to_string(),
            reason: toml_error.to_string(),
        })
        .with_suggestion("Check the TOML syntax in your ccpm.toml file. Verify quotes, brackets, and indentation")
        .with_details("TOML parsing errors are usually caused by syntax issues like missing quotes or mismatched brackets");
    }

    // Generic error
    ErrorContext::new(CcpmError::Other {
        message: error.to_string(),
    })
}

/// Create appropriate [`ErrorContext`] with suggestions for specific CCPM errors
///
/// This internal function maps each [`CcpmError`] variant to an appropriate
/// [`ErrorContext`] with tailored suggestions and details. It's used by
/// [`user_friendly_error`] to provide consistent, helpful error messages.
///
/// # Implementation Notes
///
/// - Each error type has specific suggestions based on common resolution steps
/// - Platform-specific suggestions are provided where applicable
/// - Error messages focus on actionable steps rather than technical details
/// - Cross-references to related commands or documentation are included
fn create_error_context(error: CcpmError) -> ErrorContext {
    match &error {
        CcpmError::GitNotFound => ErrorContext::new(CcpmError::GitNotFound)
            .with_suggestion("Install git from https://git-scm.com/ or your package manager (e.g., 'brew install git', 'apt install git')")
            .with_details("CCPM requires git to be installed and available in your PATH to manage repositories"),

        CcpmError::GitCommandError { operation, stderr } => {
            ErrorContext::new(CcpmError::GitCommandError {
                operation: operation.clone(),
                stderr: stderr.clone(),
            })
            .with_suggestion(match operation.as_str() {
                op if op.contains("clone") => "Check the repository URL and your internet connection. Verify you have access to the repository",
                op if op.contains("fetch") => "Check your internet connection and repository access. Try 'git fetch' manually in the repository directory",
                op if op.contains("checkout") => "Verify the branch, tag, or commit exists. Use 'git tag -l' or 'git branch -r' to list available references",
                _ => "Check your git configuration and repository access. Try running the git command manually for more details",
            })
            .with_details("Git operations failed. This is often due to network issues, authentication problems, or invalid references")
        }

        CcpmError::GitAuthenticationFailed { url } => ErrorContext::new(CcpmError::GitAuthenticationFailed {
            url: url.clone(),
        })
            .with_suggestion("Configure git authentication: use 'git config --global user.name' and 'git config --global user.email', or set up SSH keys")
            .with_details("Authentication is required for private repositories. You may need to log in with 'git credential-manager-core' or similar"),

        CcpmError::GitCloneFailed { url, reason } => ErrorContext::new(CcpmError::GitCloneFailed {
            url: url.clone(),
            reason: reason.clone(),
        })
            .with_suggestion(format!(
                "Verify the repository URL is correct: {}. Check your internet connection and repository access",
                url
            ))
            .with_details("Clone operations can fail due to invalid URLs, network issues, or access restrictions"),

        CcpmError::ManifestNotFound => ErrorContext::new(CcpmError::ManifestNotFound)
            .with_suggestion("Create a ccpm.toml file in your project directory. See documentation for the manifest format")
            .with_details("CCPM looks for ccpm.toml in the current directory and parent directories up to the filesystem root"),

        CcpmError::ManifestParseError { file, reason } => ErrorContext::new(CcpmError::ManifestParseError {
            file: file.clone(),
            reason: reason.clone(),
        })
            .with_suggestion(format!(
                "Check the TOML syntax in {}. Common issues: missing quotes, unmatched brackets, invalid characters",
                file
            ))
            .with_details("Use a TOML validator or check the ccpm documentation for correct manifest format"),

        CcpmError::SourceNotFound { name } => ErrorContext::new(CcpmError::SourceNotFound {
            name: name.clone(),
        })
            .with_suggestion(format!(
                "Add source '{}' to the [sources] section in ccpm.toml with the repository URL",
                name
            ))
            .with_details("All dependencies must reference a source defined in the [sources] section"),

        CcpmError::ResourceFileNotFound { path, source_name } => ErrorContext::new(CcpmError::ResourceFileNotFound {
            path: path.clone(),
            source_name: source_name.clone(),
        })
            .with_suggestion(format!(
                "Verify the file '{}' exists in the '{}' repository at the specified version/commit",
                path, source_name
            ))
            .with_details("The resource file may have been moved, renamed, or deleted in the repository"),

        CcpmError::VersionNotFound { resource, version } => ErrorContext::new(CcpmError::VersionNotFound {
            resource: resource.clone(),
            version: version.clone(),
        })
            .with_suggestion(format!(
                "Check available versions for '{}' using 'git tag -l' in the repository, or use 'main' or 'master' branch",
                resource
            ))
            .with_details(format!(
                "The version '{}' doesn't exist as a git tag, branch, or commit in the repository",
                version
            )),

        CcpmError::CircularDependency { chain } => ErrorContext::new(CcpmError::CircularDependency {
            chain: chain.clone(),
        })
            .with_suggestion("Review your dependency graph and remove circular references")
            .with_details(format!(
                "Circular dependency chain detected: {}. Dependencies cannot depend on themselves directly or indirectly",
                chain
            )),

        CcpmError::PermissionDenied { operation, path } => ErrorContext::new(CcpmError::PermissionDenied {
            operation: operation.clone(),
            path: path.clone(),
        })
            .with_suggestion(match cfg!(windows) {
                true => "Run as Administrator or check file permissions in File Explorer",
                false => "Use 'sudo' or check file permissions with 'ls -la'",
            })
            .with_details(format!(
                "Cannot {} due to insufficient permissions on {}",
                operation, path
            )),

        CcpmError::ChecksumMismatch { name, expected, actual } => ErrorContext::new(CcpmError::ChecksumMismatch {
            name: name.clone(),
            expected: expected.clone(),
            actual: actual.clone(),
        })
            .with_suggestion("The file may have been corrupted or modified. Try reinstalling with --force")
            .with_details(format!(
                "Resource '{}' has checksum {} but expected {}. This indicates file corruption or tampering",
                name, actual, expected
            )),

        _ => ErrorContext::new(error.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = CcpmError::GitNotFound;
        assert_eq!(
            error.to_string(),
            "Git is not installed or not found in PATH"
        );

        let error = CcpmError::ResourceNotFound {
            name: "test".to_string(),
        };
        assert_eq!(error.to_string(), "Resource 'test' not found");

        let error = CcpmError::InvalidVersionConstraint {
            constraint: "bad-version".to_string(),
        };
        assert_eq!(error.to_string(), "Invalid version constraint: bad-version");

        let error = CcpmError::GitCommandError {
            operation: "clone".to_string(),
            stderr: "repository not found".to_string(),
        };
        assert_eq!(error.to_string(), "Git operation failed: clone");
    }

    #[test]
    fn test_error_context() {
        let ctx = ErrorContext::new(CcpmError::GitNotFound)
            .with_suggestion("Install git using your package manager")
            .with_details("Git is required for CCPM to function");

        assert_eq!(
            ctx.suggestion,
            Some("Install git using your package manager".to_string())
        );
        assert_eq!(
            ctx.details,
            Some("Git is required for CCPM to function".to_string())
        );
    }

    #[test]
    fn test_error_context_display() {
        let ctx = ErrorContext::new(CcpmError::GitNotFound).with_suggestion("Install git");

        let display = format!("{}", ctx);
        assert!(display.contains("Git is not installed or not found in PATH"));
        assert!(display.contains("Install git"));
    }

    #[test]
    fn test_user_friendly_error_permission_denied() {
        use std::io::{Error, ErrorKind};

        let io_error = Error::new(ErrorKind::PermissionDenied, "access denied");
        let anyhow_error = anyhow::Error::from(io_error);

        let ctx = user_friendly_error(anyhow_error);
        match ctx.error {
            CcpmError::PermissionDenied { .. } => {}
            _ => panic!("Expected PermissionDenied error"),
        }
        assert!(ctx.suggestion.is_some());
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_user_friendly_error_not_found() {
        use std::io::{Error, ErrorKind};

        let io_error = Error::new(ErrorKind::NotFound, "file not found");
        let anyhow_error = anyhow::Error::from(io_error);

        let ctx = user_friendly_error(anyhow_error);
        match ctx.error {
            CcpmError::FileSystemError { .. } => {}
            _ => panic!("Expected FileSystemError"),
        }
        assert!(ctx.suggestion.is_some());
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_from_io_error() {
        use std::io::Error;

        let io_error = Error::other("test error");
        let ccpm_error = CcpmError::from(io_error);

        match ccpm_error {
            CcpmError::IoError(_) => {}
            _ => panic!("Expected IoError"),
        }
    }

    #[test]
    fn test_from_toml_error() {
        let toml_str = "invalid = toml {";
        let result: Result<toml::Value, _> = toml::from_str(toml_str);

        if let Err(e) = result {
            let ccpm_error = CcpmError::from(e);
            match ccpm_error {
                CcpmError::TomlError(_) => {}
                _ => panic!("Expected TomlError"),
            }
        }
    }

    #[test]
    fn test_create_error_context_git_not_found() {
        let ctx = create_error_context(CcpmError::GitNotFound);
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("Install git"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_git_command_error() {
        let ctx = create_error_context(CcpmError::GitCommandError {
            operation: "clone".to_string(),
            stderr: "error".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("repository URL"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_git_auth_failed() {
        let ctx = create_error_context(CcpmError::GitAuthenticationFailed {
            url: "https://github.com/test/repo".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx
            .suggestion
            .unwrap()
            .contains("Configure git authentication"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_manifest_not_found() {
        let ctx = create_error_context(CcpmError::ManifestNotFound);
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("Create a ccpm.toml"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_source_not_found() {
        let ctx = create_error_context(CcpmError::SourceNotFound {
            name: "test-source".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("test-source"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_version_not_found() {
        let ctx = create_error_context(CcpmError::VersionNotFound {
            resource: "test-resource".to_string(),
            version: "v1.0.0".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("test-resource"));
        assert!(ctx.details.is_some());
        assert!(ctx.details.unwrap().contains("v1.0.0"));
    }

    #[test]
    fn test_create_error_context_circular_dependency() {
        let ctx = create_error_context(CcpmError::CircularDependency {
            chain: "a -> b -> c -> a".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("remove circular"));
        assert!(ctx.details.is_some());
        assert!(ctx.details.unwrap().contains("a -> b -> c -> a"));
    }

    #[test]
    fn test_create_error_context_permission_denied() {
        let ctx = create_error_context(CcpmError::PermissionDenied {
            operation: "write".to_string(),
            path: "/test/path".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.details.is_some());
        assert!(ctx.details.unwrap().contains("/test/path"));
    }

    #[test]
    fn test_create_error_context_checksum_mismatch() {
        let ctx = create_error_context(CcpmError::ChecksumMismatch {
            name: "test-resource".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("reinstalling"));
        assert!(ctx.details.is_some());
        assert!(ctx.details.unwrap().contains("abc123"));
    }

    #[test]
    fn test_error_clone() {
        let error1 = CcpmError::GitNotFound;
        let error2 = error1.clone();
        assert_eq!(error1.to_string(), error2.to_string());

        let error1 = CcpmError::ResourceNotFound {
            name: "test".to_string(),
        };
        let error2 = error1.clone();
        assert_eq!(error1.to_string(), error2.to_string());
    }

    #[test]
    fn test_error_context_suggestion() {
        let ctx = ErrorContext::suggestion("Test suggestion");
        assert_eq!(ctx.suggestion, Some("Test suggestion".to_string()));
        assert!(ctx.details.is_none());
    }

    #[test]
    fn test_into_anyhow_with_context() {
        let error = CcpmError::GitNotFound;
        let context = ErrorContext::new(CcpmError::Other {
            message: "dummy".to_string(),
        })
        .with_suggestion("Test suggestion")
        .with_details("Test details");

        let anyhow_error = error.into_anyhow_with_context(context);
        let display = format!("{}", anyhow_error);
        assert!(display.contains("Git is not installed"));
    }

    #[test]
    fn test_user_friendly_error_already_exists() {
        use std::io::{Error, ErrorKind};

        let io_error = Error::new(ErrorKind::AlreadyExists, "file exists");
        let anyhow_error = anyhow::Error::from(io_error);

        let ctx = user_friendly_error(anyhow_error);
        match ctx.error {
            CcpmError::FileSystemError { .. } => {}
            _ => panic!("Expected FileSystemError"),
        }
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("overwrite"));
    }

    #[test]
    fn test_user_friendly_error_invalid_data() {
        use std::io::{Error, ErrorKind};

        let io_error = Error::new(ErrorKind::InvalidData, "corrupt data");
        let anyhow_error = anyhow::Error::from(io_error);

        let ctx = user_friendly_error(anyhow_error);
        match ctx.error {
            CcpmError::InvalidResource { .. } => {}
            _ => panic!("Expected InvalidResource"),
        }
        assert!(ctx.suggestion.is_some());
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_user_friendly_error_ccpm_error() {
        let error = CcpmError::GitNotFound;
        let anyhow_error = anyhow::Error::from(error);

        let ctx = user_friendly_error(anyhow_error);
        match ctx.error {
            CcpmError::GitNotFound => {}
            _ => panic!("Expected GitNotFound"),
        }
        assert!(ctx.suggestion.is_some());
    }

    #[test]
    fn test_user_friendly_error_toml_parse() {
        let toml_str = "invalid = toml {";
        let result: Result<toml::Value, _> = toml::from_str(toml_str);

        if let Err(e) = result {
            let anyhow_error = anyhow::Error::from(e);
            let ctx = user_friendly_error(anyhow_error);

            match ctx.error {
                CcpmError::ManifestParseError { .. } => {}
                _ => panic!("Expected ManifestParseError"),
            }
            assert!(ctx.suggestion.is_some());
            assert!(ctx.suggestion.unwrap().contains("TOML syntax"));
        }
    }

    #[test]
    fn test_user_friendly_error_generic() {
        let error = anyhow::anyhow!("Generic error");
        let ctx = user_friendly_error(error);

        match ctx.error {
            CcpmError::Other { message } => {
                assert_eq!(message, "Generic error");
            }
            _ => panic!("Expected Other error"),
        }
    }

    #[test]
    fn test_from_semver_error() {
        let result = semver::Version::parse("invalid-version");
        if let Err(e) = result {
            let ccpm_error = CcpmError::from(e);
            match ccpm_error {
                CcpmError::SemverError(_) => {}
                _ => panic!("Expected SemverError"),
            }
        }
    }

    #[test]
    fn test_error_display_all_variants() {
        // Test display for various error variants
        let errors = vec![
            CcpmError::GitRepoInvalid {
                path: "/test/path".to_string(),
            },
            CcpmError::GitCheckoutFailed {
                reference: "main".to_string(),
                reason: "not found".to_string(),
            },
            CcpmError::ConfigError {
                message: "config issue".to_string(),
            },
            CcpmError::ManifestValidationError {
                reason: "invalid format".to_string(),
            },
            CcpmError::LockfileParseError {
                file: "ccpm.lock".to_string(),
                reason: "syntax error".to_string(),
            },
            CcpmError::ResourceFileNotFound {
                path: "test.md".to_string(),
                source_name: "source".to_string(),
            },
            CcpmError::DirectoryNotEmpty {
                path: "/some/dir".to_string(),
            },
            CcpmError::InvalidDependency {
                name: "dep".to_string(),
                reason: "bad format".to_string(),
            },
            CcpmError::DependencyNotMet {
                name: "dep".to_string(),
                required: "v1.0".to_string(),
                found: "v2.0".to_string(),
            },
            CcpmError::ConfigNotFound {
                path: "/config/path".to_string(),
            },
            CcpmError::PlatformNotSupported {
                operation: "test op".to_string(),
            },
        ];

        for error in errors {
            let display = format!("{}", error);
            assert!(!display.is_empty());
        }
    }

    #[test]
    fn test_create_error_context_git_operations() {
        // Test different git operations
        let operations = vec![
            ("fetch", "internet connection"),
            ("checkout", "branch, tag"),
            ("pull", "git configuration"),
        ];

        for (op, expected_text) in operations {
            let ctx = create_error_context(CcpmError::GitCommandError {
                operation: op.to_string(),
                stderr: "error".to_string(),
            });
            assert!(ctx.suggestion.is_some());
            assert!(ctx
                .suggestion
                .unwrap()
                .to_lowercase()
                .contains(expected_text));
        }
    }

    #[test]
    fn test_create_error_context_resource_file_not_found() {
        let ctx = create_error_context(CcpmError::ResourceFileNotFound {
            path: "agents/test.md".to_string(),
            source_name: "official".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        let suggestion = ctx.suggestion.unwrap();
        assert!(suggestion.contains("agents/test.md"));
        assert!(suggestion.contains("official"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_manifest_parse_error() {
        let ctx = create_error_context(CcpmError::ManifestParseError {
            file: "custom.toml".to_string(),
            reason: "invalid syntax".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        let suggestion = ctx.suggestion.unwrap();
        assert!(suggestion.contains("custom.toml"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_git_clone_failed() {
        let ctx = create_error_context(CcpmError::GitCloneFailed {
            url: "https://example.com/repo.git".to_string(),
            reason: "network error".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        let suggestion = ctx.suggestion.unwrap();
        assert!(suggestion.contains("https://example.com/repo.git"));
        assert!(ctx.details.is_some());
    }
}
