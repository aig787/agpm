//! Error handling for AGPM
//!
//! This module provides comprehensive error types and user-friendly error reporting for the
//! AGPM package manager. The error system is designed around two core principles:
//! 1. **Strongly-typed errors** for precise error handling in code
//! 2. **User-friendly messages** with actionable suggestions for CLI users
//!
//! # Architecture
//!
//! The error system consists of two main types:
//! - [`AgpmError`] - Enumerated error types for all failure cases in AGPM
//! - [`ErrorContext`] - Wrapper that adds user-friendly messages and suggestions
//!
//! # Error Categories
//!
//! AGPM errors are organized into several categories:
//! - **Git Operations**: [`AgpmError::GitNotFound`], [`AgpmError::GitCommandError`], etc.
//! - **File System**: [`AgpmError::FileSystemError`], [`AgpmError::PermissionDenied`], etc.
//! - **Configuration**: [`AgpmError::ManifestNotFound`], [`AgpmError::ManifestParseError`], etc.
//! - **Dependencies**: [`AgpmError::CircularDependency`], [`AgpmError::DependencyNotMet`], etc.
//! - **Resources**: [`AgpmError::ResourceNotFound`], [`AgpmError::InvalidResource`], etc.
//!
//! # Error Conversion and Context
//!
//! Common standard library errors are automatically converted to AGPM errors:
//! - [`std::io::Error`] → [`AgpmError::IoError`]
//! - [`toml::de::Error`] → [`AgpmError::TomlError`]
//! - [`semver::Error`] → [`AgpmError::SemverError`]
//!
//! Use [`user_friendly_error`] to convert any error into a user-friendly format with
//! contextual suggestions.
//!
//! # Examples
//!
//! ## Basic Error Handling
//!
//! ```rust,no_run
//! use agpm_cli::core::{AgpmError, ErrorContext, user_friendly_error};
//!
//! fn handle_git_operation() -> Result<(), AgpmError> {
//!     // Simulate a git operation failure
//!     Err(AgpmError::GitNotFound)
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
//! ```rust,no_run
//! use agpm_cli::core::{AgpmError, ErrorContext};
//!
//! let error = AgpmError::ManifestNotFound;
//! let context = ErrorContext::new(error)
//!     .with_suggestion("Create a agpm.toml file in your project directory")
//!     .with_details("AGPM searches for agpm.toml in current and parent directories");
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
//! ```rust,no_run
//! use agpm_cli::core::{AgpmError, user_friendly_error};
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

use colored::Colorize;
use std::fmt;
use thiserror::Error;

