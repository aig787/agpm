//! Mutation operations for manifest data.
//!
//! This module contains all methods that modify the manifest, including:
//! - Adding sources
//! - Adding dependencies
//! - Getting mutable references to dependencies

use crate::manifest::{Manifest, ResourceDependency};
use std::collections::HashMap;

impl Manifest {
    /// Get mutable dependencies for a specific resource type.
    ///
    /// Returns a mutable reference to the `HashMap` of dependencies for the specified resource type.
    #[must_use]
    pub fn get_dependencies_mut(
        &mut self,
        resource_type: crate::core::ResourceType,
    ) -> Option<&mut HashMap<String, ResourceDependency>> {
        use crate::core::ResourceType;
        match resource_type {
            ResourceType::Agent => Some(&mut self.agents),
            ResourceType::Snippet => Some(&mut self.snippets),
            ResourceType::Command => Some(&mut self.commands),
            ResourceType::Script => Some(&mut self.scripts),
            ResourceType::Hook => Some(&mut self.hooks),
            ResourceType::McpServer => Some(&mut self.mcp_servers),
        }
    }

    /// Add or update a source repository in the `[sources]` section.
    ///
    /// Sources map convenient names to Git repository URLs. These names can
    /// then be referenced in dependency specifications to avoid repeating
    /// long URLs throughout the manifest.
    ///
    /// # Parameters
    ///
    /// - `name`: Short, convenient name for the source (e.g., "official", "community")
    /// - `url`: Git repository URL (HTTPS, SSH, or file:// protocol)
    ///
    /// # URL Validation
    ///
    /// The URL is not validated when added - validation occurs during
    /// [`Self::validate`]. Supported URL formats:
    /// - `https://github.com/owner/repo.git`
    /// - `git@github.com:owner/repo.git`
    /// - `file:///absolute/path/to/repo`
    /// - `file:///path/to/local/repo`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::Manifest;
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add public repository
    /// manifest.add_source(
    ///     "community".to_string(),
    ///     "https://github.com/claude-community/resources.git".to_string()
    /// );
    ///
    /// // Add private repository (SSH)
    /// manifest.add_source(
    ///     "private".to_string(),
    ///     "git@github.com:company/private-resources.git".to_string()
    /// );
    ///
    /// // Add local repository
    /// manifest.add_source(
    ///     "local".to_string(),
    ///     "file:///home/user/my-resources".to_string()
    /// );
    /// ```
    ///
    /// # Security Note
    ///
    /// Never include authentication tokens in the URL. Use SSH keys or
    /// configure authentication globally in `~/.agpm/config.toml`.
    pub fn add_source(&mut self, name: String, url: String) {
        self.sources.insert(name, url);
    }

    /// Add or update a dependency in the appropriate section.
    ///
    /// Adds the dependency to either the `[agents]`, `[snippets]`, or `[commands]` section
    /// based on the `is_agent` parameter. If a dependency with the same name
    /// already exists in the target section, it will be replaced.
    ///
    /// **Note**: This method is deprecated in favor of [`Self::add_typed_dependency`]
    /// which provides explicit control over resource types.
    ///
    /// # Parameters
    ///
    /// - `name`: Unique name for the dependency within its section
    /// - `dep`: The dependency specification (Simple or Detailed)
    /// - `is_agent`: If true, adds to `[agents]`; if false, adds to `[snippets]`
    ///   (Note: Use [`Self::add_typed_dependency`] for commands and other resource types)
    ///
    /// # Validation
    ///
    /// The dependency is not validated when added - validation occurs during
    /// [`Self::validate`]. This allows for building manifests incrementally
    /// before all sources are defined.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency, DetailedDependency};
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add local agent dependency
    /// manifest.add_dependency(
    ///     "helper".to_string(),
    ///     ResourceDependency::Simple("../local/helper.md".to_string()),
    ///     true  // is_agent = true
    /// );
    ///
    /// // Add remote snippet dependency
    /// manifest.add_dependency(
    ///     "utils".to_string(),
    ///     ResourceDependency::Detailed(Box::new(DetailedDependency {
    ///         source: Some("community".to_string()),
    ///         path: "snippets/utils.md".to_string(),
    ///         version: Some("v1.0.0".to_string()),
    ///         branch: None,
    ///         rev: None,
    ///         command: None,
    ///         args: None,
    ///         target: None,
    ///         filename: None,
    ///         dependencies: None,
    ///         tool: Some("claude-code".to_string()),
    ///         flatten: None,
    ///         install: None,
    ///         template_vars: Some(serde_json::Value::Object(serde_json::Map::new())),
    ///     })),
    ///     false  // is_agent = false (snippet)
    /// );
    /// ```
    ///
    /// # Name Conflicts
    ///
    /// This method allows the same dependency name to exist in both the
    /// `[agents]` and `[snippets]` sections. However, some operations like
    /// [`Self::get_dependency`] will prefer agents over snippets when
    /// searching by name.
    pub fn add_dependency(&mut self, name: String, dep: ResourceDependency, is_agent: bool) {
        if is_agent {
            self.agents.insert(name, dep);
        } else {
            self.snippets.insert(name, dep);
        }
    }

    /// Add or update a dependency with specific resource type.
    ///
    /// This is the preferred method for adding dependencies as it explicitly
    /// specifies the resource type using the `ResourceType` enum.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    /// use agpm_cli::core::ResourceType;
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add command dependency
    /// manifest.add_typed_dependency(
    ///     "build".to_string(),
    ///     ResourceDependency::Simple("../commands/build.md".to_string()),
    ///     ResourceType::Command
    /// );
    /// ```
    pub fn add_typed_dependency(
        &mut self,
        name: String,
        dep: ResourceDependency,
        resource_type: crate::core::ResourceType,
    ) {
        match resource_type {
            crate::core::ResourceType::Agent => {
                self.agents.insert(name, dep);
            }
            crate::core::ResourceType::Snippet => {
                self.snippets.insert(name, dep);
            }
            crate::core::ResourceType::Command => {
                self.commands.insert(name, dep);
            }
            crate::core::ResourceType::McpServer => {
                // MCP servers don't use ResourceDependency, they have their own type
                // This method shouldn't be called for MCP servers
                panic!("Use add_mcp_server() for MCP server dependencies");
            }
            crate::core::ResourceType::Script => {
                self.scripts.insert(name, dep);
            }
            crate::core::ResourceType::Hook => {
                self.hooks.insert(name, dep);
            }
        }
    }

    /// Add or update an MCP server configuration.
    ///
    /// MCP servers now use standard `ResourceDependency` format,
    /// pointing to JSON configuration files in source repositories.
    ///
    /// # Examples
    ///
    /// ```rust,no_run,ignore
    /// use agpm_cli::manifest::{Manifest, ResourceDependency};
    ///
    /// let mut manifest = Manifest::new();
    ///
    /// // Add MCP server from source repository
    /// manifest.add_mcp_server(
    ///     "filesystem".to_string(),
    ///     ResourceDependency::Simple("../local/mcp-servers/filesystem.json".to_string())
    /// );
    /// ```
    pub fn add_mcp_server(&mut self, name: String, dependency: ResourceDependency) {
        self.mcp_servers.insert(name, dependency);
    }
}
