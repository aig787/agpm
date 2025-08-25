//! Resource abstractions for CCPM
//!
//! This module defines the core resource types and management traits that form the foundation
//! of CCPM's resource system. Resources are the fundamental units that CCPM manages, installs,
//! and tracks across different source repositories.
//!
//! # Resource Model
//!
//! CCPM supports different types of resources, each with specific characteristics:
//! - **Agents**: AI assistant configurations and prompts
//! - **Snippets**: Reusable code templates and examples
//!
//! Resources are distributed as markdown files (.md) that may contain frontmatter metadata
//! for configuration and dependency information.
//!
//! # Core Types
//!
//! - [`ResourceType`] - Enumeration of supported resource types
//! - [`Resource`] - Trait defining the interface for all resource types
//! - [`detect_resource_type`] - Function to identify resource types from filesystem
//!
//! # Resource Detection
//!
//! CCPM identifies resource types by looking for specific manifest files in directories:
//! - `agent.toml` → [`ResourceType::Agent`]
//! - `snippet.toml` → [`ResourceType::Snippet`]
//!
//! Agent resources take precedence when both manifest files are present.
//!
//! # Examples
//!
//! ## Working with Resource Types
//!
//! ```rust
//! use ccpm::core::{ResourceType, detect_resource_type};
//! use std::path::Path;
//!
//! // Convert strings to resource types
//! let agent_type: ResourceType = "agent".parse().unwrap();
//! let snippet_type: ResourceType = "snippet".parse().unwrap();
//!
//! // Get manifest filenames
//! assert_eq!(agent_type.manifest_filename(), "agent.toml");
//! assert_eq!(snippet_type.manifest_filename(), "snippet.toml");
//!
//! // Get default directory names
//! assert_eq!(agent_type.default_directory(), "agents");
//! assert_eq!(snippet_type.default_directory(), "snippets");
//! ```
//!
//! ## Detecting Resource Types
//!
//! ```rust
//! use ccpm::core::{ResourceType, detect_resource_type};
//! use std::path::Path;
//! use tempfile::tempdir;
//!
//! let temp_dir = tempdir().unwrap();
//! let path = temp_dir.path();
//!
//! // Initially no resource type detected
//! assert_eq!(detect_resource_type(path), None);
//!
//! // Create agent manifest
//! std::fs::write(path.join("agent.toml"), "# Agent configuration").unwrap();
//! assert_eq!(detect_resource_type(path), Some(ResourceType::Agent));
//! ```
//!
//! ## Serialization Support
//!
//! ```rust
//! use ccpm::core::ResourceType;
//!
//! // ResourceType implements Serialize/Deserialize
//! let agent = ResourceType::Agent;
//! let json = serde_json::to_string(&agent).unwrap();
//! assert_eq!(json, "\"agent\"");
//!
//! let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
//! assert_eq!(deserialized, ResourceType::Agent);
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Enumeration of supported resource types in CCPM
///
/// This enum defines the different categories of resources that CCPM can manage.
/// Each resource type has specific characteristics, installation paths, and
/// manifest file requirements.
///
/// # Serialization
///
/// `ResourceType` implements [`serde::Serialize`] and [`serde::Deserialize`]
/// using lowercase string representations ("agent", "snippet") for JSON/TOML
/// compatibility.
///
/// # Resource Type Characteristics
///
/// ## Agent
/// - **Purpose**: AI assistant configurations, prompts, and behavioral definitions
/// - **Manifest**: `agent.toml` file containing configuration
/// - **Default Directory**: `agents/` in project root
/// - **Common Use Cases**: Claude Code agents, custom AI assistants, specialized prompts
///
/// ## Snippet  
/// - **Purpose**: Reusable code templates, examples, and documentation fragments
/// - **Manifest**: `snippet.toml` file containing metadata
/// - **Default Directory**: `snippets/` in project root
/// - **Common Use Cases**: Code templates, configuration examples, documentation
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust
/// use ccpm::core::ResourceType;
///
/// let agent = ResourceType::Agent;
/// let snippet = ResourceType::Snippet;
///
/// assert_eq!(agent.to_string(), "agent");
/// assert_eq!(snippet.to_string(), "snippet");
/// ```
///
/// ## String Parsing
///
/// ```rust
/// use ccpm::core::ResourceType;
/// use std::str::FromStr;
///
/// let agent: ResourceType = "agent".parse().unwrap();
/// let snippet: ResourceType = "SNIPPET".parse().unwrap(); // Case insensitive
///
/// assert_eq!(agent, ResourceType::Agent);
/// assert_eq!(snippet, ResourceType::Snippet);
///
/// // Invalid resource type
/// assert!("invalid".parse::<ResourceType>().is_err());
/// ```
///
/// ## Manifest and Directory Names
///
/// ```rust
/// use ccpm::core::ResourceType;
///
/// let agent = ResourceType::Agent;
/// assert_eq!(agent.manifest_filename(), "agent.toml");
/// assert_eq!(agent.default_directory(), "agents");
///
/// let snippet = ResourceType::Snippet;  
/// assert_eq!(snippet.manifest_filename(), "snippet.toml");
/// assert_eq!(snippet.default_directory(), "snippets");
/// ```
///
/// ## JSON Serialization
///
/// ```rust
/// use ccpm::core::ResourceType;
///
/// let agent = ResourceType::Agent;
/// let json = serde_json::to_string(&agent).unwrap();
/// assert_eq!(json, "\"agent\"");
///
/// let parsed: ResourceType = serde_json::from_str("\"snippet\"").unwrap();
/// assert_eq!(parsed, ResourceType::Snippet);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    /// AI assistant configurations and prompts
    ///
    /// Agents define AI assistant behavior, including system prompts, specialized
    /// capabilities, and configuration parameters. They are typically stored as
    /// markdown files with frontmatter containing metadata.
    Agent,

    /// Reusable code templates and examples
    ///
    /// Snippets contain reusable code fragments, configuration examples, or
    /// documentation templates that can be shared across projects.
    Snippet,
    // Future resource types can be added here
}

