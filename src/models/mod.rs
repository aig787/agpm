//! Shared data models for CCPM operations
//!
//! This module provides reusable data structures that are used across
//! different CLI commands and core operations, ensuring consistency
//! and reducing code duplication.

use clap::Args;
use serde::{Deserialize, Serialize};

/// Common dependency specification used across commands
#[derive(Debug, Clone, Args)]
pub struct DependencySpec {
    /// Dependency specification string
    ///
    /// Format: `source:path[@version]` for Git sources or `path` for local files
    ///
    /// Git dependency formats:
    /// • `source:path@version` - Git source with specific version
    /// • `source:path` - Git source (defaults to "main")
    ///
    /// Local dependency formats:
    /// • `/absolute/path/file.md` - Absolute path
    /// • `./relative/path/file.md` - Relative path
    /// • `file:///path/to/file.md` - File URL
    /// • `C:\Windows\path\file.md` - Windows path
    ///
    /// Pattern formats (using glob patterns):
    /// • `source:agents/*.md@v1.0` - All .md files in agents/
    /// • `source:agents/**/review*.md` - All review files recursively
    /// • `./local/**/*.json` - All JSON files recursively
    ///
    /// Examples:
    /// • `official:agents/reviewer.md@v1.0.0`
    /// • `community:snippets/utils.md`
    /// • `./agents/local-agent.md`
    /// • `../shared/resources/hook.json`
    #[arg(value_name = "SPEC")]
    pub spec: String,

    /// Custom name for the dependency
    ///
    /// If not provided, the name will be derived from the file path.
    /// This allows for more descriptive or shorter names in the manifest.
    #[arg(long)]
    pub name: Option<String>,

    /// Force overwrite if dependency exists
    ///
    /// By default, adding a duplicate dependency will fail.
    /// Use this flag to replace existing dependencies.
    #[arg(long, short = 'f')]
    pub force: bool,
}

/// Arguments for adding an agent dependency
#[derive(Debug, Clone, Args)]
pub struct AgentDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding a snippet dependency
#[derive(Debug, Clone, Args)]
pub struct SnippetDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding a command dependency
#[derive(Debug, Clone, Args)]
pub struct CommandDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding an MCP server dependency
#[derive(Debug, Clone, Args)]
pub struct McpServerDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Enum representing all possible dependency types
#[derive(Debug, Clone)]
pub enum DependencyType {
    /// An agent dependency
    Agent(AgentDependency),
    /// A snippet dependency
    Snippet(SnippetDependency),
    /// A command dependency
    Command(CommandDependency),
    /// A script dependency
    Script(ScriptDependency),
    /// A hook dependency
    Hook(HookDependency),
    /// An MCP server dependency
    McpServer(McpServerDependency),
}

impl DependencyType {
    /// Get the common dependency specification
    #[must_use]
    pub fn common(&self) -> &DependencySpec {
        match self {
            DependencyType::Agent(dep) => &dep.common,
            DependencyType::Snippet(dep) => &dep.common,
            DependencyType::Command(dep) => &dep.common,
            DependencyType::Script(dep) => &dep.common,
            DependencyType::Hook(dep) => &dep.common,
            DependencyType::McpServer(dep) => &dep.common,
        }
    }

    /// Get the resource type as a string
    #[must_use]
    pub fn resource_type(&self) -> &'static str {
        match self {
            DependencyType::Agent(_) => "agent",
            DependencyType::Snippet(_) => "snippet",
            DependencyType::Command(_) => "command",
            DependencyType::Script(_) => "script",
            DependencyType::Hook(_) => "hook",
            DependencyType::McpServer(_) => "mcp-server",
        }
    }
}

/// Source repository specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    /// Name for the source
    pub name: String,

    /// Git repository URL
    pub url: String,
}

/// Resource installation options
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Skip installation, only update lockfile
    pub no_install: bool,

    /// Force reinstallation even if up to date
    pub force: bool,

    /// Suppress progress indicators
    pub quiet: bool,

    /// Use cached data only, don't fetch updates
    pub offline: bool,
}

/// Resource update options
#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    /// Update all dependencies
    pub all: bool,

    /// Specific dependencies to update
    pub dependencies: Vec<String>,

    /// Allow updating to incompatible versions
    pub breaking: bool,

    /// Suppress progress indicators
    pub quiet: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_type_common() {
        let agent = DependencyType::Agent(AgentDependency {
            common: DependencySpec {
                spec: "test:agent.md".to_string(),
                name: None,
                force: false,
            },
        });

        assert_eq!(agent.common().spec, "test:agent.md");
        assert_eq!(agent.resource_type(), "agent");
    }

    #[test]
    fn test_mcp_server_dependency() {
        let mcp = DependencyType::McpServer(McpServerDependency {
            common: DependencySpec {
                spec: "test:mcp.toml".to_string(),
                name: Some("test-server".to_string()),
                force: true,
            },
        });

        assert_eq!(mcp.common().spec, "test:mcp.toml");
        assert_eq!(mcp.common().name, Some("test-server".to_string()));
        assert!(mcp.common().force);
        assert_eq!(mcp.resource_type(), "mcp-server");
    }
}

/// Arguments for adding a script dependency
#[derive(Debug, Clone, Args)]
pub struct ScriptDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}

/// Arguments for adding a hook dependency
#[derive(Debug, Clone, Args)]
pub struct HookDependency {
    /// Common dependency specification fields
    #[command(flatten)]
    pub common: DependencySpec,
}
