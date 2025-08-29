//! Core types and functionality for CCPM
//!
//! This module forms the foundation of CCPM's type system, providing fundamental abstractions
//! for error handling, resource management, and core operations. It defines the contracts and
//! interfaces used throughout the CCPM codebase.
//!
//! # Architecture Overview
//!
//! The core module is organized around several key concepts:
//!
//! ## Error Management
//! CCPM uses a sophisticated error handling system designed for both developer ergonomics
//! and end-user experience:
//! - **Strongly-typed errors** ([`CcpmError`]) for precise error handling in code
//! - **User-friendly contexts** ([`ErrorContext`]) with actionable suggestions for CLI users
//! - **Automatic error conversion** from common standard library errors
//! - **Contextual suggestions** tailored to specific error conditions
//!
//! ## Resource Abstractions
//! Resources are the core entities managed by CCPM:
//! - **Resource types** ([`ResourceType`]) define categories (agents, snippets)
//! - **Resource trait** provides common interface for all resource types  
//! - **Type detection** automatically identifies resources
//! - **Extensible design** allows future resource types to be added easily
//!
//! # Modules
//!
//! ## `error` - Comprehensive Error Handling
//!
//! The error module provides:
//! - [`CcpmError`] - Enumerated error types covering all CCPM failure modes
//! - [`ErrorContext`] - User-friendly error wrapper with suggestions and details
//! - [`user_friendly_error`] - Convert any error to user-friendly format
//! - [`IntoAnyhowWithContext`] - Extension trait for error conversion
//!
//! ## `resource` - Resource Type System
//!
//! The resource module provides:
//! - [`ResourceType`] - Enumeration of supported resource types
//! - `Resource` - Trait interface for all resource implementations
//! - Type detection functions - Automatic resource type detection
//!
//! # Design Principles
//!
//! ## Error First Design
//! Every operation that can fail returns a [`Result`] type with meaningful error information.
//! Errors are designed to be informative, actionable, and user-friendly.
//!
//! ## Type Safety
//! Strong typing prevents invalid operations and catches errors at compile time.
//! Resource types, error variants, and operation modes are all statically typed.
//!
//! ## Extensibility
//! The core abstractions are designed to support future expansion without breaking changes.
//! New resource types and error variants can be added while maintaining compatibility.
//!
//! ## User Experience
//! All user-facing errors include contextual suggestions and clear guidance for resolution.
//! Terminal colors and formatting enhance readability and highlight important information.
//!
//! # Examples
//!
//! ## Error Handling Pattern
//!
//! ```rust
//! use ccpm::core::{CcpmError, ErrorContext, user_friendly_error};
//! use anyhow::Result;
//!
//! fn example_operation() -> Result<String> {
//!     // Simulate an operation that might fail
//!     Err(CcpmError::ManifestNotFound.into())
//! }
//!
//! fn handle_operation() {
//!     match example_operation() {
//!         Ok(result) => println!("Success: {}", result),
//!         Err(e) => {
//!             // Convert to user-friendly error and display
//!             let friendly = user_friendly_error(e);
//!             friendly.display(); // Shows colored error with suggestions
//!         }
//!     }
//! }
//! ```
//!
//! ## Resource Type Detection
//!
//! ```rust
//! use ccpm::core::{ResourceType, detect_resource_type};
//! use std::path::Path;
//! use tempfile::tempdir;
//!
//! fn discover_resources() -> anyhow::Result<()> {
//!     let temp_dir = tempdir()?;
//!     let path = temp_dir.path();
//!
//!     // Create a resource manifest
//!     std::fs::write(path.join("agent.toml"), "# Agent configuration")?;
//!     
//!     // Detect the resource type
//!     if let Some(resource_type) = detect_resource_type(path) {
//!         match resource_type {
//!             ResourceType::Agent => {
//!                 println!("Found agent resource");
//!                 println!("Install dir: {}", resource_type.default_directory());
//!             }
//!             ResourceType::Snippet => {
//!                 println!("Found snippet resource");
//!                 println!("Install dir: {}", resource_type.default_directory());
//!             }
//!             ResourceType::Command => {
//!                 println!("Found command resource");
//!                 println!("Install dir: {}", resource_type.default_directory());
//!             }
//!             ResourceType::McpServer => {
//!                 println!("Found MCP server configuration");
//!                 println!("Install dir: {}", resource_type.default_directory());
//!             }
//!             ResourceType::Script => {
//!                 println!("Found script resource");
//!                 println!("Install dir: {}", resource_type.default_directory());
//!             }
//!             ResourceType::Hook => {
//!                 println!("Found hook configuration");
//!                 println!("Install dir: {}", resource_type.default_directory());
//!             }
//!         }
//!     } else {
//!         println!("No recognized resource found");
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Resource Trait Usage
//!
//! ```rust
//! use ccpm::core::{Resource, ResourceType};
//! use anyhow::Result;
//! use std::path::Path;
//!
//! fn process_any_resource(resource: &dyn Resource) -> Result<()> {
//!     // Get basic information
//!     println!("Processing: {} ({})", resource.name(), resource.resource_type());
//!     
//!     if let Some(desc) = resource.description() {
//!         println!("Description: {}", desc);
//!     }
//!     
//!     // Validate before processing
//!     resource.validate()?;
//!     
//!     // Get metadata for analysis
//!     let metadata = resource.metadata()?;
//!     if let Some(version) = metadata.get("version") {
//!         println!("Version: {}", version);
//!     }
//!     
//!     // Install to target location
//!     let target = Path::new("./resources");
//!     let install_path = resource.install_path(target);
//!     println!("Would install to: {}", install_path.display());
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Error Context Creation
//!
//! ```rust
//! use ccpm::core::{CcpmError, ErrorContext};
//!
//! fn create_helpful_error() -> ErrorContext {
//!     ErrorContext::new(CcpmError::GitNotFound)
//!         .with_suggestion("Install git from https://git-scm.com/ or use your package manager")
//!         .with_details("CCPM requires git to clone and manage source repositories")
//! }
//!
//! fn display_error() {
//!     let error = create_helpful_error();
//!     error.display(); // Shows colored output with error, details, and suggestion
//! }
//! ```
//!
//! # Integration with Other Modules
//!
//! The core module provides types used throughout CCPM:
//! - **CLI commands** use [`CcpmError`] and [`ErrorContext`] for user feedback
//! - **Git operations** return [`CcpmError`] variants for specific failure modes
//! - **Manifest parsing** uses [`Resource`] trait for type-agnostic operations
//! - **Installation** relies on [`ResourceType`] for path generation and validation
//! - **Dependency resolution** uses error types for constraint violations
//!
//! # Thread Safety
//!
//! All core types are designed to be thread-safe where appropriate:
//! - [`ResourceType`] is [`Copy`] and can be shared freely
//! - [`CcpmError`] implements [`Clone`] for error propagation
//! - [`Resource`] trait is object-safe for dynamic dispatch
//!
//! [`Result`]: std::result::Result
//! [`Copy`]: std::marker::Copy  
//! [`Clone`]: std::clone::Clone