impl ResourceType {
    /// Get the manifest filename for this resource type
    ///
    /// Each resource type has a specific manifest file that CCPM looks for
    /// when detecting and validating resources.
    ///
    /// # Returns
    ///
    /// - [`Agent`] → `"agent.toml"`
    /// - [`Snippet`] → `"snippet.toml"`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::ResourceType;
    ///
    /// assert_eq!(ResourceType::Agent.manifest_filename(), "agent.toml");
    /// assert_eq!(ResourceType::Snippet.manifest_filename(), "snippet.toml");
    /// ```
    ///
    /// [`Agent`]: ResourceType::Agent
    /// [`Snippet`]: ResourceType::Snippet
    pub fn manifest_filename(&self) -> &str {
        match self {
            ResourceType::Agent => "agent.toml",
            ResourceType::Snippet => "snippet.toml",
        }
    }

    /// Get the default installation directory name for this resource type
    ///
    /// Returns the conventional directory name where resources of this type
    /// are typically installed in CCPM projects.
    ///
    /// # Returns
    ///
    /// - [`Agent`] → `"agents"`
    /// - [`Snippet`] → `"snippets"`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::ResourceType;
    ///
    /// assert_eq!(ResourceType::Agent.default_directory(), "agents");
    /// assert_eq!(ResourceType::Snippet.default_directory(), "snippets");
    /// ```
    ///
    /// # Note
    ///
    /// This is just the default convention. Users can install resources to any
    /// directory by specifying custom paths in their manifest files.
    ///
    /// [`Agent`]: ResourceType::Agent
    /// [`Snippet`]: ResourceType::Snippet
    pub fn default_directory(&self) -> &str {
        match self {
            ResourceType::Agent => "agents",
            ResourceType::Snippet => "snippets",
        }
    }
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Agent => write!(f, "agent"),
            ResourceType::Snippet => write!(f, "snippet"),
        }
    }
}

