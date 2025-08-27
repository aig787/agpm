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
//!
//! # Resource Management
//!
//! Resources are defined in the project's `ccpm.toml` file and installed to specific
//! directories based on their type. Scripts and hooks have special handling for
//! Claude Code integration.
//!
//! # Examples
//!
//! ## Working with Resource Types
//!
//! ```rust
//! use ccpm::core::ResourceType;
//! use std::path::Path;
//!
//! // Convert strings to resource types
//! let agent_type: ResourceType = "agent".parse().unwrap();
//! let snippet_type: ResourceType = "snippet".parse().unwrap();
//! let script_type: ResourceType = "script".parse().unwrap();
//! let hook_type: ResourceType = "hook".parse().unwrap();
//!
//! // Get default directory names
//! assert_eq!(agent_type.default_directory(), ".claude/agents/ccpm");
//! assert_eq!(snippet_type.default_directory(), ".claude/ccpm/snippets");
//! assert_eq!(script_type.default_directory(), ".claude/ccpm/scripts");
//! assert_eq!(hook_type.default_directory(), ".claude/ccpm/hooks");
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
/// - **Default Directory**: `.claude/agents/ccpm`
/// - **Common Use Cases**: Claude Code agents, custom AI assistants, specialized prompts
///
/// ## Snippet  
/// - **Purpose**: Reusable code templates, examples, and documentation fragments
/// - **Default Directory**: `.claude/ccpm/snippets`
/// - **Common Use Cases**: Code templates, configuration examples, documentation
///
/// ## Script
/// - **Purpose**: Executable files that can be run by hooks or independently
/// - **Default Directory**: `.claude/ccpm/scripts`
/// - **Common Use Cases**: Validation scripts, automation tools, hook executables
///
/// ## Hook
/// - **Purpose**: Event-based automation configurations for Claude Code
/// - **Default Directory**: `.claude/ccpm/hooks`
/// - **Common Use Cases**: Pre/Post tool use validation, custom event handlers
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
/// ## Directory Names
///
/// ```rust
/// use ccpm::core::ResourceType;
///
/// let agent = ResourceType::Agent;
/// assert_eq!(agent.default_directory(), ".claude/agents/ccpm");
///
/// let snippet = ResourceType::Snippet;  
/// assert_eq!(snippet.default_directory(), ".claude/ccpm/snippets");
///
/// let script = ResourceType::Script;
/// assert_eq!(script.default_directory(), ".claude/ccpm/scripts");
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

    /// Claude Code commands
    ///
    /// Commands define custom slash commands that can be used within Claude Code
    /// to perform specific actions or automate workflows.
    Command,

    /// MCP (Model Context Protocol) servers
    ///
    /// MCP servers provide integrations with external systems and services,
    /// allowing Claude Code to access databases, APIs, and other tools.
    #[serde(rename = "mcp-server")]
    McpServer,

    /// Executable script files
    ///
    /// Scripts are executable files (.sh, .js, .py, etc.) that can be referenced
    /// by hooks or run independently. They are installed to .claude/ccpm/scripts/
    Script,

    /// Hook configuration files
    ///
    /// Hooks define event-based automation in Claude Code. They are JSON files
    /// that configure scripts to run at specific events (PreToolUse, PostToolUse, etc.)
    /// and are merged into settings.local.json
    Hook,
    // Future resource types can be added here
}

impl ResourceType {
    /// Get the default installation directory name for this resource type
    ///
    /// Returns the conventional directory name where resources of this type
    /// are typically installed in CCPM projects.
    ///
    /// # Returns
    ///
    /// - [`Agent`] → `".claude/agents/ccpm"`
    /// - [`Snippet`] → `".claude/ccpm/snippets"`
    /// - [`Command`] → `.claude/commands/ccpm`
    /// - [`McpServer`] → `.claude/ccpm/mcp-servers`
    /// - [`Script`] → `.claude/ccpm/scripts`
    /// - [`Hook`] → `.claude/ccpm/hooks`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ccpm::core::ResourceType;
    ///
    /// assert_eq!(ResourceType::Agent.default_directory(), ".claude/agents/ccpm");
    /// assert_eq!(ResourceType::Snippet.default_directory(), ".claude/ccpm/snippets");
    /// assert_eq!(ResourceType::Command.default_directory(), ".claude/commands/ccpm");
    /// assert_eq!(ResourceType::McpServer.default_directory(), ".claude/ccpm/mcp-servers");
    /// assert_eq!(ResourceType::Script.default_directory(), ".claude/ccpm/scripts");
    /// assert_eq!(ResourceType::Hook.default_directory(), ".claude/ccpm/hooks");
    /// ```
    ///
    /// # Note
    ///
    /// This is just the default convention. Users can install resources to any
    /// directory by specifying custom paths in their manifest files.
    ///
    /// [`Agent`]: ResourceType::Agent
    /// [`Snippet`]: ResourceType::Snippet
    /// [`Command`]: ResourceType::Command
    /// [`McpServer`]: ResourceType::McpServer
    /// [`Script`]: ResourceType::Script
    /// [`Hook`]: ResourceType::Hook
    #[must_use]
    pub fn default_directory(&self) -> &str {
        match self {
            ResourceType::Agent => ".claude/agents/ccpm",
            ResourceType::Snippet => ".claude/ccpm/snippets",
            ResourceType::Command => ".claude/commands/ccpm",
            ResourceType::McpServer => ".claude/ccpm/mcp-servers",
            ResourceType::Script => ".claude/ccpm/scripts",
            ResourceType::Hook => ".claude/ccpm/hooks",
        }
    }
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Agent => write!(f, "agent"),
            ResourceType::Snippet => write!(f, "snippet"),
            ResourceType::Command => write!(f, "command"),
            ResourceType::McpServer => write!(f, "mcp-server"),
            ResourceType::Script => write!(f, "script"),
            ResourceType::Hook => write!(f, "hook"),
        }
    }
}