/// The main error type for AGPM operations
///
/// This enum represents all possible errors that can occur during AGPM operations.
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
/// - [`ManifestNotFound`] - agpm.toml file missing
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
/// ```rust,no_run
/// use agpm_cli::core::AgpmError;
///
/// fn handle_error(error: AgpmError) {
///     match error {
///         AgpmError::GitNotFound => {
///             eprintln!("Please install git to use AGPM");
///             std::process::exit(1);
///         }
///         AgpmError::ManifestNotFound => {
///             eprintln!("Run 'agpm init' to create a manifest file");
///         }
///         AgpmError::NetworkError { operation, .. } => {
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
/// ```rust,no_run
/// use agpm_cli::core::AgpmError;
///
/// // Create a git command error with context
/// let error = AgpmError::GitCommandError {
///     operation: "clone".to_string(),
///     stderr: "repository not found".to_string(),
/// };
///
/// // Create a resource not found error
/// let error = AgpmError::ResourceNotFound {
///     name: "my-agent".to_string(),
/// };
///
/// // Create a version constraint error
/// let error = AgpmError::InvalidVersionConstraint {
///     constraint: "~1.x.y".to_string(),
/// };
/// ```
///
/// [`GitNotFound`]: AgpmError::GitNotFound
/// [`GitCommandError`]: AgpmError::GitCommandError
/// [`GitAuthenticationFailed`]: AgpmError::GitAuthenticationFailed
/// [`GitCloneFailed`]: AgpmError::GitCloneFailed
/// [`GitCheckoutFailed`]: AgpmError::GitCheckoutFailed
/// [`FileSystemError`]: AgpmError::FileSystemError
/// [`PermissionDenied`]: AgpmError::PermissionDenied
/// [`DirectoryNotEmpty`]: AgpmError::DirectoryNotEmpty
/// [`IoError`]: AgpmError::IoError
/// [`ManifestNotFound`]: AgpmError::ManifestNotFound
/// [`ManifestParseError`]: AgpmError::ManifestParseError
/// [`ManifestValidationError`]: AgpmError::ManifestValidationError
/// [`LockfileParseError`]: AgpmError::LockfileParseError
/// [`ConfigError`]: AgpmError::ConfigError
/// [`TomlError`]: AgpmError::TomlError
/// [`TomlSerError`]: AgpmError::TomlSerError
/// [`ResourceNotFound`]: AgpmError::ResourceNotFound
/// [`ResourceFileNotFound`]: AgpmError::ResourceFileNotFound
/// [`InvalidResourceType`]: AgpmError::InvalidResourceType
/// [`InvalidResourceStructure`]: AgpmError::InvalidResourceStructure
/// [`InvalidResource`]: AgpmError::InvalidResource
/// [`AlreadyInstalled`]: AgpmError::AlreadyInstalled
/// [`CircularDependency`]: AgpmError::CircularDependency
/// [`DependencyResolutionFailed`]: AgpmError::DependencyResolutionFailed
/// [`DependencyNotMet`]: AgpmError::DependencyNotMet
/// [`InvalidDependency`]: AgpmError::InvalidDependency
/// [`InvalidVersionConstraint`]: AgpmError::InvalidVersionConstraint
/// [`VersionNotFound`]: AgpmError::VersionNotFound
/// [`SemverError`]: AgpmError::SemverError
/// [`SourceNotFound`]: AgpmError::SourceNotFound
/// [`SourceUnreachable`]: AgpmError::SourceUnreachable
/// [`NetworkError`]: AgpmError::NetworkError
/// [`PlatformNotSupported`]: AgpmError::PlatformNotSupported
/// [`ChecksumMismatch`]: AgpmError::ChecksumMismatch
#[derive(Error, Debug)]
pub enum AgpmError {
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
    /// This error occurs when AGPM cannot locate the `git` command in the system PATH.
    /// AGPM requires git to be installed and available for repository operations.
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