impl std::str::FromStr for ResourceType {
    type Err = crate::core::CcpmError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "agent" => Ok(ResourceType::Agent),
            "snippet" => Ok(ResourceType::Snippet),
            _ => Err(crate::core::CcpmError::InvalidResourceType {
                resource_type: s.to_string(),
            }),
        }
    }
}

/// Detect the resource type in a directory by examining manifest files
///
/// This function examines a directory and determines what type of resource it contains
/// based on the presence of specific manifest files. It's used during resource discovery
/// and validation to automatically classify resources.
///
/// # Detection Logic
///
/// 1. Checks for `agent.toml` - if found, returns [`ResourceType::Agent`]
/// 2. Checks for `snippet.toml` - if found, returns [`ResourceType::Snippet`]  
/// 3. If neither file exists, returns `None`
///
/// # Precedence
///
/// Agent resources take precedence over snippet resources. If both `agent.toml` and
/// `snippet.toml` exist in the same directory, the function returns [`ResourceType::Agent`].
///
/// # Arguments
///
/// * `path` - The directory path to examine
///
/// # Returns
///
/// - `Some(ResourceType::Agent)` if `agent.toml` exists
/// - `Some(ResourceType::Snippet)` if `snippet.toml` exists (and no `agent.toml`)
/// - `None` if no recognized manifest files are found
///
/// # Examples
///
/// ## Basic Detection
///
/// ```rust
/// use ccpm::core::{ResourceType, detect_resource_type};
/// use tempfile::tempdir;
/// use std::fs;
///
/// let temp = tempdir().unwrap();
/// let path = temp.path();
///
/// // No manifest files - no resource type detected
/// assert_eq!(detect_resource_type(path), None);
///
/// // Create agent manifest
/// fs::write(path.join("agent.toml"), "# Agent configuration").unwrap();
/// assert_eq!(detect_resource_type(path), Some(ResourceType::Agent));
/// ```
///
/// ## Precedence Example  
///
/// ```rust
/// use ccpm::core::{ResourceType, detect_resource_type};
/// use tempfile::tempdir;
/// use std::fs;
///
/// let temp = tempdir().unwrap();
/// let path = temp.path();
///
/// // Create both manifest files
/// fs::write(path.join("agent.toml"), "# Agent").unwrap();
/// fs::write(path.join("snippet.toml"), "# Snippet").unwrap();
///
/// // Agent takes precedence
/// assert_eq!(detect_resource_type(path), Some(ResourceType::Agent));
/// ```
///
/// # File System Requirements
///
/// The function only checks for file existence using [`Path::exists`]. It does not:
/// - Validate manifest file syntax or content
/// - Check file permissions or readability
/// - Examine the actual resource files (*.md)
///
/// For full resource validation, use the [`Resource::validate`] method on loaded resources.
///
/// [`Resource::validate`]: crate::core::Resource::validate
#[allow(dead_code)]
pub fn detect_resource_type(path: &Path) -> Option<ResourceType> {
    // Check for agent.toml first (takes precedence)
    if path.join("agent.toml").exists() {
        return Some(ResourceType::Agent);
    }

    // Check for snippet.toml
    if path.join("snippet.toml").exists() {
        return Some(ResourceType::Snippet);
    }

    None
}

