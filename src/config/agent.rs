//! Agent and snippet configuration structures.
//!
//! This module defines the configuration structures for CCPM resources (agents and snippets).
//! These structures can be used standalone or embedded as frontmatter in Markdown files.
//! The configuration system supports rich metadata, dependency management, and platform-specific
//! requirements.
//!
//! # Resource Types
//!
//! CCPM supports two main types of resources:
//!
//! ## Agents
//!
//! AI agents are sophisticated Claude Code resources that provide specialized functionality.
//! They typically include:
//! - Complex prompt engineering
//! - Multi-step workflows
//! - Context management
//! - Integration with external tools
//!
//! ## Snippets
//!
//! Code snippets are reusable pieces of code or configuration that can be:
//! - Language-specific code patterns
//! - Configuration templates
//! - Documentation examples
//! - Utility functions
//!
//! # Configuration Formats
//!
//! Resource configuration can be specified in multiple ways:
//!
//! ## Standalone TOML Files
//!
//! Dedicated configuration files (e.g., `agent.toml`, `snippet.toml`):
//!
//! ```toml
//! [metadata]
//! name = "rust-expert"
//! description = "Expert Rust development agent"
//! author = "CCPM Community"
//! license = "MIT"
//! homepage = "https://github.com/ccpm-community/rust-expert"
//! keywords = ["rust", "programming", "expert", "development"]
//! categories = ["development", "programming-languages"]
//!
//! [requirements]
//! ccpm_version = ">=0.1.0"
//! claude_version = "latest"
//! platforms = ["windows", "macos", "linux"]
//!
//! [[requirements.dependencies]]
//! name = "code-formatter"
//! version = "^1.0"
//! type = "snippet"
//! source = "community"
//!
//! [config]
//! max_context_length = 8000
//! preferred_style = "verbose"
//! ```
//!
//! ## Markdown Frontmatter
//!
//! Configuration embedded in `.md` files using TOML frontmatter:
//!
//! ```markdown
//! +++
//! [metadata]
//! name = "python-expert"
//! description = "Expert Python development agent"
//! author = "Jane Developer <jane@example.com>"
//! license = "Apache-2.0"
//! keywords = ["python", "expert", "development"]
//!
//! [requirements]
//! ccpm_version = ">=0.1.0"
//! +++
//!
//! # Python Expert Agent
//!
//! You are an expert Python developer with deep knowledge...
//! ```
//!
//! # Metadata Fields
//!
//! All resources support common metadata fields:
//!
//! - **name**: Unique identifier for the resource
//! - **description**: Human-readable description
//! - **author**: Author information (name and optional email)
//! - **license**: SPDX license identifier
//! - **homepage**: Optional homepage URL
//! - **repository**: Optional source repository URL
//! - **keywords**: List of searchable keywords
//! - **categories**: Hierarchical categorization
//!
//! # Dependency Management
//!
//! Resources can declare dependencies on other resources:
//!
//! ```toml
//! [[requirements.dependencies]]
//! name = "base-formatter"        # Name of dependency
//! version = "^1.2"              # Version constraint
//! type = "snippet"              # Resource type (agent/snippet)
//! source = "community"          # Source repository
//! optional = false              # Required vs optional
//! ```
//!
//! # Version Constraints
//!
//! Dependencies support semantic versioning constraints:
//!
//! - `"1.2.3"` - Exact version
//! - `"^1.2"` - Compatible version (>=1.2.0, <2.0.0)
//! - `"~1.2.3"` - Patch-level changes (>=1.2.3, <1.3.0)
//! - `">=1.0.0"` - Minimum version
//! - `"latest"` - Latest available version
//!
//! # Platform Support
//!
//! Resources can specify platform requirements:
//!
//! ```toml
//! [requirements]
//! platforms = ["windows", "macos", "linux", "web"]
//! ```
//!
//! Available platforms:
//! - `windows` - Windows operating system
//! - `macos` - macOS operating system  
//! - `linux` - Linux distributions
//! - `web` - Web-based environments
//!
//! # Custom Configuration
//!
//! Resources can include custom configuration using the `config` section:
//!
//! ```toml
//! [config]
//! max_tokens = 4000
//! temperature = 0.7
//! style = "concise"
//! features = ["formatting", "linting"]
//!
//! [config.advanced]
//! retry_count = 3
//! timeout = 30
//! ```
//!
//! # Examples
//!
//! ## Loading Agent Configuration
//!
//! ```rust,no_run
//! use ccpm::config::AgentManifest;
//! use std::path::Path;
//!
//! # fn example() -> anyhow::Result<()> {
//! let manifest = AgentManifest::load(Path::new("agent.toml"))?;
//!
//! println!("Agent: {} by {}",
//!          manifest.metadata.name,
//!          manifest.metadata.author);
//!
//! if let Some(requirements) = &manifest.requirements {
//!     println!("Dependencies: {}", requirements.dependencies.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Creating Default Configuration
//!
//! ```rust,ignore
//! use ccpm::config::create_agent_manifest;
//!
//! let manifest = create_agent_manifest(
//!     "my-agent".to_string(),
//!     "John Developer <john@example.com>".to_string()
//! );
//!
//! assert_eq!(manifest.metadata.name, "my-agent");
//! assert_eq!(manifest.metadata.license, "MIT");
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Agent configuration manifest.
///
/// Represents the complete configuration for a CCPM agent, including metadata,
/// requirements, and custom configuration. This structure can be loaded from
/// standalone TOML files or extracted from Markdown frontmatter.
///
/// # Structure
///
/// - [`metadata`](Self::metadata): Core information about the agent
/// - [`requirements`](Self::requirements): Optional dependency and platform requirements  
/// - [`config`](Self::config): Custom configuration as key-value pairs
///
/// # Examples
///
/// ## Minimal Agent
///
/// ```rust,no_run
/// use ccpm::config::{AgentManifest, AgentMetadata};
/// use std::collections::HashMap;
///
/// let manifest = AgentManifest {
///     metadata: AgentMetadata {
///         name: "simple-agent".to_string(),
///         description: "A simple agent".to_string(),
///         author: "Developer".to_string(),
///         license: "MIT".to_string(),
///         homepage: None,
///         repository: None,
///         keywords: vec![],
///         categories: vec![],
///     },
///     requirements: None,
///     config: HashMap::new(),
/// };
/// ```
///
/// ## Loading from File
///
/// ```rust,no_run
/// use ccpm::config::AgentManifest;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let manifest = AgentManifest::load(Path::new("my-agent.toml"))?;
/// println!("Loaded agent: {}", manifest.metadata.name);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Core metadata about the agent.
    ///
    /// Contains essential information like name, description, author, and categorization.
    /// This metadata is used for discovery, documentation, and dependency resolution.
    pub metadata: AgentMetadata,

    /// Optional requirements and dependencies.
    ///
    /// Specifies version requirements, platform constraints, and dependencies on other
    /// resources. If `None`, the agent has no special requirements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirements: Option<Requirements>,

    /// Custom configuration values.
    ///
    /// Arbitrary key-value pairs that can be used by the agent for configuration.
    /// Values can be any valid TOML type (string, number, boolean, array, table).
    ///
    /// # Examples
    ///
    /// ```toml
    /// [config]
    /// max_tokens = 4000
    /// style = "verbose"
    /// features = ["linting", "formatting"]
    ///
    /// [config.advanced]
    /// retry_attempts = 3
    /// timeout_seconds = 30
    /// ```
    #[serde(default)]
    pub config: HashMap<String, toml::Value>,
}