pub mod error;
pub mod error_builders;
pub mod error_helpers;
mod resource;
pub mod resource_iterator;

pub use error::{user_friendly_error, CcpmError, ErrorContext, IntoAnyhowWithContext};
pub use error_builders::{
    file_error_context, git_error_context, manifest_error_context, ErrorContextExt,
};
pub use error_helpers::{
    FileOperations, FileOps, JsonOperations, JsonOps, LockfileOperations, LockfileOps,
    ManifestOperations, ManifestOps, MarkdownOperations, MarkdownOps,
};
pub use resource::{Resource, ResourceType};
pub use resource_iterator::{ResourceIterator, ResourceTypeExt};

use std::path::Path;

/// Detect the resource type in a directory by examining manifest files
///
/// This function provides automatic resource type detection based on manifest file presence.
///
/// # Arguments
///
/// * `path` - The directory path to examine for resource manifests
///
/// # Returns
///
/// - `Some(ResourceType::Agent)` if `agent.toml` exists
/// - `Some(ResourceType::Snippet)` if `snippet.toml` exists (and no `agent.toml`)
/// - `None` if no recognized manifest files are found
///
/// # Examples
///
/// ```rust
/// use ccpm::core::{ResourceType, detect_resource_type};
/// use tempfile::tempdir;
/// use std::fs;
///
/// let temp = tempdir().unwrap();
/// let path = temp.path();
///
/// // No manifest initially
/// assert_eq!(detect_resource_type(path), None);
///
/// // Add agent manifest
/// fs::write(path.join("agent.toml"), "# Agent config").unwrap();
/// assert_eq!(detect_resource_type(path), Some(ResourceType::Agent));
/// ```
#[must_use]
pub fn detect_resource_type(path: &Path) -> Option<ResourceType> {
    if path.join("agent.toml").exists() {
        Some(ResourceType::Agent)
    } else if path.join("snippet.toml").exists() {
        Some(ResourceType::Snippet)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_resource_type_agent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Create agent.toml
        fs::write(path.join("agent.toml"), "").unwrap();

        assert_eq!(detect_resource_type(path), Some(ResourceType::Agent));
    }

    #[test]
    fn test_detect_resource_type_snippet() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Create snippet.toml
        fs::write(path.join("snippet.toml"), "").unwrap();

        assert_eq!(detect_resource_type(path), Some(ResourceType::Snippet));
    }

    #[test]
    fn test_detect_resource_type_none() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // No resource files
        assert_eq!(detect_resource_type(path), None);
    }

    #[test]
    fn test_detect_resource_type_prefers_agent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Create both files - agent should take precedence
        fs::write(path.join("agent.toml"), "").unwrap();
        fs::write(path.join("snippet.toml"), "").unwrap();

        assert_eq!(detect_resource_type(path), Some(ResourceType::Agent));
    }
}