/// Base trait defining the interface for all CCPM resources
///
/// This trait provides a common interface for different types of resources (agents, snippets)
/// managed by CCPM. It abstracts the core operations that can be performed on any resource,
/// including validation, installation, and metadata access.
///
/// # Design Principles
///
/// - **Type Safety**: Each resource has a specific [`ResourceType`]
/// - **Validation**: Resources can validate their own structure and dependencies
/// - **Installation**: Resources know how to install themselves to target locations
/// - **Metadata**: Resources provide structured metadata for tooling and display
/// - **Flexibility**: Resources can be profiled or configured during installation
///
/// # Implementation Requirements
///
/// Implementors of this trait should:
/// - Provide meaningful error messages in validation failures
/// - Support atomic installation operations (no partial installs on failure)
/// - Generate deterministic installation paths
/// - Include rich metadata for resource discovery and management
///
/// # Examples
///
/// ## Basic Resource Usage Pattern
///
/// ```rust
/// use ccpm::core::{Resource, ResourceType};
/// use anyhow::Result;
/// use std::path::Path;
///
/// fn process_resource(resource: &dyn Resource) -> Result<()> {
///     // Get basic information
///     println!("Processing resource: {}", resource.name());
///     println!("Type: {}", resource.resource_type());
///     
///     if let Some(description) = resource.description() {
///         println!("Description: {}", description);
///     }
///     
///     // Validate the resource
///     resource.validate()?;
///     
///     // Install to default location
///     let target = Path::new("./resources");
///     let install_path = resource.install_path(target);
///     resource.install(&install_path, None)?;
///     
///     Ok(())
/// }
/// ```
///
/// ## Metadata Extraction
///
/// ```rust
/// use ccpm::core::Resource;
/// use anyhow::Result;
///
/// fn extract_metadata(resource: &dyn Resource) -> Result<()> {
///     let metadata = resource.metadata()?;
///     
///     // Metadata is JSON Value for flexibility
///     if let Some(version) = metadata.get("version") {
///         println!("Resource version: {}", version);
///     }
///     
///     if let Some(tags) = metadata.get("tags").and_then(|t| t.as_array()) {
///         println!("Tags: {:?}", tags);
///     }
///     
///     Ok(())
/// }
/// ```
///
/// # Trait Object Usage
///
/// The trait is object-safe and can be used as a trait object:
///
/// ```rust
/// use ccpm::core::Resource;
/// use std::any::Any;
///
/// fn handle_resource(resource: Box<dyn Resource>) {
///     println!("Handling resource: {}", resource.name());
///     
///     // Can be downcasted to concrete types if needed
///     let any = resource.as_any();
///     // ... downcasting logic
/// }
/// ```
pub trait Resource {
    /// Get the unique name identifier for this resource
    ///
    /// The name is used to identify the resource in manifests, lockfiles,
    /// and CLI operations. It should be unique within a project's namespace.
    ///
    /// # Returns
    ///
    /// A string slice containing the resource name
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    ///
    /// fn print_resource_info(resource: &dyn Resource) {
    ///     println!("Resource name: {}", resource.name());
    /// }
    /// ```
    fn name(&self) -> &str;

    /// Get the resource type classification
    ///
    /// Returns the [`ResourceType`] enum value that identifies what kind of
    /// resource this is (Agent, Snippet, etc.).
    ///
    /// # Returns
    ///
    /// The [`ResourceType`] for this resource
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::{Resource, ResourceType};
    ///
    /// fn categorize_resource(resource: &dyn Resource) {
    ///     match resource.resource_type() {
    ///         ResourceType::Agent => println!("This is an AI agent"),
    ///         ResourceType::Snippet => println!("This is a code snippet"),
    ///     }
    /// }
    /// ```
    fn resource_type(&self) -> ResourceType;

    /// Get the human-readable description of this resource
    ///
    /// Returns an optional description that explains what the resource does
    /// or how it should be used. This is typically displayed in resource
    /// listings and help text.
    ///
    /// # Returns
    ///
    /// - `Some(description)` if the resource has a description
    /// - `None` if no description is available
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    ///
    /// fn show_resource_details(resource: &dyn Resource) {
    ///     println!("Name: {}", resource.name());
    ///     if let Some(desc) = resource.description() {
    ///         println!("Description: {}", desc);
    ///     } else {
    ///         println!("No description available");
    ///     }
    /// }
    /// ```
    fn description(&self) -> Option<&str>;

    /// Get the list of dependencies required by this resource
    ///
    /// Returns dependencies that must be installed before this resource
    /// can function properly. Dependencies are resolved automatically
    /// during installation.
    ///
    /// # Returns
    ///
    /// - `Some(dependencies)` if the resource has dependencies
    /// - `None` if the resource has no dependencies
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    ///
    /// fn check_dependencies(resource: &dyn Resource) {
    ///     if let Some(deps) = resource.dependencies() {
    ///         println!("{} has {} dependencies", resource.name(), deps.len());
    ///         for dep in deps {
    ///             // Process dependency...
    ///         }
    ///     } else {
    ///         println!("{} has no dependencies", resource.name());
    ///     }
    /// }
    /// ```
    fn dependencies(&self) -> Option<&[crate::config::Dependency]>;