impl AgentManifest {
    /// Load agent manifest from a TOML file.
    ///
    /// Reads and parses an agent configuration file from the specified path.
    ///
    /// # Parameters
    ///
    /// - `path`: Path to the TOML configuration file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use ccpm::config::AgentManifest;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let manifest = AgentManifest::load(Path::new("agents/rust-expert.toml"))?;
    /// println!("Agent: {}", manifest.metadata.name);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read (not found, permissions, etc.)
    /// - The file contains invalid TOML syntax
    /// - The TOML structure doesn't match the expected schema
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read agent manifest: {}", path.display()))?;
        let manifest: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse agent manifest: {}", path.display()))?;
        Ok(manifest)
    }
}

/// Agent metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    /// Agent name
    pub name: String,

    /// Agent description
    pub description: String,

    /// Author information
    pub author: String,

    /// License
    pub license: String,

    /// Homepage URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// Repository URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// Keywords for discovery
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Categories
    #[serde(default)]
    pub categories: Vec<String>,
}

/// Snippet manifest (snippet.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetManifest {
    /// Snippet metadata
    pub metadata: SnippetMetadata,

    /// Snippet content (can be inline or file reference)
    pub content: SnippetContent,

    /// Custom configuration values specific to this snippet.
    ///
    /// Similar to agent configuration, this allows arbitrary key-value pairs
    /// for snippet-specific settings like formatting options, execution parameters,
    /// or integration settings.
    #[serde(default)]
    pub config: HashMap<String, toml::Value>,
}