    /// Manifest file (agpm.toml) not found
    ///
    /// This error occurs when AGPM cannot locate a agpm.toml file in the current
    /// directory or any parent directory up to the filesystem root.
    ///
    /// AGPM searches for agpm.toml starting from the current working directory
    /// and walking up the directory tree, similar to how git searches for .git.
    #[error("Manifest file agpm.toml not found in current directory or any parent directory")]
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

impl Clone for AgpmError {
    fn clone(&self) -> Self {
        match self {
            Self::GitCommandError {
                operation,
                stderr,
            } => Self::GitCommandError {
                operation: operation.clone(),
                stderr: stderr.clone(),
            },
            Self::GitNotFound => Self::GitNotFound,
            Self::GitRepoInvalid {
                path,
            } => Self::GitRepoInvalid {
                path: path.clone(),
            },
            Self::GitAuthenticationFailed {
                url,
            } => Self::GitAuthenticationFailed {
                url: url.clone(),
            },
            Self::GitCloneFailed {
                url,
                reason,
            } => Self::GitCloneFailed {
                url: url.clone(),
                reason: reason.clone(),
            },
            Self::GitCheckoutFailed {
                reference,
                reason,
            } => Self::GitCheckoutFailed {
                reference: reference.clone(),
                reason: reason.clone(),
            },
            Self::ConfigError {
                message,
            } => Self::ConfigError {
                message: message.clone(),
            },
            Self::ManifestNotFound => Self::ManifestNotFound,
            Self::ManifestParseError {
                file,
                reason,
            } => Self::ManifestParseError {
                file: file.clone(),
                reason: reason.clone(),
            },
            Self::ManifestValidationError {
                reason,
            } => Self::ManifestValidationError {
                reason: reason.clone(),
            },
            Self::LockfileParseError {
                file,
                reason,
            } => Self::LockfileParseError {
                file: file.clone(),
                reason: reason.clone(),
            },
            Self::ResourceNotFound {
                name,
            } => Self::ResourceNotFound {
                name: name.clone(),
            },
            Self::ResourceFileNotFound {
                path,
                source_name,
            } => Self::ResourceFileNotFound {
                path: path.clone(),
                source_name: source_name.clone(),
            },
            Self::SourceNotFound {
                name,
            } => Self::SourceNotFound {
                name: name.clone(),
            },
            Self::SourceUnreachable {
                name,
                url,
            } => Self::SourceUnreachable {
                name: name.clone(),
                url: url.clone(),
            },
            Self::InvalidVersionConstraint {
                constraint,
            } => Self::InvalidVersionConstraint {
                constraint: constraint.clone(),
            },
            Self::VersionNotFound {
                resource,
                version,
            } => Self::VersionNotFound {
                resource: resource.clone(),
                version: version.clone(),
            },
            Self::AlreadyInstalled {
                name,
            } => Self::AlreadyInstalled {
                name: name.clone(),
            },
            Self::InvalidResourceType {
                resource_type,
            } => Self::InvalidResourceType {
                resource_type: resource_type.clone(),
            },
            Self::InvalidResourceStructure {
                file,
                reason,
            } => Self::InvalidResourceStructure {
                file: file.clone(),
                reason: reason.clone(),
            },
            Self::CircularDependency {
                chain,
            } => Self::CircularDependency {
                chain: chain.clone(),
            },
            Self::DependencyResolutionFailed {
                reason,
            } => Self::DependencyResolutionFailed {
                reason: reason.clone(),
            },
            Self::NetworkError {
                operation,
                reason,
            } => Self::NetworkError {
                operation: operation.clone(),
                reason: reason.clone(),
            },
            Self::FileSystemError {
                operation,
                path,
            } => Self::FileSystemError {
                operation: operation.clone(),
                path: path.clone(),
            },
            Self::PermissionDenied {
                operation,
                path,
            } => Self::PermissionDenied {
                operation: operation.clone(),
                path: path.clone(),
            },
            Self::DirectoryNotEmpty {
                path,
            } => Self::DirectoryNotEmpty {
                path: path.clone(),
            },
            Self::InvalidDependency {
                name,
                reason,
            } => Self::InvalidDependency {
                name: name.clone(),
                reason: reason.clone(),
            },
            Self::InvalidResource {
                name,
                reason,
            } => Self::InvalidResource {
                name: name.clone(),
                reason: reason.clone(),
            },
            Self::DependencyNotMet {
                name,
                required,
                found,
            } => Self::DependencyNotMet {
                name: name.clone(),
                required: required.clone(),
                found: found.clone(),
            },
            Self::ConfigNotFound {
                path,
            } => Self::ConfigNotFound {
                path: path.clone(),
            },
            Self::ChecksumMismatch {
                name,
                expected,
                actual,
            } => Self::ChecksumMismatch {
                name: name.clone(),
                expected: expected.clone(),
                actual: actual.clone(),
            },
            Self::PlatformNotSupported {
                operation,
            } => Self::PlatformNotSupported {
                operation: operation.clone(),
            },
            // For errors that don't implement Clone, convert to Other
            Self::IoError(e) => Self::Other {
                message: format!("IO error: {e}"),
            },
            Self::TomlError(e) => Self::Other {
                message: format!("TOML parsing error: {e}"),
            },
            Self::TomlSerError(e) => Self::Other {
                message: format!("TOML serialization error: {e}"),
            },
            Self::SemverError(e) => Self::Other {
                message: format!("Semver parsing error: {e}"),
            },
            Self::Other {
                message,
            } => Self::Other {
                message: message.clone(),
            },
        }
    }
}

/// Error context wrapper that provides user-friendly error information
///
/// `ErrorContext` wraps a [`AgpmError`] and adds optional user-friendly messages,
/// suggestions for resolution, and additional details. This is the primary way
/// AGPM presents errors to CLI users.
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
/// ```rust,no_run
/// use agpm_cli::core::{AgpmError, ErrorContext};
///
/// let error = AgpmError::GitNotFound;
/// let context = ErrorContext::new(error)
///     .with_suggestion("Install git from https://git-scm.com/")
///     .with_details("AGPM requires git for repository operations");
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
/// ```rust,no_run
/// use agpm_cli::core::{AgpmError, ErrorContext};
///
/// let context = ErrorContext::new(AgpmError::ManifestNotFound)
///     .with_suggestion("Create a agpm.toml file in your project directory")
///     .with_details("AGPM searches current and parent directories for agpm.toml");
///
/// println!("{}", context);
/// ```
///
/// ## Quick Suggestion Creation
///
/// ```rust,no_run
/// use agpm_cli::core::ErrorContext;
///
/// // Create context with just a suggestion (useful for generic errors)
/// let context = ErrorContext::suggestion("Try running the command with --verbose");
/// ```
#[derive(Debug)]
pub struct ErrorContext {
    /// The underlying AGPM error
    pub error: AgpmError,
    /// Optional suggestion for resolving the error
    pub suggestion: Option<String>,
    /// Optional additional details about the error
    pub details: Option<String>,
}

impl ErrorContext {
    /// Create a new error context from a [`AgpmError`]
    ///
    /// This creates a basic error context with no additional suggestions or details.
    /// Use the builder methods [`with_suggestion`] and [`with_details`] to add
    /// user-friendly information.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::core::{AgpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(AgpmError::GitNotFound);
    /// ```
    ///
    /// [`with_suggestion`]: ErrorContext::with_suggestion
    /// [`with_details`]: ErrorContext::with_details
    #[must_use]
    pub const fn new(error: AgpmError) -> Self {
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
    /// ```rust,no_run
    /// use agpm_cli::core::{AgpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(AgpmError::GitNotFound)
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
    /// ```rust,no_run
    /// use agpm_cli::core::{AgpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(AgpmError::ManifestNotFound)
    ///     .with_details("AGPM looks for agpm.toml in current directory and parent directories");
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
    /// This is the primary way AGPM presents errors to users in the CLI.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::core::{AgpmError, ErrorContext};
    ///
    /// let context = ErrorContext::new(AgpmError::GitNotFound)
    ///     .with_suggestion("Install git from https://git-scm.com/")
    ///     .with_details("AGPM requires git for repository operations");
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
            write!(f, "\nDetails: {details}")?;
        }