    /// Validate the resource structure and content
    ///
    /// Performs comprehensive validation of the resource including:
    /// - File structure integrity
    /// - Content format validation
    /// - Dependency constraint checking
    /// - Metadata consistency
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the resource is valid
    /// - `Err(error)` with detailed validation failure information
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    /// use anyhow::Result;
    ///
    /// fn validate_before_install(resource: &dyn Resource) -> Result<()> {
    ///     resource.validate()
    ///         .map_err(|e| anyhow::anyhow!("Resource validation failed: {}", e))?;
    ///     
    ///     println!("Resource {} is valid", resource.name());
    ///     Ok(())
    /// }
    /// ```
    fn validate(&self) -> Result<()>;

    /// Install the resource to the specified target path
    ///
    /// Performs the actual installation of the resource files to the target
    /// location. This operation should be atomic - either it succeeds completely
    /// or fails without making any changes.
    ///
    /// # Arguments
    ///
    /// * `target` - The directory path where the resource should be installed
    /// * `profile` - Optional profile name for customized installation (may be unused)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if installation succeeds
    /// - `Err(error)` if installation fails with detailed error information
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    /// use std::path::Path;
    /// use anyhow::Result;
    ///
    /// fn install_resource(resource: &dyn Resource) -> Result<()> {
    ///     let target = Path::new("./installed-resources");
    ///     
    ///     // Validate first
    ///     resource.validate()?;
    ///     
    ///     // Install without profile
    ///     resource.install(target, None)?;
    ///     
    ///     println!("Successfully installed {}", resource.name());
    ///     Ok(())
    /// }
    /// ```
    fn install(&self, target: &Path, profile: Option<&str>) -> Result<()>;

    /// Calculate the installation path for this resource
    ///
    /// Determines where this resource would be installed relative to a base
    /// directory. This is used for path planning and conflict detection.
    ///
    /// # Arguments
    ///
    /// * `base` - The base directory for installation
    ///
    /// # Returns
    ///
    /// The full path where this resource would be installed
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    /// use std::path::Path;
    ///
    /// fn check_install_location(resource: &dyn Resource) {
    ///     let base = Path::new("/project/resources");
    ///     let install_path = resource.install_path(base);
    ///     
    ///     println!("{} would be installed to: {}",
    ///         resource.name(),
    ///         install_path.display()
    ///     );
    /// }
    /// ```
    fn install_path(&self, base: &Path) -> std::path::PathBuf;

    /// Get structured metadata for this resource as JSON
    ///
    /// Returns resource metadata in a flexible JSON format that can include
    /// version information, tags, author details, and other custom fields.
    /// This metadata is used for resource discovery, filtering, and display.
    ///
    /// # Returns
    ///
    /// - `Ok(json_value)` containing the metadata
    /// - `Err(error)` if metadata cannot be generated or parsed
    ///
    /// # Metadata Structure
    ///
    /// While flexible, metadata typically includes:
    /// - `name`: Resource name
    /// - `type`: Resource type
    /// - `version`: Version information
    /// - `description`: Human-readable description
    /// - `tags`: Array of classification tags
    /// - `dependencies`: Dependency information
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    /// use anyhow::Result;
    ///
    /// fn show_resource_metadata(resource: &dyn Resource) -> Result<()> {
    ///     let metadata = resource.metadata()?;
    ///     
    ///     if let Some(version) = metadata.get("version") {
    ///         println!("Version: {}", version);
    ///     }
    ///     
    ///     if let Some(tags) = metadata.get("tags").and_then(|t| t.as_array()) {
    ///         print!("Tags: ");
    ///         for tag in tags {
    ///             print!("{} ", tag.as_str().unwrap_or("?"));
    ///         }
    ///         println!();
    ///     }
    ///     
    ///     Ok(())
    /// }
    /// ```
    fn metadata(&self) -> Result<serde_json::Value>;