impl std::str::FromStr for ResourceType {
    type Err = crate::core::CcpmError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "agent" => Ok(ResourceType::Agent),
            "snippet" => Ok(ResourceType::Snippet),
            "command" => Ok(ResourceType::Command),
            "mcp-server" | "mcpserver" | "mcp" => Ok(ResourceType::McpServer),
            "script" => Ok(ResourceType::Script),
            "hook" => Ok(ResourceType::Hook),
            _ => Err(crate::core::CcpmError::InvalidResourceType {
                resource_type: s.to_string(),
            }),
        }
    }
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
    ///         ResourceType::Command => println!("This is a Claude Code command"),
    ///         ResourceType::McpServer => println!("This is an MCP server"),
    ///         ResourceType::Script => println!("This is an executable script"),
    ///         ResourceType::Hook => println!("This is a hook configuration"),
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

    #[test]
    fn test_resource_type_default_directory() {
        assert_eq!(
            ResourceType::Agent.default_directory(),
            ".claude/agents/ccpm"
        );
        assert_eq!(
            ResourceType::Snippet.default_directory(),
            ".claude/ccpm/snippets"
        );
        assert_eq!(
            ResourceType::Command.default_directory(),
            ".claude/commands/ccpm"
        );
        assert_eq!(
            ResourceType::McpServer.default_directory(),
            ".claude/ccpm/mcp-servers"
        );
        assert_eq!(
            ResourceType::Script.default_directory(),
            ".claude/ccpm/scripts"
        );
        assert_eq!(ResourceType::Hook.default_directory(), ".claude/ccpm/hooks");
    }

    #[test]
    fn test_resource_type_display() {
        assert_eq!(ResourceType::Agent.to_string(), "agent");
        assert_eq!(ResourceType::Snippet.to_string(), "snippet");
        assert_eq!(ResourceType::Command.to_string(), "command");
        assert_eq!(ResourceType::McpServer.to_string(), "mcp-server");
        assert_eq!(ResourceType::Script.to_string(), "script");
        assert_eq!(ResourceType::Hook.to_string(), "hook");
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
        assert_eq!(
            ResourceType::from_str("command").unwrap(),
            ResourceType::Command
        );
        assert_eq!(
            ResourceType::from_str("COMMAND").unwrap(),
            ResourceType::Command
        );
        assert_eq!(
            ResourceType::from_str("mcp-server").unwrap(),
            ResourceType::McpServer
        );
        assert_eq!(
            ResourceType::from_str("MCP").unwrap(),
            ResourceType::McpServer
        );
        assert_eq!(
            ResourceType::from_str("script").unwrap(),
            ResourceType::Script
        );
        assert_eq!(
            ResourceType::from_str("SCRIPT").unwrap(),
            ResourceType::Script
        );
        assert_eq!(ResourceType::from_str("hook").unwrap(), ResourceType::Hook);
        assert_eq!(ResourceType::from_str("HOOK").unwrap(), ResourceType::Hook);

        assert!(ResourceType::from_str("invalid").is_err());
    }

    #[test]
    fn test_resource_type_serialization() {
        let agent = ResourceType::Agent;
        let json = serde_json::to_string(&agent).unwrap();
        assert_eq!(json, "\"agent\"");

        let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ResourceType::Agent);

        // Test command serialization
        let command = ResourceType::Command;
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(json, "\"command\"");

        let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ResourceType::Command);

        // Test mcp-server serialization
        let mcp_server = ResourceType::McpServer;
        let json = serde_json::to_string(&mcp_server).unwrap();
        assert_eq!(json, "\"mcp-server\"");

        let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ResourceType::McpServer);

        // Test script serialization
        let script = ResourceType::Script;
        let json = serde_json::to_string(&script).unwrap();
        assert_eq!(json, "\"script\"");

        let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ResourceType::Script);

        // Test hook serialization
        let hook = ResourceType::Hook;
        let json = serde_json::to_string(&hook).unwrap();
        assert_eq!(json, "\"hook\"");

        let deserialized: ResourceType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ResourceType::Hook);
    }

    #[test]
    fn test_resource_type_equality() {
        assert_eq!(ResourceType::Command, ResourceType::Command);
        assert_ne!(ResourceType::Command, ResourceType::Agent);
        assert_ne!(ResourceType::Command, ResourceType::Snippet);
        assert_eq!(ResourceType::McpServer, ResourceType::McpServer);
        assert_ne!(ResourceType::McpServer, ResourceType::Agent);
        assert_eq!(ResourceType::Script, ResourceType::Script);
        assert_ne!(ResourceType::Script, ResourceType::Hook);
        assert_eq!(ResourceType::Hook, ResourceType::Hook);
        assert_ne!(ResourceType::Hook, ResourceType::Agent);
    }

    #[test]
    fn test_resource_type_copy() {
        let command = ResourceType::Command;
        let copied = command; // ResourceType implements Copy, so this creates a copy
        assert_eq!(command, copied);
    }
}