        if let Some(suggestion) = &self.suggestion {
            write!(f, "\nSuggestion: {suggestion}")?;
        }

        Ok(())
    }
}

impl std::error::Error for ErrorContext {}

/// Extension trait for converting [`AgpmError`] to [`anyhow::Error`] with context
///
/// This trait provides a method to convert AGPM-specific errors into generic
/// [`anyhow::Error`] instances while preserving user-friendly context information.
///
/// # Examples
///
/// ```rust,no_run
/// use agpm_cli::core::{AgpmError, ErrorContext, IntoAnyhowWithContext};
///
/// let error = AgpmError::GitNotFound;
/// let context = ErrorContext::new(AgpmError::Other { message: "dummy".to_string() })
///     .with_suggestion("Install git");
///
/// let anyhow_error = error.into_anyhow_with_context(context);
/// ```
pub trait IntoAnyhowWithContext {
    /// Convert the error to an [`anyhow::Error`] with the provided context
    fn into_anyhow_with_context(self, context: ErrorContext) -> anyhow::Error;
}

impl IntoAnyhowWithContext for AgpmError {
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
    /// but don't have a specific [`AgpmError`] variant.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::core::ErrorContext;
    ///
    /// let context = ErrorContext::suggestion("Try running with --verbose for more information");
    /// context.display();
    /// ```
    pub fn suggestion(suggestion: impl Into<String>) -> Self {
        Self {
            error: AgpmError::Other {
                message: String::new(),
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
/// - [`AgpmError`] variants with tailored suggestions
/// - [`std::io::Error`] with filesystem-specific guidance
/// - [`toml::de::Error`] with TOML syntax help
/// - Generic errors with basic context
///
/// # Examples
///
/// ## Converting AGPM Errors
///
/// ```rust,no_run
/// use agpm_cli::core::{AgpmError, user_friendly_error};
///
/// let error = AgpmError::GitNotFound;
/// let anyhow_error = anyhow::Error::from(error);
/// let context = user_friendly_error(anyhow_error);
///
/// context.display(); // Shows git installation suggestions
/// ```
///
/// ## Converting IO Errors
///
/// ```rust,no_run
/// use agpm_cli::core::user_friendly_error;
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
/// ```rust,no_run
/// use agpm_cli::core::user_friendly_error;
///
/// let error = anyhow::anyhow!("Something went wrong");
/// let context = user_friendly_error(error);
///
/// context.display(); // Shows the error with generic formatting
/// ```
#[must_use]
pub fn user_friendly_error(error: anyhow::Error) -> ErrorContext {
    // Check for specific error types and provide helpful suggestions
    if let Some(ccmp_error) = error.downcast_ref::<AgpmError>() {
        return create_error_context(ccmp_error.clone());
    }

    if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
        match io_error.kind() {
            std::io::ErrorKind::PermissionDenied => {
                return ErrorContext::new(AgpmError::PermissionDenied {
                    operation: "file access".to_string(),
                    path: "unknown".to_string(),
                })
                .with_suggestion("Try running with elevated permissions (sudo/Administrator) or check file ownership")
                .with_details("This error occurs when AGPM doesn't have permission to read or write files");
            }
            std::io::ErrorKind::NotFound => {
                return ErrorContext::new(AgpmError::FileSystemError {
                    operation: "file access".to_string(),
                    path: "unknown".to_string(),
                })
                .with_suggestion("Check that the file or directory exists and the path is correct")
                .with_details(
                    "This error occurs when a required file or directory cannot be found",
                );
            }
            std::io::ErrorKind::AlreadyExists => {
                return ErrorContext::new(AgpmError::FileSystemError {
                    operation: "file creation".to_string(),
                    path: "unknown".to_string(),
                })
                .with_suggestion("Remove the existing file or use --force to overwrite")
                .with_details("The target file or directory already exists");
            }
            std::io::ErrorKind::InvalidData => {
                return ErrorContext::new(AgpmError::InvalidResource {
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
        return ErrorContext::new(AgpmError::ManifestParseError {
            file: "agpm.toml".to_string(),
            reason: toml_error.to_string(),
        })
        .with_suggestion("Check the TOML syntax in your agpm.toml file. Verify quotes, brackets, and indentation")
        .with_details("TOML parsing errors are usually caused by syntax issues like missing quotes or mismatched brackets");
    }

    // Check for template rendering errors by examining the error message
    let error_msg = error.to_string().to_lowercase();
    let is_template_error = error_msg.contains("template")
        || error_msg.contains("variable")
        || error_msg.contains("filter")
        || error_msg.contains("tera");

    if is_template_error {
        // Build full error chain for template errors
        let mut message = error.to_string();
        let chain: Vec<String> =
            error.chain().skip(1).map(std::string::ToString::to_string).collect();

        if !chain.is_empty() {
            message.push_str("\n\nCaused by:");
            for (i, cause) in chain.iter().enumerate() {
                message.push_str(&format!("\n  {}: {}", i + 1, cause));
            }
        }

        return ErrorContext::new(AgpmError::InvalidResource {
            name: "template".to_string(),
            reason: message,
        })
        .with_suggestion(
            "Check template syntax: variables use {{ var }}, comments use {# #}, control flow uses {% %}. \
             Ensure all variables referenced in the template exist in the context (agpm.resource, agpm.deps)",
        )
        .with_details(
            "Template errors occur when Tera cannot render the template. Common issues:\n\
             - Undefined variables (use {% if var is defined %} to check)\n\
             - Syntax errors (unclosed {{ or {% delimiters)\n\
             - Invalid filters or functions\n\
             - Type mismatches in operations",
        );
    }

    // Generic error - include the full error chain for better diagnostics
    let mut message = error.to_string();

    // Append error chain if available
    let chain: Vec<String> = error
        .chain()
        .skip(1) // Skip the root cause which is already in to_string()
        .map(std::string::ToString::to_string)
        .collect();

    if !chain.is_empty() {
        message.push_str("\n\nCaused by:");
        for (i, cause) in chain.iter().enumerate() {
            message.push_str(&format!("\n  {}: {}", i + 1, cause));
        }
    }

    ErrorContext::new(AgpmError::Other {
        message,
    })
}

/// Create appropriate [`ErrorContext`] with suggestions for specific AGPM errors
///
/// This internal function maps each [`AgpmError`] variant to an appropriate
/// [`ErrorContext`] with tailored suggestions and details. It's used by
/// [`user_friendly_error`] to provide consistent, helpful error messages.
///
/// # Implementation Notes
///
/// - Each error type has specific suggestions based on common resolution steps
/// - Platform-specific suggestions are provided where applicable
/// - Error messages focus on actionable steps rather than technical details
/// - Cross-references to related commands or documentation are included
fn create_error_context(error: AgpmError) -> ErrorContext {
    match &error {
        AgpmError::GitNotFound => ErrorContext::new(AgpmError::GitNotFound)
            .with_suggestion("Install git from https://git-scm.com/ or your package manager (e.g., 'brew install git', 'apt install git')")
            .with_details("AGPM requires git to be installed and available in your PATH to manage repositories"),

        AgpmError::GitCommandError { operation, stderr } => {
            ErrorContext::new(AgpmError::GitCommandError {
                operation: operation.clone(),
                stderr: stderr.clone(),
            })
            .with_suggestion(match operation.as_str() {
                op if op.contains("clone") => "Check the repository URL and your internet connection. Verify you have access to the repository",
                op if op.contains("fetch") => "Check your internet connection and repository access. Try 'git fetch' manually in the repository directory",
                op if op.contains("checkout") => "Verify the branch, tag, or commit exists. Use 'git tag -l' or 'git branch -r' to list available references",
                op if op.contains("worktree") => {
                    if stderr.contains("invalid reference")
                        || stderr.contains("not a valid object name")
                        || stderr.contains("pathspec")
                        || stderr.contains("did not match")
                        || stderr.contains("unknown revision") {
                        "Invalid version: The specified version/tag/branch does not exist in the repository. Check available versions with 'git tag -l' or 'git branch -r'"
                    } else {
                        "Failed to create worktree. Check that the reference exists and the repository is valid"
                    }
                },
                _ => "Check your git configuration and repository access. Try running the git command manually for more details",
            })
            .with_details(if operation.contains("worktree") && (stderr.contains("invalid reference") || stderr.contains("not a valid object name") || stderr.contains("pathspec") || stderr.contains("did not match") || stderr.contains("unknown revision")) {
                "Invalid version specification: Failed to checkout reference - the specified version/tag/branch does not exist"
            } else {
                "Git operations failed. This is often due to network issues, authentication problems, or invalid references"
            })
        }

        AgpmError::GitAuthenticationFailed { url } => ErrorContext::new(AgpmError::GitAuthenticationFailed {
            url: url.clone(),
        })
            .with_suggestion("Configure git authentication: use 'git config --global user.name' and 'git config --global user.email', or set up SSH keys")
            .with_details("Authentication is required for private repositories. You may need to log in with 'git credential-manager-core' or similar"),

        AgpmError::GitCloneFailed { url, reason } => ErrorContext::new(AgpmError::GitCloneFailed {
            url: url.clone(),
            reason: reason.clone(),
        })
            .with_suggestion(format!(
                "Verify the repository URL is correct: {url}. Check your internet connection and repository access"
            ))
            .with_details("Clone operations can fail due to invalid URLs, network issues, or access restrictions"),

        AgpmError::ManifestNotFound => ErrorContext::new(AgpmError::ManifestNotFound)
            .with_suggestion("Create a agpm.toml file in your project directory. See documentation for the manifest format")
            .with_details("AGPM looks for agpm.toml in the current directory and parent directories up to the filesystem root"),

        AgpmError::ManifestParseError { file, reason } => ErrorContext::new(AgpmError::ManifestParseError {
            file: file.clone(),
            reason: reason.clone(),
        })
            .with_suggestion(format!(
                "Check the TOML syntax in {file}. Common issues: missing quotes, unmatched brackets, invalid characters"
            ))
            .with_details("Use a TOML validator or check the agpm documentation for correct manifest format"),

        AgpmError::SourceNotFound { name } => ErrorContext::new(AgpmError::SourceNotFound {
            name: name.clone(),
        })
            .with_suggestion(format!(
                "Add source '{name}' to the [sources] section in agpm.toml with the repository URL"
            ))
            .with_details("All dependencies must reference a source defined in the [sources] section"),

        AgpmError::ResourceFileNotFound { path, source_name } => ErrorContext::new(AgpmError::ResourceFileNotFound {
            path: path.clone(),
            source_name: source_name.clone(),
        })
            .with_suggestion(format!(
                "Verify the file '{path}' exists in the '{source_name}' repository at the specified version/commit"
            ))
            .with_details("The resource file may have been moved, renamed, or deleted in the repository"),

        AgpmError::VersionNotFound { resource, version } => ErrorContext::new(AgpmError::VersionNotFound {
            resource: resource.clone(),
            version: version.clone(),
        })
            .with_suggestion(format!(
                "Check available versions for '{resource}' using 'git tag -l' in the repository, or use 'main' or 'master' branch"
            ))
            .with_details(format!(
                "The version '{version}' doesn't exist as a git tag, branch, or commit in the repository"
            )),

        AgpmError::CircularDependency { chain } => ErrorContext::new(AgpmError::CircularDependency {
            chain: chain.clone(),
        })
            .with_suggestion("Review your dependency graph and remove circular references")
            .with_details(format!(
                "Circular dependency chain detected: {chain}. Dependencies cannot depend on themselves directly or indirectly"
            )),

        AgpmError::PermissionDenied { operation, path } => ErrorContext::new(AgpmError::PermissionDenied {
            operation: operation.clone(),
            path: path.clone(),
        })
            .with_suggestion(match cfg!(windows) {
                true => "Run as Administrator or check file permissions in File Explorer",
                false => "Use 'sudo' or check file permissions with 'ls -la'",
            })
            .with_details(format!(
                "Cannot {operation} due to insufficient permissions on {path}"
            )),

        AgpmError::ChecksumMismatch { name, expected, actual } => ErrorContext::new(AgpmError::ChecksumMismatch {
            name: name.clone(),
            expected: expected.clone(),
            actual: actual.clone(),
        })
            .with_suggestion("The file may have been corrupted or modified. Try reinstalling with --force")
            .with_details(format!(
                "Resource '{name}' has checksum {actual} but expected {expected}. This indicates file corruption or tampering"
            )),

        _ => ErrorContext::new(error.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = AgpmError::GitNotFound;
        assert_eq!(error.to_string(), "Git is not installed or not found in PATH");

        let error = AgpmError::ResourceNotFound {
            name: "test".to_string(),
        };
        assert_eq!(error.to_string(), "Resource 'test' not found");

        let error = AgpmError::InvalidVersionConstraint {
            constraint: "bad-version".to_string(),
        };
        assert_eq!(error.to_string(), "Invalid version constraint: bad-version");

        let error = AgpmError::GitCommandError {
            operation: "clone".to_string(),
            stderr: "repository not found".to_string(),
        };
        assert_eq!(error.to_string(), "Git operation failed: clone");
    }

    #[test]
    fn test_error_context() {
        let ctx = ErrorContext::new(AgpmError::GitNotFound)
            .with_suggestion("Install git using your package manager")
            .with_details("Git is required for AGPM to function");

        assert_eq!(ctx.suggestion, Some("Install git using your package manager".to_string()));
        assert_eq!(ctx.details, Some("Git is required for AGPM to function".to_string()));
    }

    #[test]
    fn test_error_context_display() {
        let ctx = ErrorContext::new(AgpmError::GitNotFound).with_suggestion("Install git");

        let display = format!("{ctx}");
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
            AgpmError::PermissionDenied {
                ..
            } => {}
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
            AgpmError::FileSystemError {
                ..
            } => {}
            _ => panic!("Expected FileSystemError"),
        }
        assert!(ctx.suggestion.is_some());
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_from_io_error() {
        use std::io::Error;

        let io_error = Error::other("test error");
        let agpm_error = AgpmError::from(io_error);

        match agpm_error {
            AgpmError::IoError(_) => {}
            _ => panic!("Expected IoError"),
        }
    }

    #[test]
    fn test_from_toml_error() {
        let toml_str = "invalid = toml {";
        let result: Result<toml::Value, _> = toml::from_str(toml_str);

        if let Err(e) = result {
            let agpm_error = AgpmError::from(e);
            match agpm_error {
                AgpmError::TomlError(_) => {}
                _ => panic!("Expected TomlError"),
            }
        }
    }

    #[test]
    fn test_create_error_context_git_not_found() {
        let ctx = create_error_context(AgpmError::GitNotFound);
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("Install git"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_git_command_error() {
        let ctx = create_error_context(AgpmError::GitCommandError {
            operation: "clone".to_string(),
            stderr: "error".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("repository URL"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_git_auth_failed() {
        let ctx = create_error_context(AgpmError::GitAuthenticationFailed {
            url: "https://github.com/test/repo".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("Configure git authentication"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_manifest_not_found() {
        let ctx = create_error_context(AgpmError::ManifestNotFound);
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("Create a agpm.toml"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_source_not_found() {
        let ctx = create_error_context(AgpmError::SourceNotFound {
            name: "test-source".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("test-source"));
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_create_error_context_version_not_found() {
        let ctx = create_error_context(AgpmError::VersionNotFound {
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
        let ctx = create_error_context(AgpmError::CircularDependency {
            chain: "a -> b -> c -> a".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.suggestion.unwrap().contains("remove circular"));
        assert!(ctx.details.is_some());
        assert!(ctx.details.unwrap().contains("a -> b -> c -> a"));
    }

    #[test]
    fn test_create_error_context_permission_denied() {
        let ctx = create_error_context(AgpmError::PermissionDenied {
            operation: "write".to_string(),
            path: "/test/path".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        assert!(ctx.details.is_some());
        assert!(ctx.details.unwrap().contains("/test/path"));
    }

    #[test]
    fn test_create_error_context_checksum_mismatch() {
        let ctx = create_error_context(AgpmError::ChecksumMismatch {
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
        let error1 = AgpmError::GitNotFound;
        let error2 = error1.clone();
        assert_eq!(error1.to_string(), error2.to_string());

        let error1 = AgpmError::ResourceNotFound {
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
        let error = AgpmError::GitNotFound;
        let context = ErrorContext::new(AgpmError::Other {
            message: "dummy".to_string(),
        })
        .with_suggestion("Test suggestion")
        .with_details("Test details");

        let anyhow_error = error.into_anyhow_with_context(context);
        let display = format!("{anyhow_error}");
        assert!(display.contains("Git is not installed"));
    }

    #[test]
    fn test_user_friendly_error_already_exists() {
        use std::io::{Error, ErrorKind};

        let io_error = Error::new(ErrorKind::AlreadyExists, "file exists");
        let anyhow_error = anyhow::Error::from(io_error);

        let ctx = user_friendly_error(anyhow_error);
        match ctx.error {
            AgpmError::FileSystemError {
                ..
            } => {}
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
            AgpmError::InvalidResource {
                ..
            } => {}
            _ => panic!("Expected InvalidResource"),
        }
        assert!(ctx.suggestion.is_some());
        assert!(ctx.details.is_some());
    }

    #[test]
    fn test_user_friendly_error_agpm_error() {
        let error = AgpmError::GitNotFound;
        let anyhow_error = anyhow::Error::from(error);

        let ctx = user_friendly_error(anyhow_error);
        match ctx.error {
            AgpmError::GitNotFound => {}
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
                AgpmError::ManifestParseError {
                    ..
                } => {}
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
            AgpmError::Other {
                message,
            } => {
                assert_eq!(message, "Generic error");
            }
            _ => panic!("Expected Other error"),
        }
    }

    #[test]
    fn test_from_semver_error() {
        let result = semver::Version::parse("invalid-version");
        if let Err(e) = result {
            let agpm_error = AgpmError::from(e);
            match agpm_error {
                AgpmError::SemverError(_) => {}
                _ => panic!("Expected SemverError"),
            }
        }
    }

    #[test]
    fn test_error_display_all_variants() {
        // Test display for various error variants
        let errors = vec![
            AgpmError::GitRepoInvalid {
                path: "/test/path".to_string(),
            },
            AgpmError::GitCheckoutFailed {
                reference: "main".to_string(),
                reason: "not found".to_string(),
            },
            AgpmError::ConfigError {
                message: "config issue".to_string(),
            },
            AgpmError::ManifestValidationError {
                reason: "invalid format".to_string(),
            },
            AgpmError::LockfileParseError {
                file: "agpm.lock".to_string(),
                reason: "syntax error".to_string(),
            },
            AgpmError::ResourceFileNotFound {
                path: "test.md".to_string(),
                source_name: "source".to_string(),
            },
            AgpmError::DirectoryNotEmpty {
                path: "/some/dir".to_string(),
            },
            AgpmError::InvalidDependency {
                name: "dep".to_string(),
                reason: "bad format".to_string(),
            },
            AgpmError::DependencyNotMet {
                name: "dep".to_string(),
                required: "v1.0".to_string(),
                found: "v2.0".to_string(),
            },
            AgpmError::ConfigNotFound {
                path: "/config/path".to_string(),
            },
            AgpmError::PlatformNotSupported {
                operation: "test op".to_string(),
            },
        ];

        for error in errors {
            let display = format!("{error}");
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
            let ctx = create_error_context(AgpmError::GitCommandError {
                operation: op.to_string(),
                stderr: "error".to_string(),
            });
            assert!(ctx.suggestion.is_some());
            assert!(ctx.suggestion.unwrap().to_lowercase().contains(expected_text));
        }
    }

    #[test]
    fn test_create_error_context_resource_file_not_found() {
        let ctx = create_error_context(AgpmError::ResourceFileNotFound {
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
        let ctx = create_error_context(AgpmError::ManifestParseError {
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
        let ctx = create_error_context(AgpmError::GitCloneFailed {
            url: "https://example.com/repo.git".to_string(),
            reason: "network error".to_string(),
        });
        assert!(ctx.suggestion.is_some());
        let suggestion = ctx.suggestion.unwrap();
        assert!(suggestion.contains("https://example.com/repo.git"));
        assert!(ctx.details.is_some());
    }
}