    /// Get a reference to this resource as [`std::any::Any`] for downcasting
    ///
    /// This method enables downcasting from the [`Resource`] trait object to
    /// concrete resource implementations when needed for type-specific operations.
    ///
    /// # Returns
    ///
    /// A reference to this resource as [`std::any::Any`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::Resource;
    /// use std::any::Any;
    ///
    /// // Hypothetical concrete resource type
    /// struct MyAgent {
    ///     name: String,
    ///     // ... other fields
    /// }
    ///
    /// fn try_downcast_to_agent(resource: &dyn Resource) {
    ///     let any = resource.as_any();
    ///     
    ///     if let Some(agent) = any.downcast_ref::<MyAgent>() {
    ///         println!("Successfully downcasted to MyAgent: {}", agent.name);
    ///     } else {
    ///         println!("Resource is not a MyAgent type");
    ///     }
    /// }
    /// ```
    fn as_any(&self) -> &dyn std::any::Any;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_resource_type_manifest_filename() {
        assert_eq!(ResourceType::Agent.manifest_filename(), "agent.toml");
        assert_eq!(ResourceType::Snippet.manifest_filename(), "snippet.toml");
    }

    #[test]
    fn test_resource_type_default_directory() {
        assert_eq!(ResourceType::Agent.default_directory(), "agents");
        assert_eq!(ResourceType::Snippet.default_directory(), "snippets");
    }

    #[test]
    fn test_resource_type_display() {
        assert_eq!(ResourceType::Agent.to_string(), "agent");
        assert_eq!(ResourceType::Snippet.to_string(), "snippet");
    }

    #[test]
    fn test_resource_type_from_str() {
        use std::str::FromStr;

        assert_eq!(
            ResourceType::from_str("agent").unwrap(),
            ResourceType::Agent
        );
        assert_eq!(
            ResourceType::from_str("snippet").unwrap(),
            ResourceType::Snippet
        );
        assert_eq!(
            ResourceType::from_str("AGENT").unwrap(),
            ResourceType::Agent
        );
        assert_eq!(
            ResourceType::from_str("Snippet").unwrap(),
            ResourceType::Snippet
        );

        assert!(ResourceType::from_str("invalid").is_err());
    }

    #[test]
    fn test_resource_type_serialization() {
        let agent = ResourceType::Agent;
        let json = serde_json::to_string(&agent).unwrap();
        assert_eq!(json, "\"agent\"");

        let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ResourceType::Agent);
    }

    #[test]
    fn test_detect_resource_type_agent() {
        let temp = tempdir().unwrap();
        let agent_toml = temp.path().join("agent.toml");
        std::fs::write(&agent_toml, "# Agent manifest").unwrap();

        assert_eq!(detect_resource_type(temp.path()), Some(ResourceType::Agent));
    }

    #[test]
    fn test_detect_resource_type_snippet() {
        let temp = tempdir().unwrap();
        let snippet_toml = temp.path().join("snippet.toml");
        std::fs::write(&snippet_toml, "# Snippet manifest").unwrap();

        assert_eq!(
            detect_resource_type(temp.path()),
            Some(ResourceType::Snippet)
        );
    }

    #[test]
    fn test_detect_resource_type_none() {
        let temp = tempdir().unwrap();
        assert_eq!(detect_resource_type(temp.path()), None);
    }

    #[test]
    fn test_detect_resource_type_both() {
        let temp = tempdir().unwrap();
        let agent_toml = temp.path().join("agent.toml");
        let snippet_toml = temp.path().join("snippet.toml");
        std::fs::write(&agent_toml, "# Agent manifest").unwrap();
        std::fs::write(&snippet_toml, "# Snippet manifest").unwrap();

        // Agent takes precedence when both exist
        assert_eq!(detect_resource_type(temp.path()), Some(ResourceType::Agent));
    }
}