impl SnippetManifest {
    /// Loads a snippet manifest from a TOML file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the snippet manifest file
    ///
    /// # Returns
    ///
    /// Returns the parsed `SnippetManifest` on success
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read
    /// - The TOML content is invalid
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read snippet manifest: {}", path.display()))?;
        let manifest: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse snippet manifest: {}", path.display()))?;
        Ok(manifest)
    }
}

/// Snippet metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetMetadata {
    /// Snippet name
    pub name: String,

    /// Snippet description
    pub description: String,

    /// Author information
    pub author: String,

    /// Programming language
    pub language: String,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Keywords for discovery
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Snippet content specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SnippetContent {
    /// Inline snippet content
    Inline {
        /// The snippet content as a string
        content: String,
    },

    /// File-based snippet content
    File {
        /// Path to the file containing the snippet
        file: String,
    },

    /// Multiple files
    Files {
        /// List of file paths containing snippet parts
        files: Vec<String>,
    },
}

/// Requirements and dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirements {
    /// Minimum CCPM version required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccpm_version: Option<String>,

    /// Required Claude version/features
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_version: Option<String>,

    /// Dependencies on other resources
    #[serde(default)]
    pub dependencies: Vec<Dependency>,

    /// Platform requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<String>>,
}

/// Resource dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// Dependency name
    pub name: String,

    /// Version constraint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Dependency type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,

    /// Source repository
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Optional dependency
    #[serde(default)]
    pub optional: bool,
}

/// Load agent manifest from file
#[allow(dead_code)]
pub fn load_agent_manifest(path: &Path) -> Result<AgentManifest> {
    let content = std::fs::read_to_string(path)?;
    let manifest: AgentManifest = toml::from_str(&content)?;
    Ok(manifest)
}

/// Load snippet manifest from file
#[allow(dead_code)]
pub fn load_snippet_manifest(path: &Path) -> Result<SnippetManifest> {
    let content = std::fs::read_to_string(path)?;
    let manifest: SnippetManifest = toml::from_str(&content)?;
    Ok(manifest)
}

/// Create a default agent manifest
#[allow(dead_code)]
pub fn create_agent_manifest(name: String, author: String) -> AgentManifest {
    AgentManifest {
        metadata: AgentMetadata {
            name: name.clone(),
            description: format!("{name} agent for Claude Code"),
            author,
            license: "MIT".to_string(),
            homepage: None,
            repository: None,
            keywords: vec![],
            categories: vec![],
        },
        requirements: None,
        config: HashMap::new(),
    }
}

/// Create a default snippet manifest
#[allow(dead_code)]
pub fn create_snippet_manifest(name: String, author: String, language: String) -> SnippetManifest {
    SnippetManifest {
        metadata: SnippetMetadata {
            name: name.clone(),
            description: format!("{name} snippet"),
            author,
            language,
            tags: vec![],
            keywords: vec![],
        },
        content: SnippetContent::File {
            file: "snippet.md".to_string(),
        },
        config: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_agent_manifest() {
        let manifest = create_agent_manifest("test-agent".to_string(), "John Doe".to_string());
        assert_eq!(manifest.metadata.name, "test-agent");
        assert_eq!(manifest.metadata.author, "John Doe");
        assert_eq!(manifest.metadata.license, "MIT");
        assert_eq!(
            manifest.metadata.description,
            "test-agent agent for Claude Code"
        );
    }

    #[test]
    fn test_create_snippet_manifest() {
        let manifest = create_snippet_manifest(
            "test-snippet".to_string(),
            "Jane Doe".to_string(),
            "python".to_string(),
        );
        assert_eq!(manifest.metadata.name, "test-snippet");
        assert_eq!(manifest.metadata.author, "Jane Doe");
        assert_eq!(manifest.metadata.language, "python");
        assert_eq!(manifest.metadata.description, "test-snippet snippet");
    }

    #[test]
    fn test_snippet_content_variants() {
        let inline = SnippetContent::Inline {
            content: "print('hello')".to_string(),
        };

        let file = SnippetContent::File {
            file: "snippet.py".to_string(),
        };

        let files = SnippetContent::Files {
            files: vec!["file1.py".to_string(), "file2.py".to_string()],
        };

        // Test serialization
        let inline_json = serde_json::to_string(&inline).unwrap();
        assert!(inline_json.contains("content"));

        let file_json = serde_json::to_string(&file).unwrap();
        assert!(file_json.contains("file"));

        let files_json = serde_json::to_string(&files).unwrap();
        assert!(files_json.contains("files"));
    }

    #[test]
    fn test_dependency() {
        let dep = Dependency {
            name: "test-dep".to_string(),
            version: Some("^1.0.0".to_string()),
            r#type: Some("agent".to_string()),
            source: Some("github".to_string()),
            optional: false,
        };

        assert_eq!(dep.name, "test-dep");
        assert_eq!(dep.version, Some("^1.0.0".to_string()));
        assert!(!dep.optional);
    }

    #[test]
    fn test_requirements() {
        let req = Requirements {
            ccpm_version: Some(">=0.1.0".to_string()),
            claude_version: Some("latest".to_string()),
            dependencies: vec![Dependency {
                name: "dep1".to_string(),
                version: None,
                r#type: None,
                source: None,
                optional: false,
            }],
            platforms: Some(vec!["windows".to_string(), "macos".to_string()]),
        };

        assert_eq!(req.ccpm_version, Some(">=0.1.0".to_string()));
        assert_eq!(req.dependencies.len(), 1);
        assert_eq!(req.platforms.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_save_and_load_agent_manifest() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("agent.toml");

        let manifest = create_agent_manifest("test".to_string(), "author".to_string());

        let toml_str = toml::to_string(&manifest).unwrap();
        std::fs::write(&manifest_path, toml_str).unwrap();

        let loaded = load_agent_manifest(&manifest_path).unwrap();
        assert_eq!(loaded.metadata.name, "test");
        assert_eq!(loaded.metadata.author, "author");
    }

    #[test]
    fn test_save_and_load_snippet_manifest() {
        let temp = tempdir().unwrap();
        let manifest_path = temp.path().join("snippet.toml");

        let manifest =
            create_snippet_manifest("test".to_string(), "author".to_string(), "rust".to_string());

        let toml_str = toml::to_string(&manifest).unwrap();
        std::fs::write(&manifest_path, toml_str).unwrap();

        let loaded = load_snippet_manifest(&manifest_path).unwrap();
        assert_eq!(loaded.metadata.name, "test");
        assert_eq!(loaded.metadata.language, "rust");
    }
}
